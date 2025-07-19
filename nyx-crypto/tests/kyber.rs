#![cfg(feature = "pq")]

use nyx_crypto::noise::pq::{responder_keypair, initiator_encapsulate, responder_decapsulate};

#[test]
fn kyber_kem_session_key_matches() {
    // Responder generates keypair
    let (pk, sk) = responder_keypair();

    // Initiator encapsulates
    let (ct, key_i) = initiator_encapsulate(&pk);

    // Responder decapsulates
    let key_r = responder_decapsulate(&ct, &sk);

    // Session keys must match
    assert_eq!(key_i.0, key_r.0);
} 