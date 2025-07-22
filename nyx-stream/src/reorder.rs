//! Packet Reordering Buffer.
//! Maintains in-order delivery for Multipath reception.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

/// ReorderBuffer collects packets keyed by monotonically increasing sequence numbers
/// and releases them in-order.
///
/// • Auto‐scale up: when the observed gap (highest – expected) nears the current
///   window size the buffer doubles up to 8192 slots. これは経路 RTT 差で発生する
///   リオーダ幅に追従する。
/// • Auto‐shrink: 当面のリオーダが減ったら 1/4 未満で半減。
pub struct ReorderBuffer<T> {
    next_seq: u64,
    window: BTreeMap<u64, T>,
    max_window: usize,
}

impl<T> ReorderBuffer<T> {
    /// Create a new buffer starting at `initial_seq`.
    pub fn new(initial_seq: u64) -> Self {
        Self { next_seq: initial_seq, window: BTreeMap::new(), max_window: 128 }
    }

    /// Push packet with `seq`. Returns a vector of in-order packets now ready.
    pub fn push(&mut self, seq: u64, pkt: T) -> Vec<T> {
        if seq < self.next_seq {
            // Duplicate / too-old packet.
            return Vec::new();
        }
        self.window.insert(seq, pkt);

        // Observe current gap between next expected and highest received.
        if let Some((&high, _)) = self.window.iter().rev().next() {
            let gap = (high - self.next_seq + 1) as usize;
            // Up-scale: if gap approaches 80% of current window, double with cap 8192.
            if gap * 5 / 4 > self.max_window && self.max_window < 8192 {
                self.max_window = (self.max_window * 2).min(8192);
            }
        }
        self.drain_ready()
    }

    fn drain_ready(&mut self) -> Vec<T> {
        let mut ready = Vec::new();
        while let Some(pkt) = self.window.remove(&self.next_seq) {
            ready.push(pkt);
            self.next_seq += 1;
        }
        // adaptive window: if buffer consistently small, shrink max_window
        if self.window.len() < self.max_window / 4 && self.max_window > 32 {
            self.max_window /= 2;
        }
        // evict old if exceed window size
        while self.window.len() > self.max_window {
            // drop the highest-seq (assume lost earlier seq)
            if let Some((&last, _)) = self.window.iter().rev().next() {
                self.window.remove(&last);
            } else { break; }
        }
        ready
    }

    /// Pop a single in-order packet if available.
    pub fn pop_front(&mut self) -> Option<T> {
        if let Some(pkt) = self.window.remove(&self.next_seq) {
            self.next_seq += 1;
            Some(pkt)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_order_delivery() {
        let mut buf = ReorderBuffer::new(0);
        // Push out-of-order: 1,0,2
        assert!(buf.push(1, 1).is_empty());
        let r1 = buf.push(0, 0);
        assert_eq!(r1, vec![0,1]);
        let r2 = buf.push(2, 2);
        assert_eq!(r2, vec![2]);
    }
} 