#![forbid(unsafe_code)]

//! Minimal BBR-like congestion control skeleton (not full implementation).

use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct CongestionCtrl {
    cwnd: f64,          // congestion window in packets
    pacing_rate: f64,   // packets per second
    min_rtt: Duration,
    inflight: usize,
    last_update: Instant,
}

impl CongestionCtrl {
    pub fn new() -> Self {
        Self {
            cwnd: 10.0,
            pacing_rate: 10.0,
            min_rtt: Duration::from_millis(200),
            inflight: 0,
            last_update: Instant::now(),
        }
    }

    /// Called when a packet is sent.
    pub fn on_send(&mut self, bytes: usize) {
        self.inflight += bytes;
    }

    /// Called when ACK is received with RTT sample.
    pub fn on_ack(&mut self, bytes: usize, rtt: Duration) {
        self.inflight = self.inflight.saturating_sub(bytes);
        if rtt < self.min_rtt {
            self.min_rtt = rtt;
        }
        // very naive cwnd growth
        self.cwnd += (bytes as f64) / self.cwnd;
    }

    pub fn available_window(&self) -> f64 {
        self.cwnd - (self.inflight as f64 / 1280.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_growth() {
        let mut cc = CongestionCtrl::new();
        cc.on_send(1280);
        cc.on_ack(1280, Duration::from_millis(100));
        assert!(cc.cwnd > 10.0);
    }
} 