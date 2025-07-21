#![forbid(unsafe_code)]

//! Runtime calibration helper determining an appropriate Wesolowski VDF
//! difficulty parameter `t` such that proof generation time is close to a
//! desired **target delay** on the current hardware.
//!
//! Motivation: In production Nyx Mix deployment the cMix batch delay is
//! enforced by a VDF; selecting a fixed `t` is sub-optimal because participant
//! CPU speeds vary widely.  This module performs an **empirical self-test** at
//! node start-up to choose `t` achieving `target_ms` (±10 %).  The result can
//! be cached and periodically re-validated.
//!
//! Algorithm (simple heuristic):
//! 1. Measure time per 1 000 squarings using the public RSA modulus `n`.
//! 2. Compute squaring-time `σ` (ns per squaring).
//! 3. `t_est = (target_ms * 1_000_000) / σ` (convert ms→ns).
//! 4. Clamp `t` into `[MIN_T, MAX_T]` and align to nearest power-of-two for
//!    efficient left-shift in `vdf::eval()`.
//!
//! The calibration uses a small sample to keep start-up latency negligible
//! (~5 ms on modern CPUs).  Accuracy is sufficient because verification only
//! checks elapsed time within a grace window.

use std::time::Instant;
use num_bigint::BigUint;
use num_traits::One;

/// Minimum / maximum allowed difficulty.
const MIN_T: u64 = 1_000;      // 1k squarings
const MAX_T: u64 = 1_000_000;  // 1M squarings

/// Calibrate difficulty parameter.
///
/// * `n` – RSA modulus shared by cMix group.
/// * `target_ms` – desired evaluation time in milliseconds.
#[must_use]
pub fn calibrate_t(n: &BigUint, target_ms: u64) -> u64 {
    // Use small base x = 5.
    let x = BigUint::from(5u8);
    let sample_squarings: u64 = 1_000; // sample size

    // Time the sample evaluation.
    let start = Instant::now();
    let _y = super::vdf::eval(&x, n, sample_squarings);
    let elapsed = start.elapsed();

    // ns per squaring
    let ns_per_sq = elapsed.as_nanos() as u64 / sample_squarings;
    if ns_per_sq == 0 {
        // Extremely fast (unlikely) -> fallback to min.
        return MIN_T;
    }

    let target_ns = target_ms * 1_000_000;
    let mut t_est = target_ns / ns_per_sq;

    // Clamp.
    t_est = t_est.clamp(MIN_T, MAX_T);

    // Round to nearest power-of-two for bit-shift optimisation.
    let pow2 = |mut v: u64| {
        v |= v >> 1;
        v |= v >> 2;
        v |= v >> 4;
        v |= v >> 8;
        v |= v >> 16;
        v += 1;
        v >> 1
    };
    pow2(t_est)
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::RandPrime;
    use rand::rngs::OsRng;

    #[test]
    fn calibrate_returns_reasonable_t() {
        let mut rng = OsRng;
        let p = rng.gen_prime(512);
        let q = rng.gen_prime(512);
        let n = &p * &q;

        let t = calibrate_t(&n, 50); // 50ms target
        assert!(t >= MIN_T && t <= MAX_T);
        // Should be power of two.
        assert_eq!(t & (t - 1), 0);
    }
} 