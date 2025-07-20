use nyx_transport::ice::{ice_lite_handshake};
use tokio::net::UdpSocket;

#[tokio::test]
async fn ice_lite_txid_mismatch() {
    let sock_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let sock_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr_b = sock_b.local_addr().unwrap();

    // Responder sends response with wrong txid
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        if let Ok((_len, src)) = sock_b.recv_from(&mut buf).await {
            let mut resp = [0u8; 20];
            resp[0..2].copy_from_slice(&0x0101u16.to_be_bytes());
            resp[2..4].copy_from_slice(&0u16.to_be_bytes());
            resp[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes());
            // fill txid with different bytes
            resp[8..20].copy_from_slice(&[0xAAu8; 12]);
            let _ = sock_b.send_to(&resp, src).await;
        }
    });

    let ok = ice_lite_handshake(&sock_a, addr_b).await;
    assert!(!ok, "Handshake should fail on txid mismatch");
} 