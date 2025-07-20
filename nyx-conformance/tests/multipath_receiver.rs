use nyx_stream::MultipathReceiver;

#[test]
fn multipath_cross_path_ordering() {
    let mut rx = MultipathReceiver::new();
    // Path 1 seq 0,1
    assert_eq!(rx.push(1, 1, vec![b'a']), Vec::<Vec<u8>>::new());
    // Path 2 seq 0 arrives, should deliver immediately because independent path.
    let out = rx.push(2, 0, vec![b'X']);
    assert_eq!(out, vec![vec![b'X']]);
    // Path1 seq0 arrives, now path1 releases 0 then 1.
    let out2 = rx.push(1, 0, vec![b'b']);
    assert_eq!(out2, vec![vec![b'b'], vec![b'a']]);
} 