#![forbid(unsafe_code)]

//! Cross-platform IPC transport for Nyx plugins.
//!
//! • Linux / macOS: Unix Domain Socket path `$XDG_RUNTIME_DIR/nyx-ipc-{pid}-{id}.sock`
//! • Windows      : Named Pipe `\\.\pipe\nyx-ipc-{pid}-{id}`
//!
//! Frames are length-prefixed (u32 BE) for simplicity. All APIs are async and
//! integrate with Tokio.  This module is **internal** to the plugin runtime and
//! not exposed by the public crate root.

use bytes::{Buf, BufMut, BytesMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, error};

#[cfg(unix)]
use tokio::net::{UnixStream, UnixListener};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer};

const MAX_FRAME: usize = 16 * 1024; // plugins exchange small control messages

/// Outbound half used by core to talk to plugin.
#[derive(Clone)]
pub struct PluginIpcTx {
    tx: mpsc::Sender<Vec<u8>>,
}

impl PluginIpcTx {
    pub async fn send(&self, data: &[u8]) {
        let _ = self.tx.send(data.to_vec()).await;
    }
}

/// Inbound receiver half.
pub type PluginIpcRx = mpsc::Receiver<Vec<u8>>;

/// Spawn an IPC server endpoint and return (tx,rx) channel pair.
/// The caller should hold `tx` for sending to plugin; `rx` yields inbound frames.
#[cfg(unix)]
pub async fn spawn_ipc_server(id: &str) -> std::io::Result<(PluginIpcTx, PluginIpcRx)> {
    use std::path::PathBuf;
    let path = std::env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into());
    let mut sock_path = PathBuf::from(path);
    sock_path.push(format!("nyx-ipc-{}.sock", id));
    // Remove stale
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;
    let (stream, _) = listener.accept().await?;
    setup_stream(stream).await
}

#[cfg(windows)]
pub async fn spawn_ipc_server(id: &str) -> std::io::Result<(PluginIpcTx, PluginIpcRx)> {
    let pipe_name = format!("\\\\.\\pipe\\nyx-ipc-{}", id);
    let server = NamedPipeServer::create(&pipe_name)?;
    let stream = server.connect().await?;
    setup_stream(stream).await
}

/// Connect as client (plugin side)
#[cfg(unix)]
pub async fn connect_client(path: &str) -> std::io::Result<(PluginIpcTx, PluginIpcRx)> {
    let stream = UnixStream::connect(path).await?;
    setup_stream(stream).await
}

#[cfg(windows)]
pub async fn connect_client(pipe: &str) -> std::io::Result<(PluginIpcTx, PluginIpcRx)> {
    let stream = ClientOptions::new().open(pipe)?;
    setup_stream(stream).await
}

/// Internal helper converting a duplex stream into length-prefixed mpsc pair.
async fn setup_stream<T>(mut stream: T) -> std::io::Result<(PluginIpcTx, PluginIpcRx)>
where
    T: AsyncReadExt + AsyncWriteExt + Unpin + Send + 'static,
{
    let (tx_out, mut rx_out) = mpsc::channel::<Vec<u8>>(32);
    let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(32);

    // TX task
    tokio::spawn(async move {
        while let Some(buf) = rx_out.recv().await {
            if buf.len() > MAX_FRAME { continue; }
            let mut len_buf = [0u8; 4];
            len_buf.as_mut().put_u32(buf.len() as u32);
            if stream.write_all(&len_buf).await.is_err() { break; }
            if stream.write_all(&buf).await.is_err() { break; }
        }
    });

    // RX task
    tokio::spawn(async move {
        let mut len_buf = [0u8; 4];
        loop {
            if stream.read_exact(&mut len_buf).await.is_err() { break; }
            let len = u32::from_be_bytes(len_buf) as usize;
            if len == 0 || len > MAX_FRAME { break; }
            let mut data = vec![0u8; len];
            if stream.read_exact(&mut data).await.is_err() { break; }
            if tx_in.send(data).await.is_err() { break; }
        }
        debug!("plugin ipc closed");
    });

    Ok((PluginIpcTx { tx: tx_out }, rx_in))
} 