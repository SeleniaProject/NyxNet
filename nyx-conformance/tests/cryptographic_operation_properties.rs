//! Property-based tests for cryptographic operations in Nyx protocol.
//!
//! This module implements comprehensive property-based testing for:
//! - HPKE key derivation and encryption operations
//! - Cryptographic security properties preservation
//! - Noise protocol handshake properties
//!
//! These tests verify that the cryptographic implementations maintain
//! security properties under all possible inputs and edge cases.

use proptest::prelude::*;
use nyx_crypto::{
    noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key, SessionKey},
    kdf::{hkdf_expand, KdfLabel},
    pcr_rekey,
};

#[cfg(feature = "hpke")]
use nyx_crypto::hpke::{generate_keypair, seal, open, generate_and_seal_session, open_session, export_from_session};

/// Strategy for generating random byte arrays of various sizes
fn byte_array_strategy(min_len: usize, max_len: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), min_len..=max_len)
}

/// Strategy for generating info and AAD data for HPKE operations
fn hpke_context_strategy() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    (
        byte_array_strategy(0, 64), // info
        byte_array_strategy(0, 128), // aad
    )
}

/// Strategy for generating plaintext data
fn plaintext_strategy() -> impl Strategy<Value = Vec<u8>> {
    byte_array_strategy(0, 1024)
}

#[cfg(feature = "hpke")]
mod hpke_properties {
    use super::*;

    proptest! {
        /// Property: HPKE encryption/decryption roundtrip should always succeed
        /// and preserve plaintext for valid inputs
        #[test]
        fn hpke_roundtrip_correctness(
            plaintext in plaintext_strategy(),
            (info, aad) in hpke_context_strategy()
        ) {
            let (sk_r, pk_r) = generate_keypair();
            
            // Encrypt
            let (enc, ciphertext) = seal(&pk_r, &info, &aad, &plaintext)
                .expect("HPKE seal should succeed");
            
            // Decrypt
            let decrypted = open(&sk_r, &enc, &info, &aad, &ciphertext)
                .expect("HPKE open should succeed");
            
            // Verify roundtrip correctness
            prop_assert_eq!(plaintext, decrypted);
        }

        /// Property: HPKE ciphertexts should be different for the same plaintext
        /// (due to randomness in encapsulation)
        #[test]
        fn hpke_ciphertext_randomness(
            plaintext in plaintext_strategy(),
            (info, aad) in hpke_context_strategy()
        ) {
            let (_, pk_r) = generate_keypair();
            
            let (enc1, ct1) = seal(&pk_r, &info, &aad, &plaintext)
                .expect("First HPKE seal should succeed");
            let (enc2, ct2) = seal(&pk_r, &info, &aad, &plaintext)
                .expect("Second HPKE seal should succeed");
            
            // Encapsulated keys should be different (with high probability)
            prop_assert_ne!(enc1, enc2);
            // Ciphertexts should be different (with high probability)
            prop_assert_ne!(ct1, ct2);
        }

        /// Property: HPKE should fail to decrypt with wrong private key
        #[test]
        fn hpke_key_isolation(
            plaintext in plaintext_strategy(),
            (info, aad) in hpke_context_strategy()
        ) {
            let (sk_r1, pk_r1) = generate_keypair();
            let (sk_r2, _) = generate_keypair();
            
            let (enc, ciphertext) = seal(&pk_r1, &info, &aad, &plaintext)
                .expect("HPKE seal should succeed");
            
            // Attempting to decrypt with wrong key should fail
            let result = open(&sk_r2, &enc, &info, &aad, &ciphertext);
            prop_assert!(result.is_err());
        }

        /// Property: HPKE should fail with corrupted ciphertext
        #[test]
        fn hpke_ciphertext_integrity(
            plaintext in plaintext_strategy().prop_filter("non-empty", |p| !p.is_empty()),
            (info, aad) in hpke_context_strategy(),
            corruption_index in 0usize..1024
        ) {
            let (sk_r, pk_r) = generate_keypair();
            
            let (enc, mut ciphertext) = seal(&pk_r, &info, &aad, &plaintext)
                .expect("HPKE seal should succeed");
            
            // Corrupt the ciphertext if it's not empty
            if !ciphertext.is_empty() {
                let idx = corruption_index % ciphertext.len();
                ciphertext[idx] ^= 0xFF;
                
                // Decryption should fail with corrupted ciphertext
                let result = open(&sk_r, &enc, &info, &aad, &ciphertext);
                prop_assert!(result.is_err());
            }
        }

        /// Property: HPKE session key generation and opening should produce identical keys
        #[test]
        fn hpke_session_key_consistency(
            (info, aad) in hpke_context_strategy()
        ) {
            let (sk_r, pk_r) = generate_keypair();
            
            let (enc, ct, sender_key) = generate_and_seal_session(&pk_r, &info, &aad)
                .expect("Session generation should succeed");
            
            let receiver_key = open_session(&sk_r, &enc, &info, &aad, &ct)
                .expect("Session opening should succeed");
            
            prop_assert_eq!(sender_key.0, receiver_key.0);
        }

        /// Property: HPKE export from session should be deterministic and unique
        #[test]
        fn hpke_export_properties(
            (info, aad) in hpke_context_strategy(),
            export_length in 1usize..=255
        ) {
            let (sk_r, pk_r) = generate_keypair();
            
            let (enc, ct, session_key) = generate_and_seal_session(&pk_r, &info, &aad)
                .expect("Session generation should succeed");
            
            // Export should be deterministic
            let export1 = export_from_session(&session_key, export_length);
            let export2 = export_from_session(&session_key, export_length);
            prop_assert_eq!(export1, export2);
            
            // Export should have correct length
            prop_assert_eq!(export1.len(), export_length);
            
            // Different session keys should produce different exports
            let (enc2, ct2, session_key2) = generate_and_seal_session(&pk_r, &info, &aad)
                .expect("Second session generation should succeed");
            
            if session_key.0 != session_key2.0 {
                let export3 = export_from_session(&session_key2, export_length);
                prop_assert_ne!(export1, export3);
            }
        }
    }
}

mod noise_handshake_properties {
    use super::*;

    proptest! {
        /// Property: Noise handshake should always produce matching session keys
        #[test]
        fn noise_handshake_correctness(_seed in any::<u64>()) {
            let (init_pub, init_sec) = initiator_generate();
            let (resp_pub, shared_resp) = responder_process(&init_pub);
            let shared_init = initiator_finalize(init_sec, &resp_pub);
            
            let key_init = derive_session_key(&shared_init);
            let key_resp = derive_session_key(&shared_resp);
            
            prop_assert_eq!(key_init.0, key_resp.0);
        }

        /// Property: Different handshakes should produce different session keys
        #[test]
        fn noise_handshake_uniqueness(_seed in any::<u64>()) {
            // First handshake
            let (init_pub1, init_sec1) = initiator_generate();
            let (resp_pub1, _shared_resp1) = responder_process(&init_pub1);
            let shared_init1 = initiator_finalize(init_sec1, &resp_pub1);
            let key1 = derive_session_key(&shared_init1);
            
            // Second handshake
            let (init_pub2, init_sec2) = initiator_generate();
            let (resp_pub2, _shared_resp2) = responder_process(&init_pub2);
            let shared_init2 = initiator_finalize(init_sec2, &resp_pub2);
            let key2 = derive_session_key(&shared_init2);
            
            // Keys should be different (with overwhelming probability)
            prop_assert_ne!(key1.0, key2.0);
        }

        /// Property: Session key derivation should be deterministic
        #[test]
        fn noise_session_key_deterministic(_seed in any::<u64>()) {
            let (init_pub, init_sec) = initiator_generate();
            let (resp_pub, _shared_resp) = responder_process(&init_pub);
            let shared_init = initiator_finalize(init_sec, &resp_pub);
            
            let key1 = derive_session_key(&shared_init);
            let key2 = derive_session_key(&shared_init);
            
            prop_assert_eq!(key1.0, key2.0);
        }

        /// Property: Session keys should not be predictable (not all zeros or ones)
        #[test]
        fn noise_session_key_entropy(_seed in any::<u64>()) {
            let (init_pub, init_sec) = initiator_generate();
            let (resp_pub, _shared_resp) = responder_process(&init_pub);
            let shared_init = initiator_finalize(init_sec, &resp_pub);
            
            let key = derive_session_key(&shared_init);
            
            // Key should not be all zeros
            prop_assert_ne!(key.0, [0u8; 32]);
            // Key should not be all ones
            prop_assert_ne!(key.0, [0xFFu8; 32]);
        }
    }
}

#[cfg(feature = "pq")]
mod kyber_properties {
    use super::*;
    use nyx_crypto::noise::kyber::{responder_keypair, initiator_encapsulate, responder_decapsulate};

    proptest! {
        /// Property: Kyber KEM should produce matching session keys
        #[test]
        fn kyber_kem_correctness(_seed in any::<u64>()) {
            let (pk, sk) = responder_keypair();
            let (ct, key_init) = initiator_encapsulate(&pk);
            let key_resp = responder_decapsulate(&ct, &sk);
            
            prop_assert_eq!(key_init.0, key_resp.0);
        }

        /// Property: Different Kyber encapsulations should produce different keys
        #[test]
        fn kyber_kem_uniqueness(_seed in any::<u64>()) {
            let (pk, _) = responder_keypair();
            
            let (_, key1) = initiator_encapsulate(&pk);
            let (_, key2) = initiator_encapsulate(&pk);
            
            // Keys should be different (with overwhelming probability)
            prop_assert_ne!(key1.0, key2.0);
        }

        /// Property: Kyber session keys should have good entropy
        #[test]
        fn kyber_session_key_entropy(_seed in any::<u64>()) {
            let (pk, _) = responder_keypair();
            let (_, key) = initiator_encapsulate(&pk);
            
            // Key should not be all zeros
            prop_assert_ne!(key.0, [0u8; 32]);
            // Key should not be all ones
            prop_assert_ne!(key.0, [0xFFu8; 32]);
        }
    }
}

#[cfg(feature = "hybrid")]
mod hybrid_properties {
    use super::*;
    use nyx_crypto::noise::hybrid::{initiator_step, responder_step, initiator_finalize};
    use nyx_crypto::noise::kyber::responder_keypair;

    proptest! {
        /// Property: Hybrid handshake should produce matching session keys
        #[test]
        fn hybrid_handshake_correctness(_seed in any::<u64>()) {
            let (pk_kyber, sk_kyber) = responder_keypair();
            
            let (init_x25519_pub, init_x25519_sec, ct, kyber_key) = initiator_step(&pk_kyber);
            let (resp_x25519_pub, resp_combined_key) = responder_step(&init_x25519_pub, &ct, &sk_kyber);
            let init_combined_key = initiator_finalize(init_x25519_sec, &resp_x25519_pub, kyber_key);
            
            prop_assert_eq!(init_combined_key.0, resp_combined_key.0);
        }

        /// Property: Hybrid keys should be different from individual components
        #[test]
        fn hybrid_key_combination(_seed in any::<u64>()) {
            let (pk_kyber, sk_kyber) = responder_keypair();
            
            let (init_x25519_pub, _init_x25519_sec, ct, kyber_key) = initiator_step(&pk_kyber);
            let (_resp_x25519_pub, combined_key) = responder_step(&init_x25519_pub, &ct, &sk_kyber);
            
            // Combined key should be different from Kyber component
            prop_assert_ne!(combined_key.0, kyber_key.0);
        }
    }
}

mod kdf_properties {
    use super::*;

    proptest! {
        /// Property: KDF should produce different outputs for different labels
        #[test]
        fn kdf_label_separation(
            ikm in byte_array_strategy(1, 64),
            length in 8usize..=255  // Use minimum 8 bytes to ensure separation
        ) {
            let session = hkdf_expand(&ikm, KdfLabel::Session, length);
            let rekey = hkdf_expand(&ikm, KdfLabel::Rekey, length);
            let export = hkdf_expand(&ikm, KdfLabel::Export, length);
            
            prop_assert_ne!(&session, &rekey);
            prop_assert_ne!(&session, &export);
            prop_assert_ne!(&rekey, &export);
        }

        /// Property: KDF should be deterministic
        #[test]
        fn kdf_deterministic(
            ikm in byte_array_strategy(1, 64),
            length in 1usize..=255
        ) {
            let output1 = hkdf_expand(&ikm, KdfLabel::Session, length);
            let output2 = hkdf_expand(&ikm, KdfLabel::Session, length);
            
            prop_assert_eq!(&output1, &output2);
            prop_assert_eq!(output1.len(), length);
        }

        /// Property: Different IKM should produce different outputs
        #[test]
        fn kdf_ikm_separation(
            ikm1 in byte_array_strategy(1, 64),
            ikm2 in byte_array_strategy(1, 64),
            length in 1usize..=255
        ) {
            prop_assume!(ikm1 != ikm2);
            let output1 = hkdf_expand(&ikm1, KdfLabel::Session, length);
            let output2 = hkdf_expand(&ikm2, KdfLabel::Session, length);
            
            prop_assert_ne!(&output1, &output2);
        }

        /// Property: Custom labels should provide separation
        #[test]
        fn kdf_custom_label_separation(
            ikm in byte_array_strategy(1, 64),
            length in 8usize..=255  // Use minimum 8 bytes to ensure separation
        ) {
            let output1 = hkdf_expand(&ikm, KdfLabel::Custom(b"label1"), length);
            let output2 = hkdf_expand(&ikm, KdfLabel::Custom(b"label2"), length);
            let output3 = hkdf_expand(&ikm, KdfLabel::Session, length);
            
            prop_assert_ne!(&output1, &output2);
            prop_assert_ne!(&output1, &output3);
            prop_assert_ne!(&output2, &output3);
        }
    }
}

mod pcr_rekey_properties {
    use super::*;

    proptest! {
        /// Property: PCR rekey should produce different keys
        #[test]
        fn pcr_rekey_forward_secrecy(
            initial_key_bytes in prop::array::uniform32(any::<u8>())
        ) {
            let mut old_key = SessionKey(initial_key_bytes);
            let old_key_copy = old_key.0;
            
            let new_key = pcr_rekey(&mut old_key);
            
            // New key should be different
            prop_assert_ne!(new_key.0, old_key_copy);
            
            // Old key should be zeroized
            prop_assert_eq!(old_key.0, [0u8; 32]);
        }

        /// Property: PCR rekey should be deterministic for same input
        #[test]
        fn pcr_rekey_deterministic(
            key_bytes in prop::array::uniform32(any::<u8>())
        ) {
            let mut key1 = SessionKey(key_bytes);
            let mut key2 = SessionKey(key_bytes);
            
            let new_key1 = pcr_rekey(&mut key1);
            let new_key2 = pcr_rekey(&mut key2);
            
            prop_assert_eq!(new_key1.0, new_key2.0);
        }

        /// Property: PCR rekey chain should produce unique keys
        #[test]
        fn pcr_rekey_chain_uniqueness(
            initial_key_bytes in prop::array::uniform32(any::<u8>()),
            chain_length in 2usize..=10
        ) {
            let mut current_key = SessionKey(initial_key_bytes);
            let mut keys = Vec::new();
            
            for _ in 0..chain_length {
                current_key = pcr_rekey(&mut current_key);
                keys.push(current_key.0);
            }
            
            // All keys in chain should be unique
            for i in 0..keys.len() {
                for j in (i+1)..keys.len() {
                    prop_assert_ne!(keys[i], keys[j]);
                }
            }
        }
    }
}

mod security_properties {
    use super::*;

    proptest! {
        /// Property: Cryptographic operations should not leak key material in outputs
        #[cfg(feature = "hpke")]
        #[test]
        fn no_key_leakage_hpke(
            plaintext in plaintext_strategy(),
            (info, aad) in hpke_context_strategy()
        ) {
            let (sk_r, pk_r) = generate_keypair();
            let (enc, ciphertext) = seal(&pk_r, &info, &aad, &plaintext)
                .expect("HPKE seal should succeed");
            
            // Ciphertext should not contain the private key
            let sk_bytes = sk_r.to_bytes();
            prop_assert!(!ciphertext.windows(sk_bytes.len()).any(|window| window == sk_bytes));
            
            // Encapsulated key should not contain the private key
            prop_assert!(!enc.windows(sk_bytes.len()).any(|window| window == sk_bytes));
        }

        /// Property: Session keys should not appear in intermediate values
        #[test]
        fn no_session_key_leakage(_seed in any::<u64>()) {
            let (init_pub, init_sec) = initiator_generate();
            let (resp_pub, _shared_resp) = responder_process(&init_pub);
            let shared_init = initiator_finalize(init_sec, &resp_pub);
            
            let session_key = derive_session_key(&shared_init);
            
            // Session key should not appear in public keys
            let init_pub_bytes = init_pub.as_bytes();
            let resp_pub_bytes = resp_pub.as_bytes();
            
            prop_assert!(!init_pub_bytes.windows(32).any(|window| window == session_key.0));
            prop_assert!(!resp_pub_bytes.windows(32).any(|window| window == session_key.0));
        }

        /// Property: Cryptographic operations should be constant-time resistant
        /// (This is a basic check - real constant-time verification requires specialized tools)
        #[cfg(feature = "hpke")]
        #[test]
        fn basic_timing_consistency(
            plaintext1 in byte_array_strategy(100, 100), // Fixed size
            plaintext2 in byte_array_strategy(100, 100), // Fixed size
            (info, aad) in hpke_context_strategy()
        ) {
            let (sk_r, pk_r) = generate_keypair();
            
            use std::time::Instant;
            
            // Time first encryption
            let start1 = Instant::now();
            let (enc1, ct1) = seal(&pk_r, &info, &aad, &plaintext1)
                .expect("First seal should succeed");
            let duration1 = start1.elapsed();
            
            // Time second encryption
            let start2 = Instant::now();
            let (enc2, ct2) = seal(&pk_r, &info, &aad, &plaintext2)
                .expect("Second seal should succeed");
            let duration2 = start2.elapsed();
            
            // Timing should be reasonably consistent for same-size inputs
            // Allow for 50% variance (this is a very loose check)
            let ratio = duration1.as_nanos() as f64 / duration2.as_nanos() as f64;
            prop_assert!(ratio > 0.5 && ratio < 2.0, "Timing variance too high: {}", ratio);
        }
    }
}

#[cfg(test)]
mod integration_tests {

    /// Integration test combining HPKE and Noise protocols
    #[cfg(feature = "hpke")]
    #[test]
    fn hpke_noise_integration() {
        use super::*;
        // Generate Noise session key
        let (init_pub, init_sec) = initiator_generate();
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        let noise_session = derive_session_key(&shared_init);
        
        // Use Noise session key as HPKE export
        let hpke_export = export_from_session(&noise_session, 32);
        
        // Generate HPKE keypair and use export as additional context
        let (sk_r, pk_r) = generate_keypair();
        let plaintext = b"integration test message";
        
        let (enc, ct) = seal(&pk_r, &hpke_export, b"", plaintext)
            .expect("HPKE seal with Noise export should succeed");
        
        let decrypted = open(&sk_r, &enc, &hpke_export, b"", &ct)
            .expect("HPKE open with Noise export should succeed");
        
        assert_eq!(plaintext.to_vec(), decrypted);
    }

    /// Test cryptographic operation error handling
    #[cfg(feature = "hpke")]
    #[test]
    fn crypto_error_handling() {
        use super::*;
        let (sk_r, pk_r) = generate_keypair();
        
        // Test with invalid encapsulated key length
        let invalid_enc = vec![0u8; 10]; // Too short
        let result = open(&sk_r, &invalid_enc, b"", b"", b"test");
        assert!(result.is_err());
        
        // Test session opening with invalid ciphertext length
        let result = open_session(&sk_r, &invalid_enc, b"", b"", b"invalid");
        assert!(result.is_err());
    }
}