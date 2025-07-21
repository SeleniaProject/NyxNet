#![forbid(unsafe_code)]

//! Hybrid post-quantum KEM: X25519 + Kyber1024/Bike.
//!
//! Sender generates an X25519 ephemeral key and performs classic DH with the
//! recipient static pubkey, then performs PQ encapsulation (Kyber or Bike)
//! producing `ct_pq`.  Shared secret = HKDF-Extract( SHA-512,
//! concatenate(dh_shared || pq_shared) ).  Ciphertext = (ct_pq, x25519_pub).

use rand_core::{CryptoRng, RngCore};
use std::convert::TryInto;
use hkdf::Hkdf;
use sha2::Sha512;
use x25519_dalek::{StaticSecret, PublicKey as XPublic};
use pqcrypto_kyber::kyber1024::{encapsulate as kyb_enc, decapsulate as kyb_dec, PublicKey as KyberPub, SecretKey as KyberSec};
// BIKE support removed for now to avoid unresolved dependency.
use pqcrypto_traits::kem::{Ciphertext as KyCtTrait, PublicKey as KyPubTrait, SecretKey as KySecTrait, SharedSecret as KySsTrait};
use crate::noise::SessionKey;

pub enum PqAlgo {
    Kyber,
}

pub struct Ciphertext {
    pub ct_pq: Vec<u8>,
    pub x25519_pub: [u8; 32],
}

/// Encapsulate hybrid secret for recipient keys.
pub fn encapsulate<R: RngCore + CryptoRng>(rng: &mut R, pq_pk: &[u8], algo: PqAlgo, x25519_pk: &XPublic) -> (Ciphertext, Vec<u8>) {
    // X25519 ephemeral
    let eph_priv = StaticSecret::random();
    let eph_pub = XPublic::from(&eph_priv);
    let dh = eph_priv.diffie_hellman(x25519_pk);

    let (ct_pq, ss_pq) = match algo {
        PqAlgo::Kyber => {
            let pk = KyberPub::from_bytes(pq_pk).expect("pk parse");
            let (ss, ct) = kyb_enc(&pk);
            (ct.as_bytes().to_vec(), ss.as_bytes().to_vec())
        }
        // Bike removed
    };

    let mut concat = Vec::with_capacity(32 + ss_pq.len());
    concat.extend_from_slice(dh.as_bytes());
    concat.extend_from_slice(&ss_pq);

    let hk = Hkdf::<Sha512>::new(None, &concat);
    let mut okm = [0u8; 64];
    hk.expand(b"nyx-hybrid", &mut okm).expect("okm");

    (
        Ciphertext { ct_pq, x25519_pub: *eph_pub.as_bytes() },
        okm.to_vec(),
    )
}

/// Decapsulate hybrid secret.
pub fn decapsulate(ct: &Ciphertext, pq_sk: &[u8], algo: PqAlgo, x25519_sk: &StaticSecret) -> Vec<u8> {
    let rem_pub = XPublic::from(ct.x25519_pub);
    let dh = x25519_sk.diffie_hellman(&rem_pub);

    let ss_pq = match algo {
        PqAlgo::Kyber => {
            let sk = KyberSec::from_bytes(pq_sk).expect("sk parse");
            let ct_k = pqcrypto_kyber::kyber1024::Ciphertext::from_bytes(&ct.ct_pq).expect("ct parse");
            let ss = kyb_dec(&ct_k, &sk);
            ss.as_bytes().to_vec()
        }
        // Bike removed
    };

    let mut concat = Vec::with_capacity(32 + ss_pq.len());
    concat.extend_from_slice(dh.as_bytes());
    concat.extend_from_slice(&ss_pq);

    let hk = Hkdf::<Sha512>::new(None, &concat);
    let mut okm = [0u8; 64];
    hk.expand(b"nyx-hybrid", &mut okm).expect("okm");
    okm.to_vec()
}

/// Encapsulate and immediately derive a 32-byte Nyx [`SessionKey`] suitable for
/// **periodic re-keying**. This is a thin wrapper around [`encapsulate`] that
/// converts the 64-byte HKDF output into Nyx’s fixed-length session key by
/// truncating to the first 32 bytes (as defined in the protocol spec §7.2).
pub fn encapsulate_session<R: RngCore + CryptoRng>(
    rng: &mut R,
    pq_pk: &[u8],
    algo: PqAlgo,
    peer_x25519: &XPublic,
) -> (Ciphertext, SessionKey) {
    let (ct, okm) = encapsulate(rng, pq_pk, algo, peer_x25519);
    debug_assert!(okm.len() >= 32);
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&okm[..32]);
    (ct, SessionKey(sk))
}

/// Decapsulate a [`Ciphertext`] received from the peer and derive the 32-byte
/// Nyx [`SessionKey`] for **periodic key update**.
pub fn decapsulate_session(
    ct: &Ciphertext,
    pq_sk: &[u8],
    algo: PqAlgo,
    local_x25519: &StaticSecret,
) -> SessionKey {
    let okm = decapsulate(ct, pq_sk, algo, local_x25519);
    debug_assert!(okm.len() >= 32);
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&okm[..32]);
    SessionKey(sk)
}

#[cfg(test)]
mod tests_rekey {
    use super::*;
    use rand_core::OsRng;
    use x25519_dalek::StaticSecret;

    #[test]
    fn hybrid_rekey_roundtrip() {
        // Generate dummy keys
        let x_sk_b = StaticSecret::random();
        let x_pk_b = XPublic::from(&x_sk_b);

        let (pq_pk_b, pq_sk_b) = pqcrypto_kyber::kyber1024::keypair();

        // A initiates re-key to B
        let (ciphertext, sk_a) = encapsulate_session(&mut OsRng, pq_pk_b.as_bytes(), PqAlgo::Kyber, &x_pk_b);

        // B decapsulates
        let sk_b = decapsulate_session(&ciphertext, pq_sk_b.as_bytes(), PqAlgo::Kyber, &x_sk_b);

        assert_eq!(sk_a.0, sk_b.0);
    }
} 