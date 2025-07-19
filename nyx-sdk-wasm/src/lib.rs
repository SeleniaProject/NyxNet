use wasm_bindgen::prelude::*;
use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};

#[wasm_bindgen]
pub fn noise_handshake_demo() -> String {
    // Simple demo performing Noise_Nyx X25519 handshake in wasm.
    let (init_pub, init_sec) = initiator_generate();
    let (resp_pub, shared_resp) = responder_process(&init_pub);
    let shared_init = initiator_finalize(init_sec, &resp_pub);
    assert_eq!(shared_init.as_bytes(), shared_resp.as_bytes());
    let key = derive_session_key(&shared_init);
    hex::encode(key.0)
} 