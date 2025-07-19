#![forbid(unsafe_code)]

//! Noise_Nyx handshake (暫定簡易版)。
//!
//! 本実装は仕様準拠の完全版ではなく、`x25519-dalek` と HKDF を用いた **PoC** です。
//! 今後、ChaCha20-Poly1305／BLAKE3 で暗号化したフレーム交換を実装し、
//! Kyber1024 フォールバックも feature `pq` で切り替えられるよう拡張します。

use rand_core::OsRng;
use sha2::Sha256;
use hkdf::Hkdf;
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

/// Derive a 32 2Dbyte session key from the shared secret using the misuse 2Dresistant HKDF wrapper.
pub fn derive_session_key(shared: &SharedSecret) -> [u8; 32] {
    let okm = hkdf_expand(shared.as_bytes(), KdfLabel::Session, 32);
    let mut out = [0u8; 32];
    out.copy_from_slice(&okm);
    out
} 