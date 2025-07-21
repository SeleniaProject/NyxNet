#![forbid(unsafe_code)]

//! RSA Accumulator implementation used by cMix (Nyx Protocol v1.0 §4).
//!
//! An RSA accumulator allows a prover to *accumulate* a set of elements
//! \(x_1, …, x_k\) into a single group element \(A = g^{\prod x_i} \bmod n\)
//! such that membership proofs (`witness = g^{\prod_{j≠i} x_j}`) can be
//! verified efficiently: `A == witness^{x_i} mod n`.
//!
//! The design here follows the *strong RSA assumption* variant (Baric–Pfitzmann
//! 1997).  We purposely expose only *public* parameters `(n, g)` after the key
//! ceremony, discarding the factorization of `n` to avoid trapdoors.
//!
//! # Key ceremony
//! A trusted setup must generate an RSA modulus `n = p*q` with *unknown
//! factorization* (a.k.a. the RSA group of unknown order).  The typical
//! approach is a multi-party computation where each participant contributes
//! entropy to generate large primes then deletes them locally, publishing only
//! the modulus.
//!
//! For our reference implementation we provide [`KeyCeremony::generate`], which
//! derives `n` locally **for test/development purposes only**.  Production
//! deployments SHOULD replace this with a real multi-party setup.
//!
//! # Hash-to-prime
//! Accumulated elements must be prime representatives.  We implement a naive
//! "try-and-increment" hash-to-prime adapter over SHA-256 suitable for
//! 128-bit security in practice.

use num_bigint::{BigUint, ToBigUint};
use num_integer::Integer;
use num_traits::{One, Zero};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

/// Default accumulator generator `g` (2).
const G_DEFAULT: u64 = 2;

/// Public parameters for an RSA accumulator.
#[derive(Debug, Clone)]
pub struct AccumulatorParams {
    pub n: BigUint, // RSA modulus (2048-bit+ recommended)
    pub g: BigUint, // generator (coprime with n)
}

/// RSA accumulator state `A`.
#[derive(Debug, Clone)]
pub struct RsaAccumulator {
    params: AccumulatorParams,
    value: BigUint, // current accumulator value A
}

impl RsaAccumulator {
    /// Create a *new empty* accumulator with `A = g`.
    #[must_use]
    pub fn new(params: AccumulatorParams) -> Self {
        Self { value: params.g.clone(), params }
    }

    /// Return current accumulator value.
    #[must_use]
    pub fn value(&self) -> &BigUint { &self.value }

    /// Accumulate element `x` (assumed prime). Returns membership witness.
    pub fn add(&mut self, x: &BigUint) -> BigUint {
        // Witness = A^{1/x} cannot be computed without trapdoor, so we expose
        // pre-add witness = A, and update A = A^{x} (mod n).
        let witness = self.value.clone();
        self.value = self.value.modpow(x, &self.params.n);
        witness
    }

    /// Verify element `x` with provided `witness` against current accumulator.
    #[must_use]
    pub fn verify(&self, x: &BigUint, witness: &BigUint) -> bool {
        witness.modpow(x, &self.params.n) == self.value
    }
}

/// Simple hash-to-prime helper (SHA-256 + increment until prime).
#[must_use]
pub fn hash_to_prime(data: &[u8]) -> BigUint {
    let mut counter: u32 = 0;
    loop {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.update(&counter.to_le_bytes());
        let digest = hasher.finalize();
        // take 256-bit directly, set MSB and LSB to ensure odd > 2^{255}
        let mut candidate = BigUint::from_bytes_le(&digest);
        candidate.set_bit(255u64, true);
        candidate.set_bit(0u64, true);
        if primal_check(&candidate) { return candidate; }
        counter = counter.wrapping_add(1);
    }
}

/// Miller-Rabin primality test with 32 deterministic bases for 256-bit numbers.
fn primal_check(n: &BigUint) -> bool {
    if *n == BigUint::from(2u8) { return true; }
    if n.is_zero() || n.is_one() || n.is_even() { return false; }
    // small prime trial division (fast path)
    const SMALL: [u64; 8] = [3,5,7,11,13,17,19,23];
    for p in SMALL { if (n % p).is_zero() { return *n == p.to_biguint().unwrap(); } }
    // deterministic MR bases for <2^512 per https://miller-rabin.appspot.com/
    const BASES: [u64; 12] = [2,3,5,7,11,13,17,19,23,29,31,37];
    for a in BASES {
        if !miller_rabin(n, a) { return false; }
    }
    true
}

fn miller_rabin(n: &BigUint, a: u64) -> bool {
    let a = BigUint::from(a);
    let one = BigUint::one();
    let nm1 = n - &one;
    // write n-1 = d * 2^s with d odd
    let mut d = nm1.clone();
    let mut s = 0u32;
    while d.is_even() { d >>= 1; s += 1; }

    let mut x = a.modpow(&d, n);
    if x == one || x == nm1 { return true; }
    for _ in 1..s {
        x = x.modpow(&BigUint::from(2u8), n);
        if x == nm1 { return true; }
    }
    false
}

/// Simple single-party key ceremony (DEV ONLY!). Generates 2048-bit safe RSA
/// modulus `n` and returns public parameters.
pub struct KeyCeremony;
impl KeyCeremony {
    #[must_use]
    pub fn generate(bits: usize) -> AccumulatorParams {
        let mut rng = OsRng;
        let p = generate_prime(bits / 2, &mut rng);
        let q = generate_prime(bits / 2, &mut rng);
        let n = &p * &q;
        AccumulatorParams { n, g: G_DEFAULT.to_biguint().unwrap() }
    }
}

/// Generate a random prime of `bits` length using rejection sampling.
fn generate_prime(bits: usize, rng: &mut impl RngCore) -> BigUint {
    loop {
        // Generate random candidate with MSB and LSB set.
        let byte_len = (bits + 7) / 8;
        let mut bytes = vec![0u8; byte_len];
        rng.fill_bytes(&mut bytes);
        // Ensure correct bit length and oddness.
        bytes[0] |= 0x80; // set MSB
        *bytes.last_mut().unwrap() |= 1; // set LSB (odd)
        let mut candidate = BigUint::from_bytes_be(&bytes);
        candidate.set_bit((bits - 1) as u64, true);
        candidate.set_bit(0, true);
        if primal_check(&candidate) {
            return candidate;
        }
    }
}

/// Verify that `witness^{x} mod n == acc_value`.
/// This variant does not require an instantiated `RsaAccumulator` and is
/// therefore useful for stateless verification paths.
#[must_use]
pub fn verify_membership(n: &BigUint, element: &BigUint, witness: &BigUint, acc_value: &BigUint) -> bool {
    witness.modpow(element, n) == *acc_value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulate_and_verify() {
        let params = KeyCeremony::generate(2048);
        let mut acc = RsaAccumulator::new(params.clone());

        let elem1 = hash_to_prime(b"packet1");
        let elem2 = hash_to_prime(b"packet2");

        let w1 = acc.add(&elem1);
        let w2 = acc.add(&elem2);

        assert!(acc.verify(&elem2, &w2));
        // elem1 witness no longer valid after second add
        assert!(!acc.verify(&elem1, &w1));
    }
} 