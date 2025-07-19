#![forbid(unsafe_code)]

//! High-level stream layer façade wiring together congestion control and timing obfuscation.
//! At this stage it provides a simple byte-oriented send/recv API and can be expanded later.

use crate::{tx::TxQueue};
use nyx_fec::timing::{TimingConfig, Packet};
use tokio::sync::mpsc;

/// StreamLayer bundles a [`TxQueue`] (with timing obfuscation) and exposes
/// an async channel‐like API to the upper layers.
pub struct StreamLayer {
    sender: mpsc::Sender<Vec<u8>>,       // in-memory TX path (will feed FEC/transport)
    tx_queue: TxQueue,                   // handles timing obfuscation
}

impl StreamLayer {
    /// Create a new StreamLayer with the provided timing configuration.
    pub fn new(timing: TimingConfig) -> Self {
        let tx_queue = TxQueue::new(timing);
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>(1024);

        // Forward from in_tx => tx_queue for obfuscation+send path.
        let txq_sender = tx_queue.clone_sender();
        tokio::spawn(async move {
            let mut rx = in_rx;
            while let Some(bytes) = rx.recv().await {
                txq_sender.send(Packet(bytes)).await.ok();
            }
        });

        // For tests we expose the obfuscated out_rx as StreamLayer receiver.
        Self { sender: in_tx, tx_queue }
    }

    /// Enqueue raw frame bytes for timed transmission.
    pub async fn send(&self, data: Vec<u8>) {
        let _ = self.sender.send(data).await;
    }

    /// Receive next obfuscated packet.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.tx_queue.recv().await
    }
} 