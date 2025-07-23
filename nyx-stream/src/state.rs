#![forbid(unsafe_code)]

//! Stream state machine handling send/recv lifecycle and retransmission bookkeeping.
//! The implementation follows the design §4.3 with states: Idle → Open → HalfClosed → Closed.
//! For brevity we model HalfClosed in two variants depending on which side closed.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio::sync::mpsc;

/// Stream logical state.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum StreamState {
    Idle,
    Open,
    // Local ended writing, still expecting remote data.
    HalfClosedLocal,
    // Remote closed writing, local can still send.
    HalfClosedRemote,
    Closed,
}

/// Sent but unacked segment.
#[derive(Debug, Clone)]
struct SentSegment {
    offset: u32,
    len: usize,
    timestamp: Instant,
}

/// Reassembly buffer entry.
#[derive(Debug)]
struct RecvSegment {
    offset: u32,
    data: Vec<u8>,
}

/// Core stream structure bound to a single Connection ID (CID).
pub struct Stream {
    pub id: u32,
    state: StreamState,
    send_offset: u32,
    recv_offset: u32,
    sent: BTreeMap<u32, SentSegment>, // keyed by offset
    recv_buffer: Vec<RecvSegment>,
    ack_tx: mpsc::Sender<u32>, // offset to acknowledge (largest)
    rto: Duration,
}

impl Stream {
    /// Create new stream in Idle state.
    pub fn new(id: u32, ack_tx: mpsc::Sender<u32>) -> Self {
        Self {
            id,
            state: StreamState::Idle,
            send_offset: 0,
            recv_offset: 0,
            sent: BTreeMap::new(),
            recv_buffer: Vec::new(),
            ack_tx,
            rto: Duration::from_millis(250),
        }
    }

    /// Transition Idle → Open on first send.
    pub fn send_data(&mut self, data: &[u8]) -> Vec<u8> {
        assert!(matches!(self.state, StreamState::Idle | StreamState::Open | StreamState::HalfClosedRemote));
        if self.state == StreamState::Idle {
            self.state = StreamState::Open;
        }
        let frame = crate::stream_frame::StreamFrame {
            stream_id: self.id,
            offset: self.send_offset,
            fin: false,
            data,
        };
        let bytes = crate::stream_frame::build_stream_frame(&frame);
        self.sent.insert(self.send_offset, SentSegment { offset: self.send_offset, len: data.len(), timestamp: Instant::now() });
        self.send_offset += data.len() as u32;
        bytes
    }

    /// Mark local side finished sending.
    pub fn finish(&mut self) -> Option<Vec<u8>> {
        if matches!(self.state, StreamState::Open | StreamState::HalfClosedRemote) {
            let frame = crate::stream_frame::StreamFrame {
                stream_id: self.id,
                offset: self.send_offset,
                fin: true,
                data: &[],
            };
            self.state = match self.state {
                StreamState::Open => StreamState::HalfClosedLocal,
                StreamState::HalfClosedRemote => StreamState::Closed,
                s => s,
            };
            Some(crate::stream_frame::build_stream_frame(&frame))
        } else { None }
    }

    /// Handle stream data from peer.
    pub fn on_receive(&mut self, frame: crate::stream_frame::StreamFrame<'_>) {
        // push to buffer; in-order delivery simplified.
        if frame.offset == self.recv_offset {
            self.recv_offset += frame.data.len() as u32;
        }
        // schedule ACK for largest offset seen
        let _ = self.ack_tx.try_send(self.recv_offset);

        if frame.fin {
            self.state = match self.state {
                StreamState::Open => StreamState::HalfClosedRemote,
                StreamState::HalfClosedLocal => StreamState::Closed,
                s => s,
            };
        }
    }

    /// Handle ACK frame acknowledging up to `largest`.
    pub fn on_ack(&mut self, largest: u32) {
        let mut to_remove = vec![];
        for (&off, _) in self.sent.iter() {
            if off + self.sent[&off].len as u32 <= largest {
                to_remove.push(off);
            }
        }
        for off in to_remove { self.sent.remove(&off); }
    }

    /// Periodic timer: detect losses (RTO) and request retransmission.
    pub async fn loss_retransmit_loop(mut self, tx: mpsc::Sender<Vec<u8>>) {
        loop {
            sleep(self.rto).await;
            let now = Instant::now();
            let mut lost: Vec<SentSegment> = Vec::new();
            for seg in self.sent.values() {
                if now.duration_since(seg.timestamp) >= self.rto {
                    lost.push(seg.clone());
                }
            }
            for seg in lost {
                if let Some(orig) = self.sent.get_mut(&seg.offset) {
                    orig.timestamp = Instant::now();
                }
                // Build retransmission frame.
                let fake_data = vec![0u8; seg.len]; // placeholder – application should cache.
                let frame = crate::stream_frame::StreamFrame {
                    stream_id: self.id,
                    offset: seg.offset,
                    fin: false,
                    data: &fake_data,
                };
                let _ = tx.send(crate::stream_frame::build_stream_frame(&frame)).await;
            }
        }
    }

    pub fn state(&self) -> StreamState { self.state }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions() {
        let (ack_tx, _rx) = mpsc::channel(1);
        let mut s = Stream::new(1, ack_tx);
        assert_eq!(s.state(), StreamState::Idle);
        let _ = s.send_data(&[1,2]);
        assert_eq!(s.state(), StreamState::Open);
        let fin = s.finish();
        assert!(fin.is_some());
        assert_eq!(s.state(), StreamState::HalfClosedLocal);
    }
} 