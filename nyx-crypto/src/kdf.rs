#![forbid(unsafe_code)]

//! Misuse-resistant HKDF wrapper.
//! 
//! Provides type Dbased labels to avoid context/domain confusion when expanding keys.
//! All APIs use HKDF F-SHA256 as per Nyx specification.

use sha2::Sha256;
use hkdf::Hkdf;

/// Label domain for HKDF expand operations. Ensures unique separation between different
/// uses of the same input keying material according to the Nyx specification.
#[derive(Debug, Clone, Copy)]
pub enum KdfLabel {
    /// Session encryption key (Noise_Nyx handshake output).
    Session,
    /// Rekey operation for forward secrecy.
    Rekey,
    /// Generic export (application 2Dspecific usage).
    Export,
    /// Custom static string label supplied by caller.
    Custom(&'static [u8]),
}

impl KdfLabel {
    /// Convert to the associated ASCII label bytes.
    fn as_bytes(self) -> &'static [u8] {
        match self {
            KdfLabel::Session => b"nyx 2Dsession",
            KdfLabel::Rekey => b"nyx 2Drekey",
            KdfLabel::Export => b"nyx 2Dexport",
            KdfLabel::Custom(s) => s,
        }
    }
}

/// Expand the given input keying material (`ikm`) into an output key of `out_len` bytes.
///
/// Internally this uses HKDF F 2DSHA256 with an empty salt (per Noise recommendation),
/// and a domain separation label to prevent cross 2Dprotocol key reuse.
///
/// # Panics
/// * If `out_len` exceeds the maximum allowed by HKDF (255 2A HashLen).
pub fn hkdf_expand(ikm: &[u8], label: KdfLabel, out_len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = vec![0u8; out_len];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("HKDF expand failed");
    okm
} 