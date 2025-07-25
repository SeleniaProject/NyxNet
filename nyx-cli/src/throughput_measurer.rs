use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Comprehensive throughput and bandwidth measurement system
#[derive(Debug, Clone)]
pub struct ThroughputMeasurer {
    start_time: Instant,
    measurements: Vec<ThroughputMeasurement>,
    total_bytes_sent: Arc<AtomicU64>,
    total_bytes_received: Arc<AtomicU64>,
    measurement_interval: Duration,
}

/// Individual throughput measurement point
#[derive(Debug, Clone)]
pub struct ThroughputMeasurement {
    pub timestamp: Instant,
    pub bytes_sent_delta: u64,
    pub bytes_received_delta: u64,
    pub instantaneous_send_rate_mbps: f64,
    pub instantaneous_receive_rate_mbps: f64,
    pub protocol_overhead_bytes: u64,
}

/// Comprehensive throughput statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputStatistics {
    pub duration_secs: f64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub avg_send_rate_mbps: f64,
    pub avg_receive_rate_mbps: f64,
    pub peak_send_rate_mbps: f64,
    pub peak_receive_rate_mbps: f64,
    pub min_send_rate_mbps: f64,
    pub min_receive_rate_mbps: f64,
    pub protocol_overhead_percentage: f64,
    pub data_transfer_efficiency: f64,
    pub bandwidth_utilization: BandwidthUtilization,
    pub performance_analysis: PerformanceAnalysis,
    pub time_series: Vec<ThroughputTimePoint>,
}

/// Bandwidth utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthUtilization {
    pub theoretical_max_mbps: f64,
    pub actual_utilization_percentage: f64,
    pub efficiency_score: f64,
    pub bottleneck_analysis: String,
}

/// Performance analysis with recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub overall_score: f64, // 0-100
    pub bottlenecks: Vec<String>,
    pub recommendations: Vec<String>,
    pub efficiency_rating: String, // "Excellent", "Good", "Fair", "Poor"
}

/// Time series data point for throughput trends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputTimePoint {
    pub timestamp: DateTime<Utc>,
    pub send_rate_mbps: f64,
    pub receive_rate_mbps: f64,
    pub moving_avg_send_mbps: f64,
    pub moving_avg_receive_mbps: f64,
}

impl ThroughputMeasurer {
    /// Create a new throughput measurer
    pub fn new(measurement_interval: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            measurements: Vec::new(),
            total_bytes_sent: Arc::new(AtomicU64::new(0)),
            total_bytes_received: Arc::new(AtomicU64::new(0)),
            measurement_interval,
        }
    }

    /// Record bytes transferred and calculate instantaneous rates
    pub fn record_transfer(&mut self, bytes_sent: u64, bytes_received: u64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.start_time);
        
        // Update totals
        let prev_sent = self.total_bytes_sent.fetch_add(bytes_sent, Ordering::Relaxed);
        let prev_received = self.total_bytes_received.fetch_add(bytes_received, Ordering::Relaxed);
        
        // Calculate instantaneous rates if we have a previous measurement
        let (send_rate_mbps, receive_rate_mbps) = if let Some(last_measurement) = self.measurements.last() {
            let time_delta = now.duration_since(last_measurement.timestamp).as_secs_f64();
            if time_delta > 0.0 {
                let send_rate = (bytes_sent as f64 * 8.0) / (time_delta * 1_000_000.0);
                let receive_rate = (bytes_received as f64 * 8.0) / (time_delta * 1_000_000.0);
                (send_rate, receive_rate)
            } else {
                (0.0, 0.0)
            }
        } else {
            // First measurement - calculate rate from start
            let time_delta = elapsed.as_secs_f64();
            if time_delta > 0.0 {
                let send_rate = ((prev_sent + bytes_sent) as f64 * 8.0) / (time_delta * 1_000_000.0);
                let receive_rate = ((prev_received + bytes_received) as f64 * 8.0) / (time_delta * 1_000_000.0);
                (send_rate, receive_rate)
            } else {
                (0.0, 0.0)
            }
        };
        
        // Estimate protocol overhead (simplified model)
        let protocol_overhead = self.estimate_protocol_overhead(bytes_sent, bytes_received);
        
        let measurement = ThroughputMeasurement {
            timestamp: now,
            bytes_sent_delta: bytes_sent,
            bytes_received_delta: bytes_received,
            instantaneous_send_rate_mbps: send_rate_mbps,
            instantaneous_receive_rate_mbps: receive_rate_mbps,
            protocol_overhead_bytes: protocol_overhead,
        };
        
        self.measurements.push(measurement);
    }

    /// Calculate comprehensive throughput statistics
    pub fn calculate_statistics(&self) -> ThroughputStatistics {
        if self.measurements.is_empty() {
            return self.empty_statistics();
        }

        let duration_secs = self.start_time.elapsed().as_secs_f64();
        let total_bytes_sent = self.total_bytes_sent.load(Ordering::Relaxed);
        let total_bytes_received = self.total_bytes_received.load(Ordering::Relaxed);
        
        // Calculate average rates
        let avg_send_rate_mbps = if duration_secs > 0.0 {
            (total_bytes_sent as f64 * 8.0) / (duration_secs * 1_000_000.0)
        } else {
            0.0
        };
        
        let avg_receive_rate_mbps = if duration_secs > 0.0 {
            (total_bytes_received as f64 * 8.0) / (duration_secs * 1_000_000.0)
        } else {
            0.0
        };
        
        // Find peak and minimum rates
        let send_rates: Vec<f64> = self.measurements.iter()
            .map(|m| m.instantaneous_send_rate_mbps)
            .collect();
        let receive_rates: Vec<f64> = self.measurements.iter()
            .map(|m| m.instantaneous_receive_rate_mbps)
            .collect();
        
        let peak_send_rate_mbps = send_rates.iter().cloned().fold(0.0, f64::max);
        let peak_receive_rate_mbps = receive_rates.iter().cloned().fold(0.0, f64::max);
        let min_send_rate_mbps = send_rates.iter().cloned().fold(f64::INFINITY, f64::min);
        let min_receive_rate_mbps = receive_rates.iter().cloned().fold(f64::INFINITY, f64::min);
        
        // Calculate protocol overhead
        let total_overhead: u64 = self.measurements.iter()
            .map(|m| m.protocol_overhead_bytes)
            .sum();
        let protocol_overhead_percentage = if total_bytes_sent > 0 {
            (total_overhead as f64 / total_bytes_sent as f64) * 100.0
        } else {
            0.0
        };
        
        // Calculate data transfer efficiency
        let data_transfer_efficiency = if total_bytes_sent > 0 {
            ((total_bytes_sent - total_overhead) as f64 / total_bytes_sent as f64) * 100.0
        } else {
            100.0
        };
        
        let bandwidth_utilization = self.calculate_bandwidth_utilization(
            avg_send_rate_mbps, 
            peak_send_rate_mbps
        );
        
        let performance_analysis = self.analyze_performance(
            avg_send_rate_mbps,
            avg_receive_rate_mbps,
            data_transfer_efficiency,
            &bandwidth_utilization
        );
        
        let time_series = self.calculate_time_series();
        
        ThroughputStatistics {
            duration_secs,
            total_bytes_sent,
            total_bytes_received,
            avg_send_rate_mbps,
            avg_receive_rate_mbps,
            peak_send_rate_mbps,
            peak_receive_rate_mbps,
            min_send_rate_mbps: if min_send_rate_mbps == f64::INFINITY { 0.0 } else { min_send_rate_mbps },
            min_receive_rate_mbps: if min_receive_rate_mbps == f64::INFINITY { 0.0 } else { min_receive_rate_mbps },
            protocol_overhead_percentage,
            data_transfer_efficiency,
            bandwidth_utilization,
            performance_analysis,
            time_series,
        }
    }

    /// Estimate protocol overhead for Nyx streams
    fn estimate_protocol_overhead(&self, bytes_sent: u64, _bytes_received: u64) -> u64 {
        // Nyx protocol overhead estimation:
        // - Stream headers: ~32 bytes per frame
        // - Mix routing overhead: ~20% of payload
        // - FEC overhead: ~15% of payload  
        // - Encryption overhead: ~16 bytes per frame
        
        let frame_overhead = 32 + 16; // Headers + encryption
        let routing_overhead = (bytes_sent as f64 * 0.20) as u64;
        let fec_overhead = (bytes_sent as f64 * 0.15) as u64;
        
        frame_overhead + routing_overhead + fec_overhead
    }

    /// Calculate bandwidth utilization metrics
    fn calculate_bandwidth_utilization(&self, avg_rate_mbps: f64, peak_rate_mbps: f64) -> BandwidthUtilization {
        // Estimate theoretical maximum based on typical network conditions
        // This is a simplified model - in practice this would be measured or configured
        let theoretical_max_mbps = 100.0; // Assume 100 Mbps theoretical max
        
        let actual_utilization_percentage = (avg_rate_mbps / theoretical_max_mbps) * 100.0;
        let efficiency_score = (avg_rate_mbps / peak_rate_mbps.max(1.0)) * 100.0;
        
        let bottleneck_analysis = if actual_utilization_percentage < 10.0 {
            "Low utilization - possible network or application bottleneck".to_string()
        } else if actual_utilization_percentage < 50.0 {
            "Moderate utilization - room for improvement".to_string()
        } else if actual_utilization_percentage < 80.0 {
            "Good utilization - performing well".to_string()
        } else {
            "High utilization - near capacity".to_string()
        };
        
        BandwidthUtilization {
            theoretical_max_mbps,
            actual_utilization_percentage,
            efficiency_score,
            bottleneck_analysis,
        }
    }

    /// Analyze performance and provide recommendations
    fn analyze_performance(
        &self,
        avg_send_rate: f64,
        avg_receive_rate: f64,
        efficiency: f64,
        utilization: &BandwidthUtilization,
    ) -> PerformanceAnalysis {
        let mut bottlenecks = Vec::new();
        let mut recommendations = Vec::new();
        
        // Analyze send/receive rate balance
        let rate_ratio = if avg_receive_rate > 0.0 {
            avg_send_rate / avg_receive_rate
        } else {
            1.0
        };
        
        if rate_ratio < 0.8 {
            bottlenecks.push("Send rate significantly lower than receive rate".to_string());
            recommendations.push("Check for send-side bottlenecks or flow control issues".to_string());
        } else if rate_ratio > 1.2 {
            bottlenecks.push("Receive rate lower than send rate".to_string());
            recommendations.push("Check for receive-side processing delays".to_string());
        }
        
        // Analyze efficiency
        if efficiency < 70.0 {
            bottlenecks.push("High protocol overhead".to_string());
            recommendations.push("Consider optimizing payload sizes or reducing encryption overhead".to_string());
        }
        
        // Analyze utilization
        if utilization.actual_utilization_percentage < 20.0 {
            bottlenecks.push("Low bandwidth utilization".to_string());
            recommendations.push("Increase concurrency or payload sizes to improve throughput".to_string());
        }
        
        // Calculate overall score
        let efficiency_score = (efficiency / 100.0) * 30.0;
        let utilization_score = (utilization.actual_utilization_percentage.min(80.0) / 80.0) * 40.0;
        let balance_score = (1.0 - (rate_ratio - 1.0).abs().min(0.5)) * 30.0;
        let overall_score = efficiency_score + utilization_score + balance_score;
        
        let efficiency_rating = if overall_score >= 80.0 {
            "Excellent".to_string()
        } else if overall_score >= 60.0 {
            "Good".to_string()
        } else if overall_score >= 40.0 {
            "Fair".to_string()
        } else {
            "Poor".to_string()
        };
        
        PerformanceAnalysis {
            overall_score,
            bottlenecks,
            recommendations,
            efficiency_rating,
        }
    }

    /// Calculate time series data for trend analysis
    fn calculate_time_series(&self) -> Vec<ThroughputTimePoint> {
        let mut time_series = Vec::new();
        let window_size = 5; // Moving average window
        
        for (i, measurement) in self.measurements.iter().enumerate() {
            // Calculate moving averages
            let start_idx = if i >= window_size { i - window_size } else { 0 };
            let window_measurements = &self.measurements[start_idx..=i];
            
            let moving_avg_send = window_measurements.iter()
                .map(|m| m.instantaneous_send_rate_mbps)
                .sum::<f64>() / window_measurements.len() as f64;
                
            let moving_avg_receive = window_measurements.iter()
                .map(|m| m.instantaneous_receive_rate_mbps)
                .sum::<f64>() / window_measurements.len() as f64;
            
            // Convert timestamp to UTC
            let elapsed = measurement.timestamp.duration_since(self.start_time);
            let timestamp = Utc::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
            
            time_series.push(ThroughputTimePoint {
                timestamp,
                send_rate_mbps: measurement.instantaneous_send_rate_mbps,
                receive_rate_mbps: measurement.instantaneous_receive_rate_mbps,
                moving_avg_send_mbps: moving_avg_send,
                moving_avg_receive_mbps: moving_avg_receive,
            });
        }
        
        time_series
    }

    /// Return empty statistics when no measurements are available
    fn empty_statistics(&self) -> ThroughputStatistics {
        ThroughputStatistics {
            duration_secs: 0.0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            avg_send_rate_mbps: 0.0,
            avg_receive_rate_mbps: 0.0,
            peak_send_rate_mbps: 0.0,
            peak_receive_rate_mbps: 0.0,
            min_send_rate_mbps: 0.0,
            min_receive_rate_mbps: 0.0,
            protocol_overhead_percentage: 0.0,
            data_transfer_efficiency: 100.0,
            bandwidth_utilization: BandwidthUtilization {
                theoretical_max_mbps: 100.0,
                actual_utilization_percentage: 0.0,
                efficiency_score: 0.0,
                bottleneck_analysis: "No data available".to_string(),
            },
            performance_analysis: PerformanceAnalysis {
                overall_score: 0.0,
                bottlenecks: vec!["No measurements available".to_string()],
                recommendations: vec!["Start benchmark to collect data".to_string()],
                efficiency_rating: "Unknown".to_string(),
            },
            time_series: Vec::new(),
        }
    }

    /// Get current throughput rates
    pub fn get_current_rates(&self) -> (f64, f64) {
        if let Some(last_measurement) = self.measurements.last() {
            (
                last_measurement.instantaneous_send_rate_mbps,
                last_measurement.instantaneous_receive_rate_mbps,
            )
        } else {
            (0.0, 0.0)
        }
    }

    /// Reset all measurements
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
        self.measurements.clear();
        self.total_bytes_sent.store(0, Ordering::Relaxed);
        self.total_bytes_received.store(0, Ordering::Relaxed);
    }
}

impl Default for ThroughputMeasurer {
    fn default() -> Self {
        Self::new(Duration::from_millis(100)) // 100ms measurement interval
    }
}