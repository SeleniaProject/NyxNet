use nyx_transport::ice::ice_lite_handshake;
use tokio::net::UdpSocket;

#[tokio::test]
async fn ice_lite_handshake_success() {
    let sock_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let sock_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr_b = sock_b.local_addr().unwrap();

    // Responder task: echo Binding Response with same transaction ID
    tokio::spawn(async move {
        let mut buf = [0u8; 1500];
        if let Ok((len, src)) = sock_b.recv_from(&mut buf).await {
            // Construct minimal Binding Response
            let mut resp = [0u8; 20];
            resp[0..2].copy_from_slice(&0x0101u16.to_be_bytes()); // Binding Response
            resp[2..4].copy_from_slice(&0u16.to_be_bytes()); // length 0
            resp[4..8].copy_from_slice(&0x2112A442u32.to_be_bytes()); // cookie
            resp[8..20].copy_from_slice(&buf[8..20]); // txid
            let _ = sock_b.send_to(&resp, src).await;
        }
    });

    let ok = ice_lite_handshake(&sock_a, addr_b).await;
    assert!(ok, "ICE handshake should succeed with valid response");
} 