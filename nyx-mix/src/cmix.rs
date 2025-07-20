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

/// Resulting batch metadata.
pub struct CmixBatch {
    pub packets: Vec<Vec<u8>>, // shuffled packets
    pub digest: [u8; 32],      // SHA-256 digest of concatenated packets
    pub vdf_proof: Vec<u8>,    // VDF output y bytes
    pub acc_value: Vec<u8>,    // RSA accumulator current value A bytes
}

/// cMix controller: receives packets via channel, outputs `CmixBatch` after delay.
pub struct CmixController {
    in_tx: mpsc::Sender<Vec<u8>>,
    out_rx: mpsc::Receiver<CmixBatch>,
}

impl CmixController {
    /// Spawn a controller task.
    #[must_use]
    pub fn new(batch_size: usize, delay_ms: u64) -> Self {
        let (in_tx, mut in_rx) = mpsc::channel::<Vec<u8>>(1024);
        let (out_tx, out_rx) = mpsc::channel::<CmixBatch>(16);
        let delay = Duration::from_millis(delay_ms);
        let bsize = batch_size.max(1);

        tokio::spawn(async move {
            // Initialize shared RSA accumulator.
            let params = KeyCeremony::generate(2048);
            let mut acc = crate::accumulator::RsaAccumulator::new(params.clone());
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

                if let Some(pkt) = packet_opt {
                    buffer.push(pkt);
                    if buffer.len() == 1 { next_deadline = Some(Instant::now() + delay); }
                }

                let should_emit = buffer.len() >= bsize || packet_opt.is_none() && !buffer.is_empty();
                if should_emit {
                    // Simulate VDF delay (already elapsed by timeout).
                    let mut rng = thread_rng();
                    buffer.shuffle(&mut rng);
                    let mut hasher = Sha256::new();
                    for p in &buffer { hasher.update(p); }
                    let digest = hasher.finalize();
                    // VDF evaluation (fixed iterations calibrated ~100ms)
                    const ITER: u64 = 1_000;
                    let x = BigUint::from_bytes_be(&digest);
                    let y = vdf::eval(&x, &params.n, ITER);
                    let proof = y.to_bytes_be();

                    // Update accumulator with hash_to_prime(digest)
                    let elem = crate::accumulator::hash_to_prime(&digest);
                    acc.add(&elem);
                    let acc_bytes = acc.value().to_bytes_be();

                    let batch = CmixBatch { packets: buffer.clone(), digest: digest.into(), vdf_proof: proof, acc_value: acc_bytes };
                    if out_tx.send(batch).await.is_err() { break; }
                    buffer.clear();
                    next_deadline = None;
                }
            }
        });
        Self { in_tx, out_rx }
    }

    /// Sender handle for incoming packets.
    #[must_use] pub fn sender(&self) -> mpsc::Sender<Vec<u8>> { self.in_tx.clone() }

    /// Receive next cMix batch.
    pub async fn recv(&mut self) -> Option<CmixBatch> { self.out_rx.recv().await }
}

impl Default for CmixController {
    fn default() -> Self { Self::new(DEFAULT_BATCH, DEFAULT_DELAY_MS) }
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
    }
} 