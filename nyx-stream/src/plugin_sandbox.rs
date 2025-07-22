//! OS-level sandbox for dynamic plugins using seccomp + PID/UTS namespaces.
//!
//! This module is only compiled on Linux when the `dynamic_plugin` Cargo
//! feature is enabled. The sandbox creates a new process in isolated PID and
//! UTS namespaces, drops capabilities, installs a strict seccomp filter, and
//! finally `exec()`s the plugin binary.  The parent process receives a handle
//! to the child so that higher-level components can monitor liveness or send
//! Unix-domain socket messages.
#![cfg(all(feature = "dynamic_plugin", target_os = "linux"))]
#![forbid(unsafe_code)]

use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::{ffi::CString, io};

use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::unistd::{self, Gid, Uid};

/// Spawn a plugin process inside new PID & UTS namespaces and activate seccomp.
///
/// * `plugin_path` – path to the plugin executable or wrapper script.
/// * The function returns the spawned [`Child`] process ready to be waited on.
///   Any error installing namespaces or seccomp is forwarded as `io::Error`.
pub fn spawn_sandboxed_plugin<P: AsRef<Path>>(plugin_path: P) -> io::Result<Child> {
    let plugin = plugin_path.as_ref();
    if !plugin.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "plugin binary not found"));
    }

    // Use std::process::Command so we do not require Tokio's process feature.
    let mut cmd = Command::new(plugin);

    // Ensure no inherited file descriptors except stdio.
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Install pre-exec hook to enter namespaces, drop privs, set seccomp.
    unsafe {
        cmd.pre_exec(|| {
            // 1. Unshare into new namespaces: PID & UTS for isolation, IPC & NET for future hardening.
            let flags = CloneFlags::CLONE_NEWPID
                | CloneFlags::CLONE_NEWUTS
                | CloneFlags::CLONE_NEWIPC
                | CloneFlags::CLONE_NEWNET
                | CloneFlags::CLONE_NEWNS;
            unshare(flags).map_err(to_io_err)?;

            // 2. Give the sandbox a deterministic hostname.
            set_hostname("nyx-plugin")?;

            // 3. Drop to nobody user/group (UID/GID 65534).
            unistd::setgid(Gid::from_raw(65534)).map_err(to_io_err)?;
            unistd::setuid(Uid::from_raw(65534)).map_err(to_io_err)?;

            // 4. Install seccomp filter (whitelist) from nyx-core.
            nyx_core::sandbox::install_seccomp().map_err(to_io_err)?;
            Ok(())
        });
    }

    cmd.spawn()
}

/// Gracefully terminate a sandboxed plugin process.
/// Returns `Ok(())` if the signal is successfully delivered.
#[allow(dead_code)]
pub fn terminate(child: &Child) -> io::Result<()> {
    kill(unistd::Pid::from_raw(child.id() as i32), Signal::SIGTERM).map_err(to_io_err)
}

fn set_hostname(name: &str) -> io::Result<()> {
    let c = CString::new(name).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "hostname NUL"))?;
    unistd::sethostname(&c).map_err(to_io_err)
}

fn to_io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("{e}"))
}

/// Dummy stub for non-Linux or non-`dynamic_plugin` builds so that references
/// remain valid but unusable. Always returns an error at runtime.
#[cfg(not(all(feature = "dynamic_plugin", target_os = "linux")))]
compile_error!("plugin_sandbox should only be compiled with feature=dynamic_plugin on Linux"); 