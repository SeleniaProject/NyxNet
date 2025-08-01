use std::env;
use std::path::PathBuf;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    
    match target_os.as_str() {
        "ios" => {
            println!("cargo:rustc-link-lib=framework=UIKit");
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=framework=Network");
            println!("cargo:rustc-link-lib=framework=CoreTelephony");
            
            // Add search path for iOS FFI library
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static=nyx_mobile_ffi");
        }
        "android" => {
            println!("cargo:rustc-link-lib=log");
            println!("cargo:rustc-link-lib=android");
            
            // Add search path for Android FFI library
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static=nyx_mobile_ffi");
        }
        _ => {
            // Desktop platforms don't need mobile FFI linking
            println!("cargo:warning=Mobile FFI not available on {}", target_os);
        }
    }
    
    // Re-run build script if any of these files change
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../nyx-mobile-ffi/src/");
    println!("cargo:rerun-if-changed=../nyx-mobile-ffi/ios/");
    println!("cargo:rerun-if-changed=../nyx-mobile-ffi/android/");
}
