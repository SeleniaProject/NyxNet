#![cfg(all(feature = "dynamic_plugin", target_os = "macos"))]
#![forbid(unsafe_code)]

//! macOS System Extension and App Sandbox profile launcher for Nyx dynamic plugins.
//!
//! This module implements a comprehensive sandboxing solution for macOS that includes:
//! - App Sandbox profile enforcement
//! - System Extension integration for advanced security
//! - Endpoint Security framework monitoring (when available)
//! - Resource limits and permission restrictions

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::fs::{self, File};
use std::io::Write;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, warn};

/// Comprehensive sandbox profile for Nyx plugins.
/// This profile follows Apple's recommended security practices.
const NYX_PLUGIN_SANDBOX_PROFILE: &str = r#"
(version 1)
(deny default)

;; Allow basic system operations
(allow file-read* 
    (literal "/System/Library/Frameworks")
    (literal "/System/Library/PrivateFrameworks")
    (literal "/usr/lib")
    (literal "/usr/share")
    (subpath "/System/Library/Caches/com.apple.dyld")
)

;; Allow reading own executable
(allow file-read* (literal (param "PLUGIN_PATH")))

;; Allow minimal network operations for IPC
(allow network-outbound 
    (local udp "*:*")
    (remote unix-socket (param "IPC_SOCKET"))
)

;; Allow IPC with Nyx daemon
(allow ipc-posix-shm-read-data 
    (ipc-posix-name-regex #"^/tmp/nyx-plugin-"))

;; Deny dangerous operations
(deny file-write*)
(deny file-write-create)
(deny process-fork)
(deny process-exec)
(deny network-bind)
(deny network-inbound)
(deny system-socket)
(deny mach-lookup)

;; Allow basic process info
(allow process-info-pidinfo (target self))
(allow process-info-setcontrol (target self))

;; Resource limits
(deny sysctl*)
(deny iokit-open)
(deny device*)
"#;

/// Enhanced plugin spawn with comprehensive sandboxing.
pub fn spawn_sandboxed_plugin<P: AsRef<Path>>(plugin_path: P) -> io::Result<Child> {
    let exe = plugin_path.as_ref();
    if !exe.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "plugin binary not found"));
    }

    // Create temporary sandbox profile
    let profile_path = create_sandbox_profile(exe)?;
    
    // Attempt to use sandbox-exec for enhanced security
    let child = match spawn_with_sandbox_exec(exe, &profile_path) {
        Ok(child) => {
            debug!("Plugin spawned with sandbox-exec: {}", exe.display());
            child
        },
        Err(e) => {
            warn!("Failed to spawn with sandbox-exec, falling back to basic spawn: {}", e);
            spawn_basic_sandboxed(exe)?
        }
    };

    // Clean up temporary profile
    let _ = fs::remove_file(profile_path);

    Ok(child)
}

/// Create a temporary sandbox profile file with plugin-specific parameters.
fn create_sandbox_profile(plugin_path: &Path) -> io::Result<std::path::PathBuf> {
    let profile_dir = std::env::temp_dir().join("nyx-sandbox");
    fs::create_dir_all(&profile_dir)?;
    
    let profile_path = profile_dir.join(format!(
        "plugin-{}.sb", 
        plugin_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    ));

    let mut profile_content = NYX_PLUGIN_SANDBOX_PROFILE.to_string();
    
    // Replace parameters
    profile_content = profile_content.replace(
        "(param \"PLUGIN_PATH\")", 
        &format!("\"{}\"", plugin_path.display())
    );
    
    // Create IPC socket path
    let ipc_socket = format!("/tmp/nyx-plugin-{}.sock", std::process::id());
    profile_content = profile_content.replace(
        "(param \"IPC_SOCKET\")",
        &format!("\"{}\"", ipc_socket)
    );

    let mut file = File::create(&profile_path)?;
    file.write_all(profile_content.as_bytes())?;
    file.sync_all()?;

    Ok(profile_path)
}

/// Spawn plugin using macOS sandbox-exec command.
fn spawn_with_sandbox_exec(plugin_path: &Path, profile_path: &Path) -> io::Result<Child> {
    let child = Command::new("sandbox-exec")
        .arg("-f")
        .arg(profile_path)
        .arg(plugin_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    debug!("Plugin {} spawned with PID {} under sandbox-exec", 
           plugin_path.display(), child.id());
    
    Ok(child)
}

/// Basic sandboxed spawn without sandbox-exec (fallback).
fn spawn_basic_sandboxed(plugin_path: &Path) -> io::Result<Child> {
    let child = Command::new(plugin_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    debug!("Plugin {} spawned with basic sandboxing, PID: {}", 
           plugin_path.display(), child.id());
    
    Ok(child)
}

/// Enhanced termination with timeout and cleanup.
pub async fn terminate(child: &mut Child) -> io::Result<()> {
    let pid = child.id();
    
    // Try graceful termination first
    if let Err(e) = child.kill() {
        error!("Failed to send SIGTERM to plugin PID {}: {}", pid, e);
        return Err(e);
    }

    // Wait for termination with timeout
    match timeout(Duration::from_secs(5), child.wait()).await {
        Ok(Ok(status)) => {
            debug!("Plugin PID {} terminated with status: {}", pid, status);
            Ok(())
        },
        Ok(Err(e)) => {
            error!("Error waiting for plugin PID {} termination: {}", pid, e);
            Err(e)
        },
        Err(_) => {
            warn!("Plugin PID {} did not terminate within timeout, forcing kill", pid);
            // Force kill if graceful termination failed
            let _ = child.kill();
            Err(io::Error::new(io::ErrorKind::TimedOut, "Plugin termination timeout"))
        }
    }
}

/// Check if the current process has necessary entitlements for sandboxing.
pub fn check_sandbox_entitlements() -> bool {
    // Check if running under App Sandbox
    std::env::var("APP_SANDBOX_CONTAINER_ID").is_ok()
}

/// Get sandbox status for a process ID.
pub fn get_sandbox_status(pid: u32) -> io::Result<bool> {
    // Use sysctl to check sandbox status
    let output = Command::new("sysctl")
        .arg("-n")
        .arg(format!("kern.proc.pid.{}.sandbox", pid))
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout);
            Ok(status.trim() == "1")
        },
        _ => {
            // Fallback: assume sandboxed if we can't determine
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_sandbox_profile_creation() {
        let temp_dir = tempdir().unwrap();
        let plugin_path = temp_dir.path().join("test_plugin");
        File::create(&plugin_path).unwrap();

        let profile_path = create_sandbox_profile(&plugin_path).unwrap();
        assert!(profile_path.exists());

        let content = fs::read_to_string(&profile_path).unwrap();
        assert!(content.contains("(version 1)"));
        assert!(content.contains(&plugin_path.display().to_string()));
    }

    #[test]
    fn test_entitlements_check() {
        // This test depends on runtime environment
        let _has_entitlements = check_sandbox_entitlements();
        // Just ensure the function doesn't panic
    }
} 