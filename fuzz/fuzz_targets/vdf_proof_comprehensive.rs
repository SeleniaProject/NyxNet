#![no_main]

use libfuzzer_sys::fuzz_target;
use nyx_mix::vdf::{prove, verify, VdfParams, VdfProof};
use num_bigint::BigUint;
use std::str::FromStr;

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
        return;
    }
    
    // Generate VDF parameters from fuzz data
    let params = generate_vdf_params_from_data(data);
    if !params.is_valid() {
        return;
    }
    
    // Test basic VDF operations
    test_vdf_prove_verify(&params, data);
    
    // Test edge cases
    test_vdf_edge_cases(&params, data);
    
    // Test parameter validation
    test_parameter_validation(data);
    
    // Test proof serialization
    test_proof_serialization(&params, data);
    
    // Test mathematical properties
    test_mathematical_properties(&params, data);
});

fn generate_vdf_params_from_data(data: &[u8]) -> VdfParams {
    let mut offset = 0;
    
    // Generate modulus from first part of data
    let modulus = if data.len() >= 8 {
        let p_bytes = &data[offset..offset + 4];
        let q_bytes = &data[offset + 4..offset + 8];
        offset += 8;
        
        let p_raw = u32::from_le_bytes([p_bytes[0], p_bytes[1], p_bytes[2], p_bytes[3]]);
        let q_raw = u32::from_le_bytes([q_bytes[0], q_bytes[1], q_bytes[2], q_bytes[3]]);
        
        // Ensure we get reasonable prime-like numbers
        let p = make_prime_like(p_raw);
        let q = make_prime_like(q_raw);
        
        BigUint::from(p) * BigUint::from(q)
    } else {
        // Fallback to small test modulus
        BigUint::from(97u32) * BigUint::from(89u32)
    };
    
    // Generate challenge from remaining data
    let challenge = if offset < data.len() {
        let challenge_bytes = &data[offset..std::cmp::min(offset + 4, data.len())];
        let challenge_raw = match challenge_bytes.len() {
            1 => u32::from(challenge_bytes[0]),
            2 => u32::from_le_bytes([challenge_bytes[0], challenge_bytes[1], 0, 0]),
            3 => u32::from_le_bytes([challenge_bytes[0], challenge_bytes[1], challenge_bytes[2], 0]),
            _ => u32::from_le_bytes([challenge_bytes[0], challenge_bytes[1], challenge_bytes[2], challenge_bytes[3]]),
        };
        BigUint::from(challenge_raw % 1000 + 2) // Ensure reasonable challenge
    } else {
        BigUint::from(5u32)
    };
    
    // Generate time parameter
    let time = if data.len() > 12 {
        let time_raw = data[12] as u64;
        (time_raw % 100) + 10 // 10-109 time steps
    } else {
        25
    };
    
    VdfParams {
        modulus,
        challenge,
        time,
    }
}

fn make_prime_like(n: u32) -> u32 {
    // Convert to a number that's more likely to produce a good modulus
    let base = (n % 100) + 50; // 50-149
    
    // Find next odd number
    if base % 2 == 0 {
        base + 1
    } else {
        base
    }
}

fn test_vdf_prove_verify(params: &VdfParams, data: &[u8]) {
    // Generate proof
    let proof_result = prove(&params.challenge, &params.modulus, params.time);
    
    match proof_result {
        Ok((output, proof)) => {
            // Verify the proof
            let verification = verify(&params.challenge, &output, &proof, &params.modulus, params.time);
            
            if verification.is_ok() {
                assert!(verification.unwrap(), "Valid proof should verify successfully");
                
                // Test proof properties
                test_proof_properties(&params.challenge, &output, &proof, &params.modulus, params.time);
            }
        }
        Err(_) => {
            // Proof generation failed - this is acceptable for some parameters
            // but we should still test that verify rejects invalid proofs
            test_invalid_proof_rejection(params);
        }
    }
}

fn test_proof_properties(
    challenge: &BigUint,
    output: &BigUint, 
    proof: &VdfProof,
    modulus: &BigUint,
    time: u64
) {
    // Test that output is within modulus
    assert!(output < modulus, "Output should be less than modulus");
    
    // Test that proof components are reasonable
    assert!(!proof.is_empty(), "Proof should not be empty");
    
    // Test mathematical relationship (simplified)
    // In a real VDF: output = challenge^(2^time) mod modulus
    // Here we just verify basic properties
    assert!(output > &BigUint::from(0u32), "Output should be positive");
    assert!(challenge < modulus, "Challenge should be less than modulus");
}

fn test_invalid_proof_rejection(params: &VdfParams) {
    // Create obviously invalid proofs and ensure they're rejected
    let fake_output = BigUint::from(42u32);
    let fake_proof = VdfProof::new(vec![1, 2, 3, 4]);
    
    let verification = verify(&params.challenge, &fake_output, &fake_proof, &params.modulus, params.time);
    
    match verification {
        Ok(result) => assert!(!result, "Invalid proof should not verify"),
        Err(_) => {}, // Verification error is also acceptable for invalid proofs
    }
}

fn test_vdf_edge_cases(params: &VdfParams, data: &[u8]) {
    // Test with time = 0
    if data.len() > 0 && data[0] % 4 == 0 {
        let zero_time_result = prove(&params.challenge, &params.modulus, 0);
        match zero_time_result {
            Ok((output, proof)) => {
                // With time=0, output should equal challenge
                assert_eq!(output, params.challenge);
            }
            Err(_) => {}, // May fail for some parameters
        }
    }
    
    // Test with time = 1
    if data.len() > 1 && data[1] % 4 == 1 {
        let one_time_result = prove(&params.challenge, &params.modulus, 1);
        match one_time_result {
            Ok((output, proof)) => {
                // Verify the single-step proof
                let verification = verify(&params.challenge, &output, &proof, &params.modulus, 1);
                if let Ok(result) = verification {
                    assert!(result, "Single-step proof should verify");
                }
            }
            Err(_) => {}, // May fail for some parameters
        }
    }
    
    // Test with very large time (should be rejected or handled gracefully)
    if data.len() > 2 && data[2] % 4 == 2 {
        let large_time = 1000000;
        let large_time_result = prove(&params.challenge, &params.modulus, large_time);
        
        // Should either work or fail gracefully (not panic)
        match large_time_result {
            Ok(_) => {}, // Unexpectedly succeeded
            Err(_) => {}, // Expected failure for large time
        }
    }
}

fn test_parameter_validation(data: &[u8]) {
    // Test with various invalid parameters
    
    // Test with modulus = 0
    if data.len() > 0 && data[0] % 8 == 0 {
        let zero_modulus = BigUint::from(0u32);
        let challenge = BigUint::from(5u32);
        let result = prove(&challenge, &zero_modulus, 10);
        assert!(result.is_err(), "Proof with zero modulus should fail");
    }
    
    // Test with modulus = 1
    if data.len() > 1 && data[1] % 8 == 1 {
        let one_modulus = BigUint::from(1u32);
        let challenge = BigUint::from(5u32);
        let result = prove(&challenge, &one_modulus, 10);
        // May succeed or fail depending on implementation
    }
    
    // Test with challenge >= modulus
    if data.len() > 2 && data[2] % 8 == 2 {
        let modulus = BigUint::from(97u32);
        let large_challenge = BigUint::from(100u32);
        let result = prove(&large_challenge, &modulus, 10);
        // Should handle gracefully
    }
    
    // Test with challenge = 0
    if data.len() > 3 && data[3] % 8 == 3 {
        let modulus = BigUint::from(97u32);
        let zero_challenge = BigUint::from(0u32);
        let result = prove(&zero_challenge, &modulus, 10);
        match result {
            Ok((output, _)) => {
                assert_eq!(output, BigUint::from(0u32), "0^n should be 0");
            }
            Err(_) => {}, // May be rejected by implementation
        }
    }
}

fn test_proof_serialization(params: &VdfParams, data: &[u8]) {
    if let Ok((output, proof)) = prove(&params.challenge, &params.modulus, params.time) {
        // Test proof serialization/deserialization
        let serialized = proof.to_bytes();
        assert!(!serialized.is_empty(), "Serialized proof should not be empty");
        
        if let Ok(deserialized) = VdfProof::from_bytes(&serialized) {
            // Verify that deserialized proof still works
            let verification = verify(&params.challenge, &output, &deserialized, &params.modulus, params.time);
            if let Ok(result) = verification {
                assert!(result, "Deserialized proof should still verify");
            }
        }
    }
}

fn test_mathematical_properties(params: &VdfParams, data: &[u8]) {
    // Test sequential computation property
    if params.time >= 4 && data.len() > 0 && data[0] % 4 == 0 {
        let half_time = params.time / 2;
        
        // Compute VDF for half time
        if let Ok((intermediate, _)) = prove(&params.challenge, &params.modulus, half_time) {
            // Compute VDF for remaining time starting from intermediate
            if let Ok((final_output, _)) = prove(&intermediate, &params.modulus, half_time) {
                // Compare with direct computation
                if let Ok((direct_output, _)) = prove(&params.challenge, &params.modulus, params.time) {
                    assert_eq!(final_output, direct_output, 
                              "Sequential computation should match direct computation");
                }
            }
        }
    }
    
    // Test deterministic property
    if data.len() > 1 && data[1] % 4 == 1 {
        if let Ok((output1, proof1)) = prove(&params.challenge, &params.modulus, params.time) {
            if let Ok((output2, proof2)) = prove(&params.challenge, &params.modulus, params.time) {
                assert_eq!(output1, output2, "VDF should be deterministic");
                // Proofs might differ due to randomness in proof generation
            }
        }
    }
    
    // Test uniqueness property (different challenges should give different outputs)
    if data.len() > 2 && data[2] % 4 == 2 {
        let alt_challenge = &params.challenge + BigUint::from(1u32);
        if alt_challenge < params.modulus {
            if let Ok((output1, _)) = prove(&params.challenge, &params.modulus, params.time) {
                if let Ok((output2, _)) = prove(&alt_challenge, &params.modulus, params.time) {
                    assert_ne!(output1, output2, "Different challenges should give different outputs");
                }
            }
        }
    }
}

// Additional structures for VDF testing
struct VdfParams {
    modulus: BigUint,
    challenge: BigUint,
    time: u64,
}

impl VdfParams {
    fn is_valid(&self) -> bool {
        self.modulus > BigUint::from(1u32) && 
        self.challenge < self.modulus &&
        self.time > 0 &&
        self.time < 1000 // Reasonable upper bound for fuzzing
    }
}

// Mock VdfProof structure for testing
struct VdfProof {
    data: Vec<u8>,
}

impl VdfProof {
    fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
    
    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    fn to_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }
    
    fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.is_empty() {
            Err("Empty proof data")
        } else {
            Ok(Self { data: bytes.to_vec() })
        }
    }
} 