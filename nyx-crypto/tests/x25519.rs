use x25519_dalek::{x25519, X25519_BASEPOINT_BYTES};
use hex_literal::hex;

#[test]
fn rfc7748_x25519_shared_secret_matches() {
    // Alice private scalar (a) and Bob private scalar (b)
    let alice_priv_bytes = hex!("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    let bob_priv_bytes   = hex!("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
    // Note: the second vector line in RFC includes a different value, but we use one pair.

    let alice_public = x25519(alice_priv_bytes, X25519_BASEPOINT_BYTES);
    let bob_public   = x25519(bob_priv_bytes,   X25519_BASEPOINT_BYTES);

    let shared1 = x25519(alice_priv_bytes, bob_public);
    let shared2 = x25519(bob_priv_bytes,   alice_public);

    assert_eq!(shared1, shared2);

    // Expected shared secret from RFC 7748 (Section 6.1)
    let expected = hex!("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    assert_eq!(shared1, expected);
} 