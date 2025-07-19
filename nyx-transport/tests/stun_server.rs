use nyx_transport::{UdpPool, ice::get_srflx_addr, stun_server::start_stun_server};
use std::net::SocketAddr;

#[tokio::test]
async fn srflx_with_local_stun_server() {
    // Start local STUN server on arbitrary port.
    let port = 3479;
    let handle = start_stun_server(port).await.unwrap();
    let pool = UdpPool::bind(0).await.unwrap();
    let stun_srv: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let addr = get_srflx_addr(&pool, stun_srv).await.unwrap();
    // Should reflect to our socket's local address.
    assert_eq!(addr.port(), pool.socket().local_addr().unwrap().port());
    let _ = handle.abort();
} 