# Nyx Mobile FFI

Foreign Function Interface (FFI) bindings for mobile platforms, enabling seamless integration between Rust and native iOS/Android APIs.

## Overview

This crate provides:

- **iOS Integration**: Objective-C/Swift bridges for UIKit, Core Foundation, and Network framework APIs
- **Android Integration**: JNI bindings for Android system services (BatteryManager, PowerManager, ConnectivityManager)
- **Cross-platform API**: Unified Rust interface for mobile device state monitoring
- **Safe Wrappers**: Memory-safe FFI implementations with comprehensive error handling

## Features

### Battery Monitoring
- Real-time battery level detection (0-100%)
- Charging state monitoring (plugged/unplugged)
- Battery state change notifications

### Power Management
- Low power mode detection
- Screen on/off state monitoring
- Power profile optimization

### App Lifecycle
- Foreground/background state tracking
- App state change notifications
- Lifecycle event handling

### Network Connectivity
- WiFi/Cellular/Ethernet detection
- Network state change monitoring
- Connectivity quality assessment

## Platform Support

### iOS
- **Minimum Version**: iOS 12.0+
- **APIs Used**: UIDevice, UIApplication, ProcessInfo, Network framework
- **Bridge**: Objective-C bridge with Swift compatibility
- **Build**: Supports both device and simulator targets

### Android
- **Minimum Version**: Android API 21 (Android 5.0)+
- **APIs Used**: BatteryManager, PowerManager, ConnectivityManager, ActivityManager
- **Bridge**: JNI bindings with Java/Kotlin support
- **Build**: Supports all Android architectures (arm64-v8a, armeabi-v7a, x86_64, x86)

## Usage

### Rust Integration

```rust
use nyx_mobile_ffi::*;

// Initialize FFI
let result = nyx_mobile_init();
assert_eq!(result, 0);

// Start monitoring
let result = nyx_mobile_start_monitoring();
assert_eq!(result, 0);

// Get battery level
let level = nyx_mobile_get_battery_level();
println!("Battery level: {}%", level);

// Check charging state
let charging = nyx_mobile_is_charging();
println!("Charging: {}", charging == 1);

// Cleanup
nyx_mobile_cleanup();
```

### iOS Integration

```objective-c
#import "NyxMobile.h"

// Initialize bridge
NyxMobileBridge *bridge = [NyxMobileBridge sharedInstance];
[bridge initializeMonitoring];

// Get battery level
NSInteger level = bridge.batteryLevel;
NSLog(@"Battery level: %ld%%", level);

// Register for notifications
[bridge registerForAppStateNotifications];
```

### Android Integration

```java
import com.nyx.mobile.NyxMobileBridge;
import com.nyx.mobile.NyxMobileJNI;

// Initialize bridge
NyxMobileBridge bridge = NyxMobileBridge.getInstance();
bridge.initialize(getApplicationContext());

// Initialize native FFI
int result = NyxMobileJNI.nativeInit();

// Get battery level
int level = bridge.getBatteryLevel();
Log.d("Nyx", "Battery level: " + level + "%");
```

## Build Configuration

### iOS Build

Add to your iOS project's `Cargo.toml`:

```toml
[dependencies]
nyx-mobile-ffi = { path = "../nyx-mobile-ffi", features = ["ios"] }

[target.'cfg(target_os = "ios")'.dependencies]
objc = "0.2"
cocoa = "0.24"
core-foundation = "0.9"
```

### Android Build

Add to your Android project's `Cargo.toml`:

```toml
[dependencies]
nyx-mobile-ffi = { path = "../nyx-mobile-ffi", features = ["android"] }

[target.'cfg(target_os = "android")'.dependencies]
jni = "0.21"
android_logger = "0.13"
```

## Error Handling

All FFI functions use standardized error codes:

- `0`: Success
- `-1`: Not initialized
- `-2`: Platform not supported
- `-3`: Permission denied
- `-4`: System error
- `-5`: Invalid parameter

```rust
let result = nyx_mobile_get_battery_level();
if result < 0 {
    let error_msg = nyx_mobile_get_error_message(result);
    eprintln!("Error: {}", error_msg);
}
```

## Safety Guarantees

- **Memory Safety**: All FFI boundaries use safe Rust wrappers
- **Thread Safety**: Concurrent access protected with proper synchronization
- **Resource Management**: Automatic cleanup of native resources
- **Error Propagation**: Comprehensive error handling at all FFI boundaries

## Testing

```bash
# Test compilation (desktop)
cargo build

# Test iOS (requires Xcode)
cargo build --target aarch64-apple-ios

# Test Android (requires NDK)
cargo build --target aarch64-linux-android

# Run tests
cargo test
```

## Platform-specific Notes

### iOS
- Requires `Info.plist` permissions for battery monitoring
- UIDevice battery monitoring must be enabled
- Network monitoring requires `Network.framework` (iOS 12+)
- Background app refresh affects monitoring accuracy

### Android
- Requires `android.permission.BATTERY_STATS` permission
- Power save mode detection requires API 21+
- Network monitoring requires `android.permission.ACCESS_NETWORK_STATE`
- Doze mode affects background monitoring

## Integration Examples

See the `examples/` directory for complete integration examples:

- `ios_example/`: Complete iOS app with Nyx integration
- `android_example/`: Complete Android app with Nyx integration
- `cross_platform/`: Shared Rust code for both platforms

## Contributing

1. Follow Rust formatting (`cargo fmt`)
2. Pass all lints (`cargo clippy`)
3. Test on both iOS and Android
4. Update documentation for API changes
5. Add integration tests for new features

## License

MIT License - see [LICENSE](../LICENSE) for details.
