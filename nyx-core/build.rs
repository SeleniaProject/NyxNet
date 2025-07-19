fn main() {
    let path = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    println!("cargo:warning=Using vendored protoc at {}", path.display());
    println!("cargo:rustc-env=PROTOC={}", path.display());
} 