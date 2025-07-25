#![forbid(unsafe_code)]

//! Noise_Nyx handshake implementation.
//!
//! This module now provides the full handshake including session key derivation.
//! Frame-level payload encryption is implemented separately in [`crate::aead`]
//! using ChaCha20-Poly1305 and a BLAKE3-based HKDF construct as mandated by the
//! Nyx Protocol v0.1/1.0 specifications.

#[cfg(feature = "classic")]
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
// no external RNG needed; use EphemeralSecret::random()
use super::kdf::{hkdf_expand, KdfLabel};
use zeroize::Zeroize;
#[cfg(feature = "classic")]
use rand_core_06::OsRng;
// rand_core 0.6 static RNG used internally by EphemeralSecret::random()

#[cfg(feature = "classic")]
/// Initiator generates ephemeral X25519 key.
pub fn initiator_generate() -> (PublicKey, EphemeralSecret) {
    let mut rng = OsRng;
    let secret = EphemeralSecret::random_from_rng(&mut rng);
    let public = PublicKey::from(&secret);
    (public, secret)
}

#[cfg(feature = "classic")]
/// Responder process for X25519.
pub fn responder_process(in_pub: &PublicKey) -> (PublicKey, SharedSecret) {
    let mut rng = OsRng;
    let secret = EphemeralSecret::random_from_rng(&mut rng);
    let public = PublicKey::from(&secret);
    let shared = secret.diffie_hellman(in_pub);
    (public, shared)
}

#[cfg(feature = "classic")]
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

#[cfg(feature = "kyber")]
pub mod kyber {
    //! Kyber1024 KEM wrapper providing the same interface semantics as the X25519
    //! Noise_Nyx handshake. When the `pq` feature is enabled at compile-time,
    //! callers can switch to these APIs to negotiate a 32-byte session key that
    //! is derived from the Kyber shared secret via the common HKDF wrapper to
    //! ensure uniform key derivation logic across classic and PQ modes.

    use pqcrypto_kyber::kyber1024::{keypair, encapsulate, decapsulate, SharedSecret};
    use crate::kdf::{hkdf_expand, KdfLabel};
    use pqcrypto_traits::kem::SharedSecret as _; // bring trait for as_bytes
    // Re-export commonly used Kyber types for external modules (Hybrid handshake, etc.).
    pub use pqcrypto_kyber::kyber1024::{PublicKey, SecretKey, Ciphertext};

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

#[cfg(feature = "bike")]
pub mod bike {
    //! BIKE post-quantum KEM integration is planned but the required Rust
    //! crate is not yet available on crates.io. Attempting to compile with
    //! `--features bike` will therefore raise a compile-time error.
    compile_error!("Feature `bike` is not yet supported – awaiting upstream pqcrypto-bike crate");
}

/// -----------------------------------------------------------------------------
/// Hybrid X25519 + Kyber Handshake (feature "hybrid")
/// -----------------------------------------------------------------------------
#[cfg(feature = "hybrid")]
pub mod hybrid {
    use super::*;
    use super::kyber; // Kyber helpers
    use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
    use rand_core_06::OsRng;

    /// Initiator generates X25519 ephemeral and Kyber encapsulation.
    pub fn initiator_step(pk_kyber: &kyber::PublicKey) -> (PublicKey, EphemeralSecret, kyber::Ciphertext, SessionKey) {
        let (ct, kyber_key) = kyber::initiator_encapsulate(pk_kyber);
        let mut rng = OsRng;
        let secret = EphemeralSecret::random_from_rng(&mut rng);
        let public = PublicKey::from(&secret);
        // Combine secrets later when responder key known; here return Kyber part as session key placeholder.
        let k = kyber_key;
        (public, secret, ct, k)
    }

    /// Responder receives initiator public keys and ciphertext; returns responder X25519 pub and combined session key.
    pub fn responder_step(init_pub: &PublicKey, ct: &kyber::Ciphertext, sk_kyber: &kyber::SecretKey) -> (PublicKey, SessionKey) {
        // Kyber part
        let kyber_key = kyber::responder_decapsulate(ct, sk_kyber);
        // X25519 part
        let mut rng = OsRng;
        let secret = EphemeralSecret::random_from_rng(&mut rng);
        let public = PublicKey::from(&secret);
        let x_key = secret.diffie_hellman(init_pub);
        // Combine
        combine_keys(&x_key, &kyber_key)
            .map(|k| (public, k))
            .unwrap()
    }

    /// Initiator finalizes with responder X25519 pub, producing combined session key.
    pub fn initiator_finalize(sec: EphemeralSecret, resp_pub: &PublicKey, kyber_key: SessionKey) -> SessionKey {
        let x_key = sec.diffie_hellman(resp_pub);
        combine_keys(&x_key, &kyber_key).expect("hkdf")
    }

    fn combine_keys(classic: &SharedSecret, pq: &SessionKey) -> Option<SessionKey> {
        use zeroize::Zeroize;
        let mut concat = Vec::with_capacity(64);
        concat.extend_from_slice(classic.as_bytes());
        concat.extend_from_slice(&pq.0);
        let okm = hkdf_expand(&concat, KdfLabel::Session, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&okm);
        // zeroize temp
        concat.zeroize();
        Some(SessionKey(out))
    }
} 