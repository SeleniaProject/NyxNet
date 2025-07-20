use nyx_stream::TxQueue;
use nyx_fec::TimingConfig;

#[tokio::test]
async fn txqueue_backpressure_stress() {
    let cfg = TimingConfig::default();
    let q = TxQueue::new(cfg);
    // send 1000 packets quickly
    for i in 0..1000u64 {
        q.send_with_path(0, vec![0u8; 10]).await;
    }
    // If no panic, test passes. Optionally wait small time.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
} 