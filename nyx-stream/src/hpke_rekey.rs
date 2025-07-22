#![forbid(unsafe_code)]

//! HPKE-based rekey frame helpers.
//!
//! This module introduces a **Cryptographic Rekey Frame** that transports a
//! freshly generated symmetric session key encrypted to the peer using
//! RFC 9180 HPKE (mode 0 = base).  The frame is carried inside the Nyx
//! *CRYPTO* packet category (type = 2, flags = 0) and has the following layout:
//!
//! ```text
//!  0               1               2               3
//! +---------------+---------------+---------------+---------------+
//! |  EncLen (16)  |        EncappedKey (N bytes)  |  ...          |
//! +---------------+---------------+---------------+---------------+
//! |  CtLen (16)   |     Ciphertext (M bytes)      |  ...          |
//! +---------------+---------------+---------------+---------------+
//! ```
//! * `EncLen`  – length of the HPKE encapped key in octets.
//! * `CtLen`   – length of the HPKE ciphertext that wraps the 32-byte     
//!               Nyx session key.
//!
//! Both length fields are unsigned big-endian 16-bit integers.  The current
//! implementation relies on the [`hpke`] crate’s X25519-HKDF-SHA256 KEM and
//! ChaCha20-Poly1305 AEAD, matching the Nyx cryptographic baseline.
//!
//! The helper API is deliberately kept **stateless** – callers supply the
//! remote party’s HPKE public key when sealing and their own private key when
//! opening.  This avoids having to thread additional context through the
//! stream layer while still providing convenient one-shot functions.

use nom::{number::complete::be_u16, bytes::complete::take, IResult};
use nyx_crypto::{hpke, noise::SessionKey};
use nyx_crypto::hpke::{PublicKey, PrivateKey, generate_and_seal_session, open_session};

/// Frame carrying an HPKE-encrypted rekey blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RekeyFrame {
    /// Encapped KEM public key (variable-length).
    pub encapped_key: Vec<u8>,
    /// Ciphertext that seals a fresh 32-byte session key.
    pub ciphertext: Vec<u8>,
}

/// Serialise a [`RekeyFrame`] to bytes suitable for on-wire transmission.
#[must_use]
pub fn build_rekey_frame(frame: &RekeyFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + frame.encapped_key.len() + frame.ciphertext.len());
    out.extend_from_slice(&(frame.encapped_key.len() as u16).to_be_bytes());
    out.extend_from_slice(&frame.encapped_key);
    out.extend_from_slice(&(frame.ciphertext.len() as u16).to_be_bytes());
    out.extend_from_slice(&frame.ciphertext);
    out
}

/// Parse a rekey frame from the input byte slice.
pub fn parse_rekey_frame(input: &[u8]) -> IResult<&[u8], RekeyFrame> {
    let (input, enc_len) = be_u16(input)?;
    let (input, enc) = take(enc_len)(input)?;
    let (input, ct_len) = be_u16(input)?;
    let (input, ct) = take(ct_len)(input)?;
    Ok((input, RekeyFrame { encapped_key: enc.to_vec(), ciphertext: ct.to_vec() }))
}

/// Seal a fresh session key to `remote_pk` and return the on-wire frame **and**
/// the locally generated [`SessionKey`].  The caller MUST switch to the new
/// key immediately after sending the frame.
pub fn seal_for_rekey(remote_pk: &PublicKey, info: &[u8]) -> Result<(RekeyFrame, SessionKey), hpke::HpkeError> {
    let (enc, ct, sk) = generate_and_seal_session(remote_pk, info, b"")?;
    Ok((RekeyFrame { encapped_key: enc, ciphertext: ct }, sk))
}

/// Open a received rekey frame using our private key `sk_r` and return the
/// decrypted [`SessionKey`].  The caller MUST adopt the new key *before*
/// acknowledging the frame to avoid key desynchronisation.
pub fn open_rekey(sk_r: &PrivateKey, frame: &RekeyFrame, info: &[u8]) -> Result<SessionKey, hpke::HpkeError> {
    open_session(sk_r, &frame.encapped_key, info, b"", &frame.ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_crypto::hpke::{generate_keypair};

    #[test]
    fn frame_roundtrip() {
        let (_, pk) = generate_keypair();
        let (frame, _key) = seal_for_rekey(&pk, b"nyx-rekey-test").unwrap();
        let bytes = build_rekey_frame(&frame);
        let (_, parsed) = parse_rekey_frame(&bytes).unwrap();
        assert_eq!(frame, parsed);
    }

    #[test]
    fn key_exchange_success() {
        let (sk_r, pk_r) = generate_keypair();
        let (frame, local_key) = seal_for_rekey(&pk_r, b"rekey").unwrap();
        let remote_key = open_rekey(&sk_r, &frame, b"rekey").unwrap();
        assert_eq!(local_key.0, remote_key.0);
    }
} 