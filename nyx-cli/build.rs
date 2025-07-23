use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile(&["../nyx-daemon/proto/control.proto"], &["../nyx-daemon/proto"])?;
    Ok(())
} 