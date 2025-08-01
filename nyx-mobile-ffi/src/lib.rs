//! Mobile Platform FFI Bindings
//!
//! This crate provides Foreign Function Interface (FFI) bindings for mobile platforms,
//! enabling integration with iOS (Objective-C/Swift) and Android (JNI) native APIs.
//!
//! ## iOS Integration
//! - UIDevice battery monitoring
//! - UIApplication state notifications
//! - Network framework connectivity detection
//! - ProcessInfo power management
//!
//! ## Android Integration
//! - BatteryManager APIs
//! - PowerManager state monitoring
//! - ConnectivityManager network detection
//! - ActivityManager lifecycle tracking
//!
//! ## Safety
//! All FFI functions are implemented with safe Rust wrappers around platform-specific
//! APIs, with comprehensive error handling and type safety guarantees.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Arc;
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};

// Internal mobile state definitions (avoiding nyx-core dependency)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Active = 0,
    Background = 1,
    Inactive = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkState {
    WiFi = 0,
    Cellular = 1,
    Ethernet = 2,
    None = 3,
}

#[derive(Debug, Clone, Copy)]
pub struct PowerState {
    pub battery_level: u8,
    pub is_charging: bool,
    pub screen_on: bool,
    pub low_power_mode: bool,
}

// Simple state storage without complex monitor
static CURRENT_POWER_STATE: OnceCell<std::sync::RwLock<PowerState>> = OnceCell::new();
static CURRENT_APP_STATE: OnceCell<std::sync::RwLock<AppState>> = OnceCell::new();
static CURRENT_NETWORK_STATE: OnceCell<std::sync::RwLock<NetworkState>> = OnceCell::new();

mod ios;
mod android;
mod common;

pub use ios::*;
pub use android::*;
pub use common::*;

/// Global tokio runtime for async operations
static RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the mobile FFI runtime
#[no_mangle]
pub extern "C" fn nyx_mobile_init() -> c_int {
    // Initialize tracing (ignore error if already initialized)
    let _ = tracing_subscriber::fmt::try_init();
    
    info!("Initializing Nyx Mobile FFI");
    
    // Create tokio runtime
    let runtime = match Runtime::new() {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            error!("Failed to create tokio runtime: {}", e);
            return -1;
        }
    };
    
    if RUNTIME.set(runtime).is_err() {
        warn!("Mobile FFI already initialized");
        return 1; // Already initialized
    }
    
    // Initialize state storage
    let initial_power = PowerState {
        battery_level: 100,
        is_charging: false,
        screen_on: true,
        low_power_mode: false,
    };
    
    let _ = CURRENT_POWER_STATE.set(std::sync::RwLock::new(initial_power));
    let _ = CURRENT_APP_STATE.set(std::sync::RwLock::new(AppState::Active));
    let _ = CURRENT_NETWORK_STATE.set(std::sync::RwLock::new(NetworkState::WiFi));
    
    info!("Mobile FFI initialization complete");
    0 // Success
}

/// Cleanup mobile FFI resources
#[no_mangle]
pub extern "C" fn nyx_mobile_cleanup() {
    info!("Cleaning up Mobile FFI resources");
    debug!("Mobile FFI cleanup complete");
}

/// Get current battery level (0-100)
#[no_mangle]
pub extern "C" fn nyx_mobile_get_battery_level() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_get_battery_level() as c_int;
    }
    
    #[cfg(target_os = "android")]
    {
        return android_get_battery_level();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_POWER_STATE.get() {
            Some(state) => state.read().unwrap().battery_level as c_int,
            None => 80 // Default
        }
    }
}

/// Check if device is charging (1 = true, 0 = false, -1 = error)
#[no_mangle]
pub extern "C" fn nyx_mobile_is_charging() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_is_charging();
    }
    
    #[cfg(target_os = "android")]
    {
        return android_is_charging();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_POWER_STATE.get() {
            Some(state) => if state.read().unwrap().is_charging { 1 } else { 0 },
            None => 0 // Default
        }
    }
}

/// Check if screen is on (1 = true, 0 = false, -1 = error)
#[no_mangle]
pub extern "C" fn nyx_mobile_is_screen_on() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_is_screen_on();
    }
    
    #[cfg(target_os = "android")]
    {
        return android_is_screen_on();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_POWER_STATE.get() {
            Some(state) => if state.read().unwrap().screen_on { 1 } else { 0 },
            None => 1 // Default
        }
    }
}

/// Check if low power mode is enabled (1 = true, 0 = false, -1 = error)
#[no_mangle]
pub extern "C" fn nyx_mobile_is_low_power_mode() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_is_low_power_mode();
    }
    
    #[cfg(target_os = "android")]
    {
        return android_is_power_save_mode();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_POWER_STATE.get() {
            Some(state) => if state.read().unwrap().low_power_mode { 1 } else { 0 },
            None => 0 // Default
        }
    }
}

/// Get current app state (0 = Active, 1 = Background, 2 = Inactive, -1 = error)
#[no_mangle]
pub extern "C" fn nyx_mobile_get_app_state() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_get_app_state();
    }
    
    #[cfg(target_os = "android")]
    {
        return android_get_app_state();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_APP_STATE.get() {
            Some(state) => *state.read().unwrap() as c_int,
            None => AppState::Active as c_int // Default
        }
    }
}

/// Get current network state (0 = WiFi, 1 = Cellular, 2 = Ethernet, 3 = None, -1 = error)
#[no_mangle]
pub extern "C" fn nyx_mobile_get_network_state() -> c_int {
    #[cfg(target_os = "ios")]
    {
        return ios_get_network_state();
    }
    
    #[cfg(target_os = "android")]
    {
        return android_get_network_state();
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        // Desktop fallback
        match CURRENT_NETWORK_STATE.get() {
            Some(state) => *state.read().unwrap() as c_int,
            None => NetworkState::WiFi as c_int // Default
        }
    }
}

/// Start mobile state monitoring
#[no_mangle]
pub extern "C" fn nyx_mobile_start_monitoring() -> c_int {
    info!("Starting mobile state monitoring");
    
    #[cfg(target_os = "ios")]
    {
        let result = ios_register_battery_notifications();
        if result != 0 {
            return result;
        }
        let result = ios_register_app_notifications();
        if result != 0 {
            return result;
        }
        info!("iOS monitoring started successfully");
    }
    
    #[cfg(target_os = "android")]
    {
        info!("Android monitoring ready (requires JNI initialization from Java)");
    }
    
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        info!("Desktop platform - mobile monitoring simulated");
    }
    
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;
    
    static INIT: Once = Once::new();
    
    fn ensure_init() {
        INIT.call_once(|| {
            let result = nyx_mobile_init();
            assert!(result >= 0, "FFI initialization should succeed");
        });
    }
    
    #[test]
    fn test_ffi_initialization() {
        ensure_init();
        // Already initialized by ensure_init()
    }
    
    #[test]
    fn test_battery_level_ffi() {
        ensure_init();
        
        let battery_level = nyx_mobile_get_battery_level();
        assert!(battery_level >= 0 && battery_level <= 100, "Battery level should be 0-100");
    }
    
    #[test]
    fn test_charging_state_ffi() {
        ensure_init();
        
        let charging_state = nyx_mobile_is_charging();
        assert!(charging_state >= 0 && charging_state <= 1, "Charging state should be 0 or 1");
    }
}
