#![forbid(unsafe_code)]

//! Fixed-size 1280-byte padding helpers used by Obfuscation pipeline.
//!
//! Outbound packets are padded up to the next multiple of 1280 bytes so that
//! an observer cannot infer the true size. Inbound packets keep the inner
//! Nyx frame `length` field, therefore callers must trim after decryption.
//!
//! Only zero-bytes are used for padding because the payload is encrypted
//! afterwards; distribution of padding values is irrelevant to entropy.

use super::SHARD_SIZE;

/// Pad the given buffer _in-place_ up to the next multiple of 1280 bytes.
/// If `buf.len()` is already aligned, no action is taken.
pub fn pad_outgoing(buf: &mut Vec<u8>) {
    let rem = buf.len() % SHARD_SIZE;
    if rem == 0 { return; }
    let pad_len = SHARD_SIZE - rem;
    buf.resize(buf.len() + pad_len, 0u8);
}

/// Return a slice trimmed to the first `actual_len` bytes.
///
/// `actual_len` is expected to be obtained from the Nyx frame header.
#[must_use]
pub fn trim_incoming(buf: &[u8], actual_len: usize) -> &[u8] {
    if actual_len > buf.len() { buf } else { &buf[..actual_len] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_aligns_to_constant() {
        let mut v = vec![1u8; 2000];
        pad_outgoing(&mut v);
        assert_eq!(v.len() % SHARD_SIZE, 0);
    }
} 