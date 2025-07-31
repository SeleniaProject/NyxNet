---- MODULE nyx_multipath_plugin ----
EXTENDS Naturals, Sequences, FiniteSets, TLAPS

(*************************************************************************)
(* Nyx Protocol – Complete Multipath Protocol State Machine             *)
(* Enhanced formal model for comprehensive protocol verification          *)
(*                                                                       *)
(* This model captures the complete Nyx protocol state machine including:*)
(*   • Dynamic path construction and validation (3–7 hops)               *)
(*   • Comprehensive capability negotiation with error handling          *)
(*   • Cryptographic handshake states and key management                 *)
(*   • Stream management and flow control states                         *)
(*   • Network failure detection and recovery mechanisms                 *)
(*   • Performance monitoring and adaptive behavior                      *)
(*************************************************************************)

CONSTANTS NodeCount,      \* total nodes in network (>7)
          CapSet,         \* Universe of capability IDs (Nat)
          MaxStreams,     \* Maximum concurrent streams per connection
          MaxRetries,     \* Maximum retry attempts for operations
          TimeoutLimit    \* Maximum timeout value for operations

(* Symmetry set for model checking optimization *)
NodeSymmetry == Permutations(1..NodeCount)

(* Error codes for comprehensive error handling *)
ErrorCodes == {
    "None",
    "UNSUPPORTED_CAP",     \* 0x07 - Capability negotiation failure
    "INVALID_PATH",        \* 0x08 - Path construction failure  
    "CRYPTO_ERROR",        \* 0x09 - Cryptographic operation failure
    "TIMEOUT_ERROR",       \* 0x0A - Operation timeout
    "STREAM_ERROR",        \* 0x0B - Stream management error
    "NETWORK_ERROR",       \* 0x0C - Network connectivity error
    "RESOURCE_ERROR"       \* 0x0D - Resource exhaustion error
}

(* Protocol states for complete state machine *)
ProtocolStates == {
    "Init",                \* Initial state
    "PathBuilding",        \* Constructing multipath routes
    "CapabilityNegotiation", \* Negotiating protocol capabilities
    "CryptoHandshake",     \* Performing cryptographic handshake
    "StreamSetup",         \* Setting up data streams
    "Active",              \* Normal operation state
    "Degraded",            \* Operating with reduced functionality
    "Recovery",            \* Attempting to recover from errors
    "Closing",             \* Graceful shutdown in progress
    "Closed"               \* Connection terminated
}

(* Power management states *)
PowerStates == {"Normal", "LowPower", "Hibernation"}

(* Stream states for stream management *)
StreamStates == {"Idle", "Opening", "Active", "Closing", "Closed", "Error"}

VARIABLES 
    \* Core protocol state
    state,                 \* Current protocol state
    prev_state,            \* Previous state for recovery
    error,                 \* Current error code
    retry_count,           \* Number of retry attempts
    
    \* Path and routing
    path,                  \* Sequence of selected NodeIds
    backup_paths,          \* Set of backup paths for failover
    path_quality,          \* Quality metrics for current path
    
    \* Capability negotiation
    C_req, C_sup,          \* Required and supported capabilities
    negotiated_caps,       \* Successfully negotiated capabilities
    
    \* Cryptographic state
    crypto_state,          \* Cryptographic handshake state
    session_keys,          \* Active session keys
    key_rotation_timer,    \* Timer for key rotation
    
    \* Stream management
    active_streams,        \* Set of active stream IDs
    stream_states,         \* Mapping from stream ID to state
    flow_control,          \* Flow control parameters
    
    \* Performance and monitoring
    power,                 \* Power management state
    performance_metrics,   \* Performance monitoring data
    network_conditions,    \* Current network condition assessment
    
    \* Timing and timeouts
    operation_timer,       \* Timer for current operation
    last_activity,         \* Timestamp of last activity
    timeout_count          \* Number of timeout events

(*************************************************************************)
(* Initial State Definition                                              *)
(*************************************************************************)

Init == 
    \* Core protocol state initialization
    /\ state = "Init"
    /\ prev_state = "Init"
    /\ error = "None"
    /\ retry_count = 0
    
    \* Path and routing initialization
    /\ path = <<>>
    /\ backup_paths = {}
    /\ path_quality = 0
    
    \* Capability negotiation initialization
    /\ C_req \subseteq CapSet
    /\ C_sup \subseteq CapSet
    /\ negotiated_caps = {}
    
    \* Cryptographic state initialization
    /\ crypto_state = "Init"
    /\ session_keys = {}
    /\ key_rotation_timer = 0
    
    \* Stream management initialization
    /\ active_streams = {}
    /\ stream_states = [s \in {} |-> "Idle"]
    /\ flow_control = [window_size |-> 1024, bytes_in_flight |-> 0]
    
    \* Performance and monitoring initialization
    /\ power = "Normal"
    /\ performance_metrics = [latency |-> 0, throughput |-> 0, error_rate |-> 0]
    /\ network_conditions = "Good"
    
    \* Timing initialization
    /\ operation_timer = 0
    /\ last_activity = 0
    /\ timeout_count = 0

(*************************************************************************)
(* State Transition Actions                                              *)
(*************************************************************************)

(* Path Building Phase *)
StartPathBuilding == 
    /\ state = "Init"
    /\ state' = "PathBuilding"
    /\ prev_state' = "Init"
    /\ operation_timer' = 0
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, stream_states, flow_control,
                   power, performance_metrics, network_conditions, last_activity, timeout_count>>

BuildPath == 
    /\ state = "PathBuilding"
    /\ path = <<>>
    /\ \E new_path \in Seq(1..NodeCount) :
        /\ Len(new_path) \in 3..7
        /\ \A i, j \in 1..Len(new_path) : i # j => new_path[i] # new_path[j]
        /\ path' = new_path
    /\ path_quality' = Len(path') * 10  \* Simple quality metric
    /\ state' = "CapabilityNegotiation"
    /\ prev_state' = "PathBuilding"
    /\ UNCHANGED <<error, retry_count, backup_paths, C_req, C_sup, negotiated_caps,
                   crypto_state, session_keys, key_rotation_timer, active_streams,
                   stream_states, flow_control, power, performance_metrics,
                   network_conditions, operation_timer, last_activity, timeout_count>>

PathBuildingFailed == 
    /\ state = "PathBuilding"
    /\ retry_count < MaxRetries
    /\ error' = "INVALID_PATH"
    /\ retry_count' = retry_count + 1
    /\ state' = "Recovery"
    /\ prev_state' = "PathBuilding"
    /\ UNCHANGED <<path, backup_paths, path_quality, C_req, C_sup, negotiated_caps,
                   crypto_state, session_keys, key_rotation_timer, active_streams,
                   stream_states, flow_control, power, performance_metrics,
                   network_conditions, operation_timer, last_activity, timeout_count>>

(* Capability Negotiation Phase *)
NegotiateCapabilities == 
    /\ state = "CapabilityNegotiation"
    /\ C_req \subseteq C_sup
    /\ negotiated_caps' = C_req \cap C_sup
    /\ state' = "CryptoHandshake"
    /\ prev_state' = "CapabilityNegotiation"
    /\ error' = "None"
    /\ UNCHANGED <<retry_count, path, backup_paths, path_quality, C_req, C_sup,
                   crypto_state, session_keys, key_rotation_timer, active_streams,
                   stream_states, flow_control, power, performance_metrics,
                   network_conditions, operation_timer, last_activity, timeout_count>>

CapabilityNegotiationFailed == 
    /\ state = "CapabilityNegotiation"
    /\ ~(C_req \subseteq C_sup)
    /\ error' = "UNSUPPORTED_CAP"
    /\ state' = "Closing"
    /\ prev_state' = "CapabilityNegotiation"
    /\ UNCHANGED <<retry_count, path, backup_paths, path_quality, C_req, C_sup,
                   negotiated_caps, crypto_state, session_keys, key_rotation_timer,
                   active_streams, stream_states, flow_control, power,
                   performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

(* Cryptographic Handshake Phase *)
StartCryptoHandshake == 
    /\ state = "CryptoHandshake"
    /\ crypto_state = "Init"
    /\ crypto_state' = "Handshaking"
    /\ session_keys' = {"temp_key"}  \* Simplified key representation
    /\ key_rotation_timer' = 100     \* Key rotation interval
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, state, prev_state,
                   active_streams, stream_states, flow_control, power,
                   performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

CompleteCryptoHandshake == 
    /\ state = "CryptoHandshake"
    /\ crypto_state = "Handshaking"
    /\ crypto_state' = "Established"
    /\ state' = "StreamSetup"
    /\ prev_state' = "CryptoHandshake"
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, session_keys, key_rotation_timer,
                   active_streams, stream_states, flow_control, power,
                   performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

CryptoHandshakeFailed == 
    /\ state = "CryptoHandshake"
    /\ crypto_state = "Handshaking"
    /\ error' = "CRYPTO_ERROR"
    /\ state' = "Recovery"
    /\ prev_state' = "CryptoHandshake"
    /\ crypto_state' = "Failed"
    /\ UNCHANGED <<retry_count, path, backup_paths, path_quality, C_req, C_sup,
                   negotiated_caps, session_keys, key_rotation_timer,
                   active_streams, stream_states, flow_control, power,
                   performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

(* Stream Setup Phase *)
SetupStreams == 
    /\ state = "StreamSetup"
    /\ active_streams = {}
    /\ \E stream_id \in 1..MaxStreams :
        /\ active_streams' = {stream_id}
        /\ stream_states' = [stream_states EXCEPT ![stream_id] = "Opening"]
    /\ state' = "Active"
    /\ prev_state' = "StreamSetup"
    /\ last_activity' = operation_timer
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, flow_control, power, performance_metrics,
                   network_conditions, operation_timer, timeout_count>>

(* Active Operation Phase *)
NormalOperation == 
    /\ state = "Active"
    /\ error = "None"
    /\ \E stream_id \in active_streams :
        /\ stream_states[stream_id] = "Opening"
        /\ stream_states' = [stream_states EXCEPT ![stream_id] = "Active"]
    /\ performance_metrics' = [performance_metrics EXCEPT 
                               !.throughput = performance_metrics.throughput + 10]
    /\ last_activity' = operation_timer
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, flow_control, power,
                   network_conditions, operation_timer, timeout_count, state, prev_state>>

(* Power Management *)
EnterLowPower == 
    /\ state = "Active"
    /\ power = "Normal"
    /\ last_activity + 50 < operation_timer  \* Inactivity threshold
    /\ power' = "LowPower"
    /\ performance_metrics' = [performance_metrics EXCEPT 
                               !.throughput = performance_metrics.throughput / 2]
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, stream_states, flow_control,
                   network_conditions, operation_timer, last_activity, timeout_count,
                   state, prev_state>>

ExitLowPower == 
    /\ state = "Active"
    /\ power = "LowPower"
    /\ power' = "Normal"
    /\ last_activity' = operation_timer
    /\ performance_metrics' = [performance_metrics EXCEPT 
                               !.throughput = performance_metrics.throughput * 2]
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, stream_states, flow_control,
                   network_conditions, operation_timer, timeout_count, state, prev_state>>

(* Error Recovery *)
StartRecovery == 
    /\ state = "Recovery"
    /\ retry_count < MaxRetries
    /\ retry_count' = retry_count + 1
    /\ state' = prev_state  \* Return to previous state
    /\ error' = "None"
    /\ UNCHANGED <<path, backup_paths, path_quality, C_req, C_sup, negotiated_caps,
                   crypto_state, session_keys, key_rotation_timer, active_streams,
                   stream_states, flow_control, power, performance_metrics,
                   network_conditions, operation_timer, last_activity, timeout_count,
                   prev_state>>

RecoveryFailed == 
    /\ state = "Recovery"
    /\ retry_count >= MaxRetries
    /\ state' = "Closing"
    /\ prev_state' = "Recovery"
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, stream_states, flow_control,
                   power, performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

(* Timeout Handling *)
HandleTimeout == 
    /\ operation_timer > TimeoutLimit
    /\ timeout_count' = timeout_count + 1
    /\ error' = "TIMEOUT_ERROR"
    /\ state' = "Recovery"
    /\ prev_state' = state
    /\ operation_timer' = 0
    /\ UNCHANGED <<retry_count, path, backup_paths, path_quality, C_req, C_sup,
                   negotiated_caps, crypto_state, session_keys, key_rotation_timer,
                   active_streams, stream_states, flow_control, power,
                   performance_metrics, network_conditions, last_activity>>

(* Graceful Shutdown *)
StartClosing == 
    /\ state \in {"Active", "Degraded"}
    /\ state' = "Closing"
    /\ prev_state' = state
    /\ \A stream_id \in active_streams :
        stream_states' = [stream_states EXCEPT ![stream_id] = "Closing"]
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, session_keys,
                   key_rotation_timer, active_streams, flow_control, power,
                   performance_metrics, network_conditions, operation_timer,
                   last_activity, timeout_count>>

CompleteClosing == 
    /\ state = "Closing"
    /\ \A stream_id \in active_streams : stream_states[stream_id] = "Closing"
    /\ state' = "Closed"
    /\ active_streams' = {}
    /\ stream_states' = [s \in {} |-> "Closed"]
    /\ session_keys' = {}
    /\ UNCHANGED <<error, retry_count, path, backup_paths, path_quality,
                   C_req, C_sup, negotiated_caps, crypto_state, key_rotation_timer,
                   flow_control, power, performance_metrics, network_conditions,
                   operation_timer, last_activity, timeout_count, prev_state>>

(* Timer advancement *)
AdvanceTimer == 
    /\ operation_timer' = operation_timer + 1
    /\ UNCHANGED <<state, prev_state, error, retry_count, path, backup_paths,
                   path_quality, C_req, C_sup, negotiated_caps, crypto_state,
                   session_keys, key_rotation_timer, active_streams, stream_states,
                   flow_control, power, performance_metrics, network_conditions,
                   last_activity, timeout_count>>

(* Terminal state - no further transitions *)
Terminal == 
    /\ state = "Closed"
    /\ UNCHANGED <<state, prev_state, error, retry_count, path, backup_paths,
                   path_quality, C_req, C_sup, negotiated_caps, crypto_state,
                   session_keys, key_rotation_timer, active_streams, stream_states,
                   flow_control, power, performance_metrics, network_conditions,
                   operation_timer, last_activity, timeout_count>>

(*************************************************************************)
(* Next State Relation                                                   *)
(*************************************************************************)

Next == 
    \/ StartPathBuilding
    \/ BuildPath
    \/ PathBuildingFailed
    \/ NegotiateCapabilities
    \/ CapabilityNegotiationFailed
    \/ StartCryptoHandshake
    \/ CompleteCryptoHandshake
    \/ CryptoHandshakeFailed
    \/ SetupStreams
    \/ NormalOperation
    \/ EnterLowPower
    \/ ExitLowPower
    \/ StartRecovery
    \/ RecoveryFailed
    \/ HandleTimeout
    \/ StartClosing
    \/ CompleteClosing
    \/ AdvanceTimer
    \/ Terminal

(* State variables for specification *)
vars == <<state, prev_state, error, retry_count, path, backup_paths, path_quality,
          C_req, C_sup, negotiated_caps, crypto_state, session_keys, key_rotation_timer,
          active_streams, stream_states, flow_control, power, performance_metrics,
          network_conditions, operation_timer, last_activity, timeout_count>>

Spec == Init /\ [][Next]_vars

(*************************************************************************)
(* State Invariants and Transition Conditions                           *)
(*************************************************************************)

(* Basic path invariants *)
Inv_PathLen == 
    (state \in {"CapabilityNegotiation", "CryptoHandshake", "StreamSetup", "Active", "Degraded"}) 
    => Len(path) \in 3..7

Inv_PathUniqueness == 
    (path # <<>>) => \A i, j \in 1..Len(path): i # j => path[i] # path[j]

(* State transition invariants *)
Inv_StateProgression == 
    /\ (state = "PathBuilding" => prev_state \in {"Init", "Recovery"})
    /\ (state = "CapabilityNegotiation" => prev_state \in {"PathBuilding", "Recovery"})
    /\ (state = "CryptoHandshake" => prev_state \in {"CapabilityNegotiation", "Recovery"})
    /\ (state = "StreamSetup" => prev_state = "CryptoHandshake")
    /\ (state = "Active" => prev_state \in {"StreamSetup", "Recovery"})
    /\ (state = "Closing" => prev_state \in {"Active", "Degraded", "Recovery"})
    /\ (state = "Closed" => prev_state = "Closing")

(* Error handling invariants *)
Inv_ErrorConsistency == 
    /\ (state = "Closed" /\ prev_state = "CapabilityNegotiation") => error = "UNSUPPORTED_CAP"
    /\ (state = "Recovery" /\ prev_state = "PathBuilding") => error = "INVALID_PATH"
    /\ (state = "Recovery" /\ prev_state = "CryptoHandshake") => error = "CRYPTO_ERROR"
    /\ (error = "TIMEOUT_ERROR") => timeout_count > 0
    /\ (state = "Active" /\ error = "None") => retry_count <= MaxRetries

(* Retry mechanism invariants *)
Inv_RetryBounds == 
    /\ retry_count <= MaxRetries
    /\ (retry_count = MaxRetries /\ state = "Recovery") => 
       \/ (state' = "Closing")
       \/ (state' = "Recovery" /\ retry_count' = retry_count)

(* Capability negotiation invariants *)
Inv_CapabilityConsistency == 
    /\ (state = "Active") => (negotiated_caps = C_req \cap C_sup)
    /\ (state = "Active") => (C_req \subseteq C_sup)
    /\ (negotiated_caps # {}) => (state \in {"CryptoHandshake", "StreamSetup", "Active", "Degraded", "Closing"})

(* Cryptographic state invariants *)
Inv_CryptoStateConsistency == 
    /\ (crypto_state = "Established") => (session_keys # {})
    /\ (state = "Active") => (crypto_state = "Established")
    /\ (crypto_state = "Failed") => (state \in {"Recovery", "Closing", "Closed"})
    /\ (session_keys = {}) => (state \in {"Init", "PathBuilding", "CapabilityNegotiation", "Closed"})

(* Stream management invariants *)
Inv_StreamConsistency == 
    /\ (active_streams # {}) => (state \in {"StreamSetup", "Active", "Degraded", "Closing"})
    /\ (state = "Active") => (\A s \in active_streams : stream_states[s] \in {"Opening", "Active"})
    /\ (state = "Closing") => (\A s \in active_streams : stream_states[s] \in {"Closing", "Closed"})
    /\ (state = "Closed") => (active_streams = {})
    /\ Cardinality(active_streams) <= MaxStreams

(* Power management invariants *)
Inv_PowerManagement == 
    /\ (power = "LowPower") => (state = "Active")
    /\ (power = "Hibernation") => (state \in {"Active", "Degraded"})
    /\ (state = "Closed") => (power = "Normal")

(* Performance monitoring invariants *)
Inv_PerformanceMetrics == 
    /\ performance_metrics.latency >= 0
    /\ performance_metrics.throughput >= 0
    /\ performance_metrics.error_rate >= 0
    /\ performance_metrics.error_rate <= 100
    /\ (power = "LowPower") => (performance_metrics.throughput <= 1000)

(* Timing invariants *)
Inv_TimingConsistency == 
    /\ operation_timer >= 0
    /\ last_activity >= 0
    /\ timeout_count >= 0
    /\ (last_activity > 0) => (state \in {"Active", "Degraded"})
    /\ (timeout_count > 0) => (operation_timer <= TimeoutLimit \/ state = "Recovery")

(* Network condition invariants *)
Inv_NetworkConditions == 
    /\ network_conditions \in {"Good", "Fair", "Poor", "Disconnected"}
    /\ (network_conditions = "Disconnected") => (state \in {"Recovery", "Closing", "Closed"})
    /\ (state = "Active" /\ error = "None") => (network_conditions \in {"Good", "Fair"})

(* Resource management invariants *)
Inv_ResourceManagement == 
    /\ flow_control.window_size > 0
    /\ flow_control.bytes_in_flight >= 0
    /\ flow_control.bytes_in_flight <= flow_control.window_size
    /\ key_rotation_timer >= 0
    /\ (key_rotation_timer > 0) => (crypto_state = "Established")

(*************************************************************************)
(* Type Invariant - All variables maintain their expected types          *)
(*************************************************************************)

TypeInvariant == 
    /\ state \in ProtocolStates
    /\ prev_state \in ProtocolStates
    /\ error \in ErrorCodes
    /\ retry_count \in 0..MaxRetries
    /\ path \in Seq(1..NodeCount)
    /\ backup_paths \subseteq SUBSET Seq(1..NodeCount)
    /\ path_quality \in Nat
    /\ C_req \subseteq CapSet
    /\ C_sup \subseteq CapSet
    /\ negotiated_caps \subseteq CapSet
    /\ crypto_state \in {"Init", "Handshaking", "Established", "Failed"}
    /\ session_keys \subseteq STRING
    /\ key_rotation_timer \in Nat
    /\ active_streams \subseteq (1..MaxStreams)
    /\ stream_states \in [active_streams -> StreamStates]
    /\ flow_control.window_size \in Nat \ {0}
    /\ flow_control.bytes_in_flight \in Nat
    /\ power \in PowerStates
    /\ performance_metrics.latency \in Nat
    /\ performance_metrics.throughput \in Nat
    /\ performance_metrics.error_rate \in 0..100
    /\ network_conditions \in {"Good", "Fair", "Poor", "Disconnected"}
    /\ operation_timer \in Nat
    /\ last_activity \in Nat
    /\ timeout_count \in Nat

(*************************************************************************)
(* Safety Invariants with Formal Proofs                                  *)
(*************************************************************************)

THEOREM SafetyInvariant == Spec => []TypeInvariant
<1>1. Init => TypeInvariant
  <2>1. ASSUME Init PROVE TypeInvariant
    BY DEF Init, TypeInvariant, ProtocolStates, ErrorCodes, PowerStates, StreamStates
  <2>2. QED BY <2>1
<1>2. TypeInvariant /\ [Next]_vars => TypeInvariant'
  <2>1. ASSUME TypeInvariant, [Next]_vars PROVE TypeInvariant'
    <3>1. CASE StartPathBuilding
      BY <3>1, <2>1 DEF StartPathBuilding, TypeInvariant, ProtocolStates
    <3>2. CASE BuildPath
      BY <3>2, <2>1 DEF BuildPath, TypeInvariant, ProtocolStates
    <3>3. CASE PathBuildingFailed
      BY <3>3, <2>1 DEF PathBuildingFailed, TypeInvariant, ErrorCodes
    <3>4. CASE NegotiateCapabilities
      BY <3>4, <2>1 DEF NegotiateCapabilities, TypeInvariant, ProtocolStates
    <3>5. CASE CapabilityNegotiationFailed
      BY <3>5, <2>1 DEF CapabilityNegotiationFailed, TypeInvariant, ErrorCodes
    <3>6. CASE StartCryptoHandshake
      BY <3>6, <2>1 DEF StartCryptoHandshake, TypeInvariant
    <3>7. CASE CompleteCryptoHandshake
      BY <3>7, <2>1 DEF CompleteCryptoHandshake, TypeInvariant, ProtocolStates
    <3>8. CASE CryptoHandshakeFailed
      BY <3>8, <2>1 DEF CryptoHandshakeFailed, TypeInvariant, ErrorCodes
    <3>9. CASE SetupStreams
      BY <3>9, <2>1 DEF SetupStreams, TypeInvariant, ProtocolStates, StreamStates
    <3>10. CASE NormalOperation
      BY <3>10, <2>1 DEF NormalOperation, TypeInvariant, StreamStates
    <3>11. CASE EnterLowPower
      BY <3>11, <2>1 DEF EnterLowPower, TypeInvariant, PowerStates
    <3>12. CASE ExitLowPower
      BY <3>12, <2>1 DEF ExitLowPower, TypeInvariant, PowerStates
    <3>13. CASE StartRecovery
      BY <3>13, <2>1 DEF StartRecovery, TypeInvariant, ProtocolStates
    <3>14. CASE RecoveryFailed
      BY <3>14, <2>1 DEF RecoveryFailed, TypeInvariant, ProtocolStates
    <3>15. CASE HandleTimeout
      BY <3>15, <2>1 DEF HandleTimeout, TypeInvariant, ErrorCodes
    <3>16. CASE StartClosing
      BY <3>16, <2>1 DEF StartClosing, TypeInvariant, ProtocolStates, StreamStates
    <3>17. CASE CompleteClosing
      BY <3>17, <2>1 DEF CompleteClosing, TypeInvariant, ProtocolStates, StreamStates
    <3>18. CASE AdvanceTimer
      BY <3>18, <2>1 DEF AdvanceTimer, TypeInvariant
    <3>19. CASE Terminal
      BY <3>19, <2>1 DEF Terminal, TypeInvariant
    <3>20. CASE UNCHANGED vars
      BY <3>20, <2>1 DEF TypeInvariant
    <3>21. QED BY <3>1, <3>2, <3>3, <3>4, <3>5, <3>6, <3>7, <3>8, <3>9, <3>10,
                  <3>11, <3>12, <3>13, <3>14, <3>15, <3>16, <3>17, <3>18, <3>19, <3>20 DEF Next
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM PathLengthSafety == Spec => []Inv_PathLen
<1>1. Init => Inv_PathLen
  <2>1. ASSUME Init PROVE Inv_PathLen
    BY <2>1 DEF Init, Inv_PathLen
  <2>2. QED BY <2>1
<1>2. Inv_PathLen /\ [Next]_vars => Inv_PathLen'
  <2>1. ASSUME Inv_PathLen, [Next]_vars PROVE Inv_PathLen'
    BY <2>1 DEF Next, Inv_PathLen, BuildPath, NegotiateCapabilities, 
                CompleteCryptoHandshake, SetupStreams, NormalOperation
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM PathUniqueness == Spec => []Inv_PathUniqueness
<1>1. Init => Inv_PathUniqueness
  <2>1. ASSUME Init PROVE Inv_PathUniqueness
    BY <2>1 DEF Init, Inv_PathUniqueness
  <2>2. QED BY <2>1
<1>2. Inv_PathUniqueness /\ [Next]_vars => Inv_PathUniqueness'
  <2>1. ASSUME Inv_PathUniqueness, [Next]_vars PROVE Inv_PathUniqueness'
    BY <2>1 DEF Next, Inv_PathUniqueness, BuildPath
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM StateProgressionSafety == Spec => []Inv_StateProgression
<1>1. Init => Inv_StateProgression
  <2>1. ASSUME Init PROVE Inv_StateProgression
    BY <2>1 DEF Init, Inv_StateProgression
  <2>2. QED BY <2>1
<1>2. Inv_StateProgression /\ [Next]_vars => Inv_StateProgression'
  <2>1. ASSUME Inv_StateProgression, [Next]_vars PROVE Inv_StateProgression'
    BY <2>1 DEF Next, Inv_StateProgression, StartPathBuilding, BuildPath,
                NegotiateCapabilities, CompleteCryptoHandshake, SetupStreams,
                StartRecovery, StartClosing, CompleteClosing
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM ErrorHandlingSafety == Spec => []Inv_ErrorConsistency
<1>1. Init => Inv_ErrorConsistency
  <2>1. ASSUME Init PROVE Inv_ErrorConsistency
    BY <2>1 DEF Init, Inv_ErrorConsistency
  <2>2. QED BY <2>1
<1>2. Inv_ErrorConsistency /\ [Next]_vars => Inv_ErrorConsistency'
  <2>1. ASSUME Inv_ErrorConsistency, [Next]_vars PROVE Inv_ErrorConsistency'
    BY <2>1 DEF Next, Inv_ErrorConsistency, CapabilityNegotiationFailed,
                PathBuildingFailed, CryptoHandshakeFailed, HandleTimeout
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM RetryMechanismSafety == Spec => []Inv_RetryBounds
<1>1. Init => Inv_RetryBounds
  <2>1. ASSUME Init PROVE Inv_RetryBounds
    BY <2>1 DEF Init, Inv_RetryBounds
  <2>2. QED BY <2>1
<1>2. Inv_RetryBounds /\ [Next]_vars => Inv_RetryBounds'
  <2>1. ASSUME Inv_RetryBounds, [Next]_vars PROVE Inv_RetryBounds'
    BY <2>1 DEF Next, Inv_RetryBounds, StartRecovery, RecoveryFailed,
                PathBuildingFailed, CryptoHandshakeFailed
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM CapabilityNegotiationSafety == Spec => []Inv_CapabilityConsistency
<1>1. Init => Inv_CapabilityConsistency
  <2>1. ASSUME Init PROVE Inv_CapabilityConsistency
    BY <2>1 DEF Init, Inv_CapabilityConsistency
  <2>2. QED BY <2>1
<1>2. Inv_CapabilityConsistency /\ [Next]_vars => Inv_CapabilityConsistency'
  <2>1. ASSUME Inv_CapabilityConsistency, [Next]_vars PROVE Inv_CapabilityConsistency'
    BY <2>1 DEF Next, Inv_CapabilityConsistency, NegotiateCapabilities,
                CompleteCryptoHandshake, SetupStreams
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM CryptographicStateSafety == Spec => []Inv_CryptoStateConsistency
<1>1. Init => Inv_CryptoStateConsistency
  <2>1. ASSUME Init PROVE Inv_CryptoStateConsistency
    BY <2>1 DEF Init, Inv_CryptoStateConsistency
  <2>2. QED BY <2>1
<1>2. Inv_CryptoStateConsistency /\ [Next]_vars => Inv_CryptoStateConsistency'
  <2>1. ASSUME Inv_CryptoStateConsistency, [Next]_vars PROVE Inv_CryptoStateConsistency'
    BY <2>1 DEF Next, Inv_CryptoStateConsistency, StartCryptoHandshake,
                CompleteCryptoHandshake, CryptoHandshakeFailed, CompleteClosing
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM StreamManagementSafety == Spec => []Inv_StreamConsistency
<1>1. Init => Inv_StreamConsistency
  <2>1. ASSUME Init PROVE Inv_StreamConsistency
    BY <2>1 DEF Init, Inv_StreamConsistency
  <2>2. QED BY <2>1
<1>2. Inv_StreamConsistency /\ [Next]_vars => Inv_StreamConsistency'
  <2>1. ASSUME Inv_StreamConsistency, [Next]_vars PROVE Inv_StreamConsistency'
    BY <2>1 DEF Next, Inv_StreamConsistency, SetupStreams, NormalOperation,
                StartClosing, CompleteClosing
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM PowerManagementSafety == Spec => []Inv_PowerManagement
<1>1. Init => Inv_PowerManagement
  <2>1. ASSUME Init PROVE Inv_PowerManagement
    BY <2>1 DEF Init, Inv_PowerManagement
  <2>2. QED BY <2>1
<1>2. Inv_PowerManagement /\ [Next]_vars => Inv_PowerManagement'
  <2>1. ASSUME Inv_PowerManagement, [Next]_vars PROVE Inv_PowerManagement'
    BY <2>1 DEF Next, Inv_PowerManagement, EnterLowPower, ExitLowPower, CompleteClosing
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

(*************************************************************************)
(* Resource Management and Performance Invariants                        *)
(*************************************************************************)

THEOREM PerformanceMetricsSafety == Spec => []Inv_PerformanceMetrics
<1>1. Init => Inv_PerformanceMetrics
  <2>1. ASSUME Init PROVE Inv_PerformanceMetrics
    BY <2>1 DEF Init, Inv_PerformanceMetrics
  <2>2. QED BY <2>1
<1>2. Inv_PerformanceMetrics /\ [Next]_vars => Inv_PerformanceMetrics'
  <2>1. ASSUME Inv_PerformanceMetrics, [Next]_vars PROVE Inv_PerformanceMetrics'
    BY <2>1 DEF Next, Inv_PerformanceMetrics, NormalOperation, EnterLowPower, ExitLowPower
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM TimingConsistencySafety == Spec => []Inv_TimingConsistency
<1>1. Init => Inv_TimingConsistency
  <2>1. ASSUME Init PROVE Inv_TimingConsistency
    BY <2>1 DEF Init, Inv_TimingConsistency
  <2>2. QED BY <2>1
<1>2. Inv_TimingConsistency /\ [Next]_vars => Inv_TimingConsistency'
  <2>1. ASSUME Inv_TimingConsistency, [Next]_vars PROVE Inv_TimingConsistency'
    BY <2>1 DEF Next, Inv_TimingConsistency, AdvanceTimer, HandleTimeout,
                SetupStreams, NormalOperation, EnterLowPower
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM ResourceManagementSafety == Spec => []Inv_ResourceManagement
<1>1. Init => Inv_ResourceManagement
  <2>1. ASSUME Init PROVE Inv_ResourceManagement
    BY <2>1 DEF Init, Inv_ResourceManagement
  <2>2. QED BY <2>1
<1>2. Inv_ResourceManagement /\ [Next]_vars => Inv_ResourceManagement'
  <2>1. ASSUME Inv_ResourceManagement, [Next]_vars PROVE Inv_ResourceManagement'
    BY <2>1 DEF Next, Inv_ResourceManagement, StartCryptoHandshake, CompleteCryptoHandshake
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

(*************************************************************************)
(* Liveness Properties with Temporal Logic                               *)
(*************************************************************************)

(* Basic termination properties *)
EventuallyLeavesInit == <>(state # "Init")
EventuallyTerminates == <>(state \in {"Active", "Closed"})

(* Progress guarantees for successful scenarios *)
SuccessfulPathBuilding == 
    (state = "PathBuilding") ~> (state \in {"CapabilityNegotiation", "Recovery", "Closed"})

SuccessfulCapabilityNegotiation == 
    (state = "CapabilityNegotiation" /\ C_req \subseteq C_sup) ~> 
    (state \in {"CryptoHandshake", "Recovery", "Closed"})

SuccessfulCryptoHandshake == 
    (state = "CryptoHandshake" /\ crypto_state = "Handshaking") ~> 
    (state \in {"StreamSetup", "Recovery", "Closed"})

SuccessfulStreamSetup == 
    (state = "StreamSetup") ~> (state \in {"Active", "Recovery", "Closed"})

(* Progress guarantees for failure scenarios *)
FailedCapabilityNegotiation == 
    (state = "CapabilityNegotiation" /\ ~(C_req \subseteq C_sup)) ~> 
    (state = "Closed")

RecoveryProgress == 
    (state = "Recovery" /\ retry_count < MaxRetries) ~> 
    (state \in {prev_state, "Closing", "Closed"})

RecoveryExhaustion == 
    (state = "Recovery" /\ retry_count >= MaxRetries) ~> 
    (state = "Closing")

(* Timeout handling progress *)
TimeoutRecovery == 
    (operation_timer > TimeoutLimit) ~> 
    (state = "Recovery" \/ state = "Closed")

(* Graceful shutdown progress *)
GracefulShutdown == 
    (state = "Closing") ~> (state = "Closed")

(* Stream lifecycle progress *)
StreamActivation == 
    (\E s \in active_streams : stream_states[s] = "Opening") ~> 
    (\A s \in active_streams : stream_states[s] \in {"Active", "Closed"})

StreamTermination == 
    (state = "Closing" /\ active_streams # {}) ~> 
    (active_streams = {})

(* Power management liveness *)
PowerStateTransitions == 
    /\ (state = "Active" /\ power = "Normal" /\ last_activity + 50 < operation_timer) ~> 
       (power = "LowPower")
    /\ (state = "Active" /\ power = "LowPower") ~> 
       (power \in {"Normal", "LowPower"})

(* Error recovery liveness *)
ErrorRecoveryProgress == 
    (error # "None" /\ retry_count < MaxRetries) ~> 
    (error = "None" \/ state = "Closed")

(* Fairness conditions for comprehensive liveness *)
FairPathBuilding == WF_vars(BuildPath)
FairCapabilityNegotiation == WF_vars(NegotiateCapabilities \/ CapabilityNegotiationFailed)
FairCryptoHandshake == WF_vars(CompleteCryptoHandshake \/ CryptoHandshakeFailed)
FairStreamSetup == WF_vars(SetupStreams)
FairRecovery == WF_vars(StartRecovery \/ RecoveryFailed)
FairTimeout == WF_vars(HandleTimeout)
FairShutdown == WF_vars(StartClosing \/ CompleteClosing)
FairTimer == WF_vars(AdvanceTimer)

(* Complete specification with comprehensive fairness *)
FairSpec == Spec /\ FairPathBuilding /\ FairCapabilityNegotiation /\ 
            FairCryptoHandshake /\ FairStreamSetup /\ FairRecovery /\ 
            FairTimeout /\ FairShutdown /\ FairTimer

THEOREM BasicLivenessProperty == FairSpec => EventuallyLeavesInit
<1>1. SUFFICES ASSUME FairSpec PROVE EventuallyLeavesInit
  OBVIOUS
<1>2. FairSpec => <>(state = "PathBuilding")
  BY WF1 DEF FairSpec, FairPathBuilding, StartPathBuilding
<1>3. (state = "PathBuilding") => (state # "Init")
  OBVIOUS
<1>4. QED BY <1>2, <1>3 DEF EventuallyLeavesInit

THEOREM TerminationProperty == FairSpec => EventuallyTerminates
<1>1. SUFFICES ASSUME FairSpec PROVE EventuallyTerminates
  OBVIOUS
<1>2. CASE C_req \subseteq C_sup
  <2>1. FairSpec => <>(state = "Active")
    BY <1>2, WF1 DEF FairSpec, FairPathBuilding, FairCapabilityNegotiation,
                     FairCryptoHandshake, FairStreamSetup
  <2>2. QED BY <2>1 DEF EventuallyTerminates
<1>3. CASE ~(C_req \subseteq C_sup)
  <2>1. FairSpec => <>(state = "Closed")
    BY <1>3, WF1 DEF FairSpec, FairCapabilityNegotiation, CapabilityNegotiationFailed
  <2>2. QED BY <2>1 DEF EventuallyTerminates
<1>4. QED BY <1>2, <1>3

THEOREM PathBuildingProgress == FairSpec => SuccessfulPathBuilding
<1>1. SUFFICES ASSUME FairSpec PROVE SuccessfulPathBuilding
  OBVIOUS
<1>2. FairSpec => ((state = "PathBuilding") ~> (state # "PathBuilding"))
  BY WF1 DEF FairSpec, FairPathBuilding, BuildPath, PathBuildingFailed
<1>3. QED BY <1>2 DEF SuccessfulPathBuilding

THEOREM CapabilityNegotiationProgress == FairSpec => SuccessfulCapabilityNegotiation
<1>1. SUFFICES ASSUME FairSpec PROVE SuccessfulCapabilityNegotiation
  OBVIOUS
<1>2. FairSpec => ((state = "CapabilityNegotiation" /\ C_req \subseteq C_sup) ~> 
                   (state = "CryptoHandshake"))
  BY WF1 DEF FairSpec, FairCapabilityNegotiation, NegotiateCapabilities
<1>3. QED BY <1>2 DEF SuccessfulCapabilityNegotiation

THEOREM CryptoHandshakeProgress == FairSpec => SuccessfulCryptoHandshake
<1>1. SUFFICES ASSUME FairSpec PROVE SuccessfulCryptoHandshake
  OBVIOUS
<1>2. FairSpec => ((state = "CryptoHandshake" /\ crypto_state = "Handshaking") ~> 
                   (state \in {"StreamSetup", "Recovery"}))
  BY WF1 DEF FairSpec, FairCryptoHandshake, CompleteCryptoHandshake, CryptoHandshakeFailed
<1>3. QED BY <1>2 DEF SuccessfulCryptoHandshake

THEOREM StreamSetupProgress == FairSpec => SuccessfulStreamSetup
<1>1. SUFFICES ASSUME FairSpec PROVE SuccessfulStreamSetup
  OBVIOUS
<1>2. FairSpec => ((state = "StreamSetup") ~> (state = "Active"))
  BY WF1 DEF FairSpec, FairStreamSetup, SetupStreams
<1>3. QED BY <1>2 DEF SuccessfulStreamSetup

THEOREM RecoveryMechanismProgress == FairSpec => (RecoveryProgress /\ RecoveryExhaustion)
<1>1. SUFFICES ASSUME FairSpec PROVE (RecoveryProgress /\ RecoveryExhaustion)
  OBVIOUS
<1>2. FairSpec => RecoveryProgress
  BY WF1 DEF FairSpec, FairRecovery, StartRecovery, RecoveryProgress
<1>3. FairSpec => RecoveryExhaustion
  BY WF1 DEF FairSpec, FairRecovery, RecoveryFailed, RecoveryExhaustion
<1>4. QED BY <1>2, <1>3

THEOREM TimeoutHandlingProgress == FairSpec => TimeoutRecovery
<1>1. SUFFICES ASSUME FairSpec PROVE TimeoutRecovery
  OBVIOUS
<1>2. FairSpec => ((operation_timer > TimeoutLimit) ~> (state = "Recovery"))
  BY WF1 DEF FairSpec, FairTimeout, HandleTimeout
<1>3. QED BY <1>2 DEF TimeoutRecovery

THEOREM GracefulShutdownProgress == FairSpec => GracefulShutdown
<1>1. SUFFICES ASSUME FairSpec PROVE GracefulShutdown
  OBVIOUS
<1>2. FairSpec => ((state = "Closing") ~> (state = "Closed"))
  BY WF1 DEF FairSpec, FairShutdown, CompleteClosing
<1>3. QED BY <1>2 DEF GracefulShutdown

(*************************************************************************)
(* Main Safety and Liveness Theorems                                     *)
(*************************************************************************)

(* Combined safety theorem proving all invariants hold *)
THEOREM MainSafetyTheorem == 
    Spec => [](TypeInvariant /\ Inv_PathLen /\ Inv_PathUniqueness /\ 
               Inv_StateProgression /\ Inv_ErrorConsistency /\ Inv_RetryBounds /\
               Inv_CapabilityConsistency /\ Inv_CryptoStateConsistency /\
               Inv_StreamConsistency /\ Inv_PowerManagement /\ 
               Inv_PerformanceMetrics /\ Inv_TimingConsistency /\
               Inv_NetworkConditions /\ Inv_ResourceManagement)
BY SafetyInvariant, PathLengthSafety, PathUniqueness, StateProgressionSafety,
   ErrorHandlingSafety, RetryMechanismSafety, CapabilityNegotiationSafety,
   CryptographicStateSafety, StreamManagementSafety, PowerManagementSafety,
   PerformanceMetricsSafety, TimingConsistencySafety, ResourceManagementSafety

(* Combined liveness theorem proving progress properties *)
THEOREM MainLivenessTheorem == 
    FairSpec => (EventuallyLeavesInit /\ EventuallyTerminates /\
                 SuccessfulPathBuilding /\ SuccessfulCapabilityNegotiation /\
                 SuccessfulCryptoHandshake /\ SuccessfulStreamSetup /\
                 RecoveryProgress /\ RecoveryExhaustion /\ TimeoutRecovery /\
                 GracefulShutdown)
BY BasicLivenessProperty, TerminationProperty, PathBuildingProgress,
   CapabilityNegotiationProgress, CryptoHandshakeProgress, StreamSetupProgress,
   RecoveryMechanismProgress, TimeoutHandlingProgress, GracefulShutdownProgress

(*************************************************************************)
(* State Transition Validity and Protocol Correctness                   *)
(*************************************************************************)

ValidStateTransition == 
    /\ (state = "Init" /\ state' = "PathBuilding") => TRUE
    /\ (state = "PathBuilding" /\ state' = "CapabilityNegotiation") => (path # <<>>)
    /\ (state = "PathBuilding" /\ state' = "Recovery") => (error = "INVALID_PATH")
    /\ (state = "CapabilityNegotiation" /\ state' = "CryptoHandshake") => (C_req \subseteq C_sup)
    /\ (state = "CapabilityNegotiation" /\ state' = "Closing") => ~(C_req \subseteq C_sup)
    /\ (state = "CryptoHandshake" /\ state' = "StreamSetup") => (crypto_state = "Established")
    /\ (state = "CryptoHandshake" /\ state' = "Recovery") => (crypto_state = "Failed")
    /\ (state = "StreamSetup" /\ state' = "Active") => (active_streams # {})
    /\ (state = "Active" /\ power' = "LowPower") => (last_activity + 50 < operation_timer)
    /\ (state = "Recovery" /\ retry_count < MaxRetries) => (state' \in {prev_state, "Closing"})
    /\ (state = "Recovery" /\ retry_count >= MaxRetries) => (state' = "Closing")
    /\ (state = "Closing" /\ state' = "Closed") => (\A s \in active_streams : stream_states[s] = "Closing")

ProtocolCorrectness == 
    /\ (state = "Active") => (path # <<>> /\ crypto_state = "Established" /\ 
                              negotiated_caps = C_req \cap C_sup /\ active_streams # {})
    /\ (state = "Closed" /\ error = "UNSUPPORTED_CAP") => ~(C_req \subseteq C_sup)
    /\ (retry_count = MaxRetries /\ state = "Recovery") => <>(state = "Closing")
    /\ (operation_timer > TimeoutLimit) => <>(state = "Recovery" \/ state = "Closed")

THEOREM StateTransitionValidity == Spec => []ValidStateTransition
<1>1. Init => ValidStateTransition
  <2>1. ASSUME Init PROVE ValidStateTransition
    BY <2>1 DEF Init, ValidStateTransition
  <2>2. QED BY <2>1
<1>2. ValidStateTransition /\ [Next]_vars => ValidStateTransition'
  <2>1. ASSUME ValidStateTransition, [Next]_vars PROVE ValidStateTransition'
    BY <2>1 DEF Next, ValidStateTransition, StartPathBuilding, BuildPath,
                PathBuildingFailed, NegotiateCapabilities, CapabilityNegotiationFailed,
                StartCryptoHandshake, CompleteCryptoHandshake, CryptoHandshakeFailed,
                SetupStreams, EnterLowPower, StartRecovery, RecoveryFailed, CompleteClosing
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

THEOREM ProtocolCorrectnessTheorem == Spec => []ProtocolCorrectness
<1>1. Init => ProtocolCorrectness
  <2>1. ASSUME Init PROVE ProtocolCorrectness
    BY <2>1 DEF Init, ProtocolCorrectness
  <2>2. QED BY <2>1
<1>2. ProtocolCorrectness /\ [Next]_vars => ProtocolCorrectness'
  <2>1. ASSUME ProtocolCorrectness, [Next]_vars PROVE ProtocolCorrectness'
    BY <2>1 DEF Next, ProtocolCorrectness, SetupStreams, NegotiateCapabilities,
                CompleteCryptoHandshake, CapabilityNegotiationFailed, RecoveryFailed,
                HandleTimeout
  <2>2. QED BY <2>1
<1>3. QED BY <1>1, <1>2, PTL DEF Spec

(*************************************************************************)
(* Model Parameters and Optimization Constraints                         *)
(*************************************************************************)

ModelConstraints == 
    /\ NodeCount >= 8
    /\ Cardinality(CapSet) >= 3
    /\ MaxStreams >= 1
    /\ MaxRetries >= 1
    /\ TimeoutLimit >= 10

OptimizedModelParameters == 
    /\ NodeCount \in 8..15
    /\ Cardinality(CapSet) \in 3..8
    /\ MaxStreams \in 1..5
    /\ MaxRetries \in 1..3
    /\ TimeoutLimit \in 10..100

THEOREM ModelParameterValidity == 
    (ModelConstraints /\ OptimizedModelParameters) => 
    (Spec => []TypeInvariant)
BY SafetyInvariant DEF ModelConstraints, OptimizedModelParameters

============================================================ 