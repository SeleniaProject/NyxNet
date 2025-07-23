#![forbid(unsafe_code)]

use std::{sync::Arc, time::Instant};
use tokio::net::UnixListener;
use tokio_stream::wrappers::{ReceiverStream, UnixListenerStream};
use tracing::{error, info};
use tonic::transport::Server;
use tokio::sync::mpsc;

use nyx_core::{install_panic_abort, NyxConfig};
use nyx_transport::{PacketHandler, Transport};
use nyx_mix::cmix::CmixController;

mod proto {
    tonic::include_proto!("nyx.api");
}

use proto::nyx_control_server::{NyxControl, NyxControlServer};
use proto::{Event, EventFilter, NodeInfo, OpenRequest, StreamId, StreamResponse};
use prost_types::Empty;

/// Basic control service implementation. Extend as subsystems mature.
struct ControlService {
    start_time: Instant,
}

impl ControlService {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }
}

#[async_trait::async_trait]
impl NyxControl for ControlService {
    async fn get_info(
        &self,
        _request: tonic::Request<Empty>,
    ) -> Result<tonic::Response<NodeInfo>, tonic::Status> {
        let info = NodeInfo {
            node_id: "local".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_sec: self.start_time.elapsed().as_secs() as u32,
            bytes_in: 0,
            bytes_out: 0,
        };
        Ok(tonic::Response::new(info))
    }

    async fn open_stream(
        &self,
        _request: tonic::Request<OpenRequest>,
    ) -> Result<tonic::Response<StreamResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented("open_stream is not yet implemented"))
    }

    async fn close_stream(
        &self,
        _request: tonic::Request<StreamId>,
    ) -> Result<tonic::Response<Empty>, tonic::Status> {
        Err(tonic::Status::unimplemented("close_stream is not yet implemented"))
    }

    type SubscribeEventsStream = ReceiverStream<Result<Event, tonic::Status>>;

    async fn subscribe_events(
        &self,
        _request: tonic::Request<EventFilter>,
    ) -> Result<tonic::Response<Self::SubscribeEventsStream>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "subscribe_events is not yet implemented",
        ))
    }
}

/// Packet handler that forwards inbound datagrams to the cMix controller input channel.
struct MixPacketHandler {
    tx: mpsc::Sender<Vec<u8>>, // raw packet bytes → cMix buffer
}

#[async_trait::async_trait]
impl PacketHandler for MixPacketHandler {
    async fn handle_packet(&self, _src: std::net::SocketAddr, data: &[u8]) {
        // Best-effort; drop if channel full.
        let _ = self.tx.send(data.to_vec()).await;
    }
}

#[cfg(unix)]
const DEFAULT_ENDPOINT: &str = "/tmp/nyx.sock";
#[cfg(windows)]
const DEFAULT_ENDPOINT: &str = "\\\\.\\pipe\\nyx-daemon.sock";

#[tokio::main(worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_panic_abort();

    // Load configuration (fallback to default path when env not set)
    let cfg_path = std::env::var("NYX_CONFIG").unwrap_or_else(|_| "nyx.toml".into());
    let cfg = NyxConfig::from_file(&cfg_path).unwrap_or_default();

    // Initialize tracing
    let level = cfg.log_level.clone().unwrap_or_else(|| "info".to_string());
    std::env::set_var("RUST_LOG", &level);
    tracing_subscriber::fmt::init();

    // Spawn cMix controller (batch_size=100, delay=100ms default).
    let mut cmix = CmixController::default();
    let cmix_tx = cmix.sender();

    // Transport with packet handler feeding cMix.
    let transport = Transport::start(cfg.listen_port, Arc::new(MixPacketHandler { tx: cmix_tx })).await?;

    // Task: forward emitted cMix batches to lower transport.
    let transport_clone = transport.clone();
    tokio::spawn(async move {
        while let Some(batch) = cmix.recv().await {
            for pkt in batch.packets {
                // TODO: Select next hop address via path builder (placeholder localhost)
                if let Ok(addr) = "127.0.0.1:43301".parse() {
                    transport_clone.send(addr, &pkt).await;
                }
            }
        }
    });

    // Prepare the control endpoint.
    #[cfg(unix)]
    let _ = std::fs::remove_file(DEFAULT_ENDPOINT);

    let listener = UnixListener::bind(DEFAULT_ENDPOINT)?;
    info!("control endpoint bound at {DEFAULT_ENDPOINT}");
    let incoming = UnixListenerStream::new(listener);

    let svc = ControlService::new();

    info!("Nyx daemon started – awaiting control connections");
    if let Err(e) = Server::builder()
        .add_service(NyxControlServer::new(svc))
        .serve_with_incoming(incoming)
        .await
    {
        error!("gRPC server terminated: {e}");
    }
    Ok(())
} 