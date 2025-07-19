#![forbid(unsafe_code)]

//! Noise_Nyx handshake (暫定簡易版)。
//!
//! 本実装は仕様準拠の完全版ではなく、`x25519-dalek` と HKDF を用いた **PoC** です。
//! 今後、ChaCha20-Poly1305／BLAKE3 で暗号化したフレーム交換を実装し、
//! Kyber1024 フォールバックも feature `pq` で切り替えられるよう拡張します。

use rand_core::OsRng;
use x25519_dalek::{EphemeralSecret, PublicKey, SharedSecret};
use super::kdf::{hkdf_expand, KdfLabel};

/// Initiator が生成するハンドシェイクメッセージ (e 公開鍵) と秘密状態を返す。
pub fn initiator_generate() -> (PublicKey, EphemeralSecret) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (public, secret)
}

/// Responder が受信した Initiator の公開鍵を処理し、自身の公開鍵/共有鍵を返す。
pub fn responder_process(in_pub: &PublicKey) -> (PublicKey, SharedSecret) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    let shared = secret.diffie_hellman(in_pub);
    (public, shared)
}

/// Initiator が Responder 公開鍵を受信後に共有鍵を計算する。
pub fn initiator_finalize(sec: EphemeralSecret, resp_pub: &PublicKey) -> SharedSecret {
    sec.diffie_hellman(resp_pub)
}

/// Derive a 32-byte session key from the shared secret using the misuse-resistant HKDF wrapper.
pub fn derive_session_key(shared: &SharedSecret) -> [u8; 32] {
    let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    out
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
    pub fn initiator_encapsulate(pk: &PublicKey) -> (Ciphertext, [u8; 32]) {
        let (ss, ct) = encapsulate(pk);
        (ct, derive_session_key(&ss))
    }

    /// Responder decapsulates ciphertext with its secret key and derives the
    /// matching 32-byte session key.
    pub fn responder_decapsulate(ct: &Ciphertext, sk: &SecretKey) -> [u8; 32] {
        let ss = decapsulate(ct, sk);
        derive_session_key(&ss)
    }

    /// Convert Kyber shared secret into Nyx session key via HKDF.
    fn derive_session_key(shared: &SharedSecret) -> [u8; 32] {
        let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
        let mut out = [0u8; 32];
        out.copy_from_slice(&okm);
        out
    }
} 