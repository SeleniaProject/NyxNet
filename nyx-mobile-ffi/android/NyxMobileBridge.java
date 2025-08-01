package com.nyx.mobile;

import android.app.Activity;
import android.app.ActivityManager;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkRequest;
import android.os.BatteryManager;
import android.os.Build;
import android.os.PowerManager;
import android.util.Log;

import androidx.annotation.NonNull;

/**
 * NyxMobileBridge - Android native bridge for Nyx mobile platform integration
 * 
 * This class provides Android-specific implementations for battery monitoring,
 * power management, app lifecycle tracking, and network connectivity detection.
 */
public class NyxMobileBridge {
    private static final String TAG = "NyxMobileBridge";
    
    // State enums matching Rust definitions
    public enum AppState {
        ACTIVE(0),
        BACKGROUND(1),
        INACTIVE(2);
        
        private final int value;
        AppState(int value) { this.value = value; }
        public int getValue() { return value; }
    }
    
    public enum NetworkState {
        WIFI(0),
        CELLULAR(1),
        ETHERNET(2),
        NONE(3);
        
        private final int value;
        NetworkState(int value) { this.value = value; }
        public int getValue() { return value; }
    }
    
    // Singleton instance
    private static NyxMobileBridge instance;
    
    // Android system services
    private Context context;
    private BatteryManager batteryManager;
    private PowerManager powerManager;
    private ConnectivityManager connectivityManager;
    private ActivityManager activityManager;
    
    // Current state
    private AppState currentAppState = AppState.ACTIVE;
    private NetworkState currentNetworkState = NetworkState.NONE;
    
    // Broadcast receivers
    private BatteryBroadcastReceiver batteryReceiver;
    private PowerSaveBroadcastReceiver powerSaveReceiver;
    
    // Network callback
    private ConnectivityManager.NetworkCallback networkCallback;
    
    // Native callback interface
    public interface NativeCallback {
        void onBatteryLevelChanged(int level);
        void onChargingStateChanged(boolean charging);
        void onPowerSaveModeChanged(boolean enabled);
        void onAppStateChanged(int state);
        void onNetworkStateChanged(int state);
    }
    
    private NativeCallback nativeCallback;
    
    // Singleton access
    public static synchronized NyxMobileBridge getInstance() {
        if (instance == null) {
            instance = new NyxMobileBridge();
        }
        return instance;
    }
    
    private NyxMobileBridge() {
        Log.d(TAG, "NyxMobileBridge created");
    }
    
    /**
     * Initialize the mobile bridge with Android context
     */
    public boolean initialize(Context context) {
        Log.d(TAG, "Initializing NyxMobileBridge");
        
        this.context = context.getApplicationContext();
        
        // Get system services
        batteryManager = (BatteryManager) context.getSystemService(Context.BATTERY_SERVICE);
        powerManager = (PowerManager) context.getSystemService(Context.POWER_SERVICE);
        connectivityManager = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        activityManager = (ActivityManager) context.getSystemService(Context.ACTIVITY_SERVICE);
        
        if (batteryManager == null || powerManager == null || 
            connectivityManager == null || activityManager == null) {
            Log.e(TAG, "Failed to get required system services");
            return false;
        }
        
        // Initialize monitoring
        startBatteryMonitoring();
        startPowerSaveMonitoring();
        startNetworkMonitoring();
        
        Log.d(TAG, "NyxMobileBridge initialization complete");
        return true;
    }
    
    /**
     * Set native callback for state changes
     */
    public void setNativeCallback(NativeCallback callback) {
        this.nativeCallback = callback;
    }
    
    /**
     * Cleanup resources
     */
    public void cleanup() {
        Log.d(TAG, "Cleaning up NyxMobileBridge");
        
        stopBatteryMonitoring();
        stopPowerSaveMonitoring();
        stopNetworkMonitoring();
        
        nativeCallback = null;
    }
    
    // MARK: - Battery Monitoring
    
    public int getBatteryLevel() {
        if (batteryManager == null) return -1;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            return batteryManager.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY);
        } else {
            // Fallback for older Android versions
            IntentFilter filter = new IntentFilter(Intent.ACTION_BATTERY_CHANGED);
            Intent batteryStatus = context.registerReceiver(null, filter);
            if (batteryStatus == null) return -1;
            
            int level = batteryStatus.getIntExtra(BatteryManager.EXTRA_LEVEL, -1);
            int scale = batteryStatus.getIntExtra(BatteryManager.EXTRA_SCALE, -1);
            
            if (level >= 0 && scale > 0) {
                return (int) ((level / (float) scale) * 100);
            }
            return -1;
        }
    }
    
    public boolean isCharging() {
        if (batteryManager == null) return false;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            int status = batteryManager.getIntProperty(BatteryManager.BATTERY_PROPERTY_STATUS);
            return status == BatteryManager.BATTERY_STATUS_CHARGING || 
                   status == BatteryManager.BATTERY_STATUS_FULL;
        } else {
            // Fallback for older Android versions
            IntentFilter filter = new IntentFilter(Intent.ACTION_BATTERY_CHANGED);
            Intent batteryStatus = context.registerReceiver(null, filter);
            if (batteryStatus == null) return false;
            
            int status = batteryStatus.getIntExtra(BatteryManager.EXTRA_STATUS, -1);
            return status == BatteryManager.BATTERY_STATUS_CHARGING ||
                   status == BatteryManager.BATTERY_STATUS_FULL;
        }
    }
    
    private void startBatteryMonitoring() {
        if (context == null) return;
        
        batteryReceiver = new BatteryBroadcastReceiver();
        IntentFilter filter = new IntentFilter();
        filter.addAction(Intent.ACTION_BATTERY_CHANGED);
        filter.addAction(Intent.ACTION_POWER_CONNECTED);
        filter.addAction(Intent.ACTION_POWER_DISCONNECTED);
        
        context.registerReceiver(batteryReceiver, filter);
        Log.d(TAG, "Battery monitoring started");
    }
    
    private void stopBatteryMonitoring() {
        if (context != null && batteryReceiver != null) {
            context.unregisterReceiver(batteryReceiver);
            batteryReceiver = null;
            Log.d(TAG, "Battery monitoring stopped");
        }
    }
    
    // MARK: - Power Management
    
    public boolean isScreenOn() {
        if (powerManager == null) return true;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.KITKAT_WATCH) {
            return powerManager.isInteractive();
        } else {
            @SuppressWarnings("deprecation")
            boolean screenOn = powerManager.isScreenOn();
            return screenOn;
        }
    }
    
    public boolean isPowerSaveMode() {
        if (powerManager == null) return false;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            return powerManager.isPowerSaveMode();
        }
        return false;
    }
    
    private void startPowerSaveMonitoring() {
        if (context == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.LOLLIPOP) return;
        
        powerSaveReceiver = new PowerSaveBroadcastReceiver();
        IntentFilter filter = new IntentFilter();
        filter.addAction(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED);
        
        context.registerReceiver(powerSaveReceiver, filter);
        Log.d(TAG, "Power save monitoring started");
    }
    
    private void stopPowerSaveMonitoring() {
        if (context != null && powerSaveReceiver != null) {
            context.unregisterReceiver(powerSaveReceiver);
            powerSaveReceiver = null;
            Log.d(TAG, "Power save monitoring stopped");
        }
    }
    
    // MARK: - App State Management
    
    public AppState getAppState() {
        return currentAppState;
    }
    
    public void setAppState(AppState state) {
        if (currentAppState != state) {
            currentAppState = state;
            Log.d(TAG, "App state changed: " + state);
            
            if (nativeCallback != null) {
                nativeCallback.onAppStateChanged(state.getValue());
            }
        }
    }
    
    // MARK: - Network Monitoring
    
    public NetworkState getNetworkState() {
        return currentNetworkState;
    }
    
    private void updateNetworkState() {
        if (connectivityManager == null) return;
        
        NetworkState newState = NetworkState.NONE;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            Network activeNetwork = connectivityManager.getActiveNetwork();
            if (activeNetwork != null) {
                NetworkCapabilities capabilities = connectivityManager.getNetworkCapabilities(activeNetwork);
                if (capabilities != null) {
                    if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) {
                        newState = NetworkState.WIFI;
                    } else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) {
                        newState = NetworkState.CELLULAR;
                    } else if (capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) {
                        newState = NetworkState.ETHERNET;
                    }
                }
            }
        } else {
            // Fallback for older Android versions
            @SuppressWarnings("deprecation")
            android.net.NetworkInfo activeInfo = connectivityManager.getActiveNetworkInfo();
            if (activeInfo != null && activeInfo.isConnected()) {
                switch (activeInfo.getType()) {
                    case ConnectivityManager.TYPE_WIFI:
                        newState = NetworkState.WIFI;
                        break;
                    case ConnectivityManager.TYPE_MOBILE:
                        newState = NetworkState.CELLULAR;
                        break;
                    case ConnectivityManager.TYPE_ETHERNET:
                        newState = NetworkState.ETHERNET;
                        break;
                }
            }
        }
        
        if (currentNetworkState != newState) {
            currentNetworkState = newState;
            Log.d(TAG, "Network state changed: " + newState);
            
            if (nativeCallback != null) {
                nativeCallback.onNetworkStateChanged(newState.getValue());
            }
        }
    }
    
    private void startNetworkMonitoring() {
        if (connectivityManager == null) return;
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            networkCallback = new ConnectivityManager.NetworkCallback() {
                @Override
                public void onAvailable(@NonNull Network network) {
                    updateNetworkState();
                }
                
                @Override
                public void onLost(@NonNull Network network) {
                    updateNetworkState();
                }
                
                @Override
                public void onCapabilitiesChanged(@NonNull Network network, @NonNull NetworkCapabilities capabilities) {
                    updateNetworkState();
                }
            };
            
            NetworkRequest.Builder builder = new NetworkRequest.Builder();
            connectivityManager.registerNetworkCallback(builder.build(), networkCallback);
        }
        
        // Initial state update
        updateNetworkState();
        
        Log.d(TAG, "Network monitoring started");
    }
    
    private void stopNetworkMonitoring() {
        if (connectivityManager != null && networkCallback != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            connectivityManager.unregisterNetworkCallback(networkCallback);
            networkCallback = null;
            Log.d(TAG, "Network monitoring stopped");
        }
    }
    
    // MARK: - Broadcast Receivers
    
    private class BatteryBroadcastReceiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context context, Intent intent) {
            String action = intent.getAction();
            if (action == null) return;
            
            switch (action) {
                case Intent.ACTION_BATTERY_CHANGED:
                    int level = getBatteryLevel();
                    if (nativeCallback != null && level >= 0) {
                        nativeCallback.onBatteryLevelChanged(level);
                    }
                    break;
                    
                case Intent.ACTION_POWER_CONNECTED:
                case Intent.ACTION_POWER_DISCONNECTED:
                    boolean charging = isCharging();
                    if (nativeCallback != null) {
                        nativeCallback.onChargingStateChanged(charging);
                    }
                    break;
            }
        }
    }
    
    private class PowerSaveBroadcastReceiver extends BroadcastReceiver {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (PowerManager.ACTION_POWER_SAVE_MODE_CHANGED.equals(intent.getAction())) {
                boolean powerSaveMode = isPowerSaveMode();
                if (nativeCallback != null) {
                    nativeCallback.onPowerSaveModeChanged(powerSaveMode);
                }
            }
        }
    }
}
