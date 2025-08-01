//! Common mobile platform utilities and platform detection

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use tracing::{debug, info};

/// Platform detection constants
pub const PLATFORM_IOS: c_int = 1;
pub const PLATFORM_ANDROID: c_int = 2;
pub const PLATFORM_OTHER: c_int = 0;

/// Get current platform type
#[no_mangle]
pub extern "C" fn nyx_mobile_get_platform() -> c_int {
    #[cfg(target_os = "ios")]
    {
        debug!("Platform detected: iOS");
        PLATFORM_IOS
    }
    
    #[cfg(target_os = "android")]
    {
        debug!("Platform detected: Android");
        PLATFORM_ANDROID
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        debug!("Platform detected: Other (desktop/server)");
        PLATFORM_OTHER
    }
}

/// Get platform name as string
#[no_mangle]
pub extern "C" fn nyx_mobile_get_platform_name() -> *const c_char {
    let platform_name = if cfg!(target_os = "ios") {
        "iOS"
    } else if cfg!(target_os = "android") {
        "Android"
    } else {
        "Other"
    };
    
    // Note: This returns a static string, so it's safe
    // In a real implementation, caller would need to manage memory
    platform_name.as_ptr() as *const c_char
}

/// Check if current platform supports mobile features
#[no_mangle]
pub extern "C" fn nyx_mobile_is_mobile_platform() -> c_int {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        1
    } else {
        0
    }
}

/// Initialize platform-specific monitoring
#[no_mangle]
pub extern "C" fn nyx_mobile_init_platform_monitoring() -> c_int {
    info!("Initializing platform-specific monitoring");
    
    #[cfg(target_os = "ios")]
    {
        use crate::ios::{ios_register_battery_notifications, ios_register_app_notifications};
        
        let battery_result = ios_register_battery_notifications();
        let app_result = ios_register_app_notifications();
        
        if battery_result != 0 || app_result != 0 {
            return -1;
        }
        
        info!("iOS monitoring initialized successfully");
    }
    
    #[cfg(target_os = "android")]
    {
        // Android monitoring is typically initialized through JNI
        // from the Java side when the application starts
        info!("Android monitoring ready (requires JNI initialization)");
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        info!("Desktop platform - mobile monitoring not available");
    }
    
    0
}

/// Utility function to convert C string to Rust String
pub fn c_str_to_string(c_str: *const c_char) -> Result<String, std::str::Utf8Error> {
    if c_str.is_null() {
        return Ok(String::new());
    }
    
    let cstr = unsafe { CStr::from_ptr(c_str) };
    cstr.to_str().map(|s| s.to_owned())
}

/// Utility function to convert Rust String to C string
pub fn string_to_c_str(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free C string allocated by string_to_c_str
#[no_mangle]
pub extern "C" fn nyx_mobile_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // Take ownership and drop
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

/// Get mobile feature capabilities for current platform
#[no_mangle]
pub extern "C" fn nyx_mobile_get_capabilities() -> c_int {
    let mut capabilities = 0;
    
    // Battery monitoring capability
    if cfg!(any(target_os = "ios", target_os = "android")) {
        capabilities |= 1; // CAPABILITY_BATTERY
    }
    
    // App state monitoring capability
    if cfg!(any(target_os = "ios", target_os = "android")) {
        capabilities |= 2; // CAPABILITY_APP_STATE
    }
    
    // Network monitoring capability
    if cfg!(any(target_os = "ios", target_os = "android")) {
        capabilities |= 4; // CAPABILITY_NETWORK
    }
    
    // Power management capability
    if cfg!(any(target_os = "ios", target_os = "android")) {
        capabilities |= 8; // CAPABILITY_POWER_MANAGEMENT
    }
    
    // Push notification capability (requires additional setup)
    if cfg!(any(target_os = "ios", target_os = "android")) {
        capabilities |= 16; // CAPABILITY_PUSH_NOTIFICATIONS
    }
    
    debug!("Mobile capabilities: 0x{:x}", capabilities);
    capabilities
}

/// Error handling utilities
#[repr(C)]
pub struct MobileError {
    pub code: c_int,
    pub message: *const c_char,
}

/// Common error codes
pub const ERROR_SUCCESS: c_int = 0;
pub const ERROR_NOT_INITIALIZED: c_int = -1;
pub const ERROR_PLATFORM_NOT_SUPPORTED: c_int = -2;
pub const ERROR_PERMISSION_DENIED: c_int = -3;
pub const ERROR_SYSTEM_ERROR: c_int = -4;
pub const ERROR_INVALID_PARAMETER: c_int = -5;

/// Get error message for error code
#[no_mangle]
pub extern "C" fn nyx_mobile_get_error_message(error_code: c_int) -> *const c_char {
    let message = match error_code {
        ERROR_SUCCESS => "Success",
        ERROR_NOT_INITIALIZED => "Mobile FFI not initialized",
        ERROR_PLATFORM_NOT_SUPPORTED => "Platform not supported",
        ERROR_PERMISSION_DENIED => "Permission denied",
        ERROR_SYSTEM_ERROR => "System error",
        ERROR_INVALID_PARAMETER => "Invalid parameter",
        _ => "Unknown error",
    };
    
    message.as_ptr() as *const c_char
}

/// Configuration management
#[repr(C)]
pub struct MobileConfig {
    pub polling_interval_ms: c_int,
    pub enable_battery_monitoring: c_int,
    pub enable_app_state_monitoring: c_int,
    pub enable_network_monitoring: c_int,
    pub low_power_mode_threshold: c_int,
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            polling_interval_ms: 10000, // 10 seconds
            enable_battery_monitoring: 1,
            enable_app_state_monitoring: 1,
            enable_network_monitoring: 1,
            low_power_mode_threshold: 20, // 20% battery
        }
    }
}

/// Set mobile monitoring configuration
#[no_mangle]
pub extern "C" fn nyx_mobile_set_config(config: *const MobileConfig) -> c_int {
    if config.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    
    let cfg = unsafe { &*config };
    info!("Setting mobile config: polling_interval={}ms, battery={}, app_state={}, network={}, low_power_threshold={}%",
          cfg.polling_interval_ms,
          cfg.enable_battery_monitoring,
          cfg.enable_app_state_monitoring,
          cfg.enable_network_monitoring,
          cfg.low_power_mode_threshold);
    
    // In a full implementation, this would update the global configuration
    // For now, we just log the settings
    
    ERROR_SUCCESS
}

/// Get current mobile monitoring configuration
#[no_mangle]
pub extern "C" fn nyx_mobile_get_config() -> *const MobileConfig {
    // Return a static default configuration
    // In a real implementation, this would return the current configuration
    static DEFAULT_CONFIG: MobileConfig = MobileConfig {
        polling_interval_ms: 10000,
        enable_battery_monitoring: 1,
        enable_app_state_monitoring: 1,
        enable_network_monitoring: 1,
        low_power_mode_threshold: 20,
    };
    
    &DEFAULT_CONFIG
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_detection() {
        let platform = nyx_mobile_get_platform();
        assert!(platform >= 0 && platform <= 2);
        
        let is_mobile = nyx_mobile_is_mobile_platform();
        assert!(is_mobile == 0 || is_mobile == 1);
    }
    
    #[test]
    fn test_capabilities() {
        let capabilities = nyx_mobile_get_capabilities();
        assert!(capabilities >= 0);
    }
    
    #[test]
    fn test_error_messages() {
        let message = nyx_mobile_get_error_message(ERROR_SUCCESS);
        assert!(!message.is_null());
        
        let message = nyx_mobile_get_error_message(ERROR_NOT_INITIALIZED);
        assert!(!message.is_null());
    }
    
    #[test]
    fn test_config_management() {
        let config = nyx_mobile_get_config();
        assert!(!config.is_null());
        
        let result = nyx_mobile_set_config(config);
        assert_eq!(result, ERROR_SUCCESS);
        
        let result = nyx_mobile_set_config(std::ptr::null());
        assert_eq!(result, ERROR_INVALID_PARAMETER);
    }
    
    #[test]
    fn test_string_utilities() {
        let test_str = "Hello, world!".to_string();
        let c_str = string_to_c_str(test_str.clone());
        assert!(!c_str.is_null());
        
        let converted = c_str_to_string(c_str).unwrap();
        assert_eq!(converted, test_str);
        
        nyx_mobile_free_string(c_str);
    }
}
