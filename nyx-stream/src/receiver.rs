//! Multipath receiver handling in-order delivery per PathID.
//! Uses `ReorderBuffer` to reassemble packet order for each path.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use super::ReorderBuffer;

/// MultipathReceiver buffers packets per PathID and returns in-order frames.
pub struct MultipathReceiver {
    buffers: HashMap<u8, ReorderBuffer<Vec<u8>>>,
}

impl MultipathReceiver {
    /// Create empty receiver.
    pub fn new() -> Self { Self { buffers: HashMap::new() } }

    /// Push a received packet.
    /// Returns a vector of in-order packets now ready for consumption.
    pub fn push(&mut self, path_id: u8, seq: u64, payload: Vec<u8>) -> Vec<Vec<u8>> {
        let buf = self.buffers.entry(path_id).or_insert_with(|| ReorderBuffer::new(seq));
        buf.push(seq, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_across_paths() {
        let mut rx = MultipathReceiver::new();
        // path 1 receives seq 1 then 0
        let first = rx.push(1, 1, vec![1]);
        assert_eq!(first, vec![vec![1]]);
        assert!(rx.push(1, 0, vec![0]).is_empty());
        // path 2 independent ordering
        let ready2_first = rx.push(2, 5, vec![5]);
        assert_eq!(ready2_first, vec![vec![5]]);
        let ready2 = rx.push(2, 6, vec![6]);
        assert_eq!(ready2, vec![vec![6]]);
    }
} 