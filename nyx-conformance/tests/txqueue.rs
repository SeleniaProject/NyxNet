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

#[tokio::test]
async fn txqueue_independent_path_counters() {
    let cfg = TimingConfig::default();
    let q = TxQueue::new(cfg);
    let s0 = q.send_with_path(1, vec![0]).await;
    let s1 = q.send_with_path(2, vec![1]).await;
    let s2 = q.send_with_path(1, vec![2]).await;
    assert_eq!(s0, 0);
    assert_eq!(s1, 0); // path 2 starts at 0
    assert_eq!(s2, 1); // path1 increments independently
} 