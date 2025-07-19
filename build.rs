fn main() {
    let path = protoc_bin_vendored::protoc_bin_path()
        .expect("failed to locate vendored protoc");
    println!("cargo:rustc-env=PROTOC={}", path.display());
    println!("cargo:warning=Using vendored protoc at {}", path.display());
} 