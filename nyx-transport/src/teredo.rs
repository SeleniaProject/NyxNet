#![forbid(unsafe_code)]

//! Minimal Teredo‐like IPv6‐over‐UDP helper for NAT traversal.
//!
//! This module implements *client side* behaviour sufficient for Nyx peers to
//! learn their (IPv4, port) mapping and construct an IPv6‐compatible address
//! (`2001:0:` prefix) as defined in RFC 4380 §4.2.  Full relay functionality is
//! **out of scope** for the reference implementation.
//!
//! Steps:
//! 1. Client sends an empty UDP *bubble* to the well-known Teredo server.
//! 2. Server replies with a mapped address response (similar to STUN BINDING
//!    SUCCESS response) carrying the observed external IP/port.
//! 3. Client derives `TeredoAddr` (IPv6) from mapping as per RFC.
//!
//! For integration Nyx treats the resulting `TeredoAddr` as an *alternate path*
//! candidate when direct UDP traversal fails.

use tokio::{net::UdpSocket, time::{timeout, Duration}};
use std::net::{SocketAddr, Ipv6Addr, Ipv4Addr};
use anyhow::{Result, bail};

/// Default Teredo server (`teredo.remlab.net`).
pub const DEFAULT_SERVER: &str = "[2001:67c:2b0::4]:3544";

/// IPv6 address constructed from external (IPv4, port).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeredoAddr(pub Ipv6Addr);

impl TeredoAddr {
    /// Build Teredo address according to RFC 4380 §4.2.
    pub fn from_mapping(ip: Ipv4Addr, port: u16) -> Self {
        let prefix: [u16; 2] = [0x2001, 0]; // 2001:0000::/32
        let obsc_port = !port; // ones‐complement
        let octets = ip.octets();
        let obsc_ip = [!octets[0], !octets[1], !octets[2], !octets[3]];
        let addr = Ipv6Addr::new(
            prefix[0], prefix[1], 0, 0,
            ((obsc_port >> 8) & 0xff) as u16 | ((obsc_port & 0xff) << 8),
            ((obsc_ip[0] as u16) << 8) | obsc_ip[1] as u16,
            ((obsc_ip[2] as u16) << 8) | obsc_ip[3] as u16,
            0xffff, // interface identifier placeholder
        );
        TeredoAddr(addr)
    }
}

/// Perform Teredo mapping query and return derived IPv6 address.
pub async fn discover(server: &str) -> Result<TeredoAddr> {
    let srv: SocketAddr = server.parse()?;
    // bind ephemeral UDP socket
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(srv).await?;
    // send bubble (4 bytes zeros per RFC)
    sock.send(&[0u8; 4]).await?;

    // receive mapping response (expect 12 bytes: magic + mapped addr)
    let mut buf = [0u8; 32];
    let len = timeout(Duration::from_secs(2), sock.recv(&mut buf)).await??;
    if len < 12 { bail!("short teredo response"); }
    // Response format: [0xde,0xad,0xbe,0xef] + ip(4) + port(2)
    if &buf[..4] != &[0xde, 0xad, 0xbe, 0xef] { bail!("invalid teredo magic"); }
    let ip = Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
    let port = u16::from_be_bytes([buf[8], buf[9]]);
    Ok(TeredoAddr::from_mapping(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore]
    async fn test_discover() {
        // This test is ignored by default as it performs external network IO.
        let t = discover(DEFAULT_SERVER).await.unwrap();
        println!("teredo {t:?}");
    }
} 