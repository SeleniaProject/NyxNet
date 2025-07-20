use nyx_transport::{Transport, PacketHandler};
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use async_trait::async_trait;

struct CaptureHandler {
    sender: Mutex<Option<oneshot::Sender<Vec<u8>>>>,
}

#[async_trait]
impl PacketHandler for CaptureHandler {
    async fn handle_packet(&self, _src: SocketAddr, data: &[u8]) {
        if let Some(tx) = self.sender.lock().await.take() {
            let _ = tx.send(data.to_vec());
        }
    }
}

#[tokio::test]
async fn transport_send_receive_local() {
    // Setup two transports on random ports
    let (tx1, rx1) = oneshot::channel();
    let h1 = Arc::new(CaptureHandler { sender: Mutex::new(Some(tx1)) });
    let t1 = Transport::start(0, h1.clone()).await.unwrap();

    let (tx2, rx2) = oneshot::channel();
    let h2 = Arc::new(CaptureHandler { sender: Mutex::new(Some(tx2)) });
    let t2 = Transport::start(0, h2.clone()).await.unwrap();

    let addr2 = t2.local_addr().unwrap();
    // Send payload from t1 to t2
    let payload = b"ping".to_vec();
    t1.send(addr2, &payload).await;

    // Wait for reception
    let received = tokio::time::timeout(std::time::Duration::from_millis(100), rx2).await.expect("timeout").unwrap();
    assert_eq!(received, payload);
} 