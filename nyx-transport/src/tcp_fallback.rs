//! TCP Fallback Encapsulation
//!
//! When UDP traversal fails (e.g., restrictive NAT/Firewall) Nyx can tunnel
//! its fixed-size datagrams over a single TCP connection. Each Nyx packet is
//! length-prefixed with a 2-byte big-endian size (<= 1500). This framing keeps
//! boundaries so upper layers remain unchanged.
//!
//! The fallback layer intentionally keeps logic minimal: congestion control
//! becomes TCP's responsibility, and latency cost is accepted only when UDP is
//! unavailable.
//!
//! # Example
//! ```rust,ignore
//! // server
//! let srv = TcpEncapListener::bind(44380).await?;
//! // client
//! let conn = TcpEncapConnection::connect("example.com:44380").await?;
//! conn.send(&bytes).await?;
//! ```

#![forbid(unsafe_code)]

use std::net::SocketAddr;
use tokio::{net::{TcpListener, TcpStream}, io::{AsyncReadExt, AsyncWriteExt}, sync::mpsc};
use tracing::{info, error};

const MAX_FRAME: usize = 2048; // generous upper bound; Nyx uses 1280

/// Length-prefixed read helper.
async fn read_frame(stream: &mut TcpStream) -> std::io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 2];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {
            let len = u16::from_be_bytes(len_buf) as usize;
            if len == 0 || len > MAX_FRAME { return Err(std::io::ErrorKind::InvalidData.into()); }
            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).await?;
            Ok(Some(data))
        }
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(e) => Err(e),
    }
}

async fn write_frame(stream: &mut TcpStream, data: &[u8]) -> std::io::Result<()> {
    if data.len() > MAX_FRAME { return Err(std::io::ErrorKind::InvalidInput.into()); }
    stream.write_all(&(data.len() as u16).to_be_bytes()).await?;
    stream.write_all(data).await?;
    stream.flush().await
}

/// Server-side listener accepting encapsulated TCP connections.
pub struct TcpEncapListener {
    pub incoming: mpsc::Receiver<(SocketAddr, Vec<u8>)>,
}

impl TcpEncapListener {
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).await?;
        let (tx, rx) = mpsc::channel::<(SocketAddr, Vec<u8>)>(1024);
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, addr)) => {
                        info!("tcp_fallback: connection from {}", addr);
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            loop {
                                match read_frame(&mut stream).await {
                                    Ok(Some(packet)) => {
                                        let _ = tx_clone.send((addr, packet)).await;
                                    }
                                    Ok(None) => break, // closed
                                    Err(e) => {
                                        error!("tcp_fallback recv error: {}", e);
                                        break;
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => error!("tcp_fallback accept error: {}", e),
                }
            }
        });
        Ok(Self { incoming: rx })
    }
}

/// Client/peer connection over TCP encapsulation.
#[derive(Clone)]
pub struct TcpEncapConnection {
    stream: tokio::sync::Mutex<TcpStream>,
    peer: SocketAddr,
}

impl TcpEncapConnection {
    pub async fn connect(addr: &str) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let peer = stream.peer_addr()?;
        Ok(Self { stream: tokio::sync::Mutex::new(stream), peer })
    }

    pub async fn send(&self, data: &[u8]) -> std::io::Result<()> {
        let mut guard = self.stream.lock().await;
        write_frame(&mut *guard, data).await
    }

    pub async fn recv(&self) -> std::io::Result<Option<Vec<u8>>> {
        let mut guard = self.stream.lock().await;
        read_frame(&mut *guard).await
    }

    #[must_use] pub fn peer_addr(&self) -> SocketAddr { self.peer }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let _ = tracing_subscriber::fmt::try_init();
        let listener = TcpEncapListener::bind(4480).await.unwrap();
        let conn = TcpEncapConnection::connect("127.0.0.1:4480").await.unwrap();
        conn.send(&[1,2,3]).await.unwrap();
        if let Some((_, pkt)) = listener.incoming.recv().await {
            assert_eq!(pkt, vec![1,2,3]);
        } else { panic!("no packet"); }
    }
} 