use std::{env, fs, path::Path};

fn main() {
    // If PROTOC already set (e.g. by system install or .cargo/config), respect it.
    if env::var_os("PROTOC").is_some() {
        return;
    }

    // Resolve vendored protoc binary provided by protoc-bin-vendored.
    let path = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");

    // Destination: $CARGO_HOME/bin/protoc so it matches .cargo/config.toml value.
    let cargo_home = env::var("CARGO_HOME").unwrap_or_else(|_| {
        // Default fallback as cargo does internally: $HOME/.cargo
        format!("{}/.cargo", env::var("HOME").expect("HOME unset"))
    });
    let dest = Path::new(&cargo_home).join("bin").join("protoc");
    fs::create_dir_all(dest.parent().unwrap()).expect("create cargo bin dir");

    if !dest.exists() {
        fs::copy(&path, &dest).expect("copy vendored protoc");
    }

    // Ensure downstream build scripts see the path.
    println!("cargo:rustc-env=PROTOC={}", dest.display());
    println!("cargo:warning=Using vendored protoc at {}", dest.display());
} 