use nyx_transport::{Transport, PacketHandler, hole_punch};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use async_trait::async_trait;
use std::net::SocketAddr;

struct CaptureOnce {
    tx: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait]
impl PacketHandler for CaptureOnce {
    async fn handle_packet(&self, _src: SocketAddr, _data: &[u8]) {
        if let Some(tx) = self.tx.lock().await.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn hole_punch_reaches_peer() {
    let (tx1, rx1) = oneshot::channel();
    let h1 = Arc::new(CaptureOnce { tx: Mutex::new(Some(tx1)) });
    let t1 = Transport::start(0, h1).await.unwrap();
    let addr1 = t1.local_addr().unwrap();

    let (tx2, _rx2) = oneshot::channel();
    let h2 = Arc::new(CaptureOnce { tx: Mutex::new(Some(tx2)) });
    let t2 = Transport::start(0, h2).await.unwrap();

    // Peer performs hole punch toward t1
    hole_punch(&t2, addr1).await;

    let received = tokio::time::timeout(std::time::Duration::from_millis(100), rx1).await;
    assert!(received.is_ok(), "Peer should receive hole punch packet");
} 