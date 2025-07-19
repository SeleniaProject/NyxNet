#![no_main]
use libfuzzer_sys::fuzz_target;
use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize};

fuzz_target!(|data: &[u8]| {
    // Not using data yet; ensures handshake functions are side-effect free and crash-safe.
    if data.len() < 1 { return; }
    let (init_pub, init_sec) = initiator_generate();
    let (resp_pub, _shared_resp) = responder_process(&init_pub);
    let _shared_init = initiator_finalize(init_sec, &resp_pub);
}); 