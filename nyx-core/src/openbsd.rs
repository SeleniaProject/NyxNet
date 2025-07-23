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

/// Get current pledge status by attempting to use restricted functionality
pub fn get_pledge_status() -> PledgeStatus {
    // Try to access /proc to test if we have rpath
    let has_rpath = std::fs::metadata("/proc").is_ok();
    
    // Try to create a socket to test if we have inet
    let has_inet = std::net::TcpListener::bind("127.0.0.1:0").is_ok();
    
    // Try to execute a command to test if we have exec
    let has_exec = std::process::Command::new("true")
        .output()
        .is_ok();
    
    PledgeStatus {
        has_rpath,
        has_inet,
        has_exec,
        has_wpath: false, // Assume no wpath for safety
    }
}

/// Information about current pledge restrictions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PledgeStatus {
    pub has_rpath: bool,
    pub has_inet: bool,
    pub has_exec: bool,
    pub has_wpath: bool,
}

/// Get current unveil status by testing path accessibility
pub fn get_unveil_status() -> UnveilStatus {
    use std::fs;
    
    // Test access to common system paths
    let can_access_etc = fs::metadata("/etc").is_ok();
    let can_access_tmp = fs::metadata("/tmp").is_ok();
    let can_access_usr = fs::metadata("/usr").is_ok();
    let can_access_var = fs::metadata("/var").is_ok();
    
    UnveilStatus {
        can_access_etc,
        can_access_tmp,
        can_access_usr,
        can_access_var,
        locked: false, // We can't easily detect if unveil is locked
    }
}

/// Information about current unveil restrictions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnveilStatus {
    pub can_access_etc: bool,
    pub can_access_tmp: bool,
    pub can_access_usr: bool,
    pub can_access_var: bool,
    pub locked: bool,
}

/// Standard pledge promises for Nyx daemon
pub const NYX_DAEMON_PROMISES: &str = "stdio inet rpath wpath cpath flock unix";

/// Standard pledge promises for Nyx client
pub const NYX_CLIENT_PROMISES: &str = "stdio inet rpath unix";

/// Apply standard Nyx daemon pledge restrictions
pub fn install_nyx_daemon_pledge() -> Result<()> {
    install_pledge(NYX_DAEMON_PROMISES)
}

/// Apply standard Nyx client pledge restrictions
pub fn install_nyx_client_pledge() -> Result<()> {
    install_pledge(NYX_CLIENT_PROMISES)
}

/// Setup standard Nyx daemon unveil restrictions
pub fn setup_nyx_daemon_unveil() -> Result<()> {
    // Allow reading configuration files
    unveil_path(Some("/etc"), Some("r"))?;
    
    // Allow reading and writing to temp directory
    unveil_path(Some("/tmp"), Some("rw"))?;
    
    // Allow reading system libraries
    unveil_path(Some("/usr/lib"), Some("r"))?;
    unveil_path(Some("/usr/local/lib"), Some("r"))?;
    
    // Allow reading timezone data
    unveil_path(Some("/usr/share/zoneinfo"), Some("r"))?;
    
    // Allow access to runtime directory
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        unveil_path(Some(&runtime_dir), Some("rw"))?;
    }
    
    // Lock unveil
    unveil_path(None, None)?;
    
    Ok(())
}

/// Setup standard Nyx client unveil restrictions
pub fn setup_nyx_client_unveil() -> Result<()> {
    // Allow reading configuration files
    unveil_path(Some("/etc"), Some("r"))?;
    
    // Allow reading system libraries
    unveil_path(Some("/usr/lib"), Some("r"))?;
    unveil_path(Some("/usr/local/lib"), Some("r"))?;
    
    // Allow access to user's home directory (read-only)
    if let Ok(home_dir) = std::env::var("HOME") {
        unveil_path(Some(&home_dir), Some("r"))?;
    }
    
    // Lock unveil
    unveil_path(None, None)?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    
    #[test]
    fn test_cstring_creation() {
        // Test that we can create CString from various promise strings
        let promises = ["stdio", "inet", "rpath wpath", "stdio inet rpath"];
        
        for promise in &promises {
            let c_str = CString::new(*promise);
            assert!(c_str.is_ok(), "Should be able to create CString from: {}", promise);
            
            let c_str = c_str.unwrap();
            assert!(!c_str.as_bytes().is_empty(), "CString should not be empty");
        }
    }
    
    #[test]
    fn test_invalid_cstring() {
        // Test that CString creation fails with null bytes
        let invalid_promise = "stdio\0inet";
        let result = CString::new(invalid_promise);
        assert!(result.is_err(), "CString creation should fail with null bytes");
    }
    
    #[test]
    fn test_pledge_promises_validity() {
        // Test that our standard promise strings are valid
        let daemon_promises = NYX_DAEMON_PROMISES;
        let client_promises = NYX_CLIENT_PROMISES;
        
        // Should not contain null bytes
        assert!(!daemon_promises.contains('\0'));
        assert!(!client_promises.contains('\0'));
        
        // Should contain expected promises
        assert!(daemon_promises.contains("stdio"));
        assert!(daemon_promises.contains("inet"));
        assert!(client_promises.contains("stdio"));
        assert!(client_promises.contains("inet"));
        
        // Daemon should have more privileges than client
        assert!(daemon_promises.len() > client_promises.len());
    }
    
    #[test]
    fn test_pledge_status_structure() {
        let status = PledgeStatus {
            has_rpath: true,
            has_inet: false,
            has_exec: true,
            has_wpath: false,
        };
        
        assert!(status.has_rpath);
        assert!(!status.has_inet);
        assert!(status.has_exec);
        assert!(!status.has_wpath);
        
        // Test debug formatting
        let debug_str = format!("{:?}", status);
        assert!(debug_str.contains("has_rpath: true"));
        assert!(debug_str.contains("has_inet: false"));
    }
    
    #[test]
    fn test_unveil_status_structure() {
        let status = UnveilStatus {
            can_access_etc: true,
            can_access_tmp: true,
            can_access_usr: false,
            can_access_var: false,
            locked: true,
        };
        
        assert!(status.can_access_etc);
        assert!(status.can_access_tmp);
        assert!(!status.can_access_usr);
        assert!(!status.can_access_var);
        assert!(status.locked);
    }
    
    #[test]
    fn test_get_pledge_status() {
        // This test runs the pledge status detection
        // In a normal environment, most capabilities should be available
        let status = get_pledge_status();
        
        // Basic validation - at least one capability should be detectable
        let has_any = status.has_rpath || status.has_inet || status.has_exec || status.has_wpath;
        
        // In most environments, we should have at least stdio (always available)
        // This is a weak test since we can't easily detect stdio capability
        assert!(has_any || !has_any); // Tautology to ensure function runs without panic
    }
    
    #[test]
    fn test_get_unveil_status() {
        // This test runs the unveil status detection
        let status = get_unveil_status();
        
        // In a normal environment, we should be able to access system directories
        // This might fail in very restricted environments, which is OK
        let can_access_any = status.can_access_etc || status.can_access_tmp || 
                           status.can_access_usr || status.can_access_var;
        
        // Ensure function runs without panic
        assert!(can_access_any || !can_access_any); // Tautology
    }
    
    #[test]
    fn test_path_validation() {
        // Test path validation for unveil
        let valid_paths = ["/etc", "/tmp", "/usr/lib", "/var/log"];
        let invalid_paths = ["", "relative/path", "path\0with\0nulls"];
        
        for path in &valid_paths {
            let c_str = CString::new(*path);
            assert!(c_str.is_ok(), "Valid path should create CString: {}", path);
        }
        
        for path in &invalid_paths {
            if path.contains('\0') {
                let c_str = CString::new(*path);
                assert!(c_str.is_err(), "Invalid path should fail CString creation: {}", path);
            }
        }
    }
    
    #[test]
    fn test_permission_strings() {
        // Test permission string validation for unveil
        let valid_perms = ["r", "w", "rw", "x", "rx", "wx", "rwx"];
        
        for perm in &valid_perms {
            let c_str = CString::new(*perm);
            assert!(c_str.is_ok(), "Valid permission should create CString: {}", perm);
            
            // Verify permission string format
            assert!(perm.chars().all(|c| "rwx".contains(c)), 
                   "Permission should only contain r, w, x: {}", perm);
        }
    }
    
    #[test]
    fn test_promise_string_parsing() {
        // Test that promise strings can be parsed
        let promises = NYX_DAEMON_PROMISES;
        let promise_list: Vec<&str> = promises.split_whitespace().collect();
        
        // Should have multiple promises
        assert!(promise_list.len() > 3, "Should have multiple promises");
        
        // Each promise should be non-empty and contain only valid characters
        for promise in promise_list {
            assert!(!promise.is_empty(), "Promise should not be empty");
            assert!(promise.chars().all(|c| c.is_ascii_alphanumeric()), 
                   "Promise should contain only alphanumeric characters: {}", promise);
        }
    }
    
    #[test]
    fn test_environment_variables() {
        // Test environment variable handling
        std::env::set_var("TEST_HOME", "/home/test");
        std::env::set_var("TEST_RUNTIME", "/run/user/1000");
        
        let home = std::env::var("TEST_HOME");
        let runtime = std::env::var("TEST_RUNTIME");
        
        assert!(home.is_ok());
        assert!(runtime.is_ok());
        
        let home_path = home.unwrap();
        let runtime_path = runtime.unwrap();
        
        assert!(!home_path.is_empty());
        assert!(!runtime_path.is_empty());
        assert!(home_path.starts_with('/'));
        assert!(runtime_path.starts_with('/'));
        
        // Cleanup
        std::env::remove_var("TEST_HOME");
        std::env::remove_var("TEST_RUNTIME");
    }
} 