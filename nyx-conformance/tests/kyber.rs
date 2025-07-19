#![cfg(feature = "pq")]
use nyx_crypto::noise::pq::{responder_keypair, initiator_encapsulate, responder_decapsulate};

#[test]
fn kyber_handshake_session_key_match() {
    let (pk, sk) = responder_keypair();
    let (ct, key_init) = initiator_encapsulate(&pk);
    let key_resp = responder_decapsulate(&ct, &sk);
    assert_eq!(key_init.0, key_resp.0);
} 