use nyx_stream::StreamLayer;
use nyx_fec::timing::TimingConfig;
use tokio::time::{Duration, Instant};

#[tokio::test]
async fn obfuscator_delay_nonzero() {
    let cfg = TimingConfig { mean_ms: 5.0, sigma_ms: 0.0 }; // deterministic ~5ms
    let mut layer = StreamLayer::new(cfg);

    let start = Instant::now();
    layer.send(vec![1, 2, 3, 4]).await;
    if let Some(pkt) = layer.recv().await {
        let elapsed = start.elapsed();
        // Ensure we got same data and at least ~4ms delay (tolerance)
        assert_eq!(pkt, vec![1,2,3,4]);
        assert!(elapsed >= Duration::from_millis(4));
    } else {
        panic!("no packet received");
    }
} 