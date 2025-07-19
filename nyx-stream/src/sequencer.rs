//! Per-Path sequence number generator.

#![forbid(unsafe_code)]

use std::collections::HashMap;

#[derive(Default)]
pub struct Sequencer {
    counters: HashMap<u8, u64>,
}

impl Sequencer {
    pub fn new() -> Self { Self { counters: HashMap::new() } }

    /// Obtain next sequence number for `path_id`.
    pub fn next(&mut self, path_id: u8) -> u64 {
        let counter = self.counters.entry(path_id).or_insert(0);
        let seq = *counter;
        *counter = counter.wrapping_add(1);
        seq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_increments_per_path() {
        let mut seq = Sequencer::new();
        assert_eq!(seq.next(1), 0);
        assert_eq!(seq.next(1), 1);
        assert_eq!(seq.next(2), 0);
        assert_eq!(seq.next(1), 2);
    }
} 