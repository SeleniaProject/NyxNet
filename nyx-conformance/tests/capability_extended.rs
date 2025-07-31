#![allow(clippy::unwrap_used)]
use nyx_stream::{Capability, encode_caps, decode_caps, negotiate};

#[test]
fn capability_data_roundtrip() {
    let cap = Capability { id: 0x1234, flags: 0, data: vec![1, 2, 3, 4, 5] };
    let buf = encode_caps(&[cap.clone()]);
    let decoded = decode_caps(&buf).expect("decode");
    assert_eq!(decoded, vec![cap]);
}

#[test]
fn capability_large_data_field() {
    let data: Vec<u8> = (0..=255u8).collect();
    let cap = Capability { id: 0xABCD_EF01, flags: 0, data: data.clone() };
    let buf = encode_caps(&[cap.clone()]);
    let decoded = decode_caps(&buf).expect("decode large");
    assert_eq!(decoded[0].data, data);
}

#[test]
fn negotiation_ignores_optional_unknown_caps() {
    // Local supports only id 0x0001 (core)
    let local = [0x0001u32];
    let peer_caps = vec![
        Capability { id: 0x0001, flags: 0, data: Vec::new() },
        Capability { id: 0xDEAD_BEEF, flags: 0, data: Vec::new() }, // optional unknown
    ];
    assert!(negotiate(&local, &peer_caps).is_ok());
} 