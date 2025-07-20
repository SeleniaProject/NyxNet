use nyx_fec::timing::{TimingObfuscator, TimingConfig};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn timing_obfuscator_nonnegative_delay() {
    let cfg = TimingConfig { mean_ms: 0.0, sigma_ms: 30.0 }; // large sigma may produce negatives if bug
    let mut obf = TimingObfuscator::new(cfg);
    obf.enqueue(vec![1]).await;
    let pkt = timeout(Duration::from_millis(100), obf.recv()).await.expect("timeout").expect("pkt");
    assert_eq!(pkt.0, vec![1]);
} 