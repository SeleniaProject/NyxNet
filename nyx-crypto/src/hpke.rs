//! HPKE wrapper utilities (RFC 9180) for Nyx.
//!
//! Provides Base mode (mode=0) using KEM X25519, KDF HKDF-SHA256, AEAD ChaCha20-Poly1305.
//! This aligns with Nyx's existing cryptographic primitives.
//!
//! Example:
//! ```rust
//! use nyx_crypto::hpke::{generate_keypair, seal, open};
//! let (skR, pkR) = generate_keypair();
//! let plaintext = b"hello";
//! let (enc, ct) = seal(&pkR, b"info", b"aad", plaintext).unwrap();
//! let decrypted = open(&skR, &enc, b"info", b"aad", &ct).unwrap();
//! assert_eq!(plaintext.to_vec(), decrypted);
//! ```

#![forbid(unsafe_code)]

use hpke::{kem::X25519HkdfSha256, kdf::HkdfSha256, aead::ChaCha20Poly1305, OpModeR, OpModeS};
use hpke::kem::dhkem::EncappedKey;
use hpke::Serializable;
use hpke::kem::Kem as KemTrait;
use rand_core::OsRng;
use crate::noise::SessionKey;
use crate::kdf::{hkdf_expand, KdfLabel};

pub type KemImpl = X25519HkdfSha256;
pub type Kdf = HkdfSha256;
pub type Aead = ChaCha20Poly1305;

pub type PublicKey = <KemImpl as hpke::kem::Kem>::PublicKey;
pub type PrivateKey = <KemImpl as hpke::kem::Kem>::PrivateKey;

/// Generate an X25519 keypair for HPKE.
#[must_use]
pub fn generate_keypair() -> (PrivateKey, PublicKey) {
    let mut rng = OsRng;
    KemImpl::gen_keypair(&mut rng)
}

/// Seal (encrypt) `plaintext` to `pk_r`. Returns (enc, ciphertext).
pub fn seal(pk_r: &PublicKey, info: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), hpke::HpkeError> {
    let mut rng = OsRng;
    let (enc, mut sender_ctx) = hpke::setup_sender::<Aead, Kdf, KemImpl, _>(
        &OpModeS::Base,
        pk_r,
        info,
        &mut rng,
    )?;
    let ct = sender_ctx.seal(aad, plaintext)?;
    Ok((enc.to_bytes().to_vec(), ct))
}

/// Open (decrypt) ciphertext using receiver private key.
pub fn open(
    sk_r: &PrivateKey,
    enc: &[u8],
    info: &[u8],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, hpke::HpkeError> {
    let encapped = EncappedKey::<KemImpl>::from_bytes(enc)?;
    let mut receiver_ctx = hpke::setup_receiver::<Aead, Kdf, KemImpl>(&OpModeR::Base, sk_r, &encapped, info)?;
    let pt = receiver_ctx.open(aad, ciphertext)?;
    Ok(pt)
}

/// Export secret from Nyx SessionKey using HKDF-SHA256 labelled "nyx-hpke-export".
#[must_use]
pub fn export_from_session(session: &SessionKey, length: usize) -> Vec<u8> {
    hkdf_expand(&session.0, KdfLabel::Custom(b"nyx-hpke-export"), length)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hpke_roundtrip() {
        let (sk, pk) = generate_keypair();
        let (enc, ct) = seal(&pk, b"info", b"", b"secret").unwrap();
        let pt = open(&sk, &enc, b"info", b"", &ct).unwrap();
        assert_eq!(pt, b"secret");
    }

    #[test]
    fn export_from_session_unique() {
        use crate::noise::SessionKey;
        let sk1 = SessionKey([1u8;32]);
        let sk2 = SessionKey([2u8;32]);
        assert_ne!(export_from_session(&sk1, 32), export_from_session(&sk2, 32));
    }

    #[test]
    fn noise_hpke_integration() {
        use crate::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};
        // Initiator creates ephemeral key
        let (init_pub, init_sec) = initiator_generate();
        // Responder processes initiator pub
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        // Initiator finalizes with responder pub
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        let sk_i = derive_session_key(&shared_init);
        let sk_r = derive_session_key(&shared_resp);
        // Both session keys must be identical
        assert_eq!(sk_i.0, sk_r.0);
        // Derive HKDF-based export secret and ensure match
        let exp_i = super::export_from_session(&sk_i, 32);
        let exp_r = super::export_from_session(&sk_r, 32);
        assert_eq!(exp_i, exp_r);
    }
} 