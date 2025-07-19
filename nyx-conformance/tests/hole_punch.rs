use nyx_transport::{Transport, hole_punch, PacketHandler};
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::Notify;

struct NullHandler;
#[async_trait::async_trait]
impl PacketHandler for NullHandler {
    async fn handle_packet(&self, _src: SocketAddr, _data: &[u8]) {}
}

#[tokio::test]
async fn hole_punch_no_response() {
    let notify = Arc::new(Notify::new());
    // Bind local transport on random port
    let handler = Arc::new(NullHandler);
    let transport = Transport::start(0, handler).await.unwrap();
    // Non-routable TEST-NET-1 address to ensure no response
    let peer: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 40000);

    // Send hole punch (should not panic)
    hole_punch(&transport, peer).await;
    // Wait briefly to ensure task completes without error
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    notify.notify_waiters();
} 