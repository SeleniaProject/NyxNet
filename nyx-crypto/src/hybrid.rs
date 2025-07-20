#![forbid(unsafe_code)]

//! Hybrid post-quantum KEM: X25519 + Kyber1024/Bike.
//!
//! Sender generates an X25519 ephemeral key and performs classic DH with the
//! recipient static pubkey, then performs PQ encapsulation (Kyber or Bike)
//! producing `ct_pq`.  Shared secret = HKDF-Extract( SHA-512,
//! concatenate(dh_shared || pq_shared) ).  Ciphertext = (ct_pq, x25519_pub).

use rand_core::{CryptoRng, RngCore};
use blake3::Hasher;
use hkdf::Hkdf;
use sha2::Sha512;
use x25519_dalek::{StaticSecret, PublicKey as XPublic};
use pqcrypto_kyber::kyber1024::{encapsulate as kyb_enc, decapsulate as kyb_dec, PublicKey as KyberPub, SecretKey as KyberSec};
use pqcrypto_bike::bikel1::{encapsulate as bike_enc, decapsulate as bike_dec, PublicKey as BikePub, SecretKey as BikeSec};

pub enum PqAlgo {
    Kyber,
    Bike,
}

pub struct Ciphertext {
    pub ct_pq: Vec<u8>,
    pub x25519_pub: [u8; 32],
}

/// Encapsulate hybrid secret for recipient keys.
pub fn encapsulate<R: RngCore + CryptoRng>(rng: &mut R, pq_pk: &[u8], algo: PqAlgo, x25519_pk: &XPublic) -> (Ciphertext, Vec<u8>) {
    // X25519 ephemeral
    let mut eph_priv = StaticSecret::new(rng);
    let eph_pub = XPublic::from(&eph_priv);
    let dh = eph_priv.diffie_hellman(x25519_pk);

    let (ct_pq, ss_pq) = match algo {
        PqAlgo::Kyber => {
            let pk = KyberPub::from_bytes(pq_pk).expect("kyber pk");
            let (ct, ss) = kyb_enc(&pk, rng);
            (ct.as_bytes().to_vec(), ss.as_bytes().to_vec())
        }
        PqAlgo::Bike => {
            let pk = BikePub::from_bytes(pq_pk).expect("bike pk");
            let (ct, ss) = bike_enc(&pk, rng);
            (ct.as_bytes().to_vec(), ss.as_bytes().to_vec())
        }
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
            let sk = KyberSec::from_bytes(pq_sk).expect("kyber sk");
            let ct_k = pqcrypto_kyber::kyber1024::Ciphertext::from_bytes(&ct.ct_pq).expect("ct");
            let ss = kyb_dec(&ct_k, &sk);
            ss.as_bytes().to_vec()
        }
        PqAlgo::Bike => {
            let sk = BikeSec::from_bytes(pq_sk).expect("bike sk");
            let ct_b = pqcrypto_bike::bikel1::Ciphertext::from_bytes(&ct.ct_pq).expect("ct");
            let ss = bike_dec(&ct_b, &sk);
            ss.as_bytes().to_vec()
        }
    };

    let mut concat = Vec::with_capacity(32 + ss_pq.len());
    concat.extend_from_slice(dh.as_bytes());
    concat.extend_from_slice(&ss_pq);

    let hk = Hkdf::<Sha512>::new(None, &concat);
    let mut okm = [0u8; 64];
    hk.expand(b"nyx-hybrid", &mut okm).expect("okm");
    okm.to_vec()
} 