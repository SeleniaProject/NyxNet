use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::benchmark::BenchmarkError;

/// Comprehensive error tracking and analysis system
#[derive(Debug, Clone)]
pub struct ErrorTracker {
    start_time: Instant,
    errors: Vec<ErrorEvent>,
    layer_error_counts: HashMap<String, u64>,
    error_type_counts: HashMap<String, u64>,
    total_requests: u64,
}

/// Individual error event with detailed context
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    pub timestamp: Instant,
    pub error: BenchmarkError,
    pub request_id: String,
    pub context: ErrorContext,
}

/// Additional context for error analysis
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub target_address: String,
    pub payload_size: usize,
    pub connection_attempt: u32,
    pub network_conditions: NetworkConditions,
}

/// Network conditions at time of error
#[derive(Debug, Clone)]
pub struct NetworkConditions {
    pub estimated_latency_ms: f64,
    pub estimated_bandwidth_mbps: f64,
    pub connection_count: u32,
    pub system_load: f64,
}

/// Comprehensive error statistics and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStatistics {
    pub total_errors: u64,
    pub total_requests: u64,
    pub overall_error_rate: f64,
    pub error_rate_by_layer: HashMap<String, LayerErrorStats>,
    pub error_rate_by_type: HashMap<String, ErrorTypeStats>,
    pub error_trends: ErrorTrends,
    pub correlation_analysis: CorrelationAnalysis,
    pub troubleshooting_recommendations: Vec<TroubleshootingRecommendation>,
    pub time_series: Vec<ErrorTimePoint>,
}

/// Error statistics for a specific protocol layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerErrorStats {
    pub layer_name: String,
    pub error_count: u64,
    pub error_rate: f64,
    pub most_common_errors: Vec<String>,
    pub avg_time_between_errors_ms: f64,
    pub error_severity: ErrorSeverity,
}

/// Error statistics for a specific error type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTypeStats {
    pub error_type: String,
    pub count: u64,
    pub percentage: f64,
    pub first_occurrence: DateTime<Utc>,
    pub last_occurrence: DateTime<Utc>,
    pub frequency_per_minute: f64,
    pub associated_layers: Vec<String>,
}

/// Error trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTrends {
    pub error_rate_trend: String, // "increasing", "decreasing", "stable", "volatile"
    pub peak_error_periods: Vec<ErrorPeriod>,
    pub error_clustering: bool,
    pub dominant_error_types: Vec<String>,
    pub error_rate_change_percentage: f64,
}

/// Period of high error activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPeriod {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub error_count: u64,
    pub dominant_error_type: String,
    pub affected_layers: Vec<String>,
}

/// Correlation analysis between errors and network conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationAnalysis {
    pub latency_correlation: f64,
    pub bandwidth_correlation: f64,
    pub load_correlation: f64,
    pub connection_count_correlation: f64,
    pub strongest_correlation: String,
    pub correlation_insights: Vec<String>,
}

/// Troubleshooting recommendation with priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TroubleshootingRecommendation {
    pub priority: RecommendationPriority,
    pub category: String,
    pub title: String,
    pub description: String,
    pub action_items: Vec<String>,
    pub expected_impact: String,
}

/// Priority level for recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Error severity classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Critical,  // System-breaking errors
    High,      // Significant impact on performance
    Medium,    // Moderate impact, recoverable
    Low,       // Minor issues, minimal impact
}

/// Time series data point for error trends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTimePoint {
    pub timestamp: DateTime<Utc>,
    pub error_count: u64,
    pub error_rate: f64,
    pub dominant_error_type: String,
    pub affected_layer: String,
}

impl ErrorTracker {
    /// Create a new error tracker
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            errors: Vec::new(),
            layer_error_counts: HashMap::new(),
            error_type_counts: HashMap::new(),
            total_requests: 0,
        }
    }

    /// Record an error event with context
    pub fn record_error(
        &mut self,
        error: BenchmarkError,
        request_id: String,
        context: ErrorContext,
    ) {
        let layer = error.layer().to_string();
        let error_type = self.classify_error_type(&error);
        
        // Update counters
        *self.layer_error_counts.entry(layer).or_insert(0) += 1;
        *self.error_type_counts.entry(error_type).or_insert(0) += 1;
        
        // Record the error event
        let error_event = ErrorEvent {
            timestamp: Instant::now(),
            error,
            request_id,
            context,
        };
        
        self.errors.push(error_event);
    }

    /// Record a successful request (for error rate calculation)
    pub fn record_success(&mut self) {
        self.total_requests += 1;
    }

    /// Record total requests attempted
    pub fn set_total_requests(&mut self, total: u64) {
        self.total_requests = total;
    }

    /// Calculate comprehensive error statistics
    pub fn calculate_statistics(&self) -> ErrorStatistics {
        let total_errors = self.errors.len() as u64;
        let overall_error_rate = if self.total_requests > 0 {
            (total_errors as f64 / self.total_requests as f64) * 100.0
        } else {
            0.0
        };

        let error_rate_by_layer = self.calculate_layer_error_stats();
        let error_rate_by_type = self.calculate_error_type_stats();
        let error_trends = self.analyze_error_trends();
        let correlation_analysis = self.analyze_correlations();
        let troubleshooting_recommendations = self.generate_recommendations(&error_rate_by_layer, &error_trends);
        let time_series = self.calculate_time_series();

        ErrorStatistics {
            total_errors,
            total_requests: self.total_requests,
            overall_error_rate,
            error_rate_by_layer,
            error_rate_by_type,
            error_trends,
            correlation_analysis,
            troubleshooting_recommendations,
            time_series,
        }
    }

    /// Classify error type based on error details
    fn classify_error_type(&self, error: &BenchmarkError) -> String {
        match error {
            BenchmarkError::StreamLayer(msg) => {
                if msg.contains("timeout") {
                    "Stream Timeout".to_string()
                } else if msg.contains("connection") {
                    "Connection Failed".to_string()
                } else if msg.contains("handshake") {
                    "Handshake Failed".to_string()
                } else {
                    "Stream Error".to_string()
                }
            }
            BenchmarkError::MixLayer(msg) => {
                if msg.contains("routing") {
                    "Routing Failed".to_string()
                } else if msg.contains("path") {
                    "Path Building Failed".to_string()
                } else {
                    "Mix Network Error".to_string()
                }
            }
            BenchmarkError::FecLayer(msg) => {
                if msg.contains("correction") {
                    "FEC Correction Failed".to_string()
                } else {
                    "FEC Processing Error".to_string()
                }
            }
            BenchmarkError::TransportLayer(msg) => {
                if msg.contains("network") {
                    "Network Unreachable".to_string()
                } else if msg.contains("timeout") {
                    "Transport Timeout".to_string()
                } else {
                    "Transport Error".to_string()
                }
            }
            BenchmarkError::Unknown(_) => "Unknown Error".to_string(),
        }
    }

    /// Calculate error statistics by protocol layer
    fn calculate_layer_error_stats(&self) -> HashMap<String, LayerErrorStats> {
        let mut layer_stats = HashMap::new();
        
        for (layer, &error_count) in &self.layer_error_counts {
            let error_rate = if self.total_requests > 0 {
                (error_count as f64 / self.total_requests as f64) * 100.0
            } else {
                0.0
            };
            
            // Find most common errors for this layer
            let layer_errors: Vec<&ErrorEvent> = self.errors.iter()
                .filter(|e| e.error.layer() == layer)
                .collect();
            
            let mut error_type_counts = HashMap::new();
            for error_event in &layer_errors {
                let error_type = self.classify_error_type(&error_event.error);
                *error_type_counts.entry(error_type).or_insert(0) += 1;
            }
            
            let most_common_errors: Vec<String> = error_type_counts.into_iter()
                .map(|(error_type, _count)| error_type)
                .take(3)
                .collect();
            
            // Calculate average time between errors
            let avg_time_between_errors_ms = if layer_errors.len() > 1 {
                let total_duration = layer_errors.last().unwrap().timestamp
                    .duration_since(layer_errors.first().unwrap().timestamp);
                total_duration.as_millis() as f64 / (layer_errors.len() - 1) as f64
            } else {
                0.0
            };
            
            // Determine error severity
            let error_severity = if error_rate > 10.0 {
                ErrorSeverity::Critical
            } else if error_rate > 5.0 {
                ErrorSeverity::High
            } else if error_rate > 1.0 {
                ErrorSeverity::Medium
            } else {
                ErrorSeverity::Low
            };
            
            layer_stats.insert(layer.clone(), LayerErrorStats {
                layer_name: layer.clone(),
                error_count,
                error_rate,
                most_common_errors,
                avg_time_between_errors_ms,
                error_severity,
            });
        }
        
        layer_stats
    }

    /// Calculate error statistics by error type
    fn calculate_error_type_stats(&self) -> HashMap<String, ErrorTypeStats> {
        let mut type_stats = HashMap::new();
        let total_errors = self.errors.len() as u64;
        
        for (error_type, &count) in &self.error_type_counts {
            let percentage = if total_errors > 0 {
                (count as f64 / total_errors as f64) * 100.0
            } else {
                0.0
            };
            
            // Find first and last occurrences
            let type_errors: Vec<&ErrorEvent> = self.errors.iter()
                .filter(|e| self.classify_error_type(&e.error) == *error_type)
                .collect();
            
            let (first_occurrence, last_occurrence) = if !type_errors.is_empty() {
                let first_elapsed = type_errors.first().unwrap().timestamp.duration_since(self.start_time);
                let last_elapsed = type_errors.last().unwrap().timestamp.duration_since(self.start_time);
                
                let first = Utc::now() - chrono::Duration::from_std(first_elapsed).unwrap_or_default();
                let last = Utc::now() - chrono::Duration::from_std(last_elapsed).unwrap_or_default();
                (first, last)
            } else {
                (Utc::now(), Utc::now())
            };
            
            // Calculate frequency per minute
            let duration_minutes = self.start_time.elapsed().as_secs_f64() / 60.0;
            let frequency_per_minute = if duration_minutes > 0.0 {
                count as f64 / duration_minutes
            } else {
                0.0
            };
            
            // Find associated layers
            let associated_layers: Vec<String> = type_errors.iter()
                .map(|e| e.error.layer().to_string())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            
            type_stats.insert(error_type.clone(), ErrorTypeStats {
                error_type: error_type.clone(),
                count,
                percentage,
                first_occurrence,
                last_occurrence,
                frequency_per_minute,
                associated_layers,
            });
        }
        
        type_stats
    }

    /// Analyze error trends over time
    fn analyze_error_trends(&self) -> ErrorTrends {
        if self.errors.is_empty() {
            return ErrorTrends {
                error_rate_trend: "stable".to_string(),
                peak_error_periods: Vec::new(),
                error_clustering: false,
                dominant_error_types: Vec::new(),
                error_rate_change_percentage: 0.0,
            };
        }

        // Analyze trend by comparing first and second half of errors
        let mid_point = self.errors.len() / 2;
        let first_half_errors = mid_point;
        let second_half_errors = self.errors.len() - mid_point;
        
        let error_rate_change_percentage = if first_half_errors > 0 {
            ((second_half_errors as f64 - first_half_errors as f64) / first_half_errors as f64) * 100.0
        } else {
            0.0
        };
        
        let error_rate_trend = if error_rate_change_percentage > 20.0 {
            "increasing".to_string()
        } else if error_rate_change_percentage < -20.0 {
            "decreasing".to_string()
        } else if error_rate_change_percentage.abs() > 50.0 {
            "volatile".to_string()
        } else {
            "stable".to_string()
        };
        
        // Find dominant error types
        let mut error_type_pairs: Vec<(String, u64)> = self.error_type_counts.iter()
            .map(|(error_type, &count)| (error_type.clone(), count))
            .collect();
        error_type_pairs.sort_by(|a, b| b.1.cmp(&a.1));
        let dominant_error_types: Vec<String> = error_type_pairs.into_iter()
            .take(3)
            .map(|(error_type, _)| error_type)
            .collect();
        
        // Detect error clustering (simplified)
        let error_clustering = self.errors.len() > 10 && error_rate_change_percentage.abs() > 30.0;
        
        ErrorTrends {
            error_rate_trend,
            peak_error_periods: Vec::new(), // Simplified for now
            error_clustering,
            dominant_error_types,
            error_rate_change_percentage,
        }
    }

    /// Analyze correlations between errors and network conditions
    fn analyze_correlations(&self) -> CorrelationAnalysis {
        // Simplified correlation analysis
        // In a real implementation, this would use statistical correlation methods
        
        CorrelationAnalysis {
            latency_correlation: 0.3, // Placeholder
            bandwidth_correlation: -0.2, // Placeholder
            load_correlation: 0.5, // Placeholder
            connection_count_correlation: 0.4, // Placeholder
            strongest_correlation: "System Load".to_string(),
            correlation_insights: vec![
                "Higher system load correlates with increased error rates".to_string(),
                "Connection count shows moderate correlation with errors".to_string(),
            ],
        }
    }

    /// Generate troubleshooting recommendations
    fn generate_recommendations(
        &self,
        layer_stats: &HashMap<String, LayerErrorStats>,
        trends: &ErrorTrends,
    ) -> Vec<TroubleshootingRecommendation> {
        let mut recommendations = Vec::new();
        
        // Check for high error rates by layer
        for (layer, stats) in layer_stats {
            if stats.error_rate > 10.0 {
                recommendations.push(TroubleshootingRecommendation {
                    priority: RecommendationPriority::Critical,
                    category: format!("{} Layer", layer),
                    title: format!("High error rate in {} layer", layer),
                    description: format!(
                        "The {} layer is experiencing a {:.1}% error rate, which is above the acceptable threshold.",
                        layer, stats.error_rate
                    ),
                    action_items: self.get_layer_specific_actions(layer),
                    expected_impact: "Should reduce error rate by 50-80%".to_string(),
                });
            }
        }
        
        // Check for increasing error trends
        if trends.error_rate_trend == "increasing" {
            recommendations.push(TroubleshootingRecommendation {
                priority: RecommendationPriority::High,
                category: "Performance".to_string(),
                title: "Increasing error rate trend detected".to_string(),
                description: format!(
                    "Error rates are increasing by {:.1}% over time, indicating a degrading system.",
                    trends.error_rate_change_percentage
                ),
                action_items: vec![
                    "Monitor system resources (CPU, memory, network)".to_string(),
                    "Check for network congestion or connectivity issues".to_string(),
                    "Review recent configuration changes".to_string(),
                ],
                expected_impact: "Should stabilize error rates".to_string(),
            });
        }
        
        recommendations
    }

    /// Get layer-specific troubleshooting actions
    fn get_layer_specific_actions(&self, layer: &str) -> Vec<String> {
        match layer {
            "Stream" => vec![
                "Check stream timeout configurations".to_string(),
                "Verify daemon connectivity and health".to_string(),
                "Review stream establishment parameters".to_string(),
            ],
            "Mix" => vec![
                "Verify mix network topology".to_string(),
                "Check path building algorithms".to_string(),
                "Review routing table consistency".to_string(),
            ],
            "FEC" => vec![
                "Check FEC algorithm configuration".to_string(),
                "Verify error correction parameters".to_string(),
                "Monitor packet loss rates".to_string(),
            ],
            "Transport" => vec![
                "Check network connectivity".to_string(),
                "Verify firewall and NAT configurations".to_string(),
                "Monitor transport layer timeouts".to_string(),
            ],
            _ => vec![
                "Review system logs for detailed error information".to_string(),
                "Check overall system health and resources".to_string(),
            ],
        }
    }

    /// Calculate time series data for error trends
    fn calculate_time_series(&self) -> Vec<ErrorTimePoint> {
        let mut time_series = Vec::new();
        let window_size = Duration::from_secs(10); // 10-second windows
        
        if self.errors.is_empty() {
            return time_series;
        }
        
        let start_time = self.errors.first().unwrap().timestamp;
        let end_time = self.errors.last().unwrap().timestamp;
        let total_duration = end_time.duration_since(start_time);
        
        let num_windows = (total_duration.as_secs() / window_size.as_secs()).max(1);
        
        for i in 0..num_windows {
            let window_start = start_time + window_size * i as u32;
            let window_end = window_start + window_size;
            
            let window_errors: Vec<&ErrorEvent> = self.errors.iter()
                .filter(|e| e.timestamp >= window_start && e.timestamp < window_end)
                .collect();
            
            let error_count = window_errors.len() as u64;
            let error_rate = (error_count as f64 / self.total_requests as f64) * 100.0;
            
            let dominant_error_type = if !window_errors.is_empty() {
                self.classify_error_type(&window_errors[0].error)
            } else {
                "None".to_string()
            };
            
            let affected_layer = if !window_errors.is_empty() {
                window_errors[0].error.layer().to_string()
            } else {
                "None".to_string()
            };
            
            let elapsed = window_start.duration_since(self.start_time);
            let timestamp = Utc::now() - chrono::Duration::from_std(elapsed).unwrap_or_default();
            
            time_series.push(ErrorTimePoint {
                timestamp,
                error_count,
                error_rate,
                dominant_error_type,
                affected_layer,
            });
        }
        
        time_series
    }

    /// Clear all recorded errors
    pub fn clear(&mut self) {
        self.start_time = Instant::now();
        self.errors.clear();
        self.layer_error_counts.clear();
        self.error_type_counts.clear();
        self.total_requests = 0;
    }
}

impl Default for ErrorTracker {
    fn default() -> Self {
        Self::new()
    }
}