#![forbid(unsafe_code)]

use rand_distr::{Distribution, Exp1};
use std::time::Duration;

/// Poisson cover-traffic interval generator.
/// λ = target bandwidth factor (events per second).
#[derive(Clone)]
pub struct CoverGenerator {
    pub(crate) lambda: f64,
}

impl CoverGenerator {
    pub fn new(lambda: f64) -> Self {
        Self { lambda }
    }

    /// Sample next delay duration before sending a cover packet.
    pub fn next_delay(&self) -> Duration {
        // Exponential distribution with mean 1/λ.
        let sample: f64 = Exp1.sample(&mut rand::thread_rng());
        let delay = sample / self.lambda;
        Duration::from_secs_f64(delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_close_to_inverse_lambda() {
        let gen = CoverGenerator::new(10.0); // λ=10 events/s
        let mut acc = 0.0;
        let n = 10_000;
        for _ in 0..n {
            acc += gen.next_delay().as_secs_f64();
        }
        let mean = acc / n as f64;
        // Expected mean around 0.1 s, allow 10% tolerance
        assert!((mean - 0.1).abs() < 0.02);
    }
} 