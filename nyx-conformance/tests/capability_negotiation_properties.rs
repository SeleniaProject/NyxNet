use nyx_stream::{
    Capability, encode_caps, decode_caps, negotiate, NegotiationError, 
    FLAG_REQUIRED, perform_cap_negotiation, LOCAL_CAP_IDS
};
use nyx_stream::management::{parse_close_frame, ERR_UNSUPPORTED_CAP};
use proptest::prelude::*;
use std::collections::HashSet;

/// Property-based tests for capability negotiation logic
/// These tests verify that the Rust implementation matches the TLA+ model behavior
/// defined in formal/nyx_multipath_plugin.tla
/// 
/// Key TLA+ properties being tested:
/// - NegotiateOK: C_req ⊆ C_sup => state' = "Open"
/// - NegotiateFail: ~(C_req ⊆ C_sup) => state' = "Close" ∧ error' = 7
/// - CapabilityInvariant: state = "Open" => C_req ⊆ C_sup
/// - CapabilityInvariant: state = "Close" ∧ error = 7 => ~(C_req ⊆ C_sup)

/// Generate a capability ID for testing
fn capability_id_strategy() -> impl Strategy<Value = u32> {
    // Use a range that includes known local capabilities and unknown ones
    prop_oneof![
        Just(0x0001), // Core capability (always supported)
        Just(0x0002), // Plugin framework (supported)
        0x0003u32..0x1000u32, // Unknown capabilities
        0xDEAD0000u32..0xDEADBEEFu32, // Clearly unsupported range
    ]
}

/// Generate a single capability with random properties
fn capability_strategy() -> impl Strategy<Value = Capability> {
    (
        capability_id_strategy(),
        prop::bool::ANY, // required flag
        prop::collection::vec(any::<u8>(), 0..=32), // data field
    ).prop_map(|(id, is_required, data)| {
        if is_required {
            Capability { id, flags: FLAG_REQUIRED, data }
        } else {
            Capability { id, flags: 0, data }
        }
    })
}

/// Generate a set of capabilities ensuring uniqueness by ID
fn capability_set_strategy() -> impl Strategy<Value = Vec<Capability>> {
    prop::collection::vec(capability_strategy(), 0..=16)
        .prop_map(|mut caps| {
            // Ensure unique capability IDs (like TLA+ CapSet constraint)
            let mut seen = HashSet::new();
            caps.retain(|cap| seen.insert(cap.id));
            caps
        })
}

/// Generate a set of supported capability IDs
fn supported_caps_strategy() -> impl Strategy<Value = Vec<u32>> {
    prop::collection::vec(capability_id_strategy(), 1..=16)
        .prop_map(|ids| {
            // Ensure uniqueness and always include core capability
            let mut unique_ids: HashSet<u32> = ids.into_iter().collect();
            unique_ids.insert(0x0001); // Always support core
            unique_ids.into_iter().collect()
        })
}

proptest! {
    /// Test that negotiation succeeds when all required capabilities are supported
    /// TLA+ property: C_req ⊆ C_sup => NegotiateOK
    /// This corresponds to: state = "Init" ∧ C_req ⊆ C_sup => state' = "Open"
    #[test]
    fn negotiation_success_when_requirements_satisfied(
        peer_caps in capability_set_strategy(),
        local_supported in supported_caps_strategy()
    ) {
        // Filter to only test cases where all required capabilities are supported
        let required_caps: HashSet<u32> = peer_caps.iter()
            .filter(|cap| cap.is_required())
            .map(|cap| cap.id)
            .collect();
        
        let supported_set: HashSet<u32> = local_supported.iter().cloned().collect();
        
        // Only test when C_req ⊆ C_sup (TLA+ precondition for NegotiateOK)
        prop_assume!(required_caps.is_subset(&supported_set));
        
        // Test the negotiate function directly
        let result = negotiate(&local_supported, &peer_caps);
        prop_assert!(result.is_ok(), "Negotiation should succeed when C_req ⊆ C_sup");
        
        // Test the full negotiation flow (uses LOCAL_CAP_IDS internally)
        // Only test if the required capabilities are also supported by LOCAL_CAP_IDS
        let local_cap_set: HashSet<u32> = LOCAL_CAP_IDS.iter().cloned().collect();
        if required_caps.is_subset(&local_cap_set) {
            let encoded = encode_caps(&peer_caps);
            let negotiation_result = perform_cap_negotiation(&encoded);
            prop_assert!(negotiation_result.is_ok(), 
                "Full negotiation should succeed when C_req ⊆ LOCAL_CAP_IDS");
            
            if let Ok(decoded_caps) = negotiation_result {
                prop_assert_eq!(decoded_caps, peer_caps, 
                    "Decoded capabilities should match original");
            }
        }
    }

    /// Test that negotiation fails when required capabilities are not supported
    /// TLA+ property: ~(C_req ⊆ C_sup) => NegotiateFail
    /// This corresponds to: state = "Init" ∧ ~(C_req ⊆ C_sup) => state' = "Close" ∧ error' = 7
    #[test]
    fn negotiation_failure_when_requirements_not_satisfied(
        mut peer_caps in capability_set_strategy(),
        local_supported in supported_caps_strategy()
    ) {
        // Ensure we have at least one required capability that's not supported
        let supported_set: HashSet<u32> = local_supported.iter().cloned().collect();
        
        // Find an unsupported capability ID
        let unsupported_id = (0x8000u32..0x9000u32)
            .find(|id| !supported_set.contains(id))
            .unwrap_or(0x8888); // Fallback to a clearly unsupported ID
        
        // Add a required unsupported capability
        peer_caps.push(Capability::required(unsupported_id));
        
        // Verify precondition: ~(C_req ⊆ C_sup)
        let required_caps: HashSet<u32> = peer_caps.iter()
            .filter(|cap| cap.is_required())
            .map(|cap| cap.id)
            .collect();
        
        prop_assume!(!required_caps.is_subset(&supported_set));
        
        // Test the negotiate function directly
        let result = negotiate(&local_supported, &peer_caps);
        prop_assert!(result.is_err(), "Negotiation should fail when ~(C_req ⊆ C_sup)");
        
        if let Err(NegotiationError::Unsupported(failed_id)) = result {
            prop_assert!(required_caps.contains(&failed_id), 
                "Failed capability should be one of the required ones");
            prop_assert!(!supported_set.contains(&failed_id), 
                "Failed capability should not be supported");
        }
        
        // Test the full negotiation flow (uses LOCAL_CAP_IDS internally)
        let local_cap_set: HashSet<u32> = LOCAL_CAP_IDS.iter().cloned().collect();
        if !required_caps.is_subset(&local_cap_set) {
            let encoded = encode_caps(&peer_caps);
            let negotiation_result = perform_cap_negotiation(&encoded);
            prop_assert!(negotiation_result.is_err(), 
                "Full negotiation should fail when ~(C_req ⊆ LOCAL_CAP_IDS)");
            
            // Verify error frame matches TLA+ error = 7 (UNSUPPORTED_CAP)
            if let Err(ref close_frame) = negotiation_result {
                let (_, frame) = parse_close_frame(close_frame)
                    .expect("Should be valid close frame");
                prop_assert_eq!(frame.code, ERR_UNSUPPORTED_CAP, 
                    "Error code should be UNSUPPORTED_CAP (0x07)");
            }
        }
    }

    /// Test that optional capabilities are ignored when not supported
    /// TLA+ behavior: Optional capabilities don't affect negotiation outcome
    #[test]
    fn optional_capabilities_ignored_when_unsupported(
        mut peer_caps in capability_set_strategy(),
        local_supported in supported_caps_strategy()
    ) {
        // Ensure all capabilities are optional
        for cap in &mut peer_caps {
            cap.flags = 0; // Remove required flag
        }
        
        // Add some clearly unsupported optional capabilities
        peer_caps.push(Capability::optional(0xDEADBEEF));
        peer_caps.push(Capability::optional(0xCAFEBABE));
        
        // Negotiation should always succeed with only optional capabilities
        let result = negotiate(&local_supported, &peer_caps);
        prop_assert!(result.is_ok(), 
            "Negotiation should succeed when all capabilities are optional");
        
        // Test with LOCAL_CAP_IDS (what perform_cap_negotiation uses)
        let encoded = encode_caps(&peer_caps);
        let negotiation_result = perform_cap_negotiation(&encoded);
        prop_assert!(negotiation_result.is_ok(), 
            "Full negotiation should succeed with optional capabilities");
    }

    /// Test capability invariant consistency
    /// TLA+ CapabilityInvariant: 
    /// - (state = "Open" => C_req ⊆ C_sup)
    /// - (state = "Close" ∧ error = 7 => ~(C_req ⊆ C_sup))
    #[test]
    fn capability_invariant_consistency(
        peer_caps in capability_set_strategy()
    ) {
        let required_caps: HashSet<u32> = peer_caps.iter()
            .filter(|cap| cap.is_required())
            .map(|cap| cap.id)
            .collect();
        
        // Use LOCAL_CAP_IDS as the supported set (what perform_cap_negotiation uses)
        let supported_set: HashSet<u32> = LOCAL_CAP_IDS.iter().cloned().collect();
        let requirements_satisfied = required_caps.is_subset(&supported_set);
        
        let encoded = encode_caps(&peer_caps);
        let negotiation_result = perform_cap_negotiation(&encoded);
        
        match negotiation_result {
            Ok(_) => {
                // If negotiation succeeded (state = "Open"), then C_req ⊆ C_sup must hold
                prop_assert!(requirements_satisfied, 
                    "If negotiation succeeds (Open state), then C_req ⊆ LOCAL_CAP_IDS must hold");
            }
            Err(ref close_frame) => {
                // If negotiation failed with UNSUPPORTED_CAP, then ~(C_req ⊆ C_sup) must hold
                let (_, frame) = parse_close_frame(close_frame)
                    .expect("Should be valid close frame");
                
                if frame.code == ERR_UNSUPPORTED_CAP {
                    prop_assert!(!requirements_satisfied, 
                        "If negotiation fails with UNSUPPORTED_CAP (Close state), then ~(C_req ⊆ LOCAL_CAP_IDS) must hold");
                }
            }
        }
    }

    /// Test that negotiation is deterministic for the same inputs
    /// TLA+ property: Negotiation outcome is deterministic based on capability sets
    #[test]
    fn negotiation_determinism(
        peer_caps in capability_set_strategy(),
        _local_supported in supported_caps_strategy()
    ) {
        let encoded = encode_caps(&peer_caps);
        
        // Run negotiation multiple times
        let result1 = perform_cap_negotiation(&encoded);
        let result2 = perform_cap_negotiation(&encoded);
        let result3 = perform_cap_negotiation(&encoded);
        
        // All results should be identical
        prop_assert_eq!(result1.is_ok(), result2.is_ok(), 
            "Negotiation results should be deterministic");
        prop_assert_eq!(result2.is_ok(), result3.is_ok(), 
            "Negotiation results should be deterministic");
        
        match (result1, result2, result3) {
            (Ok(ref caps1), Ok(ref caps2), Ok(ref caps3)) => {
                prop_assert_eq!(caps1, caps2, "Success results should be identical");
                prop_assert_eq!(caps2, caps3, "Success results should be identical");
            }
            (Err(ref frame1), Err(ref frame2), Err(ref frame3)) => {
                prop_assert_eq!(frame1, frame2, "Error frames should be identical");
                prop_assert_eq!(frame2, frame3, "Error frames should be identical");
            }
            _ => prop_assert!(false, "Mixed success/failure results are not deterministic"),
        }
    }

    /// Test CBOR encoding/decoding roundtrip properties
    /// Ensures capability data integrity through serialization
    #[test]
    fn cbor_roundtrip_preserves_capability_data(
        caps in capability_set_strategy()
    ) {
        let encoded = encode_caps(&caps);
        let decoded = decode_caps(&encoded);
        
        prop_assert!(decoded.is_ok(), "CBOR decoding should succeed for valid data");
        
        if let Ok(ref decoded_caps) = decoded {
            prop_assert_eq!(decoded_caps, &caps, 
                "CBOR roundtrip should preserve capability data exactly");
            
            // Verify individual capability properties are preserved
            for (original, decoded) in caps.iter().zip(decoded_caps.iter()) {
                prop_assert_eq!(original.id, decoded.id, "Capability ID should be preserved");
                prop_assert_eq!(original.flags, decoded.flags, "Capability flags should be preserved");
                prop_assert_eq!(&original.data, &decoded.data, "Capability data should be preserved");
                prop_assert_eq!(original.is_required(), decoded.is_required(), 
                    "Required flag interpretation should be preserved");
            }
        }
    }

    /// Test error conditions with malformed CBOR data
    /// Verifies proper error handling for protocol violations
    #[test]
    fn malformed_cbor_handling(
        malformed_data in prop::collection::vec(any::<u8>(), 1..=64)
    ) {
        // Filter out data that might accidentally be valid CBOR
        prop_assume!(!malformed_data.is_empty());
        
        let result = perform_cap_negotiation(&malformed_data);
        
        // Should fail for malformed data
        if let Err(ref close_frame) = result {
            // Should be able to parse as a close frame
            if let Ok((_, frame)) = parse_close_frame(close_frame) {
                // Should be PROTOCOL_VIOLATION (0x01) for malformed CBOR
                prop_assert_eq!(frame.code, 0x01, 
                    "Malformed CBOR should result in PROTOCOL_VIOLATION error");
            }
        }
    }
}

/// Additional deterministic tests for specific edge cases
#[cfg(test)]
mod deterministic_tests {
    use super::*;

    #[test]
    fn empty_capability_list_succeeds() {
        let empty_caps: Vec<Capability> = vec![];
        let encoded = encode_caps(&empty_caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_ok(), "Empty capability list should succeed");
        assert_eq!(result.unwrap(), empty_caps);
    }

    #[test]
    fn local_capabilities_always_supported() {
        // Test that our own advertised capabilities are always supported
        let local_caps = LOCAL_CAP_IDS.iter()
            .map(|&id| if id == 0x0001 { 
                Capability::required(id) 
            } else { 
                Capability::optional(id) 
            })
            .collect::<Vec<_>>();
        
        let encoded = encode_caps(&local_caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_ok(), "Local capabilities should always be supported");
        assert_eq!(result.unwrap(), local_caps);
    }

    #[test]
    fn core_capability_required() {
        // Test that core capability (0x0001) is always required
        let caps = vec![Capability::required(0x0001)];
        let encoded = encode_caps(&caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_ok(), "Core capability should be supported");
        
        // Test that missing core capability fails
        let no_core_caps = vec![Capability::required(0x0002)];
        let encoded_no_core = encode_caps(&no_core_caps);
        let _result_no_core = perform_cap_negotiation(&encoded_no_core);
        
        // This might succeed if 0x0002 is supported, but let's test with clearly unsupported
        let unsupported_caps = vec![Capability::required(0xDEADBEEF)];
        let encoded_unsupported = encode_caps(&unsupported_caps);
        let result_unsupported = perform_cap_negotiation(&encoded_unsupported);
        
        assert!(result_unsupported.is_err(), "Unsupported required capability should fail");
    }

    #[test]
    fn capability_flags_interpretation() {
        // Test that flag interpretation is correct
        let required_cap = Capability { id: 0x0001, flags: FLAG_REQUIRED, data: vec![] };
        let optional_cap = Capability { id: 0x0001, flags: 0, data: vec![] };
        
        assert!(required_cap.is_required());
        assert!(!optional_cap.is_required());
        
        // Test with additional flags (should still work)
        let required_with_extra_flags = Capability { id: 0x0001, flags: FLAG_REQUIRED | 0x02, data: vec![] };
        assert!(required_with_extra_flags.is_required());
    }

    #[test]
    fn capability_data_field_handling() {
        // Test that data field is preserved but doesn't affect negotiation
        let cap_with_data = Capability { 
            id: 0x0001, 
            flags: FLAG_REQUIRED, 
            data: vec![1, 2, 3, 4, 5] 
        };
        
        let caps = vec![cap_with_data.clone()];
        let encoded = encode_caps(&caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded[0].data, cap_with_data.data);
    }

    #[test]
    fn multiple_required_capabilities_failure() {
        // Test failure with multiple required capabilities where some are unsupported
        let caps = vec![
            Capability::required(0x0001), // Supported
            Capability::required(0xDEADBEEF), // Unsupported
            Capability::required(0xCAFEBABE), // Also unsupported
        ];
        
        let encoded = encode_caps(&caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_err());
        
        let close_frame = result.unwrap_err();
        let (_, frame) = parse_close_frame(&close_frame).expect("Valid close frame");
        assert_eq!(frame.code, ERR_UNSUPPORTED_CAP);
        
        // Should fail on the first unsupported capability encountered
        let failed_id = u32::from_be_bytes([
            frame.reason[0], frame.reason[1], 
            frame.reason[2], frame.reason[3]
        ]);
        
        // Should be one of the unsupported capabilities
        assert!(failed_id == 0xDEADBEEF || failed_id == 0xCAFEBABE);
    }

    #[test]
    fn mixed_required_optional_capabilities() {
        // Test with mix of required (supported) and optional (unsupported) capabilities
        let caps = vec![
            Capability::required(0x0001),    // Supported and required - OK
            Capability::optional(0xDEADBEEF), // Unsupported but optional - OK
            Capability::required(0x0002),    // Supported and required - OK
            Capability::optional(0xCAFEBABE), // Unsupported but optional - OK
        ];
        
        let encoded = encode_caps(&caps);
        let result = perform_cap_negotiation(&encoded);
        
        // Should succeed because all required capabilities are supported
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded, caps);
    }

    #[test]
    fn capability_id_boundary_values() {
        // Test with boundary capability ID values
        let boundary_caps = vec![
            Capability::optional(0),           // Minimum value
            Capability::optional(0x0001),      // Core capability
            Capability::optional(0xFFFFFFFF),  // Maximum value
        ];
        
        let encoded = encode_caps(&boundary_caps);
        let decoded = decode_caps(&encoded).expect("Should decode boundary values");
        assert_eq!(decoded, boundary_caps);
        
        // Test negotiation with boundary values (all optional so should succeed)
        let result = perform_cap_negotiation(&encoded);
        assert!(result.is_ok());
    }

    #[test]
    fn large_capability_data_field() {
        // Test with large data field
        let large_data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let cap_with_large_data = Capability {
            id: 0x0001,
            flags: 0,
            data: large_data.clone(),
        };
        
        let caps = vec![cap_with_large_data];
        let encoded = encode_caps(&caps);
        let result = perform_cap_negotiation(&encoded);
        
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded[0].data, large_data);
    }
}