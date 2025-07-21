use nyx_mix::vdf::{prove, verify};
use num_bigint::BigUint;

#[test]
fn wesolowski_vdf_conformance() {
    // small RSA modulus for fast test
    let n = BigUint::from(97u32) * BigUint::from(89u32);
    let x = BigUint::from(5u8);
    let t = 25;
    let (y, pi) = prove(&x, &n, t);
    assert!(verify(&x, &y, &pi, &n, t));
} 