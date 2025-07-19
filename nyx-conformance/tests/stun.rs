use nyx_transport::stun_server::start_stun_server;
use nyx_transport::ice::decode_binding_response;
use nyx_transport::UdpPool;
use std::net::SocketAddr;

#[tokio::test]
async fn stun_binding_response_reflection() {
    let port = 3480;
    let handle = start_stun_server(port).await.unwrap();

    let pool = UdpPool::bind(0).await.unwrap();
    let stun_srv: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();

    // Build minimal binding request (20 bytes)
    let mut req = [0u8; 20];
    req[0..2].copy_from_slice(&0x0001u16.to_be_bytes()); // Binding request
    req[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes()); // Magic cookie

    let sock = pool.socket();
    sock.send_to(&req, stun_srv).await.unwrap();

    let mut buf = [0u8; 1500];
    let len = sock.recv(&mut buf).await.unwrap();
    let resp = &buf[..len];
    let addr = decode_binding_response(resp).expect("decode");
    assert_eq!(addr.port(), sock.local_addr().unwrap().port());

    handle.abort();
} 