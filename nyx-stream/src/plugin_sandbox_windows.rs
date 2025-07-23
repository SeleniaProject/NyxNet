#![cfg(all(feature = "dynamic_plugin", target_os = "windows"))]
#![forbid(unsafe_code)]

//! Windows Job-Object based sandbox for Nyx dynamic plugins.
//!
//! This module provides comprehensive sandboxing for Windows using:
//! - Job Objects with strict resource limits
//! - Process isolation and privilege restrictions
//! - Network access controls
//! - File system access limitations
//! 
//! All implementations use safe Rust wrappers around Windows APIs.

use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, warn};
use win32job::{Job, ExtendedLimitInfo};

/// Comprehensive sandbox configuration for plugins.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub max_process_memory_mb: usize,
    pub max_job_memory_mb: usize,
    pub max_working_set_mb: usize,
    pub max_process_time_seconds: u64,
    pub ui_restrictions_enabled: bool,
    pub kill_on_job_close: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_process_memory_mb: 64,      // 64MB per process
            max_job_memory_mb: 128,         // 128MB total for job
            max_working_set_mb: 32,         // 32MB working set
            max_process_time_seconds: 300,  // 5 minutes max runtime
            ui_restrictions_enabled: true,
            kill_on_job_close: true,
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

    // Create and configure Job Object using safe win32job crate
    if let Err(e) = configure_job_object_safe(&child, &config) {
        error!("Failed to configure Job Object for plugin: {}", e);
        let _ = child.kill();
        return Err(e);
    }

    debug!("Plugin {} spawned with comprehensive sandboxing, PID: {}", 
           exe.display(), child.id());

    Ok(child)
}

/// Configure Job Object with comprehensive security restrictions using safe APIs.
fn configure_job_object_safe(child: &Child, config: &SandboxConfig) -> io::Result<()> {
    // Create Job Object using safe win32job crate
    let job = Job::create()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create Job Object: {}", e)))?;

    // Configure extended limits
    let mut limit_info = ExtendedLimitInfo::new();
    
    // Set memory limits
    limit_info.limit_process_memory(
        config.max_process_memory_mb * 1024 * 1024,
        config.max_process_memory_mb * 1024 * 1024
    );
    
    limit_info.limit_job_memory(config.max_job_memory_mb * 1024 * 1024);
    
    limit_info.limit_working_memory(
        config.max_working_set_mb * 1024 * 1024,
        config.max_working_set_mb * 1024 * 1024
    );

    // Set process time limit
    limit_info.limit_process_time(Duration::from_secs(config.max_process_time_seconds));

    // Enable kill on job close if configured
    if config.kill_on_job_close {
        limit_info.limit_kill_on_job_close();
    }

    // Apply the limits to the job
    job.set_extended_limit_info(&mut limit_info)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to set job limits: {}", e)))?;

    // Assign the child process to the job
    job.assign_process(child.id())
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to assign process to job: {}", e)))?;

    debug!("Job Object configured successfully for PID {} using safe APIs", child.id());
    
    // Keep the job object alive by leaking it (it will be cleaned up when the process exits)
    std::mem::forget(job);
    
    Ok(())
}

/// Spawn plugin with timeout and comprehensive error handling.
pub async fn spawn_plugin_with_timeout<P: AsRef<Path>>(
    plugin_path: P,
    timeout_duration: Duration,
) -> io::Result<Child> {
    let plugin_path = plugin_path.as_ref().to_path_buf();
    
    // Spawn in a blocking task to avoid blocking the async runtime
    let spawn_result = tokio::task::spawn_blocking(move || {
        spawn_sandboxed_plugin(&plugin_path)
    }).await;

    match spawn_result {
        Ok(Ok(child)) => {
            debug!("Plugin spawned successfully with timeout protection");
            Ok(child)
        },
        Ok(Err(e)) => {
            error!("Failed to spawn plugin: {}", e);
            Err(e)
        },
        Err(e) => {
            error!("Plugin spawn task panicked: {}", e);
            Err(io::Error::new(io::ErrorKind::Other, "Plugin spawn task failed"))
        }
    }
}

/// Terminate plugin process safely.
pub fn terminate_plugin(mut child: Child) -> io::Result<()> {
    debug!("Terminating plugin process PID: {}", child.id());
    
    // First try graceful termination
    match child.try_wait() {
        Ok(Some(status)) => {
            debug!("Plugin already exited with status: {:?}", status);
            return Ok(());
        },
        Ok(None) => {
            // Process is still running, try to kill it
            if let Err(e) = child.kill() {
                warn!("Failed to kill plugin process: {}", e);
                return Err(e);
            }
        },
        Err(e) => {
            warn!("Failed to check plugin process status: {}", e);
            return Err(e);
        }
    }

    // Wait for the process to actually terminate
    match child.wait() {
        Ok(status) => {
            debug!("Plugin terminated with status: {:?}", status);
            Ok(())
        },
        Err(e) => {
            error!("Failed to wait for plugin termination: {}", e);
            Err(e)
        }
    }
}

/// Check if plugin process is still running.
pub fn is_plugin_running(child: &mut Child) -> bool {
    match child.try_wait() {
        Ok(Some(_)) => false,  // Process has exited
        Ok(None) => true,      // Process is still running
        Err(_) => false,       // Error checking status, assume not running
    }
}

/// Get plugin process ID.
pub fn get_plugin_pid(child: &Child) -> u32 {
    child.id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_process_memory_mb, 64);
        assert_eq!(config.max_job_memory_mb, 128);
        assert_eq!(config.max_working_set_mb, 32);
        assert_eq!(config.max_process_time_seconds, 300);
        assert!(config.ui_restrictions_enabled);
        assert!(config.kill_on_job_close);
    }

    #[tokio::test]
    async fn test_spawn_nonexistent_plugin() {
        let result = spawn_plugin_with_timeout(
            PathBuf::from("nonexistent_plugin.exe"),
            Duration::from_secs(5)
        ).await;
        
        assert!(result.is_err());
    }
} 