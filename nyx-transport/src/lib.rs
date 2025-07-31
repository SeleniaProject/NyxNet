#![forbid(unsafe_code)]

//! Nyx UDP transport adapter.
//!
//! * Single `UdpSocket` bound with `SO_REUSEPORT` when supported.
//! * Async receive loop dispatches datagrams to a handler trait.
//! * Provides helper for basic UDP hole punching (ICE-lite style stub).

use std::{net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr}, sync::Arc};
use once_cell::sync::OnceCell;
use socket2::{Domain, Type};
use tokio::{net::UdpSocket, sync::mpsc};
use tracing::{info, error};
use async_trait::async_trait;
use nyx_mix::CoverGenerator;
use crate::teredo::{discover as teredo_discover, DEFAULT_SERVER, TeredoAddr};
// timing obfuscator moved to upper layer
use tokio::time::{sleep, Duration};

pub mod ice;
pub mod stun_server;

#[cfg(feature = "quic")]
pub mod quic;
#[cfg(feature = "quic")]
pub use quic::{QuicEndpoint, QuicConnection};
#[cfg(feature = "quic")]
pub mod tcp_fallback;
#[cfg(feature = "quic")]
pub use tcp_fallback::{TcpEncapListener, TcpEncapConnection};

#[cfg(feature = "quic")]
pub mod path_validation;
#[cfg(feature = "quic")]
pub use path_validation::PathValidator;

#[cfg(not(feature = "quic"))]
pub struct QuicEndpoint; // stubs
#[cfg(not(feature = "quic"))]
pub struct QuicConnection;
#[cfg(not(feature = "quic"))]
pub struct PathValidator;

pub mod teredo;

/// Maximum datagram size (aligned with 1280B spec).
const MAX_DATAGRAM: usize = 1280;

/// Trait for components that consume inbound packets.
#[async_trait]
pub trait PacketHandler: Send + Sync + 'static {
    async fn handle_packet(&self, src: SocketAddr, data: &[u8]);
}

/// UDP socket pool: wraps a single socket but keeps Arc for sharing.
#[derive(Clone)]
pub struct UdpPool {
    socket: Arc<UdpSocket>,
}

impl UdpPool {
    /// Bind on 0.0.0.0:port with reuse_port when possible.
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        // Build socket manually to set reuse_port (if available).
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let domain = Domain::for_address(addr);
        let socket = socket2::Socket::new(domain, Type::DGRAM, None)?;
        // ReusePort best-effort.
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "freebsd"))]
        socket.set_reuse_port(true)?;
        socket.set_reuse_address(true)?;
        socket.bind(&addr.into())?;
        let std_sock: std::net::UdpSocket = socket.into();
        std_sock.set_nonblocking(true)?;
        let udp = UdpSocket::from_std(std_sock)?;
        Ok(Self { socket: Arc::new(udp) })
    }

    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }
}

/// Main transport adapter. Spawns RX task and exposes TX API.
pub struct Transport {
    pool: UdpPool,
    tx: mpsc::Sender<(SocketAddr, Vec<u8>)>,

    /// Optional Teredo-derived IPv6 address of this node (lazy‐discovered).
    teredo_addr: OnceCell<TeredoAddr>,
}

impl Transport {
    /// Start transport; returns instance and transmission channel for internal use.
    pub async fn start<H: PacketHandler>(port: u16, handler: Arc<H>) -> std::io::Result<Self> {
        #[cfg(target_os = "linux")]
        let _ = nyx_core::install_seccomp();

        let pool = UdpPool::bind(port).await?;
        let sock = pool.socket();
        let (tx, mut rx) = mpsc::channel::<(SocketAddr, Vec<u8>)>(1024);

        // RX loop
        let rx_sock = sock.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; MAX_DATAGRAM];
            loop {
                match rx_sock.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        handler.handle_packet(src, &buf[..len]).await;
                    }
                    Err(e) => {
                        error!("udp recv error: {e}");
                    }
                }
            }
        });

        // TX loop
        let tx_sock = sock.clone();
        tokio::spawn(async move {
            while let Some((addr, data)) = rx.recv().await {
                if let Err(e) = tx_sock.send_to(&data, addr).await {
                    error!("udp send error: {e}");
                }
            }
        });

        info!("nyx-transport listening on {}", sock.local_addr().unwrap());
        Ok(Self { pool, tx, teredo_addr: OnceCell::new() })
    }

    /// Send datagram asynchronously.
    pub async fn send(&self, addr: SocketAddr, data: &[u8]) {
        let _ = self.tx.send((addr, data.to_vec())).await;
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.pool.socket().local_addr()
    }

    /// Return (and cache) local Teredo IPv6 address discovered via default server.
    /// This helper can be used by upper layers when IPv4 traversal fails.
    pub async fn teredo_ipv6(&self) -> Option<Ipv6Addr> {
        if let Some(addr) = self.teredo_addr.get() {
            return Some(addr.0);
        }
        match teredo_discover(DEFAULT_SERVER).await {
            Ok(t) => {
                let _ = self.teredo_addr.set(t);
                Some(t.0)
            }
            Err(e) => {
                tracing::warn!("teredo discovery failed: {e}");
                None
            }
        }
    }

    /// Spawn background task generating cover traffic to `target` at Poisson rate `lambda` (events/s).
    pub fn spawn_cover_task(&self, target: SocketAddr, lambda: f64) {
        let generator = CoverGenerator::new(lambda);
        let tx_clone = self.clone();
        tokio::spawn(async move {
            loop {
                let delay: Duration = generator.next_delay();
                sleep(delay).await;
                tx_clone.send(target, &[]).await;
            }
        });
    }
}

impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            tx: self.tx.clone(),
            teredo_addr: self.teredo_addr.clone(),
        }
    }
}

/// Very simple hole-punching stub: send empty packet to peer to open NAT.
pub async fn hole_punch(transport: &Transport, peer: SocketAddr) {
    transport.send(peer, &[]).await;
}
