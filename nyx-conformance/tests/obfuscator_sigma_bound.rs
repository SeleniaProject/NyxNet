use nyx_fec::timing::{TimingObfuscator, TimingConfig};
use tokio::time::{timeout, Duration, Instant};

#[tokio::test]
async fn obfuscator_three_sigma_bound() {
    let cfg = TimingConfig { mean_ms: 20.0, sigma_ms: 5.0 };
    let mut obf = TimingObfuscator::new(cfg);
    obf.enqueue(vec![0]).await;
    let start = Instant::now();
    let _ = timeout(Duration::from_millis(100), obf.recv()).await.expect("timeout").unwrap();
    let elapsed = start.elapsed().as_millis() as u64;
    // Expect delay <= mean + 3*sigma = 35ms with some margin
    assert!(elapsed <= 45, "delay {}ms exceeds 3σ bound", elapsed);
} 