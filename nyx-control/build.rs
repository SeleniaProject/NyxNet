fn main() {
    let path = protoc_bin_vendored::protoc_bin_path().expect("protoc");
    println!("cargo:warning=Using vendored protoc at {}", path.display());
    std::env::set_var("PROTOC", &path);
} 