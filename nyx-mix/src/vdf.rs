#![forbid(unsafe_code)]

//! Simplified Wesolowski VDF evaluator.
//!
//! This development-grade implementation repeatedly squares an input value
//! `x` modulo an RSA modulus `n` for `t` iterations and returns the result
//! `y = x^{2^t} mod n`.
//!
//! *Verification* recomputes the same squaring chain and compares against the
//! provided `y`.  While this makes verification as costly as evaluation (unlike
//! the real scheme which uses a short proof), it is sufficient for ensuring a
//! deterministic ≥100 ms CPU delay in the reference cMix batcher.
//!
//! **WARNING**: Not constant-time and *NOT* secure for production.  Replace with
//! a proper Wesolowski or Pietrzak implementation backed by a formal VDF crate
//! before main-net deployment.

use num_bigint::BigUint;
use num_traits::One;

/// Perform repeated squaring: `y = x^{2^t} mod n`.
pub fn eval(x: &BigUint, n: &BigUint, t: u64) -> BigUint {
    let mut y = x.clone();
    for _ in 0..t {
        y = y.modpow(&BigUint::from(2u8), n);
    }
    y
}

/// Verify that `y` equals `x^{2^t} mod n`.
#[must_use]
pub fn verify(x: &BigUint, y: &BigUint, n: &BigUint, t: u64) -> bool {
    eval(x, n, t) == *y
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::RandPrime;
    use rand::rngs::OsRng;

    #[test]
    fn vdf_eval_verify() {
        let mut rng = OsRng;
        let p = rng.gen_prime(512);
        let q = rng.gen_prime(512);
        let n = &p * &q;
        let x = BigUint::from(5u8);
        let y = eval(&x, &n, 20);
        assert!(verify(&x, &y, &n, 20));
    }
} 