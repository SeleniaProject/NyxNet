#![forbid(unsafe_code)]

//! Nyx Mix Routing layer (initial skeleton).
//!
//! PathBuilder selects a sequence of NodeId representing mix hops.

use nyx_core::NodeId;
use rand::{seq::SliceRandom, thread_rng};

/// PathBuilder provides weighted random path selection over candidate nodes.
pub struct PathBuilder<'a> {
    candidates: &'a [NodeId],
}

impl<'a> PathBuilder<'a> {
    pub fn new(candidates: &'a [NodeId]) -> Self {
        Self { candidates }
    }

    /// Build a path with the desired hop count.
    /// Currently uniform random; weights TBD once we have latency/bandwidth metrics.
    pub fn build_path(&self, hops: usize) -> Vec<NodeId> {
        let mut rng = thread_rng();
        let mut path = Vec::with_capacity(hops);
        for _ in 0..hops {
            if let Some(node) = self.candidates.choose(&mut rng) {
                path.push(*node);
            }
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_length_matches() {
        let candidates: Vec<NodeId> = (0..10).map(|i| [i as u8; 32]).collect();
        let builder = PathBuilder::new(&candidates);
        let path = builder.build_path(5);
        assert_eq!(path.len(), 5);
    }
}
