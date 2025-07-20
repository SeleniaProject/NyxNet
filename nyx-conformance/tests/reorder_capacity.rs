use nyx_stream::ReorderBuffer;

#[test]
fn reorder_buffer_capacity_overflow() {
    let mut buf = ReorderBuffer::new(3);
    // push 0..5 packets out of order
    for seq in 0..6 { buf.push(seq, seq as u8); }
    // buffer capacity 3, first packets should be released automatically
    let mut collected = Vec::new();
    while let Some(v) = buf.pop_front() { collected.push(v); }
    // we should have at most last 3 packets remaining (3,4,5)
    assert_eq!(collected, vec![0u8,1u8,2u8,3u8,4u8,5u8]);
} 