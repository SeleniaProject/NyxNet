#![forbid(unsafe_code)]

//! Noise_Nyx handshake (暫定簡易版)。
//!
//! 本実装は仕様準拠の完全版ではなく、`x25519-dalek` と HKDF を用いた **PoC** です。
//! 今後、ChaCha20-Poly1305／BLAKE3 で暗号化したフレーム交換を実装し、
//! Kyber1024 フォールバックも feature `pq` で切り替えられるよう拡張します。

#[cfg(not(feature = "pq_only"))]
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
#[cfg(not(feature = "pq_only"))]
use rand_core::OsRng;
use super::kdf::{hkdf_expand, KdfLabel};
use zeroize::Zeroize;

#[cfg(not(feature = "pq_only"))]
/// Initiator generates ephemeral X25519 key.
pub fn initiator_generate() -> (PublicKey, EphemeralSecret) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (public, secret)
}

#[cfg(not(feature = "pq_only"))]
/// Responder process for X25519.
pub fn responder_process(in_pub: &PublicKey) -> (PublicKey, SharedSecret) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    let shared = secret.diffie_hellman(in_pub);
    (public, shared)
}

#[cfg(not(feature = "pq_only"))]
/// Initiator finalize X25519.
pub fn initiator_finalize(sec: EphemeralSecret, resp_pub: &PublicKey) -> SharedSecret {
    sec.diffie_hellman(resp_pub)
}

/// 32-byte Nyx session key that zeroizes on drop.
#[derive(Debug, Zeroize)]
#[zeroize(drop)]
pub struct SessionKey(pub [u8; 32]);

pub fn derive_session_key(shared: &SharedSecret) -> SessionKey {
    let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    SessionKey(out)
}

// -----------------------------------------------------------------------------
// Kyber1024 Post-Quantum fallback (feature "pq")
// -----------------------------------------------------------------------------

#[cfg(feature = "pq")]
pub mod pq {
    //! Kyber1024 KEM wrapper providing the same interface semantics as the X25519
    //! Noise_Nyx handshake. When the `pq` feature is enabled at compile-time,
    //! callers can switch to these APIs to negotiate a 32-byte session key that
    //! is derived from the Kyber shared secret via the common HKDF wrapper to
    //! ensure uniform key derivation logic across classic and PQ modes.

    use pqcrypto_kyber::kyber1024::{keypair, encapsulate, decapsulate, PublicKey, SecretKey, Ciphertext, SharedSecret};
    use crate::kdf::{hkdf_expand, KdfLabel};
    use pqcrypto_traits::kem::SharedSecret as _; // bring trait for as_bytes

    /// Generate a Kyber1024 keypair for the responder.
    pub fn responder_keypair() -> (PublicKey, SecretKey) {
        keypair()
    }

    /// Initiator encapsulates to responder's public key, returning the
    /// ciphertext to transmit and the derived 32-byte session key.
    pub fn initiator_encapsulate(pk: &PublicKey) -> (Ciphertext, super::SessionKey) {
        let (ss, ct) = encapsulate(pk);
        (ct, derive_session_key(&ss))
    }

    /// Responder decapsulates ciphertext with its secret key and derives the
    /// matching 32-byte session key.
    pub fn responder_decapsulate(ct: &Ciphertext, sk: &SecretKey) -> super::SessionKey {
        let ss = decapsulate(ct, sk);
        derive_session_key(&ss)
    }

    /// Convert Kyber shared secret into Nyx session key via HKDF.
    fn derive_session_key(shared: &SharedSecret) -> super::SessionKey {
        let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&okm);
        super::SessionKey(out)
    }
} 