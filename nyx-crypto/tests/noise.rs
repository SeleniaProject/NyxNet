use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};

#[test]
fn handshake_shared_secret_matches() {
    // Initiator step 1: generate e_i
    let (init_pub, init_sec) = initiator_generate();

    // Responder processes initiator pub
    let (resp_pub, shared_responder) = responder_process(&init_pub);

    // Initiator finalizes with responder pub
    let shared_initiator = initiator_finalize(init_sec, &resp_pub);

    // Both shared secrets must match
    assert_eq!(shared_initiator.as_bytes(), shared_responder.as_bytes());

    // Derive session keys and check equality
    let key_i = derive_session_key(&shared_initiator);
    let key_r = derive_session_key(&shared_responder);
    assert_eq!(key_i, key_r);
} 