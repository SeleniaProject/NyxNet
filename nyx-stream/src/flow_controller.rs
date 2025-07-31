#![forbid(unsafe_code)]

use std::time::{Duration, Instant};
use std::collections::VecDeque;
use tracing::{debug, warn, trace, error};
use serde::{Serialize, Deserialize};

/// Network statistics for monitoring and debugging
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub bytes_sent: u64,
    pub bytes_acked: u64,
    pub packets_lost: u32,
    pub current_window: u32,
    pub congestion_window: u32,
    pub rtt: Option<Duration>,
    pub rto: Duration,
    pub throughput: f64,
    pub buffer_usage: f64,
}

/// Flow control statistics for monitoring
#[derive(Debug, Clone)]
pub struct FlowControlStats {
    pub flow_window_size: u32,
    pub bytes_in_flight: u32,
    pub congestion_window: u32,
    pub congestion_state: CongestionState,
    pub rtt: Option<Duration>,
    pub rto: Duration,
    pub bytes_sent: u64,
    pub bytes_acked: u64,
    pub packets_lost: u32,
    pub send_buffer_size: usize,
    pub throughput: f64,
    pub backpressure_active: bool,
}

/// Flow control errors
#[derive(thiserror::Error, Debug)]
pub enum FlowControlError {
    #[error("Buffer overflow: available={available}, requested={requested}")]
    BufferOverflow { available: usize, requested: usize },
    #[error("Invalid window size: {0}")]
    InvalidWindowSize(u32),
    #[error("Congestion window exhausted")]
    CongestionWindowExhausted,
    #[error("Flow control violation: {message}")]
    FlowControlViolation { message: String },
}

/// RTT estimation using exponential weighted moving average
#[derive(Debug, Clone)]
pub struct RttEstimator {
    srtt: Option<Duration>,  // Smoothed RTT
    rttvar: Duration,        // RTT variation
    rto: Duration,           // Retransmission timeout
    min_rto: Duration,
    max_rto: Duration,
    alpha: f64,              // SRTT smoothing factor
    beta: f64,               // RTTVAR smoothing factor
}

impl RttEstimator {
    pub fn new() -> Self {
        Self {
            srtt: None,
            rttvar: Duration::from_millis(0),
            rto: Duration::from_secs(1),
            min_rto: Duration::from_millis(200),
            max_rto: Duration::from_secs(60),
            alpha: 0.125,
            beta: 0.25,
        }
    }

    pub fn update(&mut self, rtt_sample: Duration) {
        match self.srtt {
            None => {
                // First measurement
                self.srtt = Some(rtt_sample);
                self.rttvar = rtt_sample / 2;
            }
            Some(srtt) => {
                // RFC 6298 algorithm
                let rtt_diff = if rtt_sample > srtt {
                    rtt_sample - srtt
                } else {
                    srtt - rtt_sample
                };
                
                self.rttvar = Duration::from_secs_f64(
                    (1.0 - self.beta) * self.rttvar.as_secs_f64() + 
                    self.beta * rtt_diff.as_secs_f64()
                );
                
                self.srtt = Some(Duration::from_secs_f64(
                    (1.0 - self.alpha) * srtt.as_secs_f64() + 
                    self.alpha * rtt_sample.as_secs_f64()
                ));
            }
        }

        // Calculate RTO
        let srtt = self.srtt.unwrap();
        self.rto = srtt + 4 * self.rttvar;
        self.rto = self.rto.clamp(self.min_rto, self.max_rto);

        trace!("RTT updated: sample={:?}, srtt={:?}, rttvar={:?}, rto={:?}", 
               rtt_sample, srtt, self.rttvar, self.rto);
    }

    pub fn rto(&self) -> Duration {
        self.rto
    }

    pub fn srtt(&self) -> Option<Duration> {
        self.srtt
    }
}

impl Default for RttEstimator {
    fn default() -> Self {
        Self::new()
    }
}

/// Congestion control state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionState {
    SlowStart,
    CongestionAvoidance,
    FastRecovery,
}

/// Congestion control using TCP-like algorithm
#[derive(Debug)]
pub struct CongestionController {
    cwnd: u32,              // Congestion window
    ssthresh: u32,          // Slow start threshold
    state: CongestionState,
    bytes_acked: u32,       // Bytes acknowledged in current RTT
    duplicate_acks: u32,    // Count of duplicate ACKs
    fast_recovery_exit_point: u32, // Sequence number to exit fast recovery
    initial_cwnd: u32,
    max_cwnd: u32,
}

impl CongestionController {
    pub fn new(initial_cwnd: u32, max_cwnd: u32) -> Self {
        Self {
            cwnd: initial_cwnd,
            ssthresh: max_cwnd / 2,
            state: CongestionState::SlowStart,
            bytes_acked: 0,
            duplicate_acks: 0,
            fast_recovery_exit_point: 0,
            initial_cwnd,
            max_cwnd,
        }
    }

    pub fn on_ack(&mut self, acked_bytes: u32, is_duplicate: bool) {
        if is_duplicate {
            self.duplicate_acks += 1;
            
            if self.duplicate_acks == 3 && self.state != CongestionState::FastRecovery {
                // Fast retransmit
                self.enter_fast_recovery();
            } else if self.state == CongestionState::FastRecovery {
                // Inflate window during fast recovery
                self.cwnd += acked_bytes;
            }
            return;
        }

        // Reset duplicate ACK counter on new ACK
        self.duplicate_acks = 0;
        self.bytes_acked += acked_bytes;

        match self.state {
            CongestionState::SlowStart => {
                self.cwnd += acked_bytes;
                if self.cwnd >= self.ssthresh {
                    self.state = CongestionState::CongestionAvoidance;
                    debug!("Entered congestion avoidance, cwnd={}", self.cwnd);
                }
            }
            CongestionState::CongestionAvoidance => {
                // Increase cwnd by (acked_bytes * acked_bytes) / cwnd
                let increase = (acked_bytes * acked_bytes) / self.cwnd;
                self.cwnd += increase.max(1);
            }
            CongestionState::FastRecovery => {
                // Exit fast recovery if we've ACKed past the recovery point
                if acked_bytes > 0 {
                    self.cwnd = self.ssthresh;
                    self.state = CongestionState::CongestionAvoidance;
                    debug!("Exited fast recovery, cwnd={}", self.cwnd);
                }
            }
        }

        // Cap congestion window
        self.cwnd = self.cwnd.min(self.max_cwnd);
        
        trace!("Congestion control update: state={:?}, cwnd={}, ssthresh={}, acked={}", 
               self.state, self.cwnd, self.ssthresh, acked_bytes);
    }

    pub fn on_loss(&mut self) {
        match self.state {
            CongestionState::FastRecovery => {
                // Already in fast recovery, don't reduce further
                return;
            }
            _ => {
                self.ssthresh = (self.cwnd / 2).max(2 * self.initial_cwnd);
                self.cwnd = self.ssthresh;
                self.state = CongestionState::CongestionAvoidance;
                debug!("Packet loss detected, cwnd={}, ssthresh={}", self.cwnd, self.ssthresh);
            }
        }
    }

    fn enter_fast_recovery(&mut self) {
        self.ssthresh = (self.cwnd / 2).max(2 * self.initial_cwnd);
        self.cwnd = self.ssthresh + 3 * self.initial_cwnd; // 3 duplicate ACKs
        self.state = CongestionState::FastRecovery;
        debug!("Entered fast recovery, cwnd={}, ssthresh={}", self.cwnd, self.ssthresh);
    }

    pub fn cwnd(&self) -> u32 {
        self.cwnd
    }

    pub fn state(&self) -> CongestionState {
        self.state
    }

    pub fn can_send(&self, bytes_in_flight: u32) -> u32 {
        if bytes_in_flight >= self.cwnd {
            0
        } else {
            self.cwnd - bytes_in_flight
        }
    }
}

/// Flow control window management
#[derive(Debug)]
pub struct FlowWindow {
    window_size: u32,
    bytes_in_flight: u32,
    max_window_size: u32,
    window_updates: VecDeque<(Instant, u32)>, // Track window updates for auto-tuning
}

impl FlowWindow {
    pub fn new(initial_window: u32, max_window: u32) -> Self {
        Self {
            window_size: initial_window,
            bytes_in_flight: 0,
            max_window_size: max_window,
            window_updates: VecDeque::new(),
        }
    }

    pub fn can_send(&self, bytes: u32) -> bool {
        self.bytes_in_flight + bytes <= self.window_size
    }

    pub fn available_window(&self) -> u32 {
        self.window_size.saturating_sub(self.bytes_in_flight)
    }

    pub fn on_data_sent(&mut self, bytes: u32) -> Result<(), FlowControlError> {
        if self.bytes_in_flight + bytes > self.window_size {
            return Err(FlowControlError::FlowControlViolation {
                message: format!("Attempted to send {} bytes but only {} available", bytes, self.available_window()),
            });
        }
        
        self.bytes_in_flight += bytes;
        trace!("Data sent: {} bytes, in_flight: {}, window: {}", 
               bytes, self.bytes_in_flight, self.window_size);
        Ok(())
    }

    pub fn on_ack_received(&mut self, acked_bytes: u32) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(acked_bytes);
        trace!("ACK received: {} bytes, in_flight: {}, window: {}", 
               acked_bytes, self.bytes_in_flight, self.window_size);
    }

    pub fn update_window(&mut self, new_window: u32) -> Result<(), FlowControlError> {
        if new_window > self.max_window_size {
            return Err(FlowControlError::InvalidWindowSize(new_window));
        }
        
        let old_window = self.window_size;
        self.window_size = new_window;
        self.window_updates.push_back((Instant::now(), new_window));
        
        // Keep only recent updates for auto-tuning
        let cutoff = Instant::now() - Duration::from_secs(10);
        while let Some(&(timestamp, _)) = self.window_updates.front() {
            if timestamp < cutoff {
                self.window_updates.pop_front();
            } else {
                break;
            }
        }
        
        debug!("Window updated: {} -> {}, available: {}", 
               old_window, new_window, self.available_window());
        Ok(())
    }

    pub fn auto_tune(&mut self, rtt: Duration, throughput: f64) {
        // Simple auto-tuning based on bandwidth-delay product
        let bdp = (throughput * rtt.as_secs_f64()) as u32;
        let target_window = (bdp * 2).min(self.max_window_size).max(self.window_size / 2);
        
        if target_window != self.window_size {
            let _ = self.update_window(target_window);
            debug!("Auto-tuned window to {} (BDP: {}, RTT: {:?}, throughput: {:.2})", 
                   target_window, bdp, rtt, throughput);
        }
    }

    pub fn window_size(&self) -> u32 {
        self.window_size
    }

    pub fn bytes_in_flight(&self) -> u32 {
        self.bytes_in_flight
    }

    /// Get the maximum window size
    pub fn max_window_size(&self) -> u32 {
        self.max_window_size
    }

    /// Consume window for sending data
    pub fn consume_window(&mut self, bytes: u32) -> Result<(), FlowControlError> {
        self.on_data_sent(bytes)
    }
}

/// Comprehensive flow controller for Nyx protocol
#[derive(Debug)]
pub struct FlowController {
    flow_window: FlowWindow,
    congestion_controller: CongestionController,
    rtt_estimator: RttEstimator,
    
    // Buffer management
    send_buffer: VecDeque<u8>,
    max_send_buffer: usize,
    
    // Statistics
    bytes_sent: u64,
    bytes_acked: u64,
    packets_lost: u32,
    last_throughput_update: Instant,
    throughput_bytes: u64,
    
    // Configuration
    enable_auto_tuning: bool,
    backpressure_threshold: f64, // Fraction of buffer that triggers backpressure
}

impl FlowController {
    /// Create a new flow controller with simplified constructor for integrated processor
    pub fn new(initial_window: u32) -> Self {
        let max_window = initial_window * 10;
        let initial_cwnd = initial_window.min(65536);
        let max_send_buffer = initial_window as usize * 4;
        
        Self {
            flow_window: FlowWindow::new(initial_window, max_window),
            congestion_controller: CongestionController::new(initial_cwnd, max_window),
            rtt_estimator: RttEstimator::new(),
            send_buffer: VecDeque::new(),
            max_send_buffer,
            bytes_sent: 0,
            bytes_acked: 0,
            packets_lost: 0,
            last_throughput_update: Instant::now(),
            throughput_bytes: 0,
            enable_auto_tuning: true,
            backpressure_threshold: 0.8,
        }
    }

    /// Create a new flow controller with full parameters
    pub fn new_with_params(
        initial_window: u32,
        max_window: u32,
        initial_cwnd: u32,
        max_send_buffer: usize,
    ) -> Self {
        Self {
            flow_window: FlowWindow::new(initial_window, max_window),
            congestion_controller: CongestionController::new(initial_cwnd, max_window),
            rtt_estimator: RttEstimator::new(),
            send_buffer: VecDeque::new(),
            max_send_buffer,
            bytes_sent: 0,
            bytes_acked: 0,
            packets_lost: 0,
            last_throughput_update: Instant::now(),
            throughput_bytes: 0,
            enable_auto_tuning: true,
            backpressure_threshold: 0.8,
        }
    }

    /// Handle data received for flow control
    pub fn on_data_received(&mut self, bytes: u32) -> Result<(), FlowControlError> {
        self.flow_window.on_data_sent(bytes)?;
        self.bytes_sent += bytes as u64;
        Ok(())
    }

    /// Check if data can be sent considering both flow control and congestion control
    pub fn can_send(&self, bytes: u32) -> bool {
        let flow_available = self.flow_window.can_send(bytes);
        let congestion_available = self.congestion_controller.can_send(self.flow_window.bytes_in_flight()) >= bytes;
        
        flow_available && congestion_available
    }

    /// Get the maximum number of bytes that can be sent
    pub fn available_to_send(&self) -> u32 {
        let flow_available = self.flow_window.available_window();
        let congestion_available = self.congestion_controller.can_send(self.flow_window.bytes_in_flight());
        
        flow_available.min(congestion_available)
    }

    /// Record data being sent
    pub fn on_data_sent(&mut self, bytes: u32) -> Result<(), FlowControlError> {
        if !self.can_send(bytes) {
            return Err(FlowControlError::CongestionWindowExhausted);
        }
        
        self.flow_window.on_data_sent(bytes)?;
        self.bytes_sent += bytes as u64;
        self.throughput_bytes += bytes as u64;
        
        trace!("Data sent: {} bytes, total sent: {}", bytes, self.bytes_sent);
        Ok(())
    }

    /// Process acknowledgment
    pub fn on_ack_received(&mut self, acked_bytes: u32, rtt: Duration, is_duplicate: bool) {
        self.flow_window.on_ack_received(acked_bytes);
        self.congestion_controller.on_ack(acked_bytes, is_duplicate);
        
        if !is_duplicate {
            self.rtt_estimator.update(rtt);
            self.bytes_acked += acked_bytes as u64;
        }
        
        // Auto-tune if enabled
        if self.enable_auto_tuning && !is_duplicate {
            self.maybe_auto_tune();
        }
        
        debug!("ACK processed: {} bytes, duplicate: {}, RTT: {:?}", 
               acked_bytes, is_duplicate, rtt);
    }

    /// Handle packet loss
    pub fn on_packet_lost(&mut self, lost_bytes: u32) {
        self.congestion_controller.on_loss();
        self.packets_lost += 1;
        
        warn!("Packet loss detected: {} bytes, total losses: {}", 
              lost_bytes, self.packets_lost);
    }

    /// Update flow control window
    pub fn update_flow_window(&mut self, new_window: u32) -> Result<(), FlowControlError> {
        self.flow_window.update_window(new_window)
    }

    /// Buffer data for sending
    pub fn buffer_data(&mut self, data: &[u8]) -> Result<usize, FlowControlError> {
        let available_buffer = self.max_send_buffer.saturating_sub(self.send_buffer.len());
        
        if data.len() > available_buffer {
            return Err(FlowControlError::BufferOverflow {
                requested: data.len(),
                available: available_buffer,
            });
        }
        
        self.send_buffer.extend(data);
        Ok(data.len())
    }

    /// Get buffered data that can be sent
    pub fn get_sendable_data(&mut self, max_bytes: u32) -> Vec<u8> {
        let available = self.available_to_send().min(max_bytes) as usize;
        let to_send = available.min(self.send_buffer.len());
        
        if to_send == 0 {
            return Vec::new();
        }
        
        let mut data = Vec::with_capacity(to_send);
        for _ in 0..to_send {
            if let Some(byte) = self.send_buffer.pop_front() {
                data.push(byte);
            } else {
                break;
            }
        }
        
        data
    }

    /// Get current throughput estimate (bytes per second)
    pub fn current_throughput(&self) -> f64 {
        let elapsed = self.last_throughput_update.elapsed();
        if elapsed.as_secs_f64() > 0.0 {
            self.throughput_bytes as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        }
    }

    /// Update flow control parameters based on network conditions
    pub fn adaptive_flow_control_update(&mut self, network_delay: Duration, packet_loss_rate: f64) {
        if !self.enable_auto_tuning {
            return;
        }

        // Adjust window based on delay and loss
        let current_window = self.flow_window.window_size();
        
        // Increase window if low delay and low loss
        if network_delay < Duration::from_millis(50) && packet_loss_rate < 0.001 {
            let new_window = (current_window as f64 * 1.1) as u32;
            let _ = self.flow_window.update_window(new_window.min(self.flow_window.max_window_size()));
            debug!("Increased flow window to {} due to good conditions", new_window);
        }
        // Decrease window if high delay or high loss
        else if network_delay > Duration::from_millis(200) || packet_loss_rate > 0.01 {
            let new_window = (current_window as f64 * 0.9) as u32;
            let _ = self.flow_window.update_window(new_window.max(1024)); // Minimum window
            debug!("Decreased flow window to {} due to poor conditions", new_window);
        }
    }

    /// Dynamic window size adjustment based on congestion
    pub fn dynamic_window_adjustment(&mut self) -> u32 {
        let congestion_window = self.congestion_controller.cwnd();
        let flow_window = self.flow_window.window_size();
        
        // Use the minimum of flow control and congestion control windows
        let effective_window = congestion_window.min(flow_window);
        
        // Apply backpressure if buffer is getting full
        if self.should_apply_backpressure() {
            let backpressure_factor = 1.0 - (self.send_buffer.len() as f64 / self.max_send_buffer as f64);
            let adjusted_window = (effective_window as f64 * backpressure_factor) as u32;
            debug!("Applied backpressure: window {} -> {}", effective_window, adjusted_window);
            adjusted_window.max(1024) // Ensure minimum window
        } else {
            effective_window
        }
    }

    /// Get network statistics for monitoring
    pub fn get_network_stats(&self) -> NetworkStats {
        NetworkStats {
            bytes_sent: self.bytes_sent,
            bytes_acked: self.bytes_acked,
            packets_lost: self.packets_lost,
            current_window: self.flow_window.window_size(),
            congestion_window: self.congestion_controller.cwnd(),
            rtt: self.rtt_estimator.srtt(),
            rto: self.rtt_estimator.rto(),
            throughput: self.current_throughput(),
            buffer_usage: self.send_buffer.len() as f64 / self.max_send_buffer as f64,
        }
    }

    /// Check if flow control allows sending more data
    pub fn can_send_more(&self) -> bool {
        self.available_to_send() > 0 && !self.should_apply_backpressure()
    }

    /// Get optimal send size based on current conditions
    pub fn optimal_send_size(&self) -> u32 {
        let available = self.available_to_send();
        let mtu_estimate = 1450; // Conservative MTU estimate
        
        if available >= mtu_estimate {
            mtu_estimate // Send full packets when possible
        } else {
            available
        }
    }

    /// Consume receive window for incoming data
    pub fn consume_receive_window(&mut self, _bytes: u32) -> Result<(), FlowControlError> {
        // For receive side, we just track the data
        // Real implementation would manage receive buffer
        Ok(())
    }

    /// Check if window update should be sent
    pub fn should_send_window_update(&self) -> bool {
        // Send update when window is getting low
        let window_usage = self.flow_window.bytes_in_flight() as f64 / self.flow_window.window_size() as f64;
        window_usage > 0.75
    }

    /// Generate window update value
    pub fn generate_window_update(&self) -> u32 {
        // Increase window based on available buffer space
        self.flow_window.window_size() * 2
    }

    /// Update receive metrics
    pub fn update_receive_metrics(&mut self, bytes_received: u32) {
        // Track received data for statistics
        self.throughput_bytes += bytes_received as u64;
    }

    /// Apply backpressure when buffer is getting full
    pub fn apply_backpressure(&mut self, pending_bytes: u32) {
        // Reduce window size to apply backpressure
        let current_window = self.flow_window.window_size();
        let reduced_window = current_window.saturating_sub(pending_bytes).max(1024);
        
        if let Err(e) = self.flow_window.update_window(reduced_window) {
            warn!("Failed to apply backpressure: {}", e);
        } else {
            debug!("Applied backpressure: window {} -> {}", current_window, reduced_window);
        }
    }

    /// Check if backpressure should be applied
    pub fn should_apply_backpressure(&self) -> bool {
        let buffer_usage = self.send_buffer.len() as f64 / self.max_send_buffer as f64;
        buffer_usage > self.backpressure_threshold
    }

    /// Get current statistics
    pub fn get_stats(&self) -> FlowControlStats {
        let now = Instant::now();
        let duration = now.duration_since(self.last_throughput_update);
        let throughput = if duration.as_secs_f64() > 0.0 {
            self.throughput_bytes as f64 / duration.as_secs_f64()
        } else {
            0.0
        };
        
        FlowControlStats {
            flow_window_size: self.flow_window.window_size(),
            bytes_in_flight: self.flow_window.bytes_in_flight(),
            congestion_window: self.congestion_controller.cwnd(),
            congestion_state: self.congestion_controller.state(),
            rtt: self.rtt_estimator.srtt(),
            rto: self.rtt_estimator.rto(),
            bytes_sent: self.bytes_sent,
            bytes_acked: self.bytes_acked,
            packets_lost: self.packets_lost,
            send_buffer_size: self.send_buffer.len(),
            throughput,
            backpressure_active: self.should_apply_backpressure(),
        }
    }

    fn maybe_auto_tune(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_throughput_update) >= Duration::from_secs(1) {
            if let Some(rtt) = self.rtt_estimator.srtt() {
                let duration = now.duration_since(self.last_throughput_update);
                let throughput = self.throughput_bytes as f64 / duration.as_secs_f64();
                
                self.flow_window.auto_tune(rtt, throughput);
                
                self.last_throughput_update = now;
                self.throughput_bytes = 0;
            }
        }
    }

    /// Enable or disable auto-tuning
    pub fn set_auto_tuning(&mut self, enabled: bool) {
        self.enable_auto_tuning = enabled;
    }

    /// Set backpressure threshold
    pub fn set_backpressure_threshold(&mut self, threshold: f64) {
        self.backpressure_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get available send window (same as available_to_send for compatibility)
    pub fn get_available_send_window(&self) -> u32 {
        self.available_to_send()
    }

    /// Get bytes in flight (same as flow window bytes in flight)
    pub fn get_bytes_in_flight(&self) -> u32 {
        self.flow_window.bytes_in_flight()
    }

    /// Get congestion window size
    pub fn get_congestion_window(&self) -> u32 {
        self.congestion_controller.cwnd()
    }

    /// Update RTT measurement
    pub fn update_rtt(&mut self, rtt: Duration) {
        self.rtt_estimator.update(rtt);
    }

    /// Handle data acknowledgment
    pub fn on_data_acked(&mut self, bytes: u32) {
        self.flow_window.on_ack_received(bytes);
        self.bytes_acked += bytes as u64;
    }

    /// Handle data loss
    pub fn on_data_lost(&mut self, bytes: u32) {
        self.packets_lost += 1;
        self.congestion_controller.on_loss();
    }

    /// Consume send window
    pub fn consume_send_window(&mut self, bytes: u32) -> Result<(), FlowControlError> {
        self.flow_window.on_data_sent(bytes)
    }

    /// Restore send window 
    pub fn restore_send_window(&mut self, bytes: u32) {
        self.flow_window.on_ack_received(bytes);
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtt_estimator() {
        let mut estimator = RttEstimator::new();
        
        // First sample
        estimator.update(Duration::from_millis(100));
        assert_eq!(estimator.srtt(), Some(Duration::from_millis(100)));
        
        // Second sample
        estimator.update(Duration::from_millis(200));
        let srtt = estimator.srtt().unwrap();
        assert!(srtt > Duration::from_millis(100));
        assert!(srtt < Duration::from_millis(200));
    }

    #[test]
    fn test_congestion_controller() {
        let mut controller = CongestionController::new(1000, 10000);
        
        // Initial state should be slow start
        assert_eq!(controller.state(), CongestionState::SlowStart);
        assert_eq!(controller.cwnd(), 1000);
        
        // ACK should increase window in slow start
        controller.on_ack(500, false);
        assert_eq!(controller.cwnd(), 1500);
        
        // Loss should reduce window
        let cwnd_before_loss = controller.cwnd();
        controller.on_loss();
        
        // The window should be set to ssthresh, which is max(cwnd/2, 2*initial_cwnd)
        let expected_ssthresh = (cwnd_before_loss / 2).max(2 * controller.initial_cwnd);
        assert_eq!(controller.cwnd(), expected_ssthresh);
        assert_eq!(controller.state(), CongestionState::CongestionAvoidance);
    }

    #[test]
    fn test_flow_window() {
        let mut window = FlowWindow::new(1000, 10000);
        
        assert!(window.can_send(500));
        assert!(!window.can_send(1500));
        
        window.on_data_sent(500).unwrap();
        assert_eq!(window.bytes_in_flight(), 500);
        assert_eq!(window.available_window(), 500);
        
        window.on_ack_received(200);
        assert_eq!(window.bytes_in_flight(), 300);
        assert_eq!(window.available_window(), 700);
    }

    #[test]
    fn test_flow_controller() {
        let mut controller = FlowController::new(1000, 10000, 1000, 4096);
        
        // Should be able to send initially
        assert!(controller.can_send(500));
        
        // Send data
        controller.on_data_sent(500).unwrap();
        assert_eq!(controller.get_stats().bytes_in_flight, 500);
        
        // Process ACK
        controller.on_ack_received(500, Duration::from_millis(100), false);
        assert_eq!(controller.get_stats().bytes_in_flight, 0);
        assert_eq!(controller.get_stats().bytes_acked, 500);
    }

    #[test]
    fn test_buffer_management() {
        let mut controller = FlowController::new(1000, 10000, 1000, 100);
        
        // Buffer some data
        let data = vec![1u8; 50];
        controller.buffer_data(&data).unwrap();
        assert_eq!(controller.get_stats().send_buffer_size, 50);
        
        // Get sendable data
        let sendable = controller.get_sendable_data(30);
        assert_eq!(sendable.len(), 30);
        assert_eq!(controller.get_stats().send_buffer_size, 20);
        
        // Test buffer overflow
        let large_data = vec![2u8; 100];
        assert!(controller.buffer_data(&large_data).is_err());
    }

    #[test]
    fn test_backpressure() {
        let mut controller = FlowController::new(1000, 10000, 1000, 100);
        controller.set_backpressure_threshold(0.5);
        
        // Should not have backpressure initially
        assert!(!controller.should_apply_backpressure());
        
        // Fill buffer beyond threshold
        let data = vec![1u8; 60];
        controller.buffer_data(&data).unwrap();
        
        // Should now have backpressure
        assert!(controller.should_apply_backpressure());
    }
}