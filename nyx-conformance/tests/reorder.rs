use nyx_stream::ReorderBuffer;

#[test]
fn reorder_buffer_handles_gaps() {
    let mut buf = ReorderBuffer::new(10);
    // Push seq 12, expected no delivery yet.
    assert!(buf.push(12, 12u8).is_empty());
    // Push seq 10, should release 10.
    let r1 = buf.push(10, 10u8);
    assert_eq!(r1, vec![10u8]);
    // Push seq 11, should now release 11 and 12.
    let r2 = buf.push(11, 11u8);
    assert_eq!(r2, vec![11u8, 12u8]);
}

#[test]
fn reorder_buffer_drops_duplicates() {
    let mut buf = ReorderBuffer::new(0);
    let first = buf.push(0, 0u8);
    assert_eq!(first, vec![0u8]);
    // Duplicate packet should be ignored
    let r1 = buf.push(0, 0u8);
    assert!(r1.is_empty());
} 