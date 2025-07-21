#![forbid(unsafe_code)]

//! Cross-platform battery state provider used for adaptive cover-traffic on
//! mobile devices.
//!
//! On Android we fetch `BatteryManager.EXTRA_BATTERY_LEVEL` via JNI. On iOS we
//! call `UIDevice.batteryState` through the `objc` runtime. Desktop builds
//! fallback to `Unknown`.

use tokio_stream::{wrappers::ReceiverStream, Stream};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    Unknown,
}

/// Get current battery state.
#[must_use]
pub fn battery_state() -> BatteryState {
    platform::battery_state()
}

/// High-level power state used by higher layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilePowerState {
    /// Screen on / foreground, device plugged in or on battery but active.
    Foreground,
    /// Screen is off and device on battery (low-power).
    ScreenOff,
    /// Device is connected to external power and charging.
    Charging,
    /// Device running on battery and discharging (screen on).
    Discharging,
}

/// Subscribe to power state changes (stubbed on desktop).
pub fn subscribe_power_events() -> impl Stream<Item = MobilePowerState> {
    // Desktop platforms: emit a single Foreground and then end.
    let (tx, rx) = watch::channel(MobilePowerState::Foreground);
    // Send initial state; no further updates in stub.
    let _ = tx.send(MobilePowerState::Foreground);
    ReceiverStream::new(rx)
}

#[cfg(target_os = "android")]
mod platform {
    use super::BatteryState;
    use jni::{JNIEnv, JavaVM};
    use jni::objects::{JObject, JValue};

    pub fn battery_state() -> BatteryState {
        // SAFETY: Called from Java thread with proper VM attached.
        let vm = match JavaVM::get_java_vm() {
            Ok(vm) => vm,
            Err(_) => return BatteryState::Unknown,
        };
        let env = match vm.attach_current_thread() {
            Ok(e) => e,
            Err(_) => return BatteryState::Unknown,
        };
        get_state(&env)
    }

    fn get_state(env: &JNIEnv) -> BatteryState {
        let context = match env.call_static_method("android/app/ActivityThread", "currentApplication", "()Landroid/app/Application;", &[]) {
            Ok(JValue::Object(obj)) => obj,
            _ => return BatteryState::Unknown,
        };
        if context.is_null() { return BatteryState::Unknown; }
        let intent_opt = env.call_method(context, "registerReceiver", "(Landroid/content/BroadcastReceiver;Landroid/content/IntentFilter;)Landroid/content/Intent;", &[JValue::Object(JObject::null()), JValue::Object(JObject::null())]);
        let intent = match intent_opt {
            Ok(JValue::Object(obj)) => obj,
            _ => return BatteryState::Unknown,
        };
        let level = env.call_method(intent, "getIntExtra", "(Ljava/lang/String;I)I", &[JValue::Object(env.new_string("level").unwrap().into()), JValue::Int(-1)]).ok();
        let scale = env.call_method(intent, "getIntExtra", "(Ljava/lang/String;I)I", &[JValue::Object(env.new_string("scale").unwrap().into()), JValue::Int(-1)]).ok();
        if let (Some(JValue::Int(l)), Some(JValue::Int(s))) = (level, scale) {
            if l == s { return BatteryState::Full; }
        }
        let status = env.call_method(intent, "getIntExtra", "(Ljava/lang/String;I)I", &[JValue::Object(env.new_string("status").unwrap().into()), JValue::Int(-1)]).ok();
        if let Some(JValue::Int(st)) = status {
            // 2 = CHARGING, 3 = DISCHARGING
            return match st {
                2 => BatteryState::Charging,
                3 => BatteryState::Discharging,
                _ => BatteryState::Unknown,
            };
        }
        BatteryState::Unknown
    }
}

#[cfg(target_os = "ios")]
mod platform {
    use super::BatteryState;
    use objc::{class, msg_send, sel, sel_impl};

    pub fn battery_state() -> BatteryState {
        unsafe {
            let device: *mut objc::runtime::Object = msg_send![class!(UIDevice), currentDevice];
            let _: () = msg_send![device, setBatteryMonitoringEnabled: true];
            let state: u32 = msg_send![device, batteryState];
            match state {
                0 => BatteryState::Unknown,
                1 => BatteryState::Unplugged,
                2 => BatteryState::Charging,
                3 => BatteryState::Full,
                _ => BatteryState::Unknown,
            }
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod platform {
    use super::BatteryState;
    pub fn battery_state() -> BatteryState { BatteryState::Unknown }
} 