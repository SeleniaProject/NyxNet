//
//  NyxMobile.m
//  Nyx Mobile iOS Bridge Implementation
//

#import "NyxMobile.h"
#import <UIKit/UIKit.h>
#import <Network/Network.h>
#import <CoreTelephony/CTTelephonyNetworkInfo.h>
#import <CoreTelephony/CTCarrier.h>

@interface NyxMobileBridge ()

@property (nonatomic, strong) NSNotificationCenter *notificationCenter;
@property (nonatomic, strong) nw_path_monitor_t pathMonitor;
@property (nonatomic, strong) dispatch_queue_t networkQueue;
@property (nonatomic, assign) NyxNetworkState currentNetworkState;
@property (nonatomic, assign) NyxAppState currentAppState;

@end

@implementation NyxMobileBridge

// MARK: - Singleton

+ (instancetype)sharedInstance {
    static NyxMobileBridge *sharedInstance = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        sharedInstance = [[NyxMobileBridge alloc] init];
    });
    return sharedInstance;
}

// MARK: - Initialization

- (instancetype)init {
    self = [super init];
    if (self) {
        _notificationCenter = [NSNotificationCenter defaultCenter];
        _networkQueue = dispatch_queue_create("com.nyx.network", DISPATCH_QUEUE_SERIAL);
        _currentNetworkState = NyxNetworkStateNone;
        _currentAppState = NyxAppStateActive;
        
        // Initialize network monitoring
        _pathMonitor = nw_path_monitor_create();
        
        NSLog(@"NyxMobileBridge initialized");
    }
    return self;
}

- (BOOL)initializeMonitoring {
    NSLog(@"Initializing iOS monitoring systems");
    
    // Enable battery monitoring
    [UIDevice currentDevice].batteryMonitoringEnabled = YES;
    
    // Register for notifications
    [self registerForAppStateNotifications];
    [self startNetworkMonitoring];
    
    NSLog(@"iOS monitoring initialization complete");
    return YES;
}

- (void)cleanup {
    NSLog(@"Cleaning up iOS monitoring systems");
    
    [self unregisterFromAppStateNotifications];
    [self stopNetworkMonitoring];
    
    // Disable battery monitoring
    [UIDevice currentDevice].batteryMonitoringEnabled = NO;
}

// MARK: - Battery Monitoring

- (NSInteger)batteryLevel {
    float level = [UIDevice currentDevice].batteryLevel;
    if (level < 0) {
        return -1; // Unknown
    }
    return (NSInteger)(level * 100);
}

- (BOOL)isCharging {
    UIDeviceBatteryState state = [UIDevice currentDevice].batteryState;
    return (state == UIDeviceBatteryStateCharging || state == UIDeviceBatteryStateFull);
}

- (BOOL)isBatteryMonitoringEnabled {
    return [UIDevice currentDevice].batteryMonitoringEnabled;
}

- (void)enableBatteryMonitoring:(BOOL)enabled {
    [UIDevice currentDevice].batteryMonitoringEnabled = enabled;
    NSLog(@"Battery monitoring %@", enabled ? @"enabled" : @"disabled");
}

// MARK: - Power Management

- (BOOL)isLowPowerModeEnabled {
    return [[NSProcessInfo processInfo] isLowPowerModeEnabled];
}

- (BOOL)isScreenOn {
    // Check app state as proxy for screen state
    UIApplicationState state = [[UIApplication sharedApplication] applicationState];
    return (state == UIApplicationStateActive);
}

// MARK: - App State Monitoring

- (NyxAppState)appState {
    UIApplicationState state = [[UIApplication sharedApplication] applicationState];
    switch (state) {
        case UIApplicationStateActive:
            return NyxAppStateActive;
        case UIApplicationStateInactive:
            return NyxAppStateInactive;
        case UIApplicationStateBackground:
            return NyxAppStateBackground;
        default:
            return NyxAppStateInactive;
    }
}

- (void)registerForAppStateNotifications {
    NSLog(@"Registering for app state notifications");
    
    [_notificationCenter addObserver:self
                            selector:@selector(appDidBecomeActive:)
                                name:UIApplicationDidBecomeActiveNotification
                              object:nil];
    
    [_notificationCenter addObserver:self
                            selector:@selector(appWillResignActive:)
                                name:UIApplicationWillResignActiveNotification
                              object:nil];
    
    [_notificationCenter addObserver:self
                            selector:@selector(appDidEnterBackground:)
                                name:UIApplicationDidEnterBackgroundNotification
                              object:nil];
    
    [_notificationCenter addObserver:self
                            selector:@selector(appWillEnterForeground:)
                                name:UIApplicationWillEnterForegroundNotification
                              object:nil];
    
    // Battery notifications
    [_notificationCenter addObserver:self
                            selector:@selector(batteryLevelChanged:)
                                name:UIDeviceBatteryLevelDidChangeNotification
                              object:nil];
    
    [_notificationCenter addObserver:self
                            selector:@selector(batteryStateChanged:)
                                name:UIDeviceBatteryStateDidChangeNotification
                              object:nil];
    
    // Power mode notifications
    [_notificationCenter addObserver:self
                            selector:@selector(powerModeChanged:)
                                name:NSProcessInfoPowerStateDidChangeNotification
                              object:nil];
}

- (void)unregisterFromAppStateNotifications {
    NSLog(@"Unregistering from app state notifications");
    [_notificationCenter removeObserver:self];
}

// MARK: - Network Monitoring

- (NyxNetworkState)networkState {
    return _currentNetworkState;
}

- (void)startNetworkMonitoring {
    NSLog(@"Starting network monitoring");
    
    __weak typeof(self) weakSelf = self;
    nw_path_monitor_set_update_handler(_pathMonitor, ^(nw_path_t path) {
        __strong typeof(weakSelf) strongSelf = weakSelf;
        if (!strongSelf) return;
        
        NyxNetworkState newState = NyxNetworkStateNone;
        
        if (nw_path_get_status(path) == nw_path_status_satisfied) {
            if (nw_path_uses_interface_type(path, nw_interface_type_wifi)) {
                newState = NyxNetworkStateWiFi;
            } else if (nw_path_uses_interface_type(path, nw_interface_type_cellular)) {
                newState = NyxNetworkStateCellular;
            } else if (nw_path_uses_interface_type(path, nw_interface_type_wired)) {
                newState = NyxNetworkStateEthernet;
            } else {
                newState = NyxNetworkStateWiFi; // Default assumption
            }
        }
        
        if (strongSelf->_currentNetworkState != newState) {
            strongSelf->_currentNetworkState = newState;
            [strongSelf onNetworkStateChanged:newState];
        }
    });
    
    nw_path_monitor_start(_pathMonitor, _networkQueue);
}

- (void)stopNetworkMonitoring {
    NSLog(@"Stopping network monitoring");
    
    if (_pathMonitor) {
        nw_path_monitor_cancel(_pathMonitor);
        _pathMonitor = nil;
    }
}

// MARK: - Notification Handlers

- (void)appDidBecomeActive:(NSNotification *)notification {
    NSLog(@"App became active");
    _currentAppState = NyxAppStateActive;
    [self onAppStateChanged:NyxAppStateActive];
}

- (void)appWillResignActive:(NSNotification *)notification {
    NSLog(@"App will resign active");
    _currentAppState = NyxAppStateInactive;
    [self onAppStateChanged:NyxAppStateInactive];
}

- (void)appDidEnterBackground:(NSNotification *)notification {
    NSLog(@"App entered background");
    _currentAppState = NyxAppStateBackground;
    [self onAppStateChanged:NyxAppStateBackground];
}

- (void)appWillEnterForeground:(NSNotification *)notification {
    NSLog(@"App will enter foreground");
    // State will be updated when app becomes active
}

- (void)batteryLevelChanged:(NSNotification *)notification {
    NSInteger level = self.batteryLevel;
    NSLog(@"Battery level changed: %ld%%", (long)level);
    [self onBatteryLevelChanged:level];
}

- (void)batteryStateChanged:(NSNotification *)notification {
    BOOL charging = self.isCharging;
    NSLog(@"Charging state changed: %@", charging ? @"YES" : @"NO");
    [self onChargingStateChanged:charging];
}

- (void)powerModeChanged:(NSNotification *)notification {
    BOOL lowPowerMode = self.isLowPowerModeEnabled;
    NSLog(@"Low power mode changed: %@", lowPowerMode ? @"YES" : @"NO");
    [self onLowPowerModeChanged:lowPowerMode];
}

// MARK: - Callback Methods

- (void)onBatteryLevelChanged:(NSInteger)level {
    // This would typically call back into Rust FFI
    NSLog(@"Battery level callback: %ld%%", (long)level);
}

- (void)onChargingStateChanged:(BOOL)charging {
    // This would typically call back into Rust FFI
    NSLog(@"Charging state callback: %@", charging ? @"YES" : @"NO");
}

- (void)onLowPowerModeChanged:(BOOL)lowPowerMode {
    // This would typically call back into Rust FFI
    NSLog(@"Low power mode callback: %@", lowPowerMode ? @"YES" : @"NO");
}

- (void)onAppStateChanged:(NyxAppState)state {
    // This would typically call back into Rust FFI
    NSLog(@"App state callback: %ld", (long)state);
}

- (void)onNetworkStateChanged:(NyxNetworkState)state {
    // This would typically call back into Rust FFI
    NSLog(@"Network state callback: %ld", (long)state);
}

@end

// MARK: - C Bridge Implementation

// These functions bridge between Objective-C and the Rust FFI

float nyx_ios_get_battery_level_objc(void) {
    return [[NyxMobileBridge sharedInstance] batteryLevel];
}

int nyx_ios_is_charging_objc(void) {
    return [[NyxMobileBridge sharedInstance] isCharging] ? 1 : 0;
}

int nyx_ios_is_screen_on_objc(void) {
    return [[NyxMobileBridge sharedInstance] isScreenOn] ? 1 : 0;
}

int nyx_ios_is_low_power_mode_objc(void) {
    return [[NyxMobileBridge sharedInstance] isLowPowerModeEnabled] ? 1 : 0;
}

int nyx_ios_get_app_state_objc(void) {
    return (int)[[NyxMobileBridge sharedInstance] appState];
}

int nyx_ios_get_network_state_objc(void) {
    return (int)[[NyxMobileBridge sharedInstance] networkState];
}

int nyx_ios_initialize_monitoring_objc(void) {
    BOOL success = [[NyxMobileBridge sharedInstance] initializeMonitoring];
    return success ? 0 : -1;
}

void nyx_ios_cleanup_monitoring_objc(void) {
    [[NyxMobileBridge sharedInstance] cleanup];
}
