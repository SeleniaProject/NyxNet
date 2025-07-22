#![forbid(unsafe_code)]

//! Minimal BBR-like congestion control skeleton (not full implementation).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const GAIN_CYCLE: [f64; 8] = [
    1.25, 1.2, 1.15, 1.1, // ProbeUp
    1.0,                  // Steady
    0.9, 0.85, 0.8        // ProbeDown
];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Mode {
    Startup,
    Drain,
    ProbeBw,
}

#[derive(Debug)]
pub struct CongestionCtrl {
    cwnd: f64,          // congestion window in packets
    #[allow(dead_code)]
    pacing_rate: f64,   // packets per second
    min_rtt: Duration,
    rtt_samples: VecDeque<Duration>,
    bw_est: f64,          // packets/s
    cycle_index: usize,
    inflight: usize,
    #[allow(dead_code)]
    last_update: Instant,

    mode: Mode,
    full_bw: f64,
    full_bw_cnt: u8,
    min_rtt_timestamp: Instant,
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

            mode: Mode::Startup,
            full_bw: 0.0,
            full_bw_cnt: 0,
            min_rtt_timestamp: Instant::now(),
        }
    }

    /// Called when a packet is sent.
    pub fn on_send(&mut self, bytes: usize) {
        self.inflight += bytes;
    }

    /// Called when ACK is received with RTT sample.
    pub fn on_ack(&mut self, bytes: usize, rtt: Duration) {
        self.inflight = self.inflight.saturating_sub(bytes);

        // RTT tracking & aging (update min_rtt every 10s)
        if self.rtt_samples.len() == 8 { self.rtt_samples.pop_front(); }
        self.rtt_samples.push_back(rtt);
        if rtt < self.min_rtt || self.min_rtt_timestamp.elapsed() > Duration::from_secs(10) {
            self.min_rtt = rtt;
            self.min_rtt_timestamp = Instant::now();
        }

        // Bandwidth sample in bytes/sec
        let delivery_rate = bytes as f64 / rtt.as_secs_f64();
        self.bw_est = 0.9 * self.bw_est + 0.1 * delivery_rate;

        // Mode transitions per BBRv2 simplified
        match self.mode {
            Mode::Startup => {
                if self.bw_est >= self.full_bw * 1.25 {
                    self.full_bw = self.bw_est;
                    self.full_bw_cnt = 0;
                } else {
                    self.full_bw_cnt += 1;
                    if self.full_bw_cnt >= 3 {
                        self.mode = Mode::Drain;
                    }
                }
            }
            Mode::Drain => {
                if self.inflight as f64 <= self.cwnd {
                    self.mode = Mode::ProbeBw;
                    self.cycle_index = 0; // reset probe cycle
                }
            }
            Mode::ProbeBw => {
                // cycle index advances each ACK below
            }
        }

        let gain = match self.mode {
            Mode::Startup => 2.0,
            Mode::Drain => 0.7,
            Mode::ProbeBw => {
                let g = GAIN_CYCLE[self.cycle_index];
                self.cycle_index = (self.cycle_index + 1) % GAIN_CYCLE.len();
                g
            }
        };

        let target = (self.bw_est * self.min_rtt.as_secs_f64() * gain).max(1280.0 * 4.0);
        self.cwnd = 0.9 * self.cwnd + 0.1 * target;
    }

    pub fn available_window(&self) -> f64 {
        self.cwnd - (self.inflight as f64 / 1280.0)
    }

    /// Return the current congestion window in packets.
    pub fn cwnd(&self) -> f64 {
        self.cwnd
    }

    /// Bytes in flight.
    pub fn inflight(&self) -> usize {
        self.inflight
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