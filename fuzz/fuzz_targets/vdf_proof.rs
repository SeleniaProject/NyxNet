#![no_main]
use libfuzzer_sys::fuzz_target;
use nyx_mix::vdf::{verify, prove};
use num_bigint::BigUint;

fuzz_target!(|input: (u64, u8)| {
    let (t, seed) = input;
    // Limit difficulty parameter
    let t = t % 128 + 1;
    // Simple RSA modulus n = p*q with small primes for fuzzing
    let p = BigUint::from(1009u32);
    let q = BigUint::from(1013u32);
    let n = &p * &q;
    let x = BigUint::from(seed as u32 + 2);
    let (y, pi) = prove(&x, &n, t);
    let _ = verify(&x, &y, &pi, &n, t);
}); 