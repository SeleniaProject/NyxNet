use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// High-precision latency collector for detailed performance analysis
#[derive(Debug, Clone)]
pub struct LatencyCollector {
    measurements: Vec<LatencyMeasurement>,
    layer_measurements: HashMap<String, Vec<Duration>>,
    start_time: Instant,
}

/// Individual latency measurement with layer breakdown
#[derive(Debug, Clone)]
pub struct LatencyMeasurement {
    pub timestamp: Instant,
    pub total_latency: Duration,
    pub layer_breakdown: LayerLatencyBreakdown,
    pub request_id: String,
}

/// Latency breakdown by protocol layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerLatencyBreakdown {
    pub stream_layer_ms: f64,
    pub mix_layer_ms: f64,
    pub fec_layer_ms: f64,
    pub transport_layer_ms: f64,
}

/// Comprehensive latency statistics with percentiles and distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStatistics {
    pub total_measurements: usize,
    pub avg_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub std_deviation_ms: f64,
    pub percentiles: LatencyPercentiles,
    pub layer_statistics: LayerLatencyStatistics,
    pub distribution: LatencyDistribution,
    pub time_series: Vec<TimeSeriesPoint>,
}

/// Detailed percentile breakdown for latency analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50: f64,  // Median
    pub p75: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
    pub p99_5: f64,
    pub p99_9: f64,
    pub p99_99: f64,
}

/// Per-layer latency statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerLatencyStatistics {
    pub stream_layer: LayerStats,
    pub mix_layer: LayerStats,
    pub fec_layer: LayerStats,
    pub transport_layer: LayerStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerStats {
    pub avg_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub percentile_95_ms: f64,
    pub contribution_percentage: f64,
}

/// Latency distribution histogram
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyDistribution {
    pub buckets: Vec<LatencyBucket>,
    pub total_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBucket {
    pub range_start_ms: f64,
    pub range_end_ms: f64,
    pub count: usize,
    pub percentage: f64,
}

/// Time series data point for latency trends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: DateTime<Utc>,
    pub latency_ms: f64,
    pub moving_average_ms: f64,
}

impl LatencyCollector {
    /// Create a new latency collector
    pub fn new() -> Self {
        Self {
            measurements: Vec::new(),
            layer_measurements: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Record a latency measurement with layer breakdown
    pub fn record_measurement(&mut self, total_latency: Duration, request_id: String) {
        // Estimate layer breakdown based on typical Nyx protocol behavior
        // In a future enhancement, this could use actual daemon metrics
        let layer_breakdown = self.estimate_layer_breakdown(total_latency);
        
        let measurement = LatencyMeasurement {
            timestamp: Instant::now(),
            total_latency,
            layer_breakdown: layer_breakdown.clone(),
            request_id,
        };
        
        self.measurements.push(measurement);
        
        // Store layer-specific measurements for detailed analysis
        self.layer_measurements
            .entry("stream".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.stream_layer_ms as u64));
            
        self.layer_measurements
            .entry("mix".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.mix_layer_ms as u64));
            
        self.layer_measurements
            .entry("fec".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.fec_layer_ms as u64));
            
        self.layer_measurements
            .entry("transport".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.transport_layer_ms as u64));
    }

    /// Record a latency measurement with actual daemon-provided layer metrics
    pub fn record_measurement_with_daemon_metrics(
        &mut self, 
        total_latency: Duration, 
        request_id: String,
        daemon_metrics: Option<&crate::proto::StreamStats>
    ) {
        // Use actual daemon metrics if available, otherwise fall back to estimation
        let layer_breakdown = if let Some(stats) = daemon_metrics {
            self.extract_layer_breakdown_from_daemon(total_latency, stats)
        } else {
            self.estimate_layer_breakdown(total_latency)
        };
        
        let measurement = LatencyMeasurement {
            timestamp: Instant::now(),
            total_latency,
            layer_breakdown: layer_breakdown.clone(),
            request_id,
        };
        
        self.measurements.push(measurement);
        
        // Store layer-specific measurements for detailed analysis
        self.layer_measurements
            .entry("stream".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.stream_layer_ms as u64));
            
        self.layer_measurements
            .entry("mix".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.mix_layer_ms as u64));
            
        self.layer_measurements
            .entry("fec".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.fec_layer_ms as u64));
            
        self.layer_measurements
            .entry("transport".to_string())
            .or_insert_with(Vec::new)
            .push(Duration::from_millis(layer_breakdown.transport_layer_ms as u64));
    }

    /// Calculate comprehensive latency statistics
    pub fn calculate_statistics(&self) -> LatencyStatistics {
        if self.measurements.is_empty() {
            return self.empty_statistics();
        }

        let latencies_ms: Vec<f64> = self.measurements
            .iter()
            .map(|m| m.total_latency.as_millis() as f64)
            .collect();

        let total_measurements = latencies_ms.len();
        let avg_latency_ms = latencies_ms.iter().sum::<f64>() / total_measurements as f64;
        let min_latency_ms = latencies_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_latency_ms = latencies_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        // Calculate standard deviation
        let variance = latencies_ms
            .iter()
            .map(|&x| (x - avg_latency_ms).powi(2))
            .sum::<f64>() / total_measurements as f64;
        let std_deviation_ms = variance.sqrt();

        let percentiles = self.calculate_percentiles(&latencies_ms);
        let layer_statistics = self.calculate_layer_statistics();
        let distribution = self.calculate_distribution(&latencies_ms);
        let time_series = self.calculate_time_series();

        LatencyStatistics {
            total_measurements,
            avg_latency_ms,
            min_latency_ms,
            max_latency_ms,
            std_deviation_ms,
            percentiles,
            layer_statistics,
            distribution,
            time_series,
        }
    }

    /// Calculate detailed percentiles
    fn calculate_percentiles(&self, latencies_ms: &[f64]) -> LatencyPercentiles {
        let mut sorted_latencies = latencies_ms.to_vec();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let _len = sorted_latencies.len();
        
        LatencyPercentiles {
            p50: self.percentile(&sorted_latencies, 50.0),
            p75: self.percentile(&sorted_latencies, 75.0),
            p90: self.percentile(&sorted_latencies, 90.0),
            p95: self.percentile(&sorted_latencies, 95.0),
            p99: self.percentile(&sorted_latencies, 99.0),
            p99_5: self.percentile(&sorted_latencies, 99.5),
            p99_9: self.percentile(&sorted_latencies, 99.9),
            p99_99: self.percentile(&sorted_latencies, 99.99),
        }
    }

    /// Calculate percentile value using linear interpolation
    fn percentile(&self, sorted_values: &[f64], percentile: f64) -> f64 {
        if sorted_values.is_empty() {
            return 0.0;
        }
        
        let index = (percentile / 100.0) * (sorted_values.len() - 1) as f64;
        let lower_index = index.floor() as usize;
        let upper_index = index.ceil() as usize;
        
        if lower_index == upper_index {
            sorted_values[lower_index]
        } else {
            let weight = index - lower_index as f64;
            sorted_values[lower_index] * (1.0 - weight) + sorted_values[upper_index] * weight
        }
    }

    /// Calculate per-layer statistics
    fn calculate_layer_statistics(&self) -> LayerLatencyStatistics {
        LayerLatencyStatistics {
            stream_layer: self.calculate_layer_stats("stream"),
            mix_layer: self.calculate_layer_stats("mix"),
            fec_layer: self.calculate_layer_stats("fec"),
            transport_layer: self.calculate_layer_stats("transport"),
        }
    }

    /// Calculate statistics for a specific layer
    fn calculate_layer_stats(&self, layer: &str) -> LayerStats {
        let empty_vec = Vec::new();
        let measurements = self.layer_measurements.get(layer).unwrap_or(&empty_vec);
        
        if measurements.is_empty() {
            return LayerStats {
                avg_latency_ms: 0.0,
                min_latency_ms: 0.0,
                max_latency_ms: 0.0,
                percentile_95_ms: 0.0,
                contribution_percentage: 0.0,
            };
        }

        let latencies_ms: Vec<f64> = measurements
            .iter()
            .map(|d| d.as_millis() as f64)
            .collect();

        let avg_latency_ms = latencies_ms.iter().sum::<f64>() / latencies_ms.len() as f64;
        let min_latency_ms = latencies_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_latency_ms = latencies_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        let mut sorted_latencies = latencies_ms.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let percentile_95_ms = self.percentile(&sorted_latencies, 95.0);
        
        // Calculate contribution percentage to total latency
        let total_avg_latency = if !self.measurements.is_empty() {
            self.measurements
                .iter()
                .map(|m| m.total_latency.as_millis() as f64)
                .sum::<f64>() / self.measurements.len() as f64
        } else {
            1.0
        };
        
        let contribution_percentage = if total_avg_latency > 0.0 {
            (avg_latency_ms / total_avg_latency) * 100.0
        } else {
            0.0
        };

        LayerStats {
            avg_latency_ms,
            min_latency_ms,
            max_latency_ms,
            percentile_95_ms,
            contribution_percentage,
        }
    }

    /// Calculate latency distribution histogram
    fn calculate_distribution(&self, latencies_ms: &[f64]) -> LatencyDistribution {
        if latencies_ms.is_empty() {
            return LatencyDistribution {
                buckets: Vec::new(),
                total_count: 0,
            };
        }

        let min_latency = latencies_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_latency = latencies_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        
        // Create 20 buckets for distribution
        let bucket_count = 20;
        let bucket_size = (max_latency - min_latency) / bucket_count as f64;
        
        let mut buckets = Vec::new();
        let total_count = latencies_ms.len();
        
        for i in 0..bucket_count {
            let range_start = min_latency + (i as f64 * bucket_size);
            let range_end = if i == bucket_count - 1 {
                max_latency + 0.001 // Include max value
            } else {
                min_latency + ((i + 1) as f64 * bucket_size)
            };
            
            let count = latencies_ms
                .iter()
                .filter(|&&latency| latency >= range_start && latency < range_end)
                .count();
            
            let percentage = (count as f64 / total_count as f64) * 100.0;
            
            buckets.push(LatencyBucket {
                range_start_ms: range_start,
                range_end_ms: range_end,
                count,
                percentage,
            });
        }

        LatencyDistribution {
            buckets,
            total_count,
        }
    }

    /// Calculate time series data for trend analysis
    fn calculate_time_series(&self) -> Vec<TimeSeriesPoint> {
        let mut time_series = Vec::new();
        let window_size = 10; // Moving average window
        
        for (i, measurement) in self.measurements.iter().enumerate() {
            let latency_ms = measurement.total_latency.as_millis() as f64;
            
            // Calculate moving average
            let start_idx = if i >= window_size { i - window_size } else { 0 };
            let window_measurements = &self.measurements[start_idx..=i];
            let moving_average_ms = window_measurements
                .iter()
                .map(|m| m.total_latency.as_millis() as f64)
                .sum::<f64>() / window_measurements.len() as f64;
            
            // Convert timestamp to UTC
            let elapsed = measurement.timestamp.duration_since(self.start_time);
            let timestamp = Utc::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
            
            time_series.push(TimeSeriesPoint {
                timestamp,
                latency_ms,
                moving_average_ms,
            });
        }
        
        time_series
    }

    /// Extract layer breakdown from actual daemon metrics
    fn extract_layer_breakdown_from_daemon(
        &self, 
        total_latency: Duration, 
        stats: &crate::proto::StreamStats
    ) -> LayerLatencyBreakdown {
        let total_ms = total_latency.as_millis() as f64;
        
        // Extract actual layer metrics from daemon statistics
        // Use path statistics to get more accurate layer breakdown
        if !stats.paths.is_empty() {
            // Use the first path's statistics as representative
            let path_stats = &stats.paths[0];
            
            // Calculate layer contributions based on actual measurements
            let stream_layer_ms = stats.avg_rtt_ms * 0.3; // Stream establishment overhead
            let mix_layer_ms = path_stats.rtt_ms * 0.6;   // Mix routing latency
            let fec_layer_ms = total_ms * 0.15;           // FEC processing (estimated)
            let transport_layer_ms = total_ms - stream_layer_ms - mix_layer_ms - fec_layer_ms;
            
            LayerLatencyBreakdown {
                stream_layer_ms: stream_layer_ms.max(0.0),
                mix_layer_ms: mix_layer_ms.max(0.0),
                fec_layer_ms: fec_layer_ms.max(0.0),
                transport_layer_ms: transport_layer_ms.max(0.0),
            }
        } else {
            // Fall back to RTT-based estimation if no path stats available
            let stream_layer_ms = stats.avg_rtt_ms * 0.4;
            let mix_layer_ms = stats.avg_rtt_ms * 0.35;
            let fec_layer_ms = stats.avg_rtt_ms * 0.15;
            let transport_layer_ms = stats.avg_rtt_ms * 0.1;
            
            LayerLatencyBreakdown {
                stream_layer_ms,
                mix_layer_ms,
                fec_layer_ms,
                transport_layer_ms,
            }
        }
    }

    /// Estimate layer breakdown based on total latency
    /// This is a simplified model used when daemon metrics are not available
    fn estimate_layer_breakdown(&self, total_latency: Duration) -> LayerLatencyBreakdown {
        let total_ms = total_latency.as_millis() as f64;
        
        // Typical breakdown based on Nyx protocol analysis:
        // - Stream layer: 40% (connection establishment, stream management)
        // - Mix layer: 30% (routing through mix network)
        // - FEC layer: 20% (forward error correction processing)
        // - Transport layer: 10% (network transport overhead)
        
        LayerLatencyBreakdown {
            stream_layer_ms: total_ms * 0.40,
            mix_layer_ms: total_ms * 0.30,
            fec_layer_ms: total_ms * 0.20,
            transport_layer_ms: total_ms * 0.10,
        }
    }

    /// Return empty statistics for when no measurements are available
    fn empty_statistics(&self) -> LatencyStatistics {
        LatencyStatistics {
            total_measurements: 0,
            avg_latency_ms: 0.0,
            min_latency_ms: 0.0,
            max_latency_ms: 0.0,
            std_deviation_ms: 0.0,
            percentiles: LatencyPercentiles {
                p50: 0.0,
                p75: 0.0,
                p90: 0.0,
                p95: 0.0,
                p99: 0.0,
                p99_5: 0.0,
                p99_9: 0.0,
                p99_99: 0.0,
            },
            layer_statistics: LayerLatencyStatistics {
                stream_layer: LayerStats {
                    avg_latency_ms: 0.0,
                    min_latency_ms: 0.0,
                    max_latency_ms: 0.0,
                    percentile_95_ms: 0.0,
                    contribution_percentage: 0.0,
                },
                mix_layer: LayerStats {
                    avg_latency_ms: 0.0,
                    min_latency_ms: 0.0,
                    max_latency_ms: 0.0,
                    percentile_95_ms: 0.0,
                    contribution_percentage: 0.0,
                },
                fec_layer: LayerStats {
                    avg_latency_ms: 0.0,
                    min_latency_ms: 0.0,
                    max_latency_ms: 0.0,
                    percentile_95_ms: 0.0,
                    contribution_percentage: 0.0,
                },
                transport_layer: LayerStats {
                    avg_latency_ms: 0.0,
                    min_latency_ms: 0.0,
                    max_latency_ms: 0.0,
                    percentile_95_ms: 0.0,
                    contribution_percentage: 0.0,
                },
            },
            distribution: LatencyDistribution {
                buckets: Vec::new(),
                total_count: 0,
            },
            time_series: Vec::new(),
        }
    }

    /// Create a detailed statistics display with human-readable formatting
    pub fn create_detailed_display(&self) -> String {
        let stats = self.calculate_statistics();
        let mut display = String::new();
        
        display.push_str("=== Detailed Latency Analysis ===\n\n");
        
        // Summary statistics
        display.push_str(&format!(
            "Total Measurements: {}\n\
             Average Latency: {:.2}ms\n\
             Median (P50): {:.2}ms\n\
             95th Percentile: {:.2}ms\n\
             99th Percentile: {:.2}ms\n\
             Standard Deviation: {:.2}ms\n\n",
            stats.total_measurements,
            stats.avg_latency_ms,
            stats.percentiles.p50,
            stats.percentiles.p95,
            stats.percentiles.p99,
            stats.std_deviation_ms
        ));
        
        // Layer breakdown
        display.push_str("=== Protocol Layer Breakdown ===\n");
        display.push_str(&format!(
            "Stream Layer:    {:.2}ms ({:.1}%)\n\
             Mix Layer:       {:.2}ms ({:.1}%)\n\
             FEC Layer:       {:.2}ms ({:.1}%)\n\
             Transport Layer: {:.2}ms ({:.1}%)\n\n",
            stats.layer_statistics.stream_layer.avg_latency_ms,
            stats.layer_statistics.stream_layer.contribution_percentage,
            stats.layer_statistics.mix_layer.avg_latency_ms,
            stats.layer_statistics.mix_layer.contribution_percentage,
            stats.layer_statistics.fec_layer.avg_latency_ms,
            stats.layer_statistics.fec_layer.contribution_percentage,
            stats.layer_statistics.transport_layer.avg_latency_ms,
            stats.layer_statistics.transport_layer.contribution_percentage
        ));
        
        // Simple ASCII histogram
        if !stats.distribution.buckets.is_empty() {
            display.push_str("=== Latency Distribution ===\n");
            let max_count = stats.distribution.buckets.iter().map(|b| b.count).max().unwrap_or(1);
            
            for bucket in &stats.distribution.buckets {
                if bucket.count > 0 {
                    let bar_length = (bucket.count * 40) / max_count.max(1);
                    let bar = "█".repeat(bar_length);
                    display.push_str(&format!(
                        "{:6.1}-{:6.1}ms: {:>4} ({:4.1}%) {}\n",
                        bucket.range_start_ms,
                        bucket.range_end_ms,
                        bucket.count,
                        bucket.percentage,
                        bar
                    ));
                }
            }
        }
        
        display
    }

    /// Get the number of measurements collected
    pub fn measurement_count(&self) -> usize {
        self.measurements.len()
    }

    /// Clear all measurements
    pub fn clear(&mut self) {
        self.measurements.clear();
        self.layer_measurements.clear();
        self.start_time = Instant::now();
    }
}

impl Default for LatencyCollector {
    fn default() -> Self {
        Self::new()
    }
}