use nyx_stream::MultipathReceiver;

#[test]
fn multipath_three_paths_order() {
    let mut rx = MultipathReceiver::new();
    rx.push(1,0,vec![0]);
    rx.push(2,0,vec![1]);
    rx.push(3,0,vec![2]);
    let out = rx.push(1,1,vec![3]);
    assert!(out.is_empty());
} 