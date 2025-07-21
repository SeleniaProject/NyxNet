#![cfg(feature = "quic")]

use nyx_transport::ice::decode_binding_response;

#[test]
fn decode_binding_response_invalid() {
    // Input shorter than minimum STUN header (20 bytes) should return None.
    let buf = [0u8; 10];
    assert!(decode_binding_response(&buf).is_none());
} 