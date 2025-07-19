#![forbid(unsafe_code)]

//! Minimal BBR-like congestion control skeleton (not full implementation).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct CongestionCtrl {
    cwnd: f64,          // congestion window in packets
    pacing_rate: f64,   // packets per second
    min_rtt: Duration,
    rtt_samples: VecDeque<Duration>,
    bw_est: f64,          // packets/s
    cycle_index: usize,
    inflight: usize,
    last_update: Instant,
}

impl CongestionCtrl {
    pub fn new() -> Self {
        Self {
            cwnd: 10.0,
            pacing_rate: 10.0,
            min_rtt: Duration::MAX,
            rtt_samples: VecDeque::with_capacity(8),
            bw_est: 0.0,
            cycle_index: 0,
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

        // RTT tracking
        if self.rtt_samples.len() == 8 { self.rtt_samples.pop_front(); }
        self.rtt_samples.push_back(rtt);
        self.min_rtt = *self.rtt_samples.iter().min().unwrap_or(&rtt);

        // Bandwidth estimate (packets/s)
        let delivery_rate = bytes as f64 / rtt.as_secs_f64();
        // EWMA
        self.bw_est = if self.bw_est == 0.0 { delivery_rate } else { 0.9 * self.bw_est + 0.1 * delivery_rate };

        // BBRv2 pacing cycle gains [1.25, 0.75]
        const GAIN_CYCLE: [f64; 2] = [1.25, 0.75];
        self.cycle_index = (self.cycle_index + 1) % GAIN_CYCLE.len();
        let gain = GAIN_CYCLE[self.cycle_index];

        // cwnd update: bw_est * min_rtt * gain
        self.cwnd = (self.bw_est * self.min_rtt.as_secs_f64() * gain).max(4.0);
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