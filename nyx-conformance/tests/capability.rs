use nyx_stream::{Capability, encode_caps, decode_caps, negotiate, NegotiationError};
use nyx_stream::management::ERR_UNSUPPORTED_CAP;

#[test]
fn capability_encode_decode_roundtrip() {
    let caps = vec![Capability::required(1), Capability::optional(2)];
    let buf = encode_caps(&caps);
    let decoded = decode_caps(&buf).expect("decode");
    assert_eq!(caps, decoded);
}

#[test]
fn capability_negotiation_success_and_failure() {
    let peer = vec![Capability::required(42), Capability::optional(200)];
    // success path
    assert!(negotiate(&[42, 300], &peer).is_ok());
    // failure path
    let err = negotiate(&[300], &peer).unwrap_err();
    assert_eq!(err, NegotiationError::Unsupported(42));
    // constant check
    assert_eq!(ERR_UNSUPPORTED_CAP, 0x07);
} 