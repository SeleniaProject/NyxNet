package com.nyx.mobile;

/**
 * NyxMobileJNI - JNI interface for Nyx mobile platform integration
 * 
 * This class provides the JNI interface between Java/Kotlin Android code
 * and the native Rust implementation.
 */
public class NyxMobileJNI {
    private static final String TAG = "NyxMobileJNI";
    
    // Load the native library
    static {
        try {
            System.loadLibrary("nyx_mobile_ffi");
        } catch (UnsatisfiedLinkError e) {
            android.util.Log.e(TAG, "Failed to load native library: " + e.getMessage());
        }
    }
    
    // Native method declarations (implemented in Rust)
    
    /**
     * Initialize the native mobile FFI system
     * @return 0 on success, negative on error
     */
    public static native int nativeInit();
    
    /**
     * Cleanup native mobile FFI resources
     */
    public static native void nativeCleanup();
    
    /**
     * Start native mobile state monitoring
     * @return 0 on success, negative on error
     */
    public static native int nativeStartMonitoring();
    
    /**
     * Get current battery level from native implementation
     * @return battery level 0-100, or negative on error
     */
    public static native int nativeGetBatteryLevel();
    
    /**
     * Check if device is charging from native implementation
     * @return 1 if charging, 0 if not, negative on error
     */
    public static native int nativeIsCharging();
    
    /**
     * Check if screen is on from native implementation
     * @return 1 if screen on, 0 if off, negative on error
     */
    public static native int nativeIsScreenOn();
    
    /**
     * Check if power save mode is enabled from native implementation
     * @return 1 if enabled, 0 if disabled, negative on error
     */
    public static native int nativeIsPowerSaveMode();
    
    /**
     * Get current app state from native implementation
     * @return app state value, negative on error
     */
    public static native int nativeGetAppState();
    
    /**
     * Get current network state from native implementation
     * @return network state value, negative on error
     */
    public static native int nativeGetNetworkState();
    
    /**
     * Initialize Android JNI environment in native code
     * This passes the JavaVM pointer and application context to Rust
     * @param context Android application context
     * @return 0 on success, negative on error
     */
    public static native int nativeInitAndroidJNI(android.content.Context context);
    
    // Callback methods called from native code
    
    /**
     * Called from native code when battery level changes
     * @param level new battery level (0-100)
     */
    public static void onBatteryLevelChanged(int level) {
        android.util.Log.d(TAG, "Battery level changed: " + level + "%");
        NyxMobileBridge.getInstance().setAppState(NyxMobileBridge.AppState.ACTIVE);
    }
    
    /**
     * Called from native code when charging state changes
     * @param charging true if charging, false if not
     */
    public static void onChargingStateChanged(boolean charging) {
        android.util.Log.d(TAG, "Charging state changed: " + charging);
    }
    
    /**
     * Called from native code when power save mode changes
     * @param enabled true if power save mode enabled
     */
    public static void onPowerSaveModeChanged(boolean enabled) {
        android.util.Log.d(TAG, "Power save mode changed: " + enabled);
    }
    
    /**
     * Called from native code when app state changes
     * @param state new app state value
     */
    public static void onAppStateChanged(int state) {
        android.util.Log.d(TAG, "App state changed: " + state);
    }
    
    /**
     * Called from native code when network state changes
     * @param state new network state value
     */
    public static void onNetworkStateChanged(int state) {
        android.util.Log.d(TAG, "Network state changed: " + state);
    }
    
    // Bridge methods that call through to Android APIs and then to native
    
    /**
     * Bridge method to get battery level via Android APIs
     * This is called from native code to get current battery level
     */
    public static int androidGetBatteryLevel() {
        return NyxMobileBridge.getInstance().getBatteryLevel();
    }
    
    /**
     * Bridge method to check charging state via Android APIs
     */
    public static boolean androidIsCharging() {
        return NyxMobileBridge.getInstance().isCharging();
    }
    
    /**
     * Bridge method to check screen state via Android APIs
     */
    public static boolean androidIsScreenOn() {
        return NyxMobileBridge.getInstance().isScreenOn();
    }
    
    /**
     * Bridge method to check power save mode via Android APIs
     */
    public static boolean androidIsPowerSaveMode() {
        return NyxMobileBridge.getInstance().isPowerSaveMode();
    }
    
    /**
     * Bridge method to get app state via Android APIs
     */
    public static int androidGetAppState() {
        return NyxMobileBridge.getInstance().getAppState().getValue();
    }
    
    /**
     * Bridge method to get network state via Android APIs
     */
    public static int androidGetNetworkState() {
        return NyxMobileBridge.getInstance().getNetworkState().getValue();
    }
}
