#![forbid(unsafe_code)]

//! Capability negotiation helper invoked during handshake.
//! Given raw CBOR from peer, verifies required capabilities against local
//! implementation and returns an optional CLOSE frame payload when
//! negotiation fails.

use crate::capability::{decode_caps, negotiate, Capability, LOCAL_CAP_IDS, NegotiationError};
use crate::management::build_close_unsupported_cap;

/// Perform capability negotiation.
///
/// * `peer_caps_buf` – CBOR-encoded capability list received from peer.
///
/// On success returns `Ok(peer_caps)`.  On failure returns `Err(close_frame)`
/// where `close_frame` is the payload for a CLOSE (0x3F) frame containing the
/// `ERR_UNSUPPORTED_CAP` error code as defined by the protocol.
pub fn perform_cap_negotiation(peer_caps_buf: &[u8]) -> Result<Vec<Capability>, Vec<u8>> {
    let peer_caps = match decode_caps(peer_caps_buf) {
        Ok(v) => v,
        Err(_) => {
            // Malformed CBOR – treat as PROTOCOL_VIOLATION (0x01)
            return Err(crate::management::build_close_frame(0x01, b"malformed capability CBOR"));
        }
    };

    match negotiate(LOCAL_CAP_IDS, &peer_caps) {
        Ok(()) => Ok(peer_caps),
        Err(NegotiationError::Unsupported(id)) => {
            Err(build_close_unsupported_cap(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{encode_caps, local_caps, FLAG_REQUIRED};
    use crate::management::{parse_close_frame, ERR_UNSUPPORTED_CAP};

    #[test]
    fn negotiation_success() {
        let buf = encode_caps(&local_caps());
        let decoded_local = perform_cap_negotiation(&buf).expect("should accept its own caps");
        assert_eq!(decoded_local, local_caps());
    }

    #[test]
    fn negotiation_failure_builds_close() {
        // Remote requires an unsupported capability 0xDEAD_BEEF
        let remote_caps = vec![Capability { id: 0xDEAD_BEEF, flags: FLAG_REQUIRED, data: Vec::new() }];
        let buf = encode_caps(&remote_caps);
        let err = perform_cap_negotiation(&buf).expect_err("expected failure");
        // Parse generated CLOSE frame
        let (_, frame) = parse_close_frame(&err).expect("parse close");
        assert_eq!(frame.code, ERR_UNSUPPORTED_CAP);
        // Reason should contain cap_id in big-endian
        assert_eq!(frame.reason, &0xDEADBEEF_u32.to_be_bytes()[..]);
    }
} 