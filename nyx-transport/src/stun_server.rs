//! Lightweight STUN Binding responder (RFC 5389) suitable for test nets.
//!
//! * Listens on a UDP socket (IPv4) and responds to Binding Requests.
//! * Handles XOR-MAPPED-ADDRESS attribute construction.
//! * Not intended for production at scale; only to aid Nyx dev / local tests.
//!
//! # Example
//! ```ignore
//! // Launch STUN server on port 3478 (tokio runtime required)
//! # async {
//! let handle = nyx_transport::stun_server::start_stun_server(3478).await.unwrap();
//! handle.await.unwrap();
//! # }
//! ```

#![forbid(unsafe_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use tokio::{net::UdpSocket, task::JoinHandle};
use tracing::{info, error};

const STUN_BINDING_REQUEST: u16 = 0x0001;
const STUN_BINDING_RESPONSE: u16 = 0x0101;
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;
const XOR_MAPPED_ADDR: u16 = 0x0020;

/// Start a background STUN server on `0.0.0.0:port`.
pub async fn start_stun_server(port: u16) -> std::io::Result<JoinHandle<()>> {
    let socket = UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))).await?;
    info!("STUN server listening on {}", socket.local_addr()?);
    Ok(tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, src)) => {
                    if len < 20 { continue; }
                    let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
                    if msg_type != STUN_BINDING_REQUEST { continue; }
                    let txid = &buf[8..20];
                    // Build response
                    let mut resp = [0u8; 32];
                    // Type & length placeholder
                    resp[0..2].copy_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
                    // Will fill length later
                    resp[4..8].copy_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
                    resp[8..20].copy_from_slice(txid);
                    // XOR-MAPPED-ADDRESS attr at offset 20
                    let mut idx = 20;
                    resp[idx..idx+2].copy_from_slice(&XOR_MAPPED_ADDR.to_be_bytes());
                    resp[idx+2..idx+4].copy_from_slice(&8u16.to_be_bytes());
                    resp[idx+4] = 0; // reserved
                    resp[idx+5] = 0x01; // IPv4
                    let x_port = src.port() ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
                    resp[idx+6..idx+8].copy_from_slice(&x_port.to_be_bytes());
                    if let IpAddr::V4(ipv4) = src.ip() {
                        for i in 0..4 {
                            resp[idx+8+i] = ipv4.octets()[i] ^ STUN_MAGIC_COOKIE.to_be_bytes()[i];
                        }
                    } else {
                        continue; // skip non-IPv4 for simplicity
                    }
                    let msg_len = 12u16; // one attribute header+value (4+8)
                    resp[2..4].copy_from_slice(&msg_len.to_be_bytes());
                    if socket.send_to(&resp[..idx+12], src).await.is_err() {
                        error!("failed to send STUN response");
                    }
                }
                Err(e) => {
                    error!("stun recv error: {}", e);
                }
            }
        }
    }))
} 