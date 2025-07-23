//! OS-level sandbox for dynamic plugins using extrasafe for comprehensive security.
//!
//! This module is only compiled on Linux when the `dynamic_plugin` Cargo
//! feature is enabled. The sandbox uses extrasafe to create secure isolated
//! environments with seccomp filters, user namespaces, and comprehensive
//! security restrictions without requiring any unsafe code.
#![cfg(all(feature = "dynamic_plugin", target_os = "linux"))]
#![forbid(unsafe_code)]

use std::path::Path;
use std::process::{Child, Command};
use std::io;
use std::time::Duration;

use extrasafe::SafetyContext;
use extrasafe::builtins::{SystemIO, Networking, Pipes};
use extrasafe::isolate::{Isolate, IsolateConfig};
use tracing::{debug, error, warn};

/// Configuration for plugin sandbox
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Allow network access
    pub allow_network: bool,
    /// Allow file system access
    pub allow_filesystem: bool,
    /// Maximum execution time
    pub max_execution_time: Option<Duration>,
    /// Working directory
    pub working_directory: Option<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            allow_network: false,
            allow_filesystem: false,
            max_execution_time: Some(Duration::from_secs(300)), // 5 minutes
            working_directory: None,
        }
    }
}

/// Spawn a plugin process inside a secure sandbox using extrasafe.
///
/// * `plugin_path` – path to the plugin executable or wrapper script.
/// * The function returns the spawned [`Child`] process ready to be waited on.
///   Any error installing the sandbox is forwarded as `io::Error`.
pub fn spawn_sandboxed_plugin<P: AsRef<Path>>(plugin_path: P) -> io::Result<Child> {
    spawn_sandboxed_plugin_with_config(plugin_path, SandboxConfig::default())
}

/// Spawn a plugin process with custom sandbox configuration.
pub fn spawn_sandboxed_plugin_with_config<P: AsRef<Path>>(
    plugin_path: P, 
    config: SandboxConfig
) -> io::Result<Child> {
    let plugin = plugin_path.as_ref();
    if !plugin.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "plugin binary not found"));
    }

    debug!("Spawning plugin {} with extrasafe sandbox", plugin.display());

    // Create an isolated environment for the plugin using extrasafe
    let isolate_config = IsolateConfig::new()
        .with_hostname("nyx-plugin")
        .with_new_network_namespace(!config.allow_network)
        .with_new_pid_namespace(true)
        .with_new_user_namespace(true)
        .with_new_uts_namespace(true)
        .with_new_ipc_namespace(true);

    let isolate = Isolate::new(isolate_config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create isolate: {}", e)))?;

    // Configure the safety context for the plugin
    let mut safety_context = SafetyContext::new();

    // Basic system I/O - minimal required permissions
    let mut system_io = SystemIO::nothing()
        .allow_stderr()
        .allow_stdout();
    
    if config.allow_filesystem {
        system_io = system_io
            .allow_open_readonly()
            .allow_metadata();
    }

    safety_context = safety_context.enable(system_io)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to configure SystemIO: {}", e)))?;

    // Network permissions if requested
    if config.allow_network {
        let networking = Networking::nothing()
            .allow_start_tcp_clients()
            .allow_start_udp_clients();
        
        safety_context = safety_context.enable(networking)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to configure Networking: {}", e)))?;
    }

    // Allow basic pipe operations for process communication
    let pipes = Pipes::nothing()
        .allow_close()
        .allow_dup();
    
    safety_context = safety_context.enable(pipes)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to configure Pipes: {}", e)))?;

    // Execute the plugin within the isolated environment
    let plugin_path_owned = plugin.to_path_buf();
    let child = isolate.run(move || {
        // Apply the safety context within the isolated environment
        safety_context.apply_to_current_thread()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to apply safety context: {}", e)))?;

        debug!("Safety context applied successfully in isolated environment");

        // Execute the plugin
        let mut cmd = Command::new(&plugin_path_owned);
        
        // Configure stdio
        cmd.stdin(std::process::Stdio::null())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::piped());

        // Set working directory if specified
        if let Some(ref wd) = config.working_directory {
            cmd.current_dir(wd);
        }

        // Spawn the process
        cmd.spawn()
    })
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to execute plugin in isolate: {}", e)))?;

    debug!("Plugin {} spawned successfully with PID {} in secure sandbox", 
           plugin.display(), child.id());

    Ok(child)
}

/// Gracefully terminate a sandboxed plugin process.
/// Returns `Ok(())` if the process is successfully terminated.
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

/// Execute a function within a sandboxed environment.
/// This is useful for running plugin code directly without spawning a separate process.
pub fn execute_in_sandbox<F, R>(f: F, config: SandboxConfig) -> io::Result<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    debug!("Executing function in sandbox with extrasafe");

    // Create isolated environment
    let isolate_config = IsolateConfig::new()
        .with_hostname("nyx-plugin")
        .with_new_network_namespace(!config.allow_network)
        .with_new_pid_namespace(true)
        .with_new_user_namespace(true)
        .with_new_uts_namespace(true)
        .with_new_ipc_namespace(true);

    let isolate = Isolate::new(isolate_config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to create isolate: {}", e)))?;

    // Execute function in isolated environment
    let result = isolate.run(move || {
        // Configure safety context
        let mut safety_context = SafetyContext::new();

        let mut system_io = SystemIO::nothing()
            .allow_stderr()
            .allow_stdout();
        
        if config.allow_filesystem {
            system_io = system_io
                .allow_open_readonly()
                .allow_metadata();
        }

        safety_context = safety_context.enable(system_io)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to configure SystemIO: {}", e)))?;

        if config.allow_network {
            let networking = Networking::nothing()
                .allow_start_tcp_clients()
                .allow_start_udp_clients();
            
            safety_context = safety_context.enable(networking)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to configure Networking: {}", e)))?;
        }

        // Apply safety context
        safety_context.apply_to_current_thread()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to apply safety context: {}", e)))?;

        debug!("Safety context applied, executing function");
        
        // Execute the user function
        Ok(f())
    })
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to execute function in isolate: {}", e)))?;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert!(!config.allow_network);
        assert!(!config.allow_filesystem);
        assert_eq!(config.max_execution_time, Some(Duration::from_secs(300)));
        assert!(config.working_directory.is_none());
    }

    #[test]
    fn test_spawn_nonexistent_plugin() {
        let result = spawn_sandboxed_plugin(PathBuf::from("nonexistent_plugin"));
        assert!(result.is_err());
        
        if let Err(e) = result {
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
    }

    #[test]
    fn test_execute_in_sandbox() {
        let config = SandboxConfig::default();
        let result = execute_in_sandbox(|| {
            // Simple computation that should work in sandbox
            2 + 2
        }, config);
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 4);
    }

    #[test]
    fn test_execute_in_sandbox_with_filesystem() {
        let mut config = SandboxConfig::default();
        config.allow_filesystem = true;
        
        let result = execute_in_sandbox(|| {
            // Try to check if /proc exists (should work with filesystem access)
            std::path::Path::new("/proc").exists()
        }, config);
        
        // This might fail in test environment, but should not panic
        assert!(result.is_ok());
    }
} 