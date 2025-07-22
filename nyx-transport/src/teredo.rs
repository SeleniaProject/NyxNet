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
use tracing::instrument;

/// Default Teredo server (`teredo.remlab.net`).
pub const DEFAULT_SERVER: &str = "[2001:67c:2b0::4]:3544";

/// IPv6 address constructed from external (IPv4, port).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeredoAddr(pub Ipv6Addr);

impl TeredoAddr {
    /// Construct a complete Teredo IPv6 address from a mapped client
    /// public IPv4 address/UDP port and the chosen Teredo *server* IPv4
    /// address. `cone` should be set to `true` when the NAT mapping is
    /// considered cone-style (RFC 3489) so the C-bit (bit 15) in the
    /// flags field is asserted. All other flag bits are zero for now.
    ///
    /// Layout (RFC 4380 §4):
    /// `[ 32b prefix | 32b server_ipv4 | 16b flags | 16b ~port | 32b ~client_ipv4 ]`.
    #[must_use]
    pub fn new(server_ipv4: Ipv4Addr, client_ipv4: Ipv4Addr, client_port: u16, cone: bool) -> Self {
        // Prefix 2001:0000::/32
        let mut segments = [0u16; 8];
        segments[0] = 0x2001;
        segments[1] = 0x0000;

        // Server IPv4 occupies next 32 bits (big-endian)
        let s_oct = server_ipv4.octets();
        segments[2] = ((s_oct[0] as u16) << 8) | s_oct[1] as u16;
        segments[3] = ((s_oct[2] as u16) << 8) | s_oct[3] as u16;

        // Flags (only C-bit currently) – see RFC 4380 §4
        segments[4] = if cone { 0x8000 } else { 0x0000 };

        // Obfuscated port and IP (ones-complement)
        let obsc_port = !client_port;
        segments[5] = obsc_port;

        let c_oct = client_ipv4.octets();
        let obsc_ip = [!c_oct[0], !c_oct[1], !c_oct[2], !c_oct[3]];
        segments[6] = ((obsc_ip[0] as u16) << 8) | obsc_ip[1] as u16;
        segments[7] = ((obsc_ip[2] as u16) << 8) | obsc_ip[3] as u16;

        TeredoAddr(Ipv6Addr::new(
            segments[0], segments[1], segments[2], segments[3],
            segments[4], segments[5], segments[6], segments[7],
        ))
    }

    /// Convenience helper for the common case where the server address is
    /// not required (e.g. when only the mapped values are known). Falls
    /// back to the *global* Teredo prefix with a zero server IPv4 field
    /// (equivalent to 0.0.0.0). Flags=C=0.
    #[must_use]
    pub fn from_mapping(ip: Ipv4Addr, port: u16) -> Self {
        Self::new(Ipv4Addr::UNSPECIFIED, ip, port, false)
    }

    /// Return the underlying IPv6 address.
    #[must_use]
    pub fn addr(&self) -> Ipv6Addr { self.0 }
}

/// Perform Teredo mapping query and return derived IPv6 address.
#[instrument(name = "teredo_discover", skip(server), fields(teredo_server = %server))]
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