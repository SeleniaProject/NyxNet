use nyx_stream::StreamLayer;
use nyx_fec::timing::TimingConfig;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn streamlayer_send_recv_roundtrip() {
    // Use small delay to keep test fast.
    let cfg = TimingConfig { mean_ms: 1.0, sigma_ms: 0.0 };
    let mut layer = StreamLayer::new(cfg);
    let payload = vec![0xAA, 0xBB, 0xCC];
    layer.send(payload.clone()).await;

    // Expect to receive same bytes within 100ms after obfuscation delay.
    let received = timeout(Duration::from_millis(100), layer.recv())
        .await
        .expect("timeout")
        .expect("no data");
    assert_eq!(received, payload);
}

#[test]
fn streamlayer_inorder_across_paths() {
    let cfg = TimingConfig { mean_ms: 0.0, sigma_ms: 0.0 };
    let mut layer = StreamLayer::new(cfg);
    // Push seq 1 then 0 on path 2, expect only seq 1 delivered initially.
    let r1 = layer.handle_incoming(2, 1, vec![1]);
    assert!(r1.is_empty());
    let r2 = layer.handle_incoming(2, 0, vec![0]);
    assert_eq!(r2, vec![vec![0], vec![1]]);
} 