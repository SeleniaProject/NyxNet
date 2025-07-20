use nyx_stream::{MultipathReceiver, Sequencer};

#[test]
fn multipath_receiver_in_order_per_path() {
    let mut rx = MultipathReceiver::new();
    // Path 1: receive packets 0,2,1 out of order, expect 0 then 1,2 together
    let first = rx.push(1, 0, vec![0]);
    assert_eq!(first, vec![vec![0]]);
    assert!(rx.push(1, 2, vec![2]).is_empty());
    let ready = rx.push(1, 1, vec![1]);
    assert_eq!(ready, vec![vec![1], vec![2]]);
}

#[test]
fn sequencer_per_path_independent() {
    let mut seq = Sequencer::new();
    assert_eq!(seq.next(10), 0);
    assert_eq!(seq.next(10), 1);
    assert_eq!(seq.next(20), 0);
    assert_eq!(seq.next(10), 2);
} 