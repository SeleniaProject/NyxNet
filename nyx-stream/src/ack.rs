#![forbid(unsafe_code)]

//! ACK frame (Type = 0x02) builder / parser and helper generator.
//!
//! Layout (big-endian):
//! ```text
//! 0               1               2               3
//! +---------------+---------------+---------------+---------------+
//! |           Largest Acked (32)                             |
//! +---------------+---------------+---------------+---------------+
//! |           ACK Delay µs (32)                              |
//! +---------------+---------------+---------------+---------------+
//! ```
//! In future spec versions this will expand with ranges; for now a single
//! largest-only encoding keeps parsing simple while enabling RTT sampling.

use nom::{number::complete::be_u32, IResult};
use std::time::Duration;
use tokio::sync::mpsc;

/// Parsed ACK frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckFrame {
    pub largest_ack: u32,
    pub ack_delay_micros: u32,
}

/// Build a raw byte vector representing an ACK frame.
pub fn build_ack_frame(ack: &AckFrame) -> Vec<u8> {
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&ack.largest_ack.to_be_bytes());
    v.extend_from_slice(&ack.ack_delay_micros.to_be_bytes());
    v
}

/// Parse an ACK frame.
pub fn parse_ack_frame(input: &[u8]) -> IResult<&[u8], AckFrame> {
    let (input, largest) = be_u32(input)?;
    let (input, delay) = be_u32(input)?;
    Ok((input, AckFrame { largest_ack: largest, ack_delay_micros: delay }))
}

/// ACK generator which coalesces ACKs over a short delay (default 25 ms) to
/// minimise feedback overhead. When fired, it sends a built ACK frame on the
/// provided channel.
///
/// The generator is extremely lightweight (~2 pointers) so cloning is cheap.
#[derive(Clone)]
pub struct AckGenerator {
    tx: mpsc::Sender<Vec<u8>>, // raw frame bytes ready for encryption / send
    delay: Duration,
}

impl AckGenerator {
    /// Create a new generator with the given outbound channel and delay.
    pub fn new(tx: mpsc::Sender<Vec<u8>>, delay: Duration) -> Self {
        Self { tx, delay }
    }

    /// Schedule an ACK covering `largest`. Subsequent calls within the delay
    /// window only extend the covered range (largest of the set).
    pub fn ack(&self, largest: u32) {
        let tx = self.tx.clone();
        let delay = self.delay;
        tokio::spawn(async move {
            // Use static once-cell to hold pending largest per task.
            use once_cell::sync::OnceCell;
            use std::sync::atomic::{AtomicU32, Ordering};
            static PENDING: OnceCell<AtomicU32> = OnceCell::new();
            let cell = PENDING.get_or_init(|| AtomicU32::new(0));
            // update largest seen
            cell.fetch_max(largest, Ordering::SeqCst);

            // wait delay then flush if we were the first.
            // naive approach: always wait; contention fine for small N.
            tokio::time::sleep(delay).await;
            let val = cell.swap(0, Ordering::SeqCst);
            if val != 0 {
                let frame = AckFrame { largest_ack: val, ack_delay_micros: delay.as_micros() as u32 };
                let _ = tx.send(build_ack_frame(&frame)).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_roundtrip() {
        let fr = AckFrame { largest_ack: 123456, ack_delay_micros: 55_000 };
        let bytes = build_ack_frame(&fr);
        let (_, parsed) = parse_ack_frame(&bytes).unwrap();
        assert_eq!(fr, parsed);
    }
} 