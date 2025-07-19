use nyx_crypto::{noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key}, pcr_rekey};

#[test]
fn pcr_rekey_changes_key() {
    // handshake to get shared key
    let (init_pub, init_sec) = initiator_generate();
    let (resp_pub, shared_resp) = responder_process(&init_pub);
    let shared_init = initiator_finalize(init_sec, &resp_pub);

    let mut key_a = derive_session_key(&shared_init);
    let key_b = derive_session_key(&shared_resp);
    assert_eq!(key_a.0, key_b.0);

    let next_a = pcr_rekey(&mut key_a);
    assert_ne!(next_a.0, key_b.0);
} 