#![no_main]
use libfuzzer_sys::fuzz_target;
use nyx_stream::{PluginHeader};

fuzz_target!(|data: &[u8]| {
    // Attempt to validate CBOR plugin frame; ignore errors.
    let _ = PluginHeader::validate(data);
}); 