#![forbid(unsafe_code)]

//! Frame-level AEAD (ChaCha20-Poly1305) encryption as required by Nyx Protocol v0.1 §7
//! and v1.0 hybrid extensions.  Each frame payload is protected with a unique
//! 96-bit nonce constructed from a monotonically increasing 64-bit sequence
//! number (little-endian) concatenated with a 32-bit constant per direction.
//! The left-most 32-bit **direction id** MUST be different for each half-duplex
//! direction and can be negotiated as `0x00000000` (initiator → responder) and
//! `0xffffffff` (responder → initiator) to avoid nonce overlap.
//!
//! In addition, the receiver tracks a sliding window of the most recent
//! `2^20` nonces (Nyx anti-replay requirement) and rejects duplicates.
//! The implementation keeps a bitmap (128 KiB) representing the window.
//! While memory-heavy, this remains acceptable for modern systems and
//! drastically simplifies constant-time duplicate checks.
//!
//! Associated data (AAD) is the Nyx packet header *excluding* the encrypted
//! payload – currently 16 bytes (§7).  This guarantees header integrity while
//! allowing middleboxes to inspect CID/flags/path fields.
//!
//! ## Usage
//! ```rust
//! use nyx_crypto::{aead::FrameCrypter, noise::SessionKey};
//! # let sk = SessionKey([0u8; 32]);
//! // Initiator side
//! let mut tx = FrameCrypter::new(sk);
//! let ciphertext = tx.encrypt(0x00000000, b"hello", b"hdr");
//! ```
//!
//! The same struct handles decryption; create another instance with the *same*
//! session key on the peer side and call `decrypt`.  Attempting to decrypt a
//! frame whose nonce falls outside the sliding window or has been seen before
//! returns `Err(AeadError::Replay)`.

use chacha20poly1305::{aead::{Aead, KeyInit, Payload}, ChaCha20Poly1305, Key, Nonce};
use crate::noise::SessionKey;
use thiserror::Error;
use zeroize::Zeroize;

/// Anti-replay window size (2^20 = 1,048,576).
const WINDOW_SIZE: u64 = 1 << 20;

/// Frame-level AEAD decryption/encryption context.
#[derive(Clone)]
pub struct FrameCrypter {
    cipher: ChaCha20Poly1305,
    send_seq: u64,
    recv_highest: u64,
    // bitmap holding WINDOW_SIZE bits; 1 = seen.  1 048 576 / 8 = 131 072 bytes.
    bitmap: Box<[u8; (WINDOW_SIZE as usize / 8)]>,
}

impl FrameCrypter {
    /// Create new crypter with given session key.  Initial sequence numbers are 0.
    pub fn new(SessionKey(key): crate::noise::SessionKey) -> Self {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
        Self {
            cipher,
            send_seq: 0,
            recv_highest: 0,
            bitmap: Box::new([0u8; WINDOW_SIZE as usize / 8]),
        }
    }

    /// Encrypt `plaintext` with given 32-bit direction id and implicit seq counter.
    /// Returns `ciphertext || tag`.
    pub fn encrypt(&mut self, dir: u32, plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
        let seq = self.send_seq;
        self.send_seq = self.send_seq.wrapping_add(1);
        let nonce = Self::make_nonce(dir, seq);
        self.cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad })
            .expect("encryption failure")
    }

    /// Decrypt frame using direction id.  Rejects out-of-window or duplicate nonces.
    pub fn decrypt(&mut self, dir: u32, seq: u64, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, AeadError> {
        self.check_replay(seq)?;
        let nonce = Self::make_nonce(dir, seq);
        let pt = self
            .cipher
            .decrypt(Nonce::from_slice(&nonce), Payload { msg: ciphertext, aad })
            .map_err(|_| AeadError::InvalidTag)?;
        self.mark_seen(seq);
        Ok(pt)
    }

    #[inline]
    fn make_nonce(dir: u32, seq: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[..4].copy_from_slice(&dir.to_le_bytes());
        nonce[4..].copy_from_slice(&seq.to_le_bytes());
        nonce
    }

    fn check_replay(&self, seq: u64) -> Result<(), AeadError> {
        if seq + WINDOW_SIZE < self.recv_highest {
            return Err(AeadError::Stale);
        }
        if seq > self.recv_highest {
            // brand-new sequence – always allowed
            return Ok(());
        }
        let idx = (self.recv_highest - seq) as usize; // distance from latest
        if idx >= WINDOW_SIZE as usize {
            return Err(AeadError::Stale);
        }
        if self.bitmap_get(idx) {
            return Err(AeadError::Replay);
        }
        Ok(())
    }

    fn mark_seen(&mut self, seq: u64) {
        if seq > self.recv_highest {
            // advance window forward
            let shift = seq - self.recv_highest;
            self.slide_window(shift);
            self.recv_highest = seq;
        }
        let idx = (self.recv_highest - seq) as usize;
        if idx < WINDOW_SIZE as usize {
            self.bitmap_set(idx);
        }
    }

    fn slide_window(&mut self, shift: u64) {
        if shift >= WINDOW_SIZE {
            // large jump – clear all bits
            for b in self.bitmap.iter_mut() { *b = 0; }
        } else {
            let shift = shift as usize;
            let byte_shift = shift / 8;
            let bit_shift = shift % 8;

            if byte_shift > 0 {
                // Move bytes towards MSB end.
                for i in (byte_shift..self.bitmap.len()).rev() {
                    self.bitmap[i] = self.bitmap[i - byte_shift];
                }
                for i in 0..byte_shift { self.bitmap[i] = 0; }
            }
            if bit_shift > 0 {
                let mut carry = 0u8;
                for byte in &mut *self.bitmap {
                    let new_carry = *byte >> (8 - bit_shift);
                    *byte = (*byte << bit_shift) | carry;
                    carry = new_carry;
                }
            }
        }
    }

    #[inline]
    fn bitmap_get(&self, idx: usize) -> bool {
        let byte = idx / 8;
        let bit = idx % 8;
        ((self.bitmap[byte] >> bit) & 1) == 1
    }

    #[inline]
    fn bitmap_set(&mut self, idx: usize) {
        let byte = idx / 8;
        let bit = idx % 8;
        self.bitmap[byte] |= 1 << bit;
    }
}

impl Drop for FrameCrypter {
    fn drop(&mut self) { self.bitmap.zeroize(); }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AeadError {
    #[error("out of window (stale)")]
    Stale,
    #[error("replayed packet")]
    Replay,
    #[error("authentication failed")]
    InvalidTag,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::SessionKey;

    #[test]
    fn round_trip() {
        let mut a = FrameCrypter::new(SessionKey([7u8; 32]));
        let mut b = FrameCrypter::new(SessionKey([7u8; 32]));

        for i in 0..100 {
            let ct = a.encrypt(0, b"hi", b"hdr");
            let pt = b.decrypt(0, i, &ct, b"hdr").unwrap();
            assert_eq!(pt, b"hi");
        }

        // replay attack detection
        let dup = a.encrypt(0, b"x", b"hdr");
        assert_eq!(b.decrypt(0, 0, &dup, b"hdr").unwrap_err(), AeadError::Replay);
    }
} 