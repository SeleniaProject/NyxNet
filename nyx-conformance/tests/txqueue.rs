use nyx_stream::TxQueue;
use nyx_fec::TimingConfig;

#[tokio::test]
async fn txqueue_path_seq_numbers() {
    let cfg = TimingConfig::default();
    let q = TxQueue::new(cfg);
    let seq0 = q.send_with_path(3, vec![1,2,3]).await;
    let seq1 = q.send_with_path(3, vec![4,5]).await;
    assert_eq!(seq0, 0);
    assert_eq!(seq1, 1);
} 