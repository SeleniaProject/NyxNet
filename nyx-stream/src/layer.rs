#![forbid(unsafe_code)]

//! High-level stream layer façade wiring together congestion control and timing obfuscation.
//! At this stage it provides a simple byte-oriented send/recv API and can be expanded later.

use crate::{tx::TxQueue, receiver::MultipathReceiver};
use nyx_fec::timing::{TimingConfig, Packet};
use tokio::sync::mpsc;

/// StreamLayer bundles a [`TxQueue`] (with timing obfuscation) and exposes
/// an async channel‐like API to the upper layers.
pub struct StreamLayer {
    sender: mpsc::Sender<Vec<u8>>,       // in-memory TX path (will feed FEC/transport)
    tx_queue: TxQueue,                   // handles timing obfuscation
    receiver: MultipathReceiver,
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
        Self { sender: in_tx, tx_queue, receiver: MultipathReceiver::new() }
    }

    /// Enqueue raw frame bytes for timed transmission.
    pub async fn send(&self, data: Vec<u8>) {
        let _ = self.sender.send(data).await;
    }

    /// Receive next obfuscated packet.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.tx_queue.recv().await
    }

    /// Handle an incoming frame `data` associated with `path_id` and `seq`.
    /// Returns any frames that are now in-order.
    pub fn handle_incoming(&mut self, path_id: u8, seq: u64, data: Vec<u8>) -> Vec<Vec<u8>> {
        self.receiver.push(path_id, seq, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn inorder_delivery_across_paths() {
        let mut layer = StreamLayer::new(TimingConfig::default());
        // push out-of-order on path 1
        let v1 = layer.handle_incoming(1, 1, vec![1]);
        assert_eq!(v1, vec![vec![1]]);
        let v2 = layer.handle_incoming(1, 0, vec![0]);
        assert!(v2.is_empty());
    }
} 