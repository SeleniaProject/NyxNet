//! Minimal ICE Lite handshake helpers.
//!
//! Implements just enough of RFC 8445 to punch through NATs for Nyx use-case.
//! Full ICE is out-of-scope; we assume server is ICE-Lite and clients perform
//! regular ICE. Here we provide:
//! 1. Candidate gathering (host only, srflx via STUN).
//! 2. STUN Binding request encoding/decoding (RFC 5389).
//! 3. Simple `perform_handshake` that exchanges Binding requests/responses over
//!    a provided [`UdpSocket`].
//!
//! The API is intentionally high-level; lower-level control should be built on
//! top when needed.

#![forbid(unsafe_code)]

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use tokio::{net::UdpSocket, time::{timeout, Duration}};
use rand::{thread_rng, Rng};
use crate::UdpPool;
// tracing removed (no direct logging in this module yet)

/// STUN constants.
const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;

/// Encode STUN Binding request with random transaction ID.
fn encode_binding_request(buf: &mut [u8]) -> [u8; 12] {
    let mut rng = thread_rng();
    let txid: [u8; 12] = rng.gen();
    // Message Type
    buf[0..2].copy_from_slice(&STUN_BINDING_REQUEST.to_be_bytes());
    // Length = 0
    buf[2..4].copy_from_slice(&0u16.to_be_bytes());
    // Magic cookie
    buf[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
    // Transaction ID
    buf[8..20].copy_from_slice(&txid);
    txid
}

/// Decode Binding response and extract XOR-Mapped address if present.
pub fn decode_binding_response(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 20 { return None; }
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    if msg_type != STUN_BINDING_RESPONSE { return None; }
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    if data.len() < 20 + msg_len { return None; }
    // Attributes start at byte 20.
    let mut idx = 20;
    while idx + 4 <= 20 + msg_len {
        let attr_type = u16::from_be_bytes([data[idx], data[idx+1]]);
        let attr_len = u16::from_be_bytes([data[idx+2], data[idx+3]]) as usize;
        idx += 4;
        if idx + attr_len > data.len() { break; }
        if attr_type == 0x0020 /* XOR-MAPPED-ADDRESS */ && attr_len >= 8 {
            let family = data[idx+1];
            if family == 0x01 { // IPv4
                let x_port = u16::from_be_bytes([data[idx+2], data[idx+3]]);
                let port = x_port ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
                let mut ip = [0u8;4];
                for i in 0..4 { ip[i] = data[idx+4+i] ^ (STUN_MAGIC_COOKIE.to_be_bytes()[i]); }
                return Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), port)));
            }
        }
        idx += (attr_len + 3) & !3; // 4-byte padding
    }
    None
}

/// Gather a server-reflexive candidate via STUN Binding request to given STUN server.
pub async fn get_srflx_addr(pool: &UdpPool, stun_server: SocketAddr) -> std::io::Result<SocketAddr> {
    let sock = pool.socket();
    let mut req = [0u8; 20];
    let _txid = encode_binding_request(&mut req);
    sock.send_to(&req, stun_server).await?;

    let mut buf = [0u8; 1500];
    let res_len = timeout(Duration::from_secs(2), sock.recv(&mut buf)).await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "stun timeout"))??;
    if let Some(addr) = decode_binding_response(&buf[..res_len]) {
        Ok(addr)
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid stun response"))
    }
}

/// Perform ICE-Lite handshake: send Binding request and wait for Binding response from peer.
/// Returns `true` if peer responded within timeout.
pub async fn ice_lite_handshake(sock: &UdpSocket, peer: SocketAddr) -> bool {
    let mut req = [0u8; 20];
    let txid = encode_binding_request(&mut req);
    if sock.send_to(&req, peer).await.is_err() { return false; }

    // Wait up to 3s for response.
    let mut buf = [0u8; 1500];
    match timeout(Duration::from_secs(3), sock.recv_from(&mut buf)).await {
        Ok(Ok((len, src))) if src == peer => {
            // Validate txid matches.
            if len >= 20 && &buf[8..20] == txid {
                return true;
            }
        }
        _ => {}
    }
    false
} 