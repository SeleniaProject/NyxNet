use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};

#[test]
fn noise_nyx_handshake_vector() {
    // Initiator generates e
    let (init_pub, init_sec) = initiator_generate();

    // Responder processes and returns its pub & shared secret
    let (resp_pub, shared_resp) = responder_process(&init_pub);

    // Initiator finalizes
    let shared_init = initiator_finalize(init_sec, &resp_pub);

    // Derive symmetric keys
    let key_init = derive_session_key(&shared_init);
    let key_resp = derive_session_key(&shared_resp);

    assert_eq!(key_init.0, key_resp.0, "Session keys must match");
} 