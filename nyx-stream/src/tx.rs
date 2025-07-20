#![forbid(unsafe_code)]

use tokio::sync::mpsc;
use nyx_fec::{TimingObfuscator, TimingConfig, Packet};
use super::Sequencer;
use tracing::instrument;

/// TxQueue integrates TimingObfuscator and provides outgoing packet stream.
pub struct TxQueue {
    in_tx: mpsc::Sender<Packet>,
    out_rx: mpsc::Receiver<Vec<u8>>, // obfuscated frames for transport
    sequencer: tokio::sync::Mutex<Sequencer>,
}

impl TxQueue {
    pub fn new(cfg: TimingConfig) -> Self {
        let obf = TimingObfuscator::new(cfg);

        let in_tx = obf.sender();
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(1024);

        // Task: forward from obf.recv -> out_tx
        tokio::spawn(async move {
            let mut recv_obf = obf;
            while let Some(pkt) = recv_obf.recv().await {
                let Packet(bytes) = pkt;
                if out_tx.send(bytes).await.is_err() {
                    break;
                }
            }
        });

        Self { in_tx, out_rx, sequencer: tokio::sync::Mutex::new(Sequencer::new()) }
    }

    /// Send frame without specific PathID.  Emits OTLP span `nyx.stream.send`.
    #[instrument(name = "nyx.stream.send", skip_all, fields(path_id = -1i8, cid = "unknown"))]
    pub async fn send(&self, bytes: Vec<u8>) {
        let _ = self.in_tx.send(Packet(bytes)).await;
    }

    /// Send bytes tagged with PathID, returning assigned sequence number.
    /// Emits OTLP span `nyx.stream.send` with `path_id` attribute.
    #[instrument(name = "nyx.stream.send", skip_all, fields(path_id = path_id, cid = "unknown"))]
    pub async fn send_with_path(&self, path_id: u8, bytes: Vec<u8>) -> u64 {
        let mut seq = self.sequencer.lock().await;
        let s = seq.next(path_id);
        // prepend seq (8 bytes LE) for now; protocol integration later.
        let mut buf = Vec::with_capacity(8 + bytes.len());
        buf.extend_from_slice(&s.to_le_bytes());
        buf.extend_from_slice(&bytes);
        let _ = self.in_tx.send(Packet(buf)).await;
        s
    }

    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.out_rx.recv().await
    }

    /// Provide a sender clone for external producers.
    pub fn clone_sender(&self) -> mpsc::Sender<Packet> {
        self.in_tx.clone()
    }
} 