//! QUIC Datagram Transport support using Quinn.
//!
//! This module offers a thin wrapper around [`quinn`] to expose unreliable datagram
//! functionality compatible with Nyx's 1280-byte fixed packet size. It is **not** a
//! full-fledged QUIC stream implementation; only the DATAGRAM extension is used.
//!
//! The primary entry points are [`QuicEndpoint`] (listener) and [`QuicConnection`]
//! (client/peer). For ease of use in testbeds the TLS certificate is generated on
//! first run using `rcgen` and stored on disk, while the client disables
//! certificate verification (peer authenticity will instead rely on Nyx
//! handshake layers).
//!
//! Security note: Disabling cert verification is acceptable here because Nyx
//! already performs its own authenticated encryption at upper layers; the TLS
//! layer merely satisfies QUIC's handshake requirements.

#![forbid(unsafe_code)]

use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use quinn::{Endpoint, EndpointConfig, TransportConfig, ServerConfig, ClientConfig, Connection, NewConnection};
use quinn::crypto::rustls::{TlsAcceptor, TlsConnector};
use rcgen::{generate_simple_self_signed, KeyPair};
use tokio::sync::mpsc;
use tracing::{info, error, instrument, Instrument};
use futures::StreamExt;

/// Default QUIC idle timeout (10s).
const MAX_IDLE_TIMEOUT_MS: u64 = 10_000;

/// Path on disk where the self-signed certificate is cached (server side).
fn cert_path() -> PathBuf {
    PathBuf::from("nyx_quic_cert.der")
}

/// Generate (or load existing) self-signed certificate for the server.
fn generate_or_load_cert() -> anyhow::Result<(rustls::Certificate, rustls::PrivateKey)> {
    let path = cert_path();
    if path.exists() {
        let bytes = std::fs::read(&path)?;
        let cert = rustls::Certificate(bytes.clone());
        // Private key is next to cert with .key extension.
        let key_bytes = std::fs::read(path.with_extension("key"))?;
        let key = rustls::PrivateKey(key_bytes);
        Ok((cert, key))
    } else {
        let subject_alt_names = vec!["localhost".into()];
        let cert = generate_simple_self_signed(subject_alt_names)?;
        let key = rustls::PrivateKey(cert.serialize_private_key_der());
        let cert_der = cert.serialize_der()?;
        let cert_path = cert_path();
        std::fs::write(&cert_path, &cert_der)?;
        std::fs::write(cert_path.with_extension("key"), &key.0)?;
        Ok((rustls::Certificate(cert_der), key))
    }
}

/// QUIC listener providing incoming datagrams via channel.
pub struct QuicEndpoint {
    endpoint: Endpoint,
    pub incoming: mpsc::Receiver<(SocketAddr, Vec<u8>)>,
}

impl QuicEndpoint {
    /// Bind QUIC listener on `0.0.0.0:port`.
    #[instrument(name = "quic_endpoint_bind", skip(port), fields(local_port = port))]
    pub async fn bind(port: u16) -> anyhow::Result<Self> {
        let (cert, key) = generate_or_load_cert()?;
        let mut server_config = ServerConfig::with_single_cert(vec![cert], key)?;
        let mut transport_config = TransportConfig::default();
        transport_config.max_idle_timeout(Some(std::time::Duration::from_millis(MAX_IDLE_TIMEOUT_MS).try_into().unwrap()));
        transport_config.datagram_receive_buffer_size(Some(1 << 20));
        server_config.transport = Arc::new(transport_config);

        let mut endpoint_config = EndpointConfig::default();
        endpoint_config.allow_unknown_connections(true);

        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
        let (endpoint, mut incoming_conn) = Endpoint::server(server_config, addr)?;
        let (tx, rx) = mpsc::channel::<(SocketAddr, Vec<u8>)>(1024);

        // Spawn task accepting connections.
        tokio::spawn(async move {
            while let Some(conn) = incoming_conn.next().await {
                match conn.await {
                    Ok(new_conn) => {
                        info!("quic: new connection from {}", new_conn.connection.remote_address());
                        let tx_clone = tx.clone();
                        tokio::spawn(Self::recv_task(new_conn.connection, tx_clone));
                    }
                    Err(e) => error!("quic accept error: {e}")
                }
            }
        }.instrument(tracing::info_span!("quic_accept_loop")));

        Ok(Self { endpoint, incoming: rx })
    }

    #[instrument(name = "quic_datagram_recv", skip(conn, tx), fields(peer = %conn.remote_address()))]
    async fn recv_task(conn: Connection, tx: mpsc::Sender<(SocketAddr, Vec<u8>)>) {
        loop {
            match conn.read_datagram().await {
                Ok(Some(data)) => {
                    let _ = tx.send((conn.remote_address(), data.to_vec())).await;
                }
                Ok(None) => break, // connection closed
                Err(e) => {
                    error!("quic datagram recv error: {e}");
                    break;
                }
            }
        }
    }
}

/// Client-side QUIC connection wrapper.
pub struct QuicConnection {
    connection: Connection,
}

impl QuicConnection {
    /// Connect to `server_addr` (e.g., "127.0.0.1:4433") without certificate verification.
    #[instrument(name = "quic_client_connect", skip(server_addr), fields(server = %server_addr))]
    pub async fn connect(server_addr: &str) -> anyhow::Result<Self> {
        let mut client_cfg = ClientConfig::with_native_roots();
        // Disable cert verification (Nyx encrypts at higher layer)
        let mut crypto = rustls::ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        client_cfg.crypto = Arc::new(crypto);

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        endpoint.set_default_client_config(client_cfg);

        let NewConnection { connection, .. } = endpoint.connect(server_addr.parse()?, "nyx")?.await?;
        Ok(Self { connection })
    }

    /// Send datagram (<= 1200B) to peer.
    pub fn send(&self, data: &[u8]) -> Result<(), quinn::WriteError> {
        self.connection.send_datagram(data.into())
    }

    /// Receive next datagram.
    #[instrument(name = "quic_client_recv", skip(self))]
    pub async fn recv(&self) -> Option<Result<Vec<u8>, quinn::ReadDatagramError>> {
        match self.connection.read_datagram().await {
            Ok(Some(d)) => Some(Ok(d.to_vec())),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }

    /// Remote address.
    #[must_use] pub fn peer_addr(&self) -> SocketAddr { self.connection.remote_address() }
}

/// No-op certificate verifier (dangerous!).
struct NoVerifier;

impl rustls::client::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::pki_types::ServerName,
        _scts: &mut dyn Iterator<Item=&[u8]>,
        _ocsp: &Option<&[u8]>,
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
} 