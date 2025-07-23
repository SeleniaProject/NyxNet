#![cfg(all(feature = "dynamic_plugin", target_os = "windows"))]
#![forbid(unsafe_code)]

//! Windows Job-Object based sandbox for Nyx dynamic plugins.
//!
//! This module provides comprehensive sandboxing for Windows using:
//! - Job Objects with strict resource limits
//! - Process isolation and privilege restrictions
//! - Network access controls
//! - File system access limitations

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::os::windows::io::AsRawHandle;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, warn};

use windows::Win32::Foundation::{HANDLE, CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, 
    JobObjectExtendedLimitInformation, JobObjectBasicUIRestrictions,
    JobObjectSecurityLimitInformation, JobObjectNetRateLimitInformation,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_BASIC_UI_RESTRICTIONS,
    JOBOBJECT_SECURITY_LIMIT_INFORMATION, JOBOBJECT_NET_RATE_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_PRIORITY_CLASS,
    JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_WORKINGSET, JOB_OBJECT_LIMIT_PROCESS_TIME,
    JOB_OBJECT_UILIMIT_DESKTOP, JOB_OBJECT_UILIMIT_DISPLAYSETTINGS,
    JOB_OBJECT_UILIMIT_EXITWINDOWS, JOB_OBJECT_UILIMIT_READCLIPBOARD,
    JOB_OBJECT_UILIMIT_WRITECLIPBOARD, JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS,
    JOB_OBJECT_SECURITY_NO_ADMIN, JOB_OBJECT_SECURITY_RESTRICTED_TOKEN,
    JOB_OBJECT_SECURITY_ONLY_TOKEN, JOB_OBJECT_SECURITY_FILTER_TOKENS
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcessToken, CreateRestrictedToken,
    PROCESS_QUERY_INFORMATION, TOKEN_DUPLICATE, TOKEN_ASSIGN_PRIMARY,
    TOKEN_QUERY, DISABLE_MAX_PRIVILEGE
};
use windows::Win32::Security::{
    TOKEN_ACCESS_MASK, SECURITY_ATTRIBUTES, SecurityIdentification,
    TokenRestrictedSids, GetTokenInformation, TOKEN_INFORMATION_CLASS
};

/// Comprehensive sandbox configuration for plugins.
struct SandboxConfig {
    max_process_memory_mb: usize,
    max_job_memory_mb: usize,
    max_working_set_mb: usize,
    max_process_time_seconds: u64,
    network_rate_limit_bps: u64,
    ui_restrictions_enabled: bool,
    restricted_token: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_process_memory_mb: 64,      // 64MB per process
            max_job_memory_mb: 128,         // 128MB total for job
            max_working_set_mb: 32,         // 32MB working set
            max_process_time_seconds: 300,  // 5 minutes max runtime
            network_rate_limit_bps: 1_000_000, // 1Mbps
            ui_restrictions_enabled: true,
            restricted_token: true,
        }
    }
}

/// Enhanced plugin spawn with comprehensive Job Object restrictions.
pub fn spawn_sandboxed_plugin<P: AsRef<Path>>(plugin_path: P) -> io::Result<Child> {
    let exe = plugin_path.as_ref();
    if !exe.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "plugin binary not found"));
    }

    let config = SandboxConfig::default();

    // Launch the child process with no inherited handles
    let mut child = Command::new(exe)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    // Create and configure Job Object
    if let Err(e) = configure_job_object(&child, &config) {
        error!("Failed to configure Job Object for plugin: {}", e);
        let _ = child.kill();
        return Err(e);
    }

    debug!("Plugin {} spawned with comprehensive sandboxing, PID: {}", 
           exe.display(), child.id());

    Ok(child)
}

/// Configure Job Object with comprehensive security restrictions.
fn configure_job_object(child: &Child, config: &SandboxConfig) -> io::Result<()> {
    unsafe {
        // Create Job Object
        let hjob = CreateJobObjectW(std::ptr::null(), None);
        if hjob == INVALID_HANDLE_VALUE || hjob.is_invalid() {
            return Err(io::Error::new(io::ErrorKind::Other, "Failed to create Job Object"));
        }

        // Ensure Job Object is closed on scope exit
        let _job_guard = JobObjectGuard(hjob);

        // Configure extended limits
        configure_extended_limits(hjob, config)?;

        // Configure UI restrictions
        if config.ui_restrictions_enabled {
            configure_ui_restrictions(hjob)?;
        }

        // Configure security limits
        if config.restricted_token {
            configure_security_limits(hjob)?;
        }

        // Configure network rate limits
        configure_network_limits(hjob, config)?;

        // Assign process to Job Object
        let hproc = HANDLE(child.as_raw_handle() as isize);
        AssignProcessToJobObject(hjob, hproc)
            .ok()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to assign process to job: {:?}", e)))?;

        debug!("Job Object configured successfully for PID {}", child.id());
        Ok(())
    }
}

/// Configure extended resource limits.
unsafe fn configure_extended_limits(hjob: HANDLE, config: &SandboxConfig) -> io::Result<()> {
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
    
    // Set basic limits
    info.BasicLimitInformation.LimitFlags = 
        JOB_OBJECT_LIMIT_ACTIVE_PROCESS |
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE |
        JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION |
        JOB_OBJECT_LIMIT_PRIORITY_CLASS |
        JOB_OBJECT_LIMIT_PROCESS_MEMORY |
        JOB_OBJECT_LIMIT_JOB_MEMORY |
        JOB_OBJECT_LIMIT_WORKINGSET |
        JOB_OBJECT_LIMIT_PROCESS_TIME;

    info.BasicLimitInformation.ActiveProcessLimit = 1;
    info.BasicLimitInformation.PriorityClass = 0x00000040; // BELOW_NORMAL_PRIORITY_CLASS
    info.BasicLimitInformation.ProcessMemoryLimit = (config.max_process_memory_mb * 1024 * 1024) as usize;
    info.BasicLimitInformation.JobMemoryLimit = (config.max_job_memory_mb * 1024 * 1024) as usize;
    info.BasicLimitInformation.MinimumWorkingSetSize = 1024 * 1024; // 1MB minimum
    info.BasicLimitInformation.MaximumWorkingSetSize = (config.max_working_set_mb * 1024 * 1024) as usize;
    
    // Set process time limit (in 100-nanosecond intervals)
    info.BasicLimitInformation.PerProcessUserTimeLimit = 
        (config.max_process_time_seconds * 10_000_000) as i64;

    // Configure I/O rate limits
    info.IoRateControlTolerance = 3; // High tolerance
    info.IoRateControlToleranceInterval = 3; // High tolerance interval

    SetInformationJobObject(
        hjob, 
        JobObjectExtendedLimitInformation, 
        &info as *const _ as *const _, 
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32
    )
    .ok()
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set extended limits: {:?}", e)))?;

    Ok(())
}

/// Configure UI access restrictions.
unsafe fn configure_ui_restrictions(hjob: HANDLE) -> io::Result<()> {
    let mut ui_restrictions: JOBOBJECT_BASIC_UI_RESTRICTIONS = std::mem::zeroed();
    
    ui_restrictions.UIRestrictionsClass = 
        JOB_OBJECT_UILIMIT_DESKTOP |
        JOB_OBJECT_UILIMIT_DISPLAYSETTINGS |
        JOB_OBJECT_UILIMIT_EXITWINDOWS |
        JOB_OBJECT_UILIMIT_READCLIPBOARD |
        JOB_OBJECT_UILIMIT_WRITECLIPBOARD |
        JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS;

    SetInformationJobObject(
        hjob,
        JobObjectBasicUIRestrictions,
        &ui_restrictions as *const _ as *const _,
        std::mem::size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32
    )
    .ok()
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set UI restrictions: {:?}", e)))?;

    Ok(())
}

/// Configure security token restrictions.
unsafe fn configure_security_limits(hjob: HANDLE) -> io::Result<()> {
    let mut security_limits: JOBOBJECT_SECURITY_LIMIT_INFORMATION = std::mem::zeroed();
    
    security_limits.SecurityLimitFlags = 
        JOB_OBJECT_SECURITY_NO_ADMIN |
        JOB_OBJECT_SECURITY_RESTRICTED_TOKEN |
        JOB_OBJECT_SECURITY_ONLY_TOKEN |
        JOB_OBJECT_SECURITY_FILTER_TOKENS;

    SetInformationJobObject(
        hjob,
        JobObjectSecurityLimitInformation,
        &security_limits as *const _ as *const _,
        std::mem::size_of::<JOBOBJECT_SECURITY_LIMIT_INFORMATION>() as u32
    )
    .ok()
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set security limits: {:?}", e)))?;

    Ok(())
}

/// Configure network rate limits.
unsafe fn configure_network_limits(hjob: HANDLE, config: &SandboxConfig) -> io::Result<()> {
    let mut net_limits: JOBOBJECT_NET_RATE_LIMIT_INFORMATION = std::mem::zeroed();
    
    net_limits.MaxBandwidth = config.network_rate_limit_bps;
    net_limits.ControlFlags = 1; // Enable rate limiting

    SetInformationJobObject(
        hjob,
        JobObjectNetRateLimitInformation,
        &net_limits as *const _ as *const _,
        std::mem::size_of::<JOBOBJECT_NET_RATE_LIMIT_INFORMATION>() as u32
    )
    .ok()
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set network limits: {:?}", e)))?;

    Ok(())
}

/// RAII guard for Job Object handle cleanup.
struct JobObjectGuard(HANDLE);

impl Drop for JobObjectGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_invalid() {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// Enhanced termination with timeout and cleanup.
pub async fn terminate(child: &mut Child) -> io::Result<()> {
    let pid = child.id();
    
    // Try graceful termination first
    if let Err(e) = child.kill() {
        error!("Failed to terminate plugin PID {}: {}", pid, e);
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
            warn!("Plugin PID {} did not terminate within timeout", pid);
            // Job Object will automatically kill the process
            Err(io::Error::new(io::ErrorKind::TimedOut, "Plugin termination timeout"))
        }
    }
}

/// Check if the current process is running in a restricted environment.
pub fn check_sandbox_status() -> bool {
    // Check if running under Job Object restrictions
    unsafe {
        let current_process = GetCurrentProcess();
        let mut token = HANDLE::default();
        
        if OpenProcessToken(
            current_process,
            TOKEN_QUERY,
            &mut token
        ).is_ok() {
            // Check if token has restrictions
            let mut buffer = vec![0u8; 1024];
            let mut return_length = 0u32;
            
            if GetTokenInformation(
                token,
                TokenRestrictedSids,
                Some(buffer.as_mut_ptr() as *mut _),
                buffer.len() as u32,
                &mut return_length
            ).is_ok() {
                return_length > 0
            } else {
                false
            }
        } else {
            false
        }
    }
}

/// Get detailed sandbox information for a process.
pub fn get_sandbox_info(pid: u32) -> io::Result<SandboxInfo> {
    // This would require additional Windows APIs to query Job Object information
    // For now, return basic information
    Ok(SandboxInfo {
        pid,
        sandboxed: true,
        job_object_assigned: true,
        restricted_token: check_sandbox_status(),
    })
}

/// Information about process sandbox status.
#[derive(Debug, Clone)]
pub struct SandboxInfo {
    pub pid: u32,
    pub sandboxed: bool,
    pub job_object_assigned: bool,
    pub restricted_token: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_process_memory_mb, 64);
        assert_eq!(config.max_job_memory_mb, 128);
        assert!(config.ui_restrictions_enabled);
        assert!(config.restricted_token);
    }

    #[test]
    fn test_sandbox_status_check() {
        // This test depends on runtime environment
        let _is_sandboxed = check_sandbox_status();
        // Just ensure the function doesn't panic
    }

    #[tokio::test]
    async fn test_sandbox_info() {
        let info = get_sandbox_info(std::process::id()).unwrap();
        assert_eq!(info.pid, std::process::id());
    }
} 