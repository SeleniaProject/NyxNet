use nyx_crypto::aead::{FrameCrypter, AeadError};
use nyx_crypto::noise::SessionKey;

/// Test AEAD encryption/decryption roundtrip
#[test]
fn aead_roundtrip_basic() {
    let key = SessionKey([42u8; 32]);
    let mut crypter = FrameCrypter::new(key);
    
    let plaintext = b"Hello, Nyx!";
    let aad = b"header_data";
    let dir = 0x00000000;
    
    let ciphertext = crypter.encrypt(dir, plaintext, aad);
    let recovered = crypter.decrypt(dir, 0, &ciphertext, aad).unwrap();
    
    assert_eq!(recovered, plaintext);
}

/// Test AEAD with different direction IDs
#[test]
fn aead_direction_separation() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let plaintext = b"test message";
    let aad = b"header";
    
    // Encrypt with direction 0
    let ct_dir0 = crypter_a.encrypt(0x00000000, plaintext, aad);
    
    // Encrypt with direction 1
    let ct_dir1 = crypter_a.encrypt(0xFFFFFFFF, plaintext, aad);
    
    // Ciphertexts should be different
    assert_ne!(ct_dir0, ct_dir1);
    
    // Decrypt with correct directions
    let pt0 = crypter_b.decrypt(0x00000000, 0, &ct_dir0, aad).unwrap();
    let pt1 = crypter_b.decrypt(0xFFFFFFFF, 1, &ct_dir1, aad).unwrap();
    
    assert_eq!(pt0, plaintext);
    assert_eq!(pt1, plaintext);
}

/// Test AEAD replay protection
#[test]
fn aead_replay_protection() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let plaintext = b"replay test";
    let aad = b"header";
    let dir = 0x00000000;
    
    let ciphertext = crypter_a.encrypt(dir, plaintext, aad);
    
    // First decryption should succeed
    let result1 = crypter_b.decrypt(dir, 0, &ciphertext, aad);
    assert!(result1.is_ok());
    
    // Second decryption with same sequence should fail (replay)
    let result2 = crypter_b.decrypt(dir, 0, &ciphertext, aad);
    assert_eq!(result2.unwrap_err(), AeadError::Replay);
}

/// Test AEAD with out-of-order packets
#[test]
fn aead_out_of_order() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let aad = b"header";
    let dir = 0x00000000;
    
    // Encrypt multiple packets
    let ct0 = crypter_a.encrypt(dir, b"packet 0", aad);
    let ct1 = crypter_a.encrypt(dir, b"packet 1", aad);
    let ct2 = crypter_a.encrypt(dir, b"packet 2", aad);
    
    // Decrypt out of order: 1, 0, 2
    let pt1 = crypter_b.decrypt(dir, 1, &ct1, aad).unwrap();
    let pt0 = crypter_b.decrypt(dir, 0, &ct0, aad).unwrap();
    let pt2 = crypter_b.decrypt(dir, 2, &ct2, aad).unwrap();
    
    assert_eq!(pt0, b"packet 0");
    assert_eq!(pt1, b"packet 1");
    assert_eq!(pt2, b"packet 2");
}

/// Test AEAD with stale packets (outside window)
#[test]
fn aead_stale_packets() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let aad = b"header";
    let dir = 0x00000000;
    
    // Encrypt a recent packet
    let ct_recent = crypter_a.encrypt(dir, b"recent", aad);
    
    // Decrypt recent packet to advance window
    let _ = crypter_b.decrypt(dir, 1000000, &ct_recent, aad).unwrap();
    
    // Try to decrypt a very old packet (outside window)
    let ct_old = crypter_a.encrypt(dir, b"old", aad);
    let result = crypter_b.decrypt(dir, 0, &ct_old, aad);
    
    assert_eq!(result.unwrap_err(), AeadError::Stale);
}

/// Test AEAD with invalid authentication tag
#[test]
fn aead_invalid_tag() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let plaintext = b"authenticated message";
    let aad = b"header";
    let dir = 0x00000000;
    
    let mut ciphertext = crypter_a.encrypt(dir, plaintext, aad);
    
    // Corrupt the last byte (authentication tag)
    let last_idx = ciphertext.len() - 1;
    ciphertext[last_idx] ^= 0xFF;
    
    let result = crypter_b.decrypt(dir, 0, &ciphertext, aad);
    assert_eq!(result.unwrap_err(), AeadError::InvalidTag);
}

/// Test AEAD with different AAD
#[test]
fn aead_aad_mismatch() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let plaintext = b"aad test";
    let aad_encrypt = b"encrypt_header";
    let aad_decrypt = b"decrypt_header";
    let dir = 0x00000000;
    
    let ciphertext = crypter_a.encrypt(dir, plaintext, aad_encrypt);
    
    // Decrypt with different AAD should fail
    let result = crypter_b.decrypt(dir, 0, &ciphertext, aad_decrypt);
    assert_eq!(result.unwrap_err(), AeadError::InvalidTag);
}

/// Test AEAD with empty plaintext
#[test]
fn aead_empty_plaintext() {
    let key = SessionKey([42u8; 32]);
    let mut crypter = FrameCrypter::new(key);
    
    let plaintext = b"";
    let aad = b"header";
    let dir = 0x00000000;
    
    let ciphertext = crypter.encrypt(dir, plaintext, aad);
    let recovered = crypter.decrypt(dir, 0, &ciphertext, aad).unwrap();
    
    assert_eq!(recovered, plaintext);
    assert!(recovered.is_empty());
}

/// Test AEAD with large plaintexts
#[test]
fn aead_large_plaintext() {
    let key = SessionKey([42u8; 32]);
    let mut crypter = FrameCrypter::new(key);
    
    let plaintext = vec![0xAA; 65536]; // 64KB
    let aad = b"large_header";
    let dir = 0x00000000;
    
    let ciphertext = crypter.encrypt(dir, &plaintext, aad);
    let recovered = crypter.decrypt(dir, 0, &ciphertext, aad).unwrap();
    
    assert_eq!(recovered, plaintext);
}

/// Test AEAD sequence number wraparound
#[test]
fn aead_sequence_wraparound() {
    let key = SessionKey([42u8; 32]);
    let mut crypter_a = FrameCrypter::new(key);
    let mut crypter_b = FrameCrypter::new(SessionKey([42u8; 32]));
    
    let plaintext = b"wraparound test";
    let aad = b"header";
    let dir = 0x00000000;
    
    // Test near u64::MAX
    let high_seq = u64::MAX - 1;
    let ct1 = crypter_a.encrypt(dir, plaintext, aad);
    let ct2 = crypter_a.encrypt(dir, plaintext, aad);
    
    // Decrypt with high sequence numbers
    let result1 = crypter_b.decrypt(dir, high_seq, &ct1, aad);
    let result2 = crypter_b.decrypt(dir, high_seq + 1, &ct2, aad);
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

/// Test AEAD performance
#[test]
fn aead_performance() {
    use std::time::Instant;
    
    let key = SessionKey([42u8; 32]);
    let mut crypter = FrameCrypter::new(key);
    
    let plaintext = vec![0x42; 1024]; // 1KB
    let aad = b"perf_header";
    let dir = 0x00000000;
    
    let iterations = 1000;
    let start = Instant::now();
    
    for i in 0..iterations {
        let ciphertext = crypter.encrypt(dir, &plaintext, aad);
        let _recovered = crypter.decrypt(dir, i, &ciphertext, aad).unwrap();
    }
    
    let duration = start.elapsed();
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();
    
    // Should achieve reasonable throughput (>1000 ops/sec)
    assert!(ops_per_sec > 1000.0, "AEAD performance too low: {} ops/sec", ops_per_sec);
} 