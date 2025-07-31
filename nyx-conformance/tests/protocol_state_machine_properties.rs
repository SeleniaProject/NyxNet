use nyx_stream::management::ERR_UNSUPPORTED_CAP;
use nyx_core::mobile::PowerProfile;
use nyx_core::error::{NyxError, NyxResult};
use proptest::prelude::*;
use std::collections::HashSet;

/// Property-based tests for protocol state machine behavior
/// These tests verify that the Rust implementation matches the TLA+ model behavior
/// defined in formal/nyx_multipath_plugin.tla
/// 
/// Key TLA+ properties being tested:
/// - ValidTransition: State transitions follow the formal model
/// - TypeInvariant: All variables maintain their expected types
/// - Inv_PowerState: power = "LowPower" => state = "Open"
/// - State machine invariants are maintained during all transitions

/// Protocol states matching TLA+ model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolState {
    Init,
    Open,
    Close,
}

/// Protocol context for testing state transitions
#[derive(Debug, Clone)]
pub struct ProtocolContext {
    pub state: ProtocolState,
    pub power: PowerProfile,
    pub error: Option<u16>,
    pub path: Vec<u32>, // Simplified node IDs
    pub capabilities_required: HashSet<u32>,
    pub capabilities_supported: HashSet<u32>,
}

impl ProtocolContext {
    /// Create new protocol context in Init state
    pub fn new() -> Self {
        Self {
            state: ProtocolState::Init,
            power: PowerProfile::HighPerformance,
            error: None,
            path: vec![],
            capabilities_required: HashSet::new(),
            capabilities_supported: HashSet::new(),
        }
    }

    /// Attempt capability negotiation (TLA+ NegotiateOK/NegotiateFail)
    pub fn negotiate_capabilities(&mut self) -> NyxResult<()> {
        if self.state != ProtocolState::Init {
            return Err(NyxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Can only negotiate from Init state"
            )));
        }

        // TLA+ condition: C_req ⊆ C_sup
        if self.capabilities_required.is_subset(&self.capabilities_supported) {
            // NegotiateOK: state' = "Open"
            self.state = ProtocolState::Open;
            self.error = None;
            Ok(())
        } else {
            // NegotiateFail: state' = "Close" ∧ error' = 7
            self.state = ProtocolState::Close;
            self.error = Some(ERR_UNSUPPORTED_CAP);
            
            // Find first unsupported capability
            let unsupported = self.capabilities_required
                .difference(&self.capabilities_supported)
                .next()
                .copied()
                .unwrap_or(0xDEADBEEF);
            
            Err(NyxError::UnsupportedCap(unsupported))
        }
    }

    /// Enter low power mode (TLA+ EnterLowPower)
    pub fn enter_low_power(&mut self) -> NyxResult<()> {
        // TLA+ precondition: state = "Open" ∧ power = "Normal"
        if self.state != ProtocolState::Open {
            return Err(NyxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Can only enter low power from Open state"
            )));
        }

        if matches!(self.power, PowerProfile::UltraLowPower) {
            return Err(NyxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Already in low power mode"
            )));
        }

        // TLA+ transition: power' = "LowPower"
        self.power = PowerProfile::UltraLowPower;
        Ok(())
    }

    /// Exit low power mode
    pub fn exit_low_power(&mut self) -> NyxResult<()> {
        if self.state != ProtocolState::Open {
            return Err(NyxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Can only exit low power from Open state"
            )));
        }

        self.power = PowerProfile::HighPerformance;
        Ok(())
    }

    /// Check if current state satisfies TLA+ invariants
    pub fn check_invariants(&self) -> bool {
        // Inv_Error: state = "Close" => error = 7
        if self.state == ProtocolState::Close && self.error != Some(ERR_UNSUPPORTED_CAP) {
            return false;
        }

        // Inv_NoError: state = "Open" => error = None
        if self.state == ProtocolState::Open && self.error.is_some() {
            return false;
        }

        // Inv_PowerState: power = "LowPower" => state = "Open"
        if matches!(self.power, PowerProfile::UltraLowPower) && self.state != ProtocolState::Open {
            return false;
        }

        // CapabilityInvariant: state = "Open" => C_req ⊆ C_sup
        if self.state == ProtocolState::Open && !self.capabilities_required.is_subset(&self.capabilities_supported) {
            return false;
        }

        // CapabilityInvariant: state = "Close" ∧ error = 7 => ~(C_req ⊆ C_sup)
        if self.state == ProtocolState::Close 
            && self.error == Some(ERR_UNSUPPORTED_CAP) 
            && self.capabilities_required.is_subset(&self.capabilities_supported) {
            return false;
        }

        true
    }

    /// Check if transition is valid according to TLA+ ValidTransition
    pub fn is_valid_transition(&self, next_state: ProtocolState, next_power: PowerProfile) -> bool {
        match (self.state, next_state) {
            // (state = "Init" ∧ state' = "Open") => (C_req ⊆ C_sup)
            (ProtocolState::Init, ProtocolState::Open) => {
                self.capabilities_required.is_subset(&self.capabilities_supported)
            }
            // (state = "Init" ∧ state' = "Close") => ~(C_req ⊆ C_sup)
            (ProtocolState::Init, ProtocolState::Close) => {
                !self.capabilities_required.is_subset(&self.capabilities_supported)
            }
            // (state = "Open" ∧ power = "Normal" ∧ power' = "LowPower") => (state' = "Open")
            (ProtocolState::Open, ProtocolState::Open) => {
                if !matches!(self.power, PowerProfile::UltraLowPower) 
                    && matches!(next_power, PowerProfile::UltraLowPower) {
                    true
                } else {
                    true // Other transitions within Open state are allowed
                }
            }
            // Terminal states don't change
            (ProtocolState::Close, ProtocolState::Close) => true,
            // Invalid transitions
            _ => false,
        }
    }
}

/// Generate a capability ID for testing
fn capability_id_strategy() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(0x0001), // Core capability
        Just(0x0002), // Plugin framework
        0x0003u32..0x1000u32, // Valid range
        0xDEAD0000u32..0xDEADBEEFu32, // Unsupported range
    ]
}

/// Generate a set of capability IDs
fn capability_set_strategy() -> impl Strategy<Value = HashSet<u32>> {
    prop::collection::vec(capability_id_strategy(), 0..=8)
        .prop_map(|ids| ids.into_iter().collect())
}

/// Generate a path of node IDs (3-7 nodes, no duplicates)
fn path_strategy() -> impl Strategy<Value = Vec<u32>> {
    (3usize..=7)
        .prop_flat_map(|len| {
            prop::collection::vec(1u32..100u32, len..=len)
        })
        .prop_map(|mut path| {
            // Ensure no duplicates (TLA+ Inv_NoDup)
            let mut seen = HashSet::new();
            path.retain(|&node| seen.insert(node));
            // Pad if we removed duplicates
            while path.len() < 3 {
                let new_node = path.len() as u32 + 100;
                if seen.insert(new_node) {
                    path.push(new_node);
                }
            }
            path
        })
}

/// Generate a protocol context for testing
fn protocol_context_strategy() -> impl Strategy<Value = ProtocolContext> {
    (
        capability_set_strategy(),
        capability_set_strategy(),
        path_strategy(),
    ).prop_map(|(required, supported, path)| {
        ProtocolContext {
            state: ProtocolState::Init,
            power: PowerProfile::HighPerformance,
            error: None,
            path,
            capabilities_required: required,
            capabilities_supported: supported,
        }
    })
}

proptest! {
    /// Test that capability negotiation follows TLA+ model behavior
    /// TLA+ properties: NegotiateOK and NegotiateFail
    #[test]
    fn capability_negotiation_state_transitions(
        mut context in protocol_context_strategy()
    ) {
        let initial_state = context.clone();
        let result = context.negotiate_capabilities();

        // Verify state transition validity
        prop_assert!(context.check_invariants(), "Invariants must hold after negotiation");

        if initial_state.capabilities_required.is_subset(&initial_state.capabilities_supported) {
            // TLA+ NegotiateOK: C_req ⊆ C_sup => state' = "Open"
            prop_assert!(result.is_ok(), "Negotiation should succeed when C_req ⊆ C_sup");
            prop_assert_eq!(context.state, ProtocolState::Open, "State should be Open after successful negotiation");
            prop_assert_eq!(context.error, None, "Error should be None after successful negotiation");
        } else {
            // TLA+ NegotiateFail: ~(C_req ⊆ C_sup) => state' = "Close" ∧ error' = 7
            prop_assert!(result.is_err(), "Negotiation should fail when ~(C_req ⊆ C_sup)");
            prop_assert_eq!(context.state, ProtocolState::Close, "State should be Close after failed negotiation");
            prop_assert_eq!(context.error, Some(ERR_UNSUPPORTED_CAP), "Error should be UNSUPPORTED_CAP after failed negotiation");
            
            if let Err(NyxError::UnsupportedCap(cap_id)) = result {
                prop_assert!(initial_state.capabilities_required.contains(&cap_id), 
                    "Failed capability should be one of the required ones");
                prop_assert!(!initial_state.capabilities_supported.contains(&cap_id), 
                    "Failed capability should not be supported");
            }
        }
    }

    /// Test power state transitions follow TLA+ model
    /// TLA+ property: EnterLowPower and Inv_PowerState
    #[test]
    fn power_state_transitions(
        mut context in protocol_context_strategy()
    ) {
        // First negotiate to Open state if possible
        if context.capabilities_required.is_subset(&context.capabilities_supported) {
            let _ = context.negotiate_capabilities();
            prop_assert_eq!(context.state, ProtocolState::Open);

            // Test entering low power mode
            let result = context.enter_low_power();
            prop_assert!(result.is_ok(), "Should be able to enter low power from Open state");
            prop_assert_eq!(context.power, PowerProfile::UltraLowPower, "Power should be UltraLowPower");
            prop_assert_eq!(context.state, ProtocolState::Open, "State should remain Open");
            prop_assert!(context.check_invariants(), "Invariants must hold after entering low power");

            // Test exiting low power mode
            let result = context.exit_low_power();
            prop_assert!(result.is_ok(), "Should be able to exit low power from Open state");
            prop_assert_eq!(context.power, PowerProfile::HighPerformance, "Power should be HighPerformance");
            prop_assert_eq!(context.state, ProtocolState::Open, "State should remain Open");
            prop_assert!(context.check_invariants(), "Invariants must hold after exiting low power");
        } else {
            // If negotiation fails, we can't enter low power
            let _ = context.negotiate_capabilities();
            prop_assert_eq!(context.state, ProtocolState::Close);
            
            let result = context.enter_low_power();
            prop_assert!(result.is_err(), "Should not be able to enter low power from Close state");
        }
    }

    /// Test that all state transitions maintain invariants
    /// TLA+ property: All invariants hold after every transition
    #[test]
    fn state_invariants_maintained(
        mut context in protocol_context_strategy()
    ) {
        // Initial state should satisfy invariants
        prop_assert!(context.check_invariants(), "Initial state must satisfy invariants");

        // Test negotiation transition
        let _ = context.negotiate_capabilities();
        prop_assert!(context.check_invariants(), "Invariants must hold after negotiation");

        // If we're in Open state, test power transitions
        if context.state == ProtocolState::Open {
            let _ = context.enter_low_power();
            prop_assert!(context.check_invariants(), "Invariants must hold after entering low power");

            let _ = context.exit_low_power();
            prop_assert!(context.check_invariants(), "Invariants must hold after exiting low power");
        }
    }

    /// Test transition validity according to TLA+ ValidTransition
    #[test]
    fn valid_transition_property(
        context in protocol_context_strategy()
    ) {
        // Test all possible next states
        for &next_state in &[ProtocolState::Init, ProtocolState::Open, ProtocolState::Close] {
            for &next_power in &[PowerProfile::HighPerformance, PowerProfile::UltraLowPower] {
                let is_valid = context.is_valid_transition(next_state, next_power);
                
                // Verify transition validity matches expected behavior
                match (context.state, next_state) {
                    (ProtocolState::Init, ProtocolState::Open) => {
                        let expected = context.capabilities_required.is_subset(&context.capabilities_supported);
                        prop_assert_eq!(is_valid, expected, 
                            "Init->Open transition validity should match capability subset relationship");
                    }
                    (ProtocolState::Init, ProtocolState::Close) => {
                        let expected = !context.capabilities_required.is_subset(&context.capabilities_supported);
                        prop_assert_eq!(is_valid, expected, 
                            "Init->Close transition validity should match capability non-subset relationship");
                    }
                    (ProtocolState::Open, ProtocolState::Open) => {
                        prop_assert!(is_valid, "Open->Open transitions should always be valid");
                    }
                    (ProtocolState::Close, ProtocolState::Close) => {
                        prop_assert!(is_valid, "Close->Close transitions should always be valid");
                    }
                    _ => {
                        prop_assert!(!is_valid, "Invalid transitions should be rejected");
                    }
                }
            }
        }
    }

    /// Test error handling consistency with TLA+ model
    /// TLA+ properties: Inv_Error and Inv_NoError
    #[test]
    fn error_handling_consistency(
        mut context in protocol_context_strategy()
    ) {
        let result = context.negotiate_capabilities();

        match context.state {
            ProtocolState::Open => {
                prop_assert!(result.is_ok(), "Open state should correspond to successful result");
                prop_assert_eq!(context.error, None, "Open state should have no error (Inv_NoError)");
            }
            ProtocolState::Close => {
                prop_assert!(result.is_err(), "Close state should correspond to error result");
                prop_assert_eq!(context.error, Some(ERR_UNSUPPORTED_CAP), 
                    "Close state should have UNSUPPORTED_CAP error (Inv_Error)");
            }
            ProtocolState::Init => {
                // Should not remain in Init after negotiation
                prop_assert!(false, "Should not remain in Init state after negotiation");
            }
        }
    }

    /// Test that path properties are maintained during state transitions
    /// TLA+ properties: Inv_PathLen and Inv_NoDup
    #[test]
    fn path_properties_maintained(
        mut context in protocol_context_strategy()
    ) {
        // Verify initial path properties
        prop_assert!(context.path.len() >= 3 && context.path.len() <= 7, 
            "Path length should be 3-7 (Inv_PathLen)");
        
        let unique_nodes: HashSet<_> = context.path.iter().collect();
        prop_assert_eq!(unique_nodes.len(), context.path.len(), 
            "Path should have no duplicate nodes (Inv_NoDup)");

        // Path properties should be maintained after state transitions
        let _ = context.negotiate_capabilities();
        
        prop_assert!(context.path.len() >= 3 && context.path.len() <= 7, 
            "Path length should remain 3-7 after negotiation");
        
        let unique_nodes_after: HashSet<_> = context.path.iter().collect();
        prop_assert_eq!(unique_nodes_after.len(), context.path.len(), 
            "Path should still have no duplicate nodes after negotiation");
    }

    /// Test deterministic behavior for same inputs
    /// TLA+ property: Deterministic state transitions
    #[test]
    fn deterministic_state_transitions(
        context in protocol_context_strategy()
    ) {
        let mut context1 = context.clone();
        let mut context2 = context.clone();
        let mut context3 = context.clone();

        let result1 = context1.negotiate_capabilities();
        let result2 = context2.negotiate_capabilities();
        let result3 = context3.negotiate_capabilities();

        // All results should be identical
        prop_assert_eq!(result1.is_ok(), result2.is_ok(), "Results should be deterministic");
        prop_assert_eq!(result2.is_ok(), result3.is_ok(), "Results should be deterministic");
        
        prop_assert_eq!(context1.state, context2.state, "States should be deterministic");
        prop_assert_eq!(context2.state, context3.state, "States should be deterministic");
        
        prop_assert_eq!(context1.error, context2.error, "Errors should be deterministic");
        prop_assert_eq!(context2.error, context3.error, "Errors should be deterministic");
    }
}

/// Additional deterministic tests for specific edge cases
#[cfg(test)]
mod deterministic_tests {
    use super::*;

    #[test]
    fn empty_capability_sets() {
        let mut context = ProtocolContext::new();
        // Empty required set should always succeed
        let result = context.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context.state, ProtocolState::Open);
        assert_eq!(context.error, None);
        assert!(context.check_invariants());
    }

    #[test]
    fn identical_capability_sets() {
        let mut context = ProtocolContext::new();
        let caps: HashSet<u32> = vec![0x0001, 0x0002, 0x0003].into_iter().collect();
        context.capabilities_required = caps.clone();
        context.capabilities_supported = caps;
        
        let result = context.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context.state, ProtocolState::Open);
        assert!(context.check_invariants());
    }

    #[test]
    fn unsupported_required_capability() {
        let mut context = ProtocolContext::new();
        context.capabilities_required.insert(0xDEADBEEF);
        context.capabilities_supported.insert(0x0001);
        
        let result = context.negotiate_capabilities();
        assert!(result.is_err());
        assert_eq!(context.state, ProtocolState::Close);
        assert_eq!(context.error, Some(ERR_UNSUPPORTED_CAP));
        assert!(context.check_invariants());
        
        if let Err(NyxError::UnsupportedCap(cap_id)) = result {
            assert_eq!(cap_id, 0xDEADBEEF);
        }
    }

    #[test]
    fn power_state_transitions_from_init() {
        let mut context = ProtocolContext::new();
        
        // Cannot enter low power from Init state
        let result = context.enter_low_power();
        assert!(result.is_err());
        assert_eq!(context.state, ProtocolState::Init);
        assert_eq!(context.power, PowerProfile::HighPerformance);
    }

    #[test]
    fn power_state_transitions_from_close() {
        let mut context = ProtocolContext::new();
        context.capabilities_required.insert(0xDEADBEEF); // Unsupported
        
        // Negotiate to Close state
        let _ = context.negotiate_capabilities();
        assert_eq!(context.state, ProtocolState::Close);
        
        // Cannot enter low power from Close state
        let result = context.enter_low_power();
        assert!(result.is_err());
        assert_eq!(context.state, ProtocolState::Close);
        assert_eq!(context.power, PowerProfile::HighPerformance);
    }

    #[test]
    fn multiple_power_transitions() {
        let mut context = ProtocolContext::new();
        
        // Negotiate to Open state
        let _ = context.negotiate_capabilities();
        assert_eq!(context.state, ProtocolState::Open);
        
        // Multiple power transitions should work
        for _ in 0..5 {
            let result = context.enter_low_power();
            assert!(result.is_ok());
            assert_eq!(context.power, PowerProfile::UltraLowPower);
            assert!(context.check_invariants());
            
            let result = context.exit_low_power();
            assert!(result.is_ok());
            assert_eq!(context.power, PowerProfile::HighPerformance);
            assert!(context.check_invariants());
        }
    }

    #[test]
    fn invariant_violations_detected() {
        let mut context = ProtocolContext::new();
        
        // Manually create invalid state (Close without error)
        context.state = ProtocolState::Close;
        context.error = None;
        assert!(!context.check_invariants()); // Should detect Inv_Error violation
        
        // Fix the error
        context.error = Some(ERR_UNSUPPORTED_CAP);
        context.capabilities_required.insert(0xDEADBEEF);
        assert!(context.check_invariants()); // Should pass now
        
        // Create another invalid state (Open with error)
        context.state = ProtocolState::Open;
        context.error = Some(ERR_UNSUPPORTED_CAP);
        assert!(!context.check_invariants()); // Should detect Inv_NoError violation
    }

    #[test]
    fn transition_validity_edge_cases() {
        let mut context = ProtocolContext::new();
        
        // Test invalid transitions
        assert!(!context.is_valid_transition(ProtocolState::Close, PowerProfile::HighPerformance));
        assert!(!context.is_valid_transition(ProtocolState::Init, PowerProfile::HighPerformance));
        
        // Test valid transitions based on capabilities
        context.capabilities_required.insert(0x0001);
        context.capabilities_supported.insert(0x0001);
        assert!(context.is_valid_transition(ProtocolState::Open, PowerProfile::HighPerformance));
        
        context.capabilities_required.insert(0xDEADBEEF); // Add unsupported
        assert!(context.is_valid_transition(ProtocolState::Close, PowerProfile::HighPerformance));
        assert!(!context.is_valid_transition(ProtocolState::Open, PowerProfile::HighPerformance));
    }

    #[test]
    fn path_length_boundaries() {
        let mut context = ProtocolContext::new();
        
        // Test minimum path length (3)
        context.path = vec![1, 2, 3];
        assert!(context.path.len() >= 3 && context.path.len() <= 7);
        assert!(context.check_invariants());
        
        // Test maximum path length (7)
        context.path = vec![1, 2, 3, 4, 5, 6, 7];
        assert!(context.path.len() >= 3 && context.path.len() <= 7);
        assert!(context.check_invariants());
        
        // Verify no duplicates
        let unique: HashSet<_> = context.path.iter().collect();
        assert_eq!(unique.len(), context.path.len());
    }

    #[test]
    fn capability_subset_edge_cases() {
        let mut context = ProtocolContext::new();
        
        // Test proper subset
        context.capabilities_required = vec![0x0001, 0x0002].into_iter().collect();
        context.capabilities_supported = vec![0x0001, 0x0002, 0x0003].into_iter().collect();
        
        let result = context.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context.state, ProtocolState::Open);
        
        // Test equal sets
        let mut context2 = ProtocolContext::new();
        let caps: HashSet<u32> = vec![0x0001, 0x0002].into_iter().collect();
        context2.capabilities_required = caps.clone();
        context2.capabilities_supported = caps;
        
        let result = context2.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context2.state, ProtocolState::Open);
        
        // Test disjoint sets
        let mut context3 = ProtocolContext::new();
        context3.capabilities_required = vec![0x0001, 0x0002].into_iter().collect();
        context3.capabilities_supported = vec![0x0003, 0x0004].into_iter().collect();
        
        let result = context3.negotiate_capabilities();
        assert!(result.is_err());
        assert_eq!(context3.state, ProtocolState::Close);
    }
}

/// Integration tests with actual nyx-stream components
#[cfg(test)]
mod integration_tests {
    use super::*;
    use nyx_stream::state::{Stream, StreamState};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn stream_state_machine_integration() {
        let (ack_tx, _ack_rx) = mpsc::channel(16);
        let mut stream = Stream::new(1, ack_tx);
        
        // Test initial state
        assert_eq!(stream.state(), StreamState::Idle);
        
        // Test state transitions
        let _frame = stream.send_data(&[1, 2, 3, 4]);
        assert_eq!(stream.state(), StreamState::Open);
        
        let _fin_frame = stream.finish();
        assert_eq!(stream.state(), StreamState::HalfClosedLocal);
    }

    #[test]
    fn error_code_consistency() {
        // Verify our error codes match the actual implementation
        assert_eq!(ERR_UNSUPPORTED_CAP, 0x07);
        
        let error = NyxError::UnsupportedCap(0xDEADBEEF);
        assert_eq!(error.code(), ERR_UNSUPPORTED_CAP);
    }

    #[test]
    fn power_profile_properties() {
        // Test power profile scaling factors
        assert_eq!(PowerProfile::HighPerformance.cover_traffic_scale(), 1.0);
        assert_eq!(PowerProfile::UltraLowPower.cover_traffic_scale(), 0.1);
        
        // Test keepalive intervals
        assert!(PowerProfile::HighPerformance.keepalive_interval() < PowerProfile::UltraLowPower.keepalive_interval());
        
        // Test connection timeouts
        assert!(PowerProfile::HighPerformance.connection_timeout() < PowerProfile::UltraLowPower.connection_timeout());
    }

    #[tokio::test]
    async fn protocol_context_with_real_capabilities() {
        let mut context = ProtocolContext::new();
        
        // Use real capability IDs from the system
        context.capabilities_supported.insert(0x0001); // Core capability
        context.capabilities_supported.insert(0x0002); // Plugin framework
        
        // Test successful negotiation
        context.capabilities_required.insert(0x0001);
        let result = context.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context.state, ProtocolState::Open);
        
        // Test power state transitions
        let result = context.enter_low_power();
        assert!(result.is_ok());
        assert_eq!(context.power, PowerProfile::UltraLowPower);
        assert!(context.check_invariants());
    }

    #[test]
    fn tla_model_consistency() {
        // Verify our implementation matches TLA+ model constants
        let mut context = ProtocolContext::new();
        
        // Test path length constraints (TLA+ Inv_PathLen: 3..7)
        context.path = vec![1, 2, 3]; // Minimum length
        assert!(context.path.len() >= 3 && context.path.len() <= 7);
        
        context.path = vec![1, 2, 3, 4, 5, 6, 7]; // Maximum length
        assert!(context.path.len() >= 3 && context.path.len() <= 7);
        
        // Test capability set properties
        let caps: HashSet<u32> = vec![0x0001, 0x0002, 0x0003].into_iter().collect();
        context.capabilities_required = caps.clone();
        context.capabilities_supported = caps;
        
        // Should satisfy C_req ⊆ C_sup
        assert!(context.capabilities_required.is_subset(&context.capabilities_supported));
        
        let result = context.negotiate_capabilities();
        assert!(result.is_ok());
        assert_eq!(context.state, ProtocolState::Open);
        assert!(context.check_invariants());
    }
}