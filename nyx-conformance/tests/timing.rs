use nyx_fec::timing::{TimingObfuscator, TimingConfig};
use tokio::time::{Instant, Duration, timeout};

#[tokio::test]
async fn timing_obfuscator_delay() {
    let cfg = TimingConfig { mean_ms: 10.0, sigma_ms: 0.0 };
    let mut obf = TimingObfuscator::new(cfg);

    let start = Instant::now();
    obf.enqueue(vec![1, 2, 3]).await;
    let pkt = timeout(Duration::from_millis(50), obf.recv()).await.expect("timeout").expect("packet");
    let elapsed_ms = start.elapsed().as_millis() as u64;
    assert!(elapsed_ms >= 10, "delay too small: {} ms", elapsed_ms);
    assert_eq!(pkt.0, vec![1,2,3]);
} 