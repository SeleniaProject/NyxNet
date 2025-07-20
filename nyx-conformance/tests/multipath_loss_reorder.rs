
use nyx_stream::MultipathReceiver;

/// Verify that the receiver does not release packets when a gap (loss) exists
/// and that once the missing packet arrives all buffered packets are flushed
/// in correct order.
#[test]
fn multipath_loss_waits_until_gap_filled() {
    let mut rx = MultipathReceiver::new();

    // First packet on path 1 (seq = 0) should be delivered immediately.
    let out0 = rx.push(1, 0, vec![0]);
    assert_eq!(out0, vec![vec![0]]);

    // seq = 1 is lost for now. Pushing higher sequence numbers must not yield any delivery.
    assert!(rx.push(1, 2, vec![2]).is_empty());
    assert!(rx.push(1, 3, vec![3]).is_empty());

    // The gap is filled by late arrival of seq = 1. All pending packets should now flush.
    let out_final = rx.push(1, 1, vec![1]);
    assert_eq!(out_final, vec![vec![1], vec![2], vec![3]]);
}

/// Verify reordering across multiple paths does not interfere and each path
/// maintains independent ordering guarantees even under out-of-order delivery.
#[test]
fn multipath_independent_reorder() {
    let mut rx = MultipathReceiver::new();

    // Path 10 receives 1 before 0 – should not deliver until 0 arrives.
    assert!(rx.push(10, 1, vec![b'B']).is_empty());

    // Path 11 receives packets in order – should deliver directly.
    let out_p11_first = rx.push(11, 0, vec![b'X']);
    assert_eq!(out_p11_first, vec![vec![b'X']]);
    let out_p11_second = rx.push(11, 1, vec![b'Y']);
    assert_eq!(out_p11_second, vec![vec![b'Y']]);

    // Now path 10 gets its missing packet and both should flush in order.
    let out_p10 = rx.push(10, 0, vec![b'A']);
    assert_eq!(out_p10, vec![vec![b'A'], vec![b'B']]);
} 