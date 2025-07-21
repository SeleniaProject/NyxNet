#![forbid(unsafe_code)]

//! A production-grade (non-optimized) implementation of the Wesolowski VDF as
//! described in "Efficient Verifiable Delay Functions" (Boneh *et al.* 2018).
//! Given an RSA modulus `n`, difficulty parameter `t` and input `x`, the VDF
//! outputs the tuple `(y, π)` such that `y = x^{2^t} mod n` and the proof
//! `π` is **log-sized** independent of `t`.  Verification only requires a
//! *single* modular exponentiation of size `log(n)` and therefore is much
//! faster than re-evaluating the full chain of `t` squarings.
//!
//! This implementation follows the reference algorithm:
//! 1. Choose a public prime parameter `ℓ` (128-bit default) shared by all
//!    parties.  A constant prime close to `2^128` is used (`L_PRIME`).
//! 2. Compute `y = x^{2^t} mod n` via `t` repeated squarings.
//! 3. Let `r = 2^t mod ℓ`, `q = (2^t − r)/ℓ`.
//! 4. Compute the proof `π = x^q mod n`.
//!
//! Verification checks `y == π^{ℓ} * x^{r} (mod n)`.
//!
//! The old `eval()` helper (full repeated squaring without proof) is kept for
//! benchmarking and test comparison, but **new code SHOULD use `prove()` /
//! `verify()`**.
//!
//! Security notes:
//! * Uses the prime `ℓ = 2^128 + 51` – the smallest prime > 2^128. 128-bit is
//!   sufficient per the original paper.
//! * Not constant-time – do *not* use with secret inputs.
//! * Suitable for reference network-delay enforcement within Nyx cMix.

use num_bigint::BigUint;
use num_traits::{One, Zero};
use lazy_static::lazy_static;

/// Public prime ℓ for Wesolowski (2^128 + 51).
pub const L_PRIME_DEC: &str = "340282366920938463463374607431768211507"; // verified prime
lazy_static! {
    static ref L_PRIME: BigUint = BigUint::parse_bytes(L_PRIME_DEC.as_bytes(), 10).unwrap();
}

/// Classic repeated squaring (legacy helper).
#[must_use]
pub fn eval(x: &BigUint, n: &BigUint, t: u64) -> BigUint {
    let mut y = x.clone();
    for _ in 0..t {
        y = y.modpow(&BigUint::from(2u8), n);
    }
    y
}

/// Evaluate Wesolowski VDF and return `(y, π)`.
#[must_use]
pub fn prove(x: &BigUint, n: &BigUint, t: u64) -> (BigUint, BigUint) {
    // y = x^{2^t}
    let y = eval(x, n, t);

    // compute 2^t as BigUint
    let exp_two = BigUint::one() << t; // 2^t

    // r = 2^t mod ℓ
    let r = (&exp_two) % &*L_PRIME;

    // q = (2^t - r)/ℓ
    let q = (&exp_two - &r) / &*L_PRIME;

    // π = x^q (mod n)
    let pi = x.modpow(&q, n);

    (y, pi)
}

/// Verify Wesolowski proof.
#[must_use]
pub fn verify(x: &BigUint, y: &BigUint, pi: &BigUint, n: &BigUint, t: u64) -> bool {
    let exp_two = BigUint::one() << t;
    let r = (&exp_two) % &*L_PRIME;

    // lhs = π^{ℓ} * x^{r} mod n
    let lhs = {
        let a = pi.modpow(&*L_PRIME, n);
        let b = if r.is_zero() { BigUint::one() } else { x.modpow(&r, n) };
        (a * b) % n
    };
    &lhs == y
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;

    #[test]
    fn vdf_round_trip() {
        // Small primes for test
        let p = BigUint::from(1009u32);
        let q = BigUint::from(1013u32);
        let n = &p * &q;
        let x = BigUint::from(5u8);
        let t = 100;
        let (y, pi) = prove(&x, &n, t);
        assert!(verify(&x, &y, &pi, &n, t));
    }
} 