use nyx_fec::timing::{TimingObfuscator, TimingConfig, Packet};
use tokio::time::{timeout, Duration, Instant};

#[tokio::test]
async fn obfuscator_min_delay_respected() {
    let cfg = TimingConfig { mean_ms: 5.0, sigma_ms: 0.0 };
    let mut obf = TimingObfuscator::new(cfg);
    let start = Instant::now();
    obf.enqueue(vec![1,2,3]).await;
    let pkt = timeout(Duration::from_millis(50), obf.recv()).await.expect("timeout").expect("packet");
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(5));
    assert_eq!(pkt.0, vec![1,2,3]);
} 