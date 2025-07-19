#![forbid(unsafe_code)]

//! Misuse-resistant HKDF wrapper.
//! 
//! Provides type-based labels to avoid context/domain confusion when expanding keys.
//! All APIs use HKDF-SHA256 as per Nyx specification.

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
    /// Generic export (application-specific usage).
    Export,
    /// Custom static string label supplied by caller.
    Custom(&'static [u8]),
}

impl KdfLabel {
    /// Convert to the associated ASCII label bytes.
    fn as_bytes(self) -> &'static [u8] {
        match self {
            KdfLabel::Session => b"nyx-session",
            KdfLabel::Rekey => b"nyx-rekey",
            KdfLabel::Export => b"nyx-export",
            KdfLabel::Custom(s) => s,
        }
    }
}

/// Expand the given input keying material (`ikm`) into an output key of `out_len` bytes.
///
/// Internally this uses HKDF-SHA256 with an empty salt (per Noise recommendation),
/// and a domain separation label to prevent cross-protocol key reuse.
///
/// # Panics
/// * If `out_len` exceeds the maximum allowed by HKDF (255 * HashLen).
pub fn hkdf_expand(ikm: &[u8], label: KdfLabel, out_len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = vec![0u8; out_len];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("HKDF expand failed");
    okm
} 