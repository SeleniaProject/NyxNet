//! Verifiable Delay Mix (cMix) experimental module
//!
//! This proof-of-concept batches outgoing packets and releases them after a
//! fixed delay enforced by a *Verifiable Delay Function* (VDF). The POW-style
//! delay provides cryptographic verifiability that each mix node actually
//! waited the specified time (100 ms by default), making traffic analysis more
//! difficult.
//!
//! Implementation outline:
//! 1. Incoming packets are buffered until `batch_size` is reached **or**
//!    `max_wait` expires.
//! 2. A VDF (e.g. Wesolowski) is computed over the batch digest to generate a
//!    proof of delay. Here we only simulate the delay with `tokio::sleep` and
//!    attach a placeholder proof.
//! 3. The batch is shuffled (Fisher–Yates) and emitted downstream.
//!
//! **Feature flag**: `cmix_experimental`
//!
//! NOTE: This is *not* production-ready. Real VDF implementation and RSA
//! accumulator integration are TODO.

#![forbid(unsafe_code)]

use rand::seq::SliceRandom;
use rand::thread_rng;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration, Instant};
use crate::{accumulator::KeyCeremony, vdf};
use num_bigint::BigUint;

/// Default delay enforced by cMix (100 ms).
const DEFAULT_DELAY_MS: u64 = 100;
/// Maximum number of packets per batch.
const DEFAULT_BATCH: usize = 100;
/// VDF iterations per millisecond (calibrated empirically ~10k on modern CPU).
const VDF_ITERS_PER_MS: u64 = 10_000;

/// Resulting batch metadata.
#[derive(Debug, Clone)]
pub struct CmixBatch {
    pub packets: Vec<Vec<u8>>, // shuffled packets (shallow copy)
    pub digest: [u8; 32],      // SHA-256 digest of concatenated packets
    // Wesolowski VDF proof components
    pub vdf_y: Vec<u8>,        // y = x^{2^t} mod n
    pub vdf_pi: Vec<u8>,       // π = x^q mod n
    pub vdf_iters: u64,        // iteration count (t)
    // RSA accumulator data
    pub acc_value: Vec<u8>,    // current accumulator value A bytes
    pub witness: Vec<u8>,      // membership witness (pre-add accumulator)
}

/// cMix controller: receives packets via channel, outputs `CmixBatch` after delay.
pub struct CmixController {
    in_tx: mpsc::Sender<Vec<u8>>,
    out_rx: mpsc::Receiver<CmixBatch>,
    params: crate::accumulator::AccumulatorParams,
}

impl CmixController {
    /// Spawn a controller task.
    #[must_use]
    pub fn new(batch_size: usize, delay_ms: u64) -> Self {
        let (in_tx, mut in_rx) = mpsc::channel::<Vec<u8>>(1024);
        let (out_tx, out_rx) = mpsc::channel::<CmixBatch>(16);

        // Shared RSA accumulator parameters (single-party setup for now)
        let params = KeyCeremony::generate(2048);
        let params_cloned = params.clone();

        let delay = Duration::from_millis(delay_ms);
        let bsize = batch_size.max(1);

        tokio::spawn(async move {
            // Initialize shared RSA accumulator.
            let mut acc = crate::accumulator::RsaAccumulator::new(params_cloned.clone());
            let delay = Duration::from_millis(delay_ms);
            let bsize = batch_size.max(1);
            let mut buffer: Vec<Vec<u8>> = Vec::with_capacity(bsize);
            let mut next_deadline: Option<Instant> = None;
            loop {
                // Compute remaining time until deadline.
                let recv_fut = in_rx.recv();
                let packet_opt = if let Some(dl) = next_deadline {
                    match timeout(dl.saturating_duration_since(Instant::now()), recv_fut).await {
                        Ok(p) => p,
                        Err(_) => None, // timeout
                    }
                } else {
                    recv_fut.await
                };

                let no_packet = packet_opt.is_none();
                if let Some(pkt) = packet_opt {
                    buffer.push(pkt);
                    if buffer.len() == 1 { next_deadline = Some(Instant::now() + delay); }
                }

                let should_emit = buffer.len() >= bsize || no_packet && !buffer.is_empty();
                if should_emit {
                    // Simulate VDF delay (already elapsed by timeout).
                    buffer.shuffle(&mut thread_rng());
                    let mut hasher = Sha256::new();
                    for p in &buffer { hasher.update(p); }
                    let digest = hasher.finalize();
                    // VDF evaluation based on delay
                    let iters = delay_ms.saturating_mul(VDF_ITERS_PER_MS);
                    let x = BigUint::from_bytes_be(&digest);
                    let (y, pi) = vdf::prove_mont(&x, &params_cloned.n, iters);

                    // Update accumulator with hash_to_prime(digest)
                    let elem = crate::accumulator::hash_to_prime(&digest);
                    let witness = acc.add(&elem); // pre-add value
                    let acc_bytes = acc.value().to_bytes_be();

                    let batch = CmixBatch {
                        packets: buffer.clone(),
                        digest: digest.into(),
                        vdf_y: y.to_bytes_be(),
                        vdf_pi: pi.to_bytes_be(),
                        vdf_iters: iters,
                        acc_value: acc_bytes,
                        witness: witness.to_bytes_be(),
                    };
                    if out_tx.send(batch).await.is_err() { break; }
                    buffer.clear();
                    next_deadline = None;
                }
            }
        });
        Self { in_tx, out_rx, params }
    }

    /// Sender handle for incoming packets.
    #[must_use] pub fn sender(&self) -> mpsc::Sender<Vec<u8>> { self.in_tx.clone() }

    /// Receive next cMix batch.
    pub async fn recv(&mut self) -> Option<CmixBatch> { self.out_rx.recv().await }

    /// Access RSA accumulator public parameters for verification.
    #[must_use] pub fn params(&self) -> &crate::accumulator::AccumulatorParams { &self.params }
}

impl Default for CmixController {
    fn default() -> Self { Self::new(DEFAULT_BATCH, DEFAULT_DELAY_MS) }
}

/// Verify `CmixBatch` integrity: digest, VDF proof, and RSA accumulator membership.
pub fn verify_batch(batch: &CmixBatch, params: &crate::accumulator::AccumulatorParams, _expected_iters: Option<u64>) -> bool {
    use crate::accumulator::verify_membership;
    // Recompute digest from packets.
    let mut hasher = Sha256::new();
    for p in &batch.packets { hasher.update(p); }
    if hasher.finalize().as_slice() != &batch.digest {
        return false;
    }

    // Verify VDF proof.
    let x = BigUint::from_bytes_be(&batch.digest);
    let y = BigUint::from_bytes_be(&batch.vdf_y);
    let pi = BigUint::from_bytes_be(&batch.vdf_pi);
    if !vdf::verify(&x, &y, &pi, &params.n, batch.vdf_iters) {
        return false;
    }

    // Verify RSA accumulator membership.
    let elem = crate::accumulator::hash_to_prime(&batch.digest);
    let witness = BigUint::from_bytes_be(&batch.witness);
    let acc_val = BigUint::from_bytes_be(&batch.acc_value);
    verify_membership(&params.n, &elem, &witness, &acc_val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emits_batch_after_timeout() {
        let mut cmix = CmixController::new(10, 50);
        let tx = cmix.sender();
        tx.send(vec![1]).await.unwrap();
        // Expect batch within ~70ms
        let batch = cmix.recv().await.expect("no batch");
        assert_eq!(batch.packets.len(), 1);
        // Verify proofs
        assert!(verify_batch(&batch, cmix.params(), None));
    }
} 