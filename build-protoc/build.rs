use std::{env, fs, path::Path};

fn main() {
    // If PROTOC already present, nothing to do.
    if env::var_os("PROTOC").is_some() {
        return;
    }

    let path = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc not found");
    let cargo_home = env::var("CARGO_HOME").unwrap_or_else(|_| {
        format!("{}/.cargo", env::var("HOME").expect("HOME unset"))
    });
    let dest = Path::new(&cargo_home).join("bin").join("protoc");
    fs::create_dir_all(dest.parent().unwrap()).expect("mkdir bin");
    if !dest.exists() {
        fs::copy(&path, &dest).expect("copy protoc");
    }
    println!("cargo:rustc-env=PROTOC={}", dest.display());
} 