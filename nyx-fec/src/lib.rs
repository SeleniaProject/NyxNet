#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Reed-Solomon FEC wrapper for Nyx fixed-length 1280-byte packets.
//! Default parameters: data shards = 10, parity shards = 3 (≈30% overhead).

use reed_solomon_erasure::{galois_8::ReedSolomon, Error as RSError};

pub mod timing;
pub use timing::{TimingObfuscator, TimingConfig, Packet};

pub const DATA_SHARDS: usize = 10;
pub const PARITY_SHARDS: usize = 3;
pub const SHARD_SIZE: usize = 1280; // One Nyx packet per shard.

#[cfg(feature = "simd")]
#[cfg_attr(docsrs, doc(cfg(feature = "simd")))]
/// Compile-time feature flag enabling SIMD-accelerated encoding via C backend.
pub const SIMD_ACCEL_ENABLED: bool = true;
#[cfg(not(feature = "simd"))]
pub const SIMD_ACCEL_ENABLED: bool = false;

/// Nyx FEC codec.
pub struct NyxFec {
    rs: ReedSolomon, // GF(2^8) codec
}

impl NyxFec {
    /// Create codec with default parameters.
    pub fn new() -> Self {
        let rs = ReedSolomon::new(DATA_SHARDS, PARITY_SHARDS).expect("valid params");
        Self { rs }
    }

    /// Encode data shards in-place. `shards` must be length DATA_SHARDS + PARITY_SHARDS.
    /// First DATA_SHARDS entries are original data; remaining must be zero-filled mutable buffers.
    pub fn encode(&self, shards: &mut [&mut [u8]]) -> Result<(), RSError> {
        self.rs.encode(shards)
    }

    /// Attempt to reconstruct missing shards.
    ///
    /// `present` is a parallel boolean slice indicating which shards are intact.
    pub fn reconstruct(&self, shards: &mut [&mut [u8]], present: &mut [bool]) -> Result<(), RSError> {
        let mut tuples: Vec<(&mut [u8], bool)> = shards
            .iter_mut()
            .enumerate()
            .map(|(i, s)| (&mut **s, present[i]))
            .collect();
        self.rs.reconstruct(&mut tuples)
    }

    /// Verify parity for provided shards without reconstruction.
    pub fn verify(&self, shards: &[&[u8]]) -> Result<bool, RSError> {
        self.rs.verify(shards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_shards() -> Vec<Vec<u8>> {
        (0..DATA_SHARDS)
            .map(|i| vec![i as u8; SHARD_SIZE])
            .collect()
    }

    #[test]
    fn encode_and_reconstruct() {
        let codec = NyxFec::new();
        let mut shards: Vec<Vec<u8>> = make_shards();
        // Add parity buffers
        shards.extend((0..PARITY_SHARDS).map(|_| vec![0u8; SHARD_SIZE]));

        // Mutable slice array
        let mut mut_slices: Vec<&mut [u8]> = shards.iter_mut().map(|v| v.as_mut_slice()).collect();
        codec.encode(&mut mut_slices).unwrap();
        let verify_vec = mut_slices.iter().map(|s| &**s).collect::<Vec<&[u8]>>();
        assert!(codec.verify(&verify_vec).unwrap());

        // Zero out two data shards to simulate loss
        let mut present: Vec<bool> = vec![true; DATA_SHARDS + PARITY_SHARDS];
        mut_slices[1].fill(0);
        mut_slices[5].fill(0);
        present[1] = false;
        present[5] = false;

        codec.reconstruct(&mut mut_slices, &mut present).unwrap();
        let verify_vec2 = mut_slices.iter().map(|s| &**s).collect::<Vec<&[u8]>>();
        assert!(codec.verify(&verify_vec2).unwrap());
    }
}
