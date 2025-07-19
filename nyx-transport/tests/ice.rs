use nyx_transport::{UdpPool, ice::get_srflx_addr};
use std::net::SocketAddr;

#[tokio::test]
async fn dummy_stun_timeout() {
    // Use non-routable address to trigger timeout path; expect error.
    let pool = UdpPool::bind(0).await.unwrap();
    let stun_srv: SocketAddr = "203.0.113.1:3478".parse().unwrap();
    let res = get_srflx_addr(&pool, stun_srv).await;
    assert!(res.is_err());
} 