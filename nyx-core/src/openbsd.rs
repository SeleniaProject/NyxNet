#![cfg(target_os = "openbsd")]
//! Thin wrappers around OpenBSD `pledge(2)` / `unveil(2)` syscalls.
#![forbid(unsafe_code)]

use std::ffi::CString;
use anyhow::{Result, anyhow};

extern "C" {
    fn pledge(promises: *const libc::c_char, execpromises: *const libc::c_char) -> libc::c_int;
    fn unveil(path: *const libc::c_char, permissions: *const libc::c_char) -> libc::c_int;
}

/// Call `pledge` limiting the process to required promises.
///
/// Example: `install_pledge("stdio inet")`.
pub fn install_pledge(promises: &str) -> Result<()> {
    let c_prom = CString::new(promises)?;
    let ret = unsafe { pledge(c_prom.as_ptr(), std::ptr::null()) };
    if ret != 0 { return Err(anyhow!("pledge failed: {}", std::io::Error::last_os_error())); }
    Ok(())
}

/// Call `unveil` to restrict filesystem view.
/// Pass `None` for `perm` to lock (`unveil(NULL, NULL)`).
pub fn unveil_path(path: Option<&str>, perm: Option<&str>) -> Result<()> {
    let c_path = match path { Some(p) => Some(CString::new(p)?), None => None };
    let c_perm = match perm { Some(p) => Some(CString::new(p)?), None => None };
    let ret = unsafe { unveil(c_path.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                               c_perm.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())) };
    if ret != 0 { return Err(anyhow!("unveil failed: {}", std::io::Error::last_os_error())); }
    Ok(())
} 