#![forbid(unsafe_code)]

use tokio::sync::mpsc;
use nyx_fec::{TimingObfuscator, TimingConfig, Packet};

/// TxQueue integrates TimingObfuscator and provides outgoing packet stream.
pub struct TxQueue {
    in_tx: mpsc::Sender<Packet>,
    out_rx: mpsc::Receiver<Vec<u8>>, // obfuscated frames for transport
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

        Self { in_tx, out_rx }
    }

    pub async fn send(&self, bytes: Vec<u8>) {
        let _ = self.in_tx.send(Packet(bytes)).await;
    }

    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.out_rx.recv().await
    }

    /// Provide a sender clone for external producers.
    pub fn clone_sender(&self) -> mpsc::Sender<Packet> {
        self.in_tx.clone()
    }
} 