---- MODULE nyx_multipath_plugin ----
EXTENDS Naturals, Sequences, FiniteSets

(*************************************************************************)
(* Nyx Protocol – Multipath selection & Plugin Capability Negotiation   *)
(* Formal model for v1.0 core features                                    *)
(*                                                                       *)
(*   • Path length is chosen dynamically (3–7).                          *)
(*   • Each hop is a 32-byte NodeId (modelled as Nat for uniqueness).    *)
(*   • During handshake the peer advertises a set of capabilities C_req. *)
(*     The local implementation supports C_sup.                          *)
(*   • If C_req ⊆ C_sup the session proceeds in state = "Open".          *)
(*   • Otherwise we immediately transition to state = "Close" with       *)
(*     error = UNSUPPORTED_CAP (0x07).                                    *)
(*************************************************************************)

CONSTANTS NodeCount \* total nodes in network (>7)
CONSTANTS CapSet    \* Universe of capability IDs (Nat)

VARIABLES path,           \* Sequence of selected NodeIds
          C_req, C_sup,   \* Sets of capabilities (subset CapSet)
          state,          \* "Init" | "Open" | "Close"
          error           \* None | 7 (UNSUPPORTED_CAP)

Init == /\ state = "Init"
        /\ path  \in Seq(1..NodeCount)
        /\ Len(path) \in 3..7
        /\ C_req \subseteq CapSet
        /\ C_sup \subseteq CapSet
        /\ error = None

ChoosePath == /\ state = "Init"
               /\ path' = path \* (fixed once)
               /\ UNCHANGED <<C_req, C_sup, error, state>>

NegotiateOK == /\ state = "Init"
               /\ C_req \subseteq C_sup
               /\ state' = "Open"
               /\ UNCHANGED <<path, C_req, C_sup, error>>

NegotiateFail == /\ state = "Init"
                 /\ ~(C_req \subseteq C_sup)
                 /\ state' = "Close"
                 /\ error' = 7
                 /\ UNCHANGED <<path, C_req, C_sup>>

(* No further state change after Open / Close for this model *)
Terminal == /\ state \in {"Open", "Close"}
            /\ UNCHANGED <<path, C_req, C_sup, state, error>>

Next == ChoosePath \/ NegotiateOK \/ NegotiateFail \/ Terminal

Spec == Init /\ [][Next]_<<path, C_req, C_sup, state, error>>

(*************************************************************************)
(* Invariants                                                            *)
(*************************************************************************)

Inv_PathLen  == state # "Init" => Len(path) \in 3..7
Inv_NoDup    == state # "Init" => \A i, j \in 1..Len(path): i # j => path[i] # path[j]
Inv_Error    == state = "Close" => error = 7
Inv_NoError  == state = "Open"  => error = None

THEOREM Spec => []Inv_PathLen
THEOREM Spec => []Inv_NoDup
THEOREM Spec => []Inv_Error
THEOREM Spec => []Inv_NoError

(*************************************************************************)
(* Liveness: eventually we leave Init                                     *)
(*************************************************************************)

Terminating == <> (state # "Init")

THEOREM Spec => Terminating

============================================================ 