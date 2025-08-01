//
//  NyxMobile.h
//  Nyx Mobile iOS Bridge
//
//  iOS Objective-C bridge for Nyx mobile platform integration
//

#import <Foundation/Foundation.h>
#import <UIKit/UIKit.h>
#import <Network/Network.h>

NS_ASSUME_NONNULL_BEGIN

// MARK: - Mobile State Types

typedef NS_ENUM(NSInteger, NyxAppState) {
    NyxAppStateActive = 0,
    NyxAppStateBackground = 1,
    NyxAppStateInactive = 2
};

typedef NS_ENUM(NSInteger, NyxNetworkState) {
    NyxNetworkStateWiFi = 0,
    NyxNetworkStateCellular = 1,
    NyxNetworkStateEthernet = 2,
    NyxNetworkStateNone = 3
};

typedef NS_ENUM(NSInteger, NyxPlatform) {
    NyxPlatformOther = 0,
    NyxPlatformiOS = 1,
    NyxPlatformAndroid = 2
};

// MARK: - iOS Bridge Interface

@interface NyxMobileBridge : NSObject

// Initialization
+ (instancetype)sharedInstance;
- (BOOL)initializeMonitoring;
- (void)cleanup;

// Battery Monitoring
@property (nonatomic, readonly) NSInteger batteryLevel;
@property (nonatomic, readonly) BOOL isCharging;
@property (nonatomic, readonly) BOOL isBatteryMonitoringEnabled;
- (void)enableBatteryMonitoring:(BOOL)enabled;

// Power Management
@property (nonatomic, readonly) BOOL isLowPowerModeEnabled;
@property (nonatomic, readonly) BOOL isScreenOn;

// App State Monitoring
@property (nonatomic, readonly) NyxAppState appState;
- (void)registerForAppStateNotifications;
- (void)unregisterFromAppStateNotifications;

// Network Monitoring
@property (nonatomic, readonly) NyxNetworkState networkState;
- (void)startNetworkMonitoring;
- (void)stopNetworkMonitoring;

// Notification Callbacks
- (void)onBatteryLevelChanged:(NSInteger)level;
- (void)onChargingStateChanged:(BOOL)charging;
- (void)onLowPowerModeChanged:(BOOL)lowPowerMode;
- (void)onAppStateChanged:(NyxAppState)state;
- (void)onNetworkStateChanged:(NyxNetworkState)state;

@end

// MARK: - C Bridge Functions

#ifdef __cplusplus
extern "C" {
#endif

// FFI Bridge Functions (implemented in Rust)
int nyx_mobile_init(void);
void nyx_mobile_cleanup(void);
int nyx_mobile_get_platform(void);

// iOS-specific functions (implemented in Rust iOS module)
float ios_get_battery_level(void);
int ios_is_charging(void);
int ios_is_screen_on(void);
int ios_is_low_power_mode(void);
int ios_get_app_state(void);
int ios_get_network_state(void);
int ios_register_battery_notifications(void);
int ios_register_app_notifications(void);

// High-level FFI functions
int nyx_mobile_get_battery_level(void);
int nyx_mobile_is_charging(void);
int nyx_mobile_is_screen_on(void);
int nyx_mobile_is_low_power_mode(void);
int nyx_mobile_get_app_state(void);
int nyx_mobile_get_network_state(void);
int nyx_mobile_start_monitoring(void);

#ifdef __cplusplus
}
#endif

NS_ASSUME_NONNULL_END
