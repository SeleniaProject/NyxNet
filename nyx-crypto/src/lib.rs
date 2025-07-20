#![forbid(unsafe_code)]

//! Nyx cryptography engine.
//!
//! This crate provides:
//! 1. Noise_Nyx handshake implementation (see [`noise`] module).
//! 2. HKDF wrappers with misuse-resistant label semantics.
//! 3. Optional Kyber1024 Post-Quantum fallback when built with `--features pq`.

use zeroize::Zeroize;

pub mod noise;
pub mod kdf;
pub mod hpke;
pub mod hybrid;
pub mod aead;

pub use kdf::KdfLabel;

/// Derive a new forward-secure session key from an existing key.
/// This provides post-compromise recovery (PCR) by hashing the current key
/// through HKDF with the dedicated `Rekey` label. The old key is zeroized
/// upon return.
pub fn pcr_rekey(old_key: &mut noise::SessionKey) -> noise::SessionKey {
    use kdf::{hkdf_expand, KdfLabel};
    let next_bytes = hkdf_expand(&old_key.0, KdfLabel::Rekey, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&next_bytes);
    // zeroize old key material before returning
    old_key.0.zeroize();
    noise::SessionKey(out)
}
