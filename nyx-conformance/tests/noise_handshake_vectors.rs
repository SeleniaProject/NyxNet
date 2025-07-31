use nyx_crypto::noise::{initiator_generate, responder_process, initiator_finalize, derive_session_key};
#[cfg(feature = "kyber")]
use nyx_crypto::noise::kyber::{responder_keypair, initiator_encapsulate, responder_decapsulate};

/// Test Noise_Nyx handshake with multiple rounds
#[test]
fn noise_handshake_multiple_rounds() {
    for round in 0..10 {
        let (init_pub, init_sec) = initiator_generate();
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        
        let key_init = derive_session_key(&shared_init);
        let key_resp = derive_session_key(&shared_resp);
        
        assert_eq!(key_init.0, key_resp.0, "Session keys must match in round {}", round);
    }
}

/// Test that different handshakes produce different keys
#[test]
fn noise_handshake_uniqueness() {
    let mut keys = Vec::new();
    
    for _ in 0..5 {
        let (init_pub, init_sec) = initiator_generate();
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        
        let key_init = derive_session_key(&shared_init);
        keys.push(key_init.0);
    }
    
    // All keys should be unique
    for i in 0..keys.len() {
        for j in (i+1)..keys.len() {
            assert_ne!(keys[i], keys[j], "Keys should be unique");
        }
    }
}

/// Test handshake with corrupted public keys
#[test]
fn noise_handshake_corrupted_keys() {
    let (init_pub, init_sec) = initiator_generate();
    
    // Create a different public key to simulate corruption
    // (since PublicKey cannot be directly mutated)
    let (corrupted_init_pub, _) = initiator_generate();
    
    let (resp_pub, shared_resp) = responder_process(&corrupted_init_pub);
    let shared_init = initiator_finalize(init_sec, &resp_pub);
    
    let key_init = derive_session_key(&shared_init);
    let key_resp = derive_session_key(&shared_resp);
    
    // Keys should still be generated but will be different due to corruption
    assert_ne!(key_init.0, key_resp.0, "Corrupted handshake should produce different keys");
}

/// Test session key derivation properties
#[test]
fn session_key_properties() {
    let (init_pub, init_sec) = initiator_generate();
    let (resp_pub, shared_resp) = responder_process(&init_pub);
    let shared_init = initiator_finalize(init_sec, &resp_pub);
    
    let key1 = derive_session_key(&shared_init);
    let key2 = derive_session_key(&shared_init);
    
    // Same shared secret should produce same session key
    assert_eq!(key1.0, key2.0);
    
    // Session key should not be all zeros
    assert_ne!(key1.0, [0u8; 32]);
    
    // Session key should not be all ones
    assert_ne!(key1.0, [0xFFu8; 32]);
}

#[cfg(feature = "kyber")]
/// Test Kyber1024 KEM handshake
#[test]
fn kyber_handshake_basic() {
    let (pk, sk) = responder_keypair();
    let (ct, key_init) = initiator_encapsulate(&pk);
    let key_resp = responder_decapsulate(&ct, &sk);
    
    assert_eq!(key_init.0, key_resp.0, "Kyber session keys must match");
}

#[cfg(feature = "kyber")]
/// Test Kyber handshake uniqueness
#[test]
fn kyber_handshake_uniqueness() {
    let (pk, sk) = responder_keypair();
    let mut keys = Vec::new();
    
    for _ in 0..5 {
        let (_, key) = initiator_encapsulate(&pk);
        keys.push(key.0);
    }
    
    // All keys should be unique (with high probability)
    for i in 0..keys.len() {
        for j in (i+1)..keys.len() {
            assert_ne!(keys[i], keys[j], "Kyber keys should be unique");
        }
    }
}

/// Test handshake performance under load
#[test]
fn noise_handshake_performance() {
    use std::time::Instant;
    
    let start = Instant::now();
    let rounds = 100;
    
    for _ in 0..rounds {
        let (init_pub, init_sec) = initiator_generate();
        let (resp_pub, shared_resp) = responder_process(&init_pub);
        let shared_init = initiator_finalize(init_sec, &resp_pub);
        
        let _key_init = derive_session_key(&shared_init);
        let _key_resp = derive_session_key(&shared_resp);
    }
    
    let duration = start.elapsed();
    let avg_ms = duration.as_millis() as f64 / rounds as f64;
    
    // Handshake should complete within reasonable time (< 10ms average)
    assert!(avg_ms < 10.0, "Average handshake time {} ms is too slow", avg_ms);
}

/// Test concurrent handshakes
#[test]
fn noise_handshake_concurrent() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    
    let success_count = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    
    for _ in 0..4 {
        let success_count = Arc::clone(&success_count);
        let handle = thread::spawn(move || {
            for _ in 0..10 {
                let (init_pub, init_sec) = initiator_generate();
                let (resp_pub, shared_resp) = responder_process(&init_pub);
                let shared_init = initiator_finalize(init_sec, &resp_pub);
                
                let key_init = derive_session_key(&shared_init);
                let key_resp = derive_session_key(&shared_resp);
                
                if key_init.0 == key_resp.0 {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    assert_eq!(success_count.load(Ordering::Relaxed), 40, "All concurrent handshakes should succeed");
} 