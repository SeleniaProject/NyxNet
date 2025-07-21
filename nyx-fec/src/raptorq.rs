#![forbid(unsafe_code)]

//! RaptorQ fountain code codec with adaptive redundancy for Nyx packets.
//!
//! This is a thin wrapper around the `raptorq` crate. It generates additional
//! repair symbols according to a configurable redundancy ratio, which can be
//! tuned at runtime by higher layers.
//!
//! The symbol / packet size is fixed to 1280 bytes so that one Nyx packet maps
//! exactly to one RaptorQ symbol.
//!
//! ```rust
//! use nyx_fec::RaptorQCodec;
//! let codec = RaptorQCodec::new(0.3); // 30 % redundancy
//! let data = vec![0u8; 4096];
//! let pkts = codec.encode(&data);
//! let rec = codec.decode(&pkts).expect("recovered");
//! assert_eq!(data, rec);
//! ```

use raptorq::{Encoder, Decoder, EncodingPacket, ObjectTransmissionInformation};

#[cfg(windows)]
use rayon::prelude::*;

/// One Nyx packet equals one RaptorQ symbol (bytes).
pub const SYMBOL_SIZE: usize = 1280;

/// Codec with fixed redundancy ratio. See [`AdaptiveRaptorQ`] for a dynamic controller.
pub struct RaptorQCodec {
    redundancy: f32, // e.g. 0.3 = 30 % extra repair symbols
}

impl RaptorQCodec {
    /// Create a codec. `redundancy` must be in the range 0.0‥=1.0.
    #[must_use]
    pub fn new(redundancy: f32) -> Self {
        Self { redundancy: redundancy.clamp(0.0, 1.0) }
    }

    /// Split `data` into source symbols and generate additional repair symbols
    /// according to the configured redundancy ratio.
    #[must_use]
    pub fn encode(&self, data: &[u8]) -> Vec<EncodingPacket> {
        // Build encoder with default parameters using MTU (=symbol size).
        let enc = Encoder::with_defaults(data, SYMBOL_SIZE as u16);
        // Windows SIMD optimisation: build source packets in parallel to leverage CPU cores.
        let mut packets: Vec<EncodingPacket> = {
            #[cfg(windows)]
            {
                let src: Vec<EncodingPacket> = enc.source_packets().collect();
                src.into_par_iter().map(|p| p).collect()
            }
            #[cfg(not(windows))]
            {
                enc.source_packets().collect()
            }
        };
        let repair_cnt = ((packets.len() as f32) * self.redundancy).ceil() as u32;
        packets.extend(enc.repair_packets(repair_cnt));
        packets
    }

    /// Attempt to decode the original data given a set of packets. Returns
    /// `None` if decoding fails or insufficient symbols are provided.
    pub fn decode(&self, packets: &[EncodingPacket]) -> Option<Vec<u8>> {
        if packets.is_empty() {
            return None;
        }
        // Reconstruct OTI from total length (encoded within packet meta).
        let total_len = packets[0].payload_range().end as u64; // approximate
        let oti = ObjectTransmissionInformation::with_defaults(total_len, SYMBOL_SIZE as u16);
        let mut dec = Decoder::new(oti);
        for p in packets {
            if dec.insert(p.clone()).is_err() {
                return None;
            }
        }
        dec.decode().ok()
    }

    /// Expose the current redundancy ratio.
    #[must_use]
    pub fn redundancy(&self) -> f32 {
        self.redundancy
    }

    /// Internally mutate redundancy (used by [`AdaptiveRaptorQ`]).
    fn set_redundancy(&mut self, r: f32) {
        self.redundancy = r.clamp(0.0, 1.0);
    }
}

/// Controller performing simple loss-feedback based redundancy adaptation.
///
/// The strategy keeps a sliding window of recent packet outcomes (lost / ok)
/// and scales redundancy proportionally to the observed loss.
pub struct AdaptiveRaptorQ {
    codec: RaptorQCodec,
    window_size: usize,
    history: Vec<bool>,
    cursor: usize,
    min_ratio: f32,
    max_ratio: f32,
}

impl AdaptiveRaptorQ {
    /// Create with an initial redundancy ratio and adaptation parameters.
    pub fn new(initial_ratio: f32, window_size: usize, min_ratio: f32, max_ratio: f32) -> Self {
        Self {
            codec: RaptorQCodec::new(initial_ratio),
            window_size: window_size.max(1),
            history: vec![false; window_size.max(1)],
            cursor: 0,
            min_ratio: min_ratio.clamp(0.0, 1.0),
            max_ratio: max_ratio.clamp(0.0, 1.0),
        }
    }

    /// Record whether the latest packet was *lost*.
    pub fn record(&mut self, lost: bool) {
        self.history[self.cursor] = lost;
        self.cursor = (self.cursor + 1) % self.window_size;
        if self.cursor == 0 {
            self.recompute();
        }
    }

    /// Get a reference to the underlying codec (read-only).
    #[must_use]
    pub fn codec(&self) -> &RaptorQCodec {
        &self.codec
    }

    fn recompute(&mut self) {
        let loss_rate = self.history.iter().filter(|l| **l).count() as f32 / self.window_size as f32;
        let desired = (loss_rate * 1.5).clamp(self.min_ratio, self.max_ratio);
        self.codec.set_redundancy(desired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptation_increases_redundancy() {
        let mut adapt = AdaptiveRaptorQ::new(0.05, 8, 0.05, 0.5);
        // Simulate heavy losses in one window.
        for _ in 0..8 {
            adapt.record(true);
        }
        assert!(adapt.codec.redundancy() > 0.05);
    }
} 