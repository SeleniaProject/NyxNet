use nyx_fec::timing::{TimingObfuscator, TimingConfig};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn obfuscator_sigma_stats() {
    let cfg = TimingConfig { mean_ms: 50.0, sigma_ms: 25.0 };
    let mut obf = TimingObfuscator::new(cfg);
    obf.enqueue(vec![0]).await;
    let pkt = timeout(Duration::from_millis(200), obf.recv()).await.expect("timeout").unwrap();
    drop(pkt);
} 