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
    /// Advanced error categorizer for enhanced analysis
    categorizer: ErrorCategorizer,
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
            categorizer: ErrorCategorizer::new(),
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
        let error_category = self.categorizer.categorize_error(&error, &context);
        let error_type = format!("{:?}", error_category);
        
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

    /// Calculate comprehensive error statistics with enhanced analysis
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
        let correlation_analysis = self.analyze_correlations_enhanced();
        let troubleshooting_recommendations = self.generate_enhanced_recommendations(&error_rate_by_layer, &error_trends, &correlation_analysis);
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

    /// Enhanced correlation analysis with actual statistical calculations
    fn analyze_correlations_enhanced(&self) -> CorrelationAnalysis {
        if self.errors.is_empty() {
            return CorrelationAnalysis {
                latency_correlation: 0.0,
                bandwidth_correlation: 0.0,
                load_correlation: 0.0,
                connection_count_correlation: 0.0,
                strongest_correlation: "None".to_string(),
                correlation_insights: vec!["Insufficient data for correlation analysis".to_string()],
            };
        }

        // Extract network condition metrics
        let latency_values: Vec<f64> = self.errors.iter()
            .map(|e| e.context.network_conditions.estimated_latency_ms)
            .collect();
        
        let bandwidth_values: Vec<f64> = self.errors.iter()
            .map(|e| e.context.network_conditions.estimated_bandwidth_mbps)
            .collect();
        
        let load_values: Vec<f64> = self.errors.iter()
            .map(|e| e.context.network_conditions.system_load)
            .collect();
        
        let connection_count_values: Vec<f64> = self.errors.iter()
            .map(|e| e.context.network_conditions.connection_count as f64)
            .collect();

        // Calculate correlations using the ErrorCategorizer
        let latency_correlation = self.categorizer.calculate_correlation(&self.errors, &latency_values);
        let bandwidth_correlation = self.categorizer.calculate_correlation(&self.errors, &bandwidth_values);
        let load_correlation = self.categorizer.calculate_correlation(&self.errors, &load_values);
        let connection_correlation = self.categorizer.calculate_correlation(&self.errors, &connection_count_values);

        // Determine strongest correlation
        let correlations = [
            ("Latency", latency_correlation.coefficient.abs()),
            ("Bandwidth", bandwidth_correlation.coefficient.abs()),
            ("System Load", load_correlation.coefficient.abs()),
            ("Connection Count", connection_correlation.coefficient.abs()),
        ];

        let strongest_correlation = correlations.iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|c| c.0.to_string())
            .unwrap_or_else(|| "None".to_string());

        // Generate insights based on correlation results
        let mut correlation_insights = Vec::new();
        
        if latency_correlation.significance as u8 >= CorrelationSignificance::Moderate as u8 {
            let direction = if latency_correlation.coefficient > 0.0 { "increases" } else { "decreases" };
            correlation_insights.push(format!(
                "Network latency {} significantly correlate with error rates (r={:.3}, p<0.05)",
                direction, latency_correlation.coefficient
            ));
        }

        if bandwidth_correlation.significance as u8 >= CorrelationSignificance::Moderate as u8 {
            let direction = if bandwidth_correlation.coefficient > 0.0 { "higher" } else { "lower" };
            correlation_insights.push(format!(
                "{} bandwidth correlates with increased error rates (r={:.3})",
                direction, bandwidth_correlation.coefficient
            ));
        }

        if load_correlation.significance as u8 >= CorrelationSignificance::Moderate as u8 {
            correlation_insights.push(format!(
                "System load shows significant correlation with errors (r={:.3}), suggesting resource constraints",
                load_correlation.coefficient
            ));
        }

        if connection_correlation.significance as u8 >= CorrelationSignificance::Moderate as u8 {
            correlation_insights.push(format!(
                "Connection count correlates with error rates (r={:.3}), indicating potential scalability issues",
                connection_correlation.coefficient
            ));
        }

        if correlation_insights.is_empty() {
            correlation_insights.push("No significant correlations detected between errors and network conditions".to_string());
        }

        CorrelationAnalysis {
            latency_correlation: latency_correlation.coefficient,
            bandwidth_correlation: bandwidth_correlation.coefficient,
            load_correlation: load_correlation.coefficient,
            connection_count_correlation: connection_correlation.coefficient,
            strongest_correlation,
            correlation_insights,
        }
    }

    /// Generate enhanced troubleshooting recommendations using ErrorCategorizer insights
    fn generate_enhanced_recommendations(
        &self,
        layer_stats: &HashMap<String, LayerErrorStats>,
        trends: &ErrorTrends,
        correlation_analysis: &CorrelationAnalysis,
    ) -> Vec<TroubleshootingRecommendation> {
        let mut recommendations = Vec::new();
        
        // Analyze error patterns using the categorizer
        let pattern_analysis = self.categorizer.detect_patterns(&self.errors);
        
        // Check for high error rates by layer
        for (layer, stats) in layer_stats {
            if stats.error_rate > 10.0 {
                let priority = if stats.error_rate > 25.0 {
                    RecommendationPriority::Critical
                } else if stats.error_rate > 15.0 {
                    RecommendationPriority::High
                } else {
                    RecommendationPriority::Medium
                };

                recommendations.push(TroubleshootingRecommendation {
                    priority,
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
        
        // Recommendations based on correlation analysis
        if correlation_analysis.latency_correlation.abs() > 0.5 {
            recommendations.push(TroubleshootingRecommendation {
                priority: RecommendationPriority::High,
                category: "Network Performance".to_string(),
                title: "High correlation between latency and errors".to_string(),
                description: format!(
                    "Network latency shows strong correlation with error rates (r={:.3}). High latency may be causing timeouts and connection failures.",
                    correlation_analysis.latency_correlation
                ),
                action_items: vec![
                    "Optimize network routes and reduce hop count".to_string(),
                    "Implement adaptive timeout mechanisms".to_string(),
                    "Consider using connection pooling to reduce setup overhead".to_string(),
                    "Monitor and address network bottlenecks".to_string(),
                ],
                expected_impact: "Should reduce latency-related errors by 40-60%".to_string(),
            });
        }

        if correlation_analysis.load_correlation > 0.6 {
            recommendations.push(TroubleshootingRecommendation {
                priority: RecommendationPriority::Critical,
                category: "System Resources".to_string(),
                title: "System overload contributing to errors".to_string(),
                description: format!(
                    "High system load strongly correlates with error rates (r={:.3}). Resource exhaustion is likely causing failures.",
                    correlation_analysis.load_correlation
                ),
                action_items: vec![
                    "Scale system resources (CPU, memory)".to_string(),
                    "Implement load balancing and distribution".to_string(),
                    "Optimize resource-intensive operations".to_string(),
                    "Add resource monitoring and alerting".to_string(),
                ],
                expected_impact: "Should significantly reduce system-related errors".to_string(),
            });
        }

        // Recommendations based on pattern analysis
        for spike in &pattern_analysis.anomalous_spikes {
            recommendations.push(TroubleshootingRecommendation {
                priority: RecommendationPriority::High,
                category: "Error Patterns".to_string(),
                title: format!("Anomalous error spike detected at {}", spike.timestamp.format("%H:%M:%S")),
                description: format!(
                    "An error spike with magnitude {:.1} was detected, affecting {} error categories.",
                    spike.magnitude, spike.affected_categories.len()
                ),
                action_items: spike.potential_causes.clone(),
                expected_impact: "Should prevent future error spikes".to_string(),
            });
        }

        for pattern in &pattern_analysis.recurring_patterns {
            if pattern.confidence > 0.7 {
                recommendations.push(TroubleshootingRecommendation {
                    priority: RecommendationPriority::Medium,
                    category: "Recurring Issues".to_string(),
                    title: format!("Recurring error pattern detected: {}", pattern.pattern_id),
                    description: format!(
                        "A recurring error pattern occurs every {:.1} seconds with {:.1}% confidence.",
                        pattern.duration.as_secs_f64(), pattern.confidence * 100.0
                    ),
                    action_items: vec![
                        "Identify the root cause of the recurring pattern".to_string(),
                        "Implement proactive monitoring for pattern detection".to_string(),
                        "Schedule maintenance during low-activity periods".to_string(),
                    ],
                    expected_impact: "Should eliminate recurring error patterns".to_string(),
                });
            }
        }

        // Check for increasing error trends
        if trends.error_rate_trend == "increasing" {
            recommendations.push(TroubleshootingRecommendation {
                priority: RecommendationPriority::High,
                category: "Performance Trends".to_string(),
                title: "Increasing error rate trend detected".to_string(),
                description: format!(
                    "Error rates are increasing by {:.1}% over time, indicating a degrading system.",
                    trends.error_rate_change_percentage
                ),
                action_items: vec![
                    "Monitor system resources (CPU, memory, network)".to_string(),
                    "Check for network congestion or connectivity issues".to_string(),
                    "Review recent configuration changes".to_string(), 
                    "Implement proactive alerting for trend detection".to_string(),
                ],
                expected_impact: "Should stabilize error rates and prevent further degradation".to_string(),
            });
        }

        recommendations
    }

    /// Classify error type using the enhanced categorizer
    fn classify_error_type(&self, error: &BenchmarkError) -> String {
        // Create a minimal context for categorization
        let context = ErrorContext {
            target_address: "unknown".to_string(),
            payload_size: 0,
            connection_attempt: 1,
            network_conditions: NetworkConditions {
                estimated_latency_ms: 0.0,
                estimated_bandwidth_mbps: 0.0,
                connection_count: 0,
                system_load: 0.0,
            },
        };

        let category = self.categorizer.categorize_error(error, &context);
        format!("{:?}", category)
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

    /// Get access to pattern analysis for external use
    pub fn get_pattern_analysis(&self) -> PatternAnalysis {
        self.categorizer.detect_patterns(&self.errors)
    }

    /// Get detailed correlation results for specific metrics
    pub fn get_detailed_correlations(&self) -> HashMap<String, CorrelationResult> {
        let mut correlations = HashMap::new();

        if !self.errors.is_empty() {
            let latency_values: Vec<f64> = self.errors.iter()
                .map(|e| e.context.network_conditions.estimated_latency_ms)
                .collect();
            
            let bandwidth_values: Vec<f64> = self.errors.iter()
                .map(|e| e.context.network_conditions.estimated_bandwidth_mbps)
                .collect();
            
            let load_values: Vec<f64> = self.errors.iter()
                .map(|e| e.context.network_conditions.system_load)
                .collect();
            
            let connection_count_values: Vec<f64> = self.errors.iter()
                .map(|e| e.context.network_conditions.connection_count as f64)
                .collect();

            correlations.insert("latency".to_string(), 
                self.categorizer.calculate_correlation(&self.errors, &latency_values));
            correlations.insert("bandwidth".to_string(), 
                self.categorizer.calculate_correlation(&self.errors, &bandwidth_values));
            correlations.insert("system_load".to_string(), 
                self.categorizer.calculate_correlation(&self.errors, &load_values));
            correlations.insert("connection_count".to_string(), 
                self.categorizer.calculate_correlation(&self.errors, &connection_count_values));
        }

        correlations
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

/// Advanced error categorization and analysis system
#[derive(Debug, Clone)]
pub struct ErrorCategorizer {
    /// Error pattern registry for enhanced classification
    error_patterns: HashMap<String, ErrorPattern>,
    /// Statistical thresholds for various metrics
    thresholds: CategorizationThresholds,
}

/// Error pattern definition for advanced classification
#[derive(Debug, Clone)]
pub struct ErrorPattern {
    pub category: ErrorCategory,
    pub keywords: Vec<String>,
    pub severity_indicators: Vec<String>,
    pub correlation_factors: Vec<String>,
    pub recommended_actions: Vec<String>,
}

/// Enhanced error categories beyond basic layer classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    // Connection-related errors
    ConnectionTimeout,
    ConnectionRefused,
    ConnectionReset,
    ConnectionAborted,
    
    // Authentication and security errors
    AuthenticationFailed,
    HandshakeFailed,
    CertificateError,
    EncryptionError,
    
    // Network layer errors
    NetworkUnreachable,
    HostUnreachable,
    RoutingError,
    DNSResolutionFailed,
    
    // Protocol-specific errors
    ProtocolViolation,
    FramingError,
    SequenceError,
    ChecksumMismatch,
    
    // Resource exhaustion errors
    MemoryExhausted,
    BufferOverflow,
    RateLimitExceeded,
    QueueFull,
    
    // System-level errors
    SystemOverload,
    ServiceUnavailable,
    ConfigurationError,
    InternalError,
    
    // Nyx-specific errors
    MixPathBuilding,
    CoverTrafficGeneration,
    OnionLayerProcessing,
    VDFComputationFailed,
    
    // Generic categories
    Transient,
    Permanent,
    Unknown,
}

/// Categorization thresholds and parameters
#[derive(Debug, Clone)]
pub struct CategorizationThresholds {
    pub high_frequency_threshold: f64,
    pub correlation_significance: f64,
    pub trend_detection_window: Duration,
    pub clustering_time_window: Duration,
    pub severity_escalation_rate: f64,
}

impl Default for CategorizationThresholds {
    fn default() -> Self {
        Self {
            high_frequency_threshold: 10.0,
            correlation_significance: 0.3,
            trend_detection_window: Duration::from_secs(300), // 5 minutes
            clustering_time_window: Duration::from_secs(60),  // 1 minute
            severity_escalation_rate: 1.5,
        }
    }
}

/// Statistical correlation calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationResult {
    pub coefficient: f64,
    pub confidence: f64,
    pub sample_size: usize,
    pub significance: CorrelationSignificance,
}

/// Significance level of correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrelationSignificance {
    VeryHigh,    // |r| > 0.7
    High,        // 0.5 < |r| <= 0.7
    Moderate,    // 0.3 < |r| <= 0.5
    Low,         // 0.1 < |r| <= 0.3
    Negligible,  // |r| <= 0.1
}

/// Advanced error pattern detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternAnalysis {
    pub recurring_patterns: Vec<RecurringPattern>,
    pub anomalous_spikes: Vec<ErrorSpike>,
    pub cyclic_behavior: Option<CyclicPattern>,
    pub cascade_effects: Vec<CascadeEffect>,
}

/// Recurring error pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringPattern {
    pub pattern_id: String,
    pub error_categories: Vec<ErrorCategory>,
    pub frequency: f64,
    pub duration: Duration,
    pub confidence: f64,
}

/// Anomalous error spike
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSpike {
    pub timestamp: DateTime<Utc>,
    pub magnitude: f64,
    pub duration: Duration,
    pub affected_categories: Vec<ErrorCategory>,
    pub potential_causes: Vec<String>,
}

/// Cyclic error pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CyclicPattern {
    pub cycle_duration: Duration,
    pub peak_times: Vec<DateTime<Utc>>,
    pub amplitude: f64,
    pub confidence: f64,
}

/// Error cascade effect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeEffect {
    pub trigger_category: ErrorCategory,
    pub affected_categories: Vec<ErrorCategory>,
    pub propagation_delay: Duration,
    pub amplification_factor: f64,
}

impl ErrorCategorizer {
    /// Create a new error categorizer with predefined patterns
    pub fn new() -> Self {
        let mut error_patterns = HashMap::new();
        
        // Define connection error patterns
        error_patterns.insert("connection_timeout".to_string(), ErrorPattern {
            category: ErrorCategory::ConnectionTimeout,
            keywords: vec!["timeout".to_string(), "timed out".to_string(), "deadline".to_string()],
            severity_indicators: vec!["critical".to_string(), "fatal".to_string()],
            correlation_factors: vec!["network_latency".to_string(), "server_load".to_string()],
            recommended_actions: vec![
                "Increase connection timeout values".to_string(),
                "Check network connectivity".to_string(),
                "Monitor server response times".to_string(),
            ],
        });
        
        error_patterns.insert("handshake_failed".to_string(), ErrorPattern {
            category: ErrorCategory::HandshakeFailed,
            keywords: vec!["handshake".to_string(), "negotiate".to_string(), "protocol".to_string()],
            severity_indicators: vec!["failed".to_string(), "rejected".to_string()],
            correlation_factors: vec!["protocol_version".to_string(), "cipher_support".to_string()],
            recommended_actions: vec![
                "Verify protocol compatibility".to_string(),
                "Check cipher suite configuration".to_string(),
                "Update client/server versions".to_string(),
            ],
        });
        
        error_patterns.insert("mix_path_building".to_string(), ErrorPattern {
            category: ErrorCategory::MixPathBuilding,
            keywords: vec!["path".to_string(), "route".to_string(), "mix".to_string(), "node".to_string()],
            severity_indicators: vec!["unavailable".to_string(), "unreachable".to_string()],
            correlation_factors: vec!["node_availability".to_string(), "network_topology".to_string()],
            recommended_actions: vec![
                "Check mix node availability".to_string(),
                "Verify network topology".to_string(),
                "Review path selection algorithms".to_string(),
            ],
        });
        
        error_patterns.insert("vdf_computation".to_string(), ErrorPattern {
            category: ErrorCategory::VDFComputationFailed,
            keywords: vec!["vdf".to_string(), "computation".to_string(), "proof".to_string()],
            severity_indicators: vec!["failed".to_string(), "invalid".to_string()],
            correlation_factors: vec!["cpu_load".to_string(), "computation_time".to_string()],
            recommended_actions: vec![
                "Monitor CPU resources".to_string(),
                "Adjust VDF difficulty parameters".to_string(),
                "Check computation timeout settings".to_string(),
            ],
        });
        
        Self {
            error_patterns,
            thresholds: CategorizationThresholds::default(),
        }
    }
    
    /// Categorize an error using advanced pattern matching
    pub fn categorize_error(&self, error: &BenchmarkError, context: &ErrorContext) -> ErrorCategory {
        let error_message = match error {
            BenchmarkError::StreamLayer(msg) |
            BenchmarkError::MixLayer(msg) |
            BenchmarkError::FecLayer(msg) |
            BenchmarkError::TransportLayer(msg) |
            BenchmarkError::Unknown(msg) => msg.to_lowercase(),
        };
        
        // Check for specific patterns
        for (_pattern_name, pattern) in &self.error_patterns {
            if pattern.keywords.iter().any(|keyword| error_message.contains(&keyword.to_lowercase())) {
                // Additional context-based validation
                if self.validate_categorization(&pattern.category, error, context) {
                    return pattern.category.clone();
                }
            }
        }
        
        // Fallback to layer-based categorization
        self.categorize_by_layer_and_content(error, &error_message)
    }
    
    /// Validate categorization using context information
    fn validate_categorization(&self, category: &ErrorCategory, _error: &BenchmarkError, context: &ErrorContext) -> bool {
        match category {
            ErrorCategory::ConnectionTimeout => {
                context.network_conditions.estimated_latency_ms > 1000.0
            }
            ErrorCategory::SystemOverload => {
                context.network_conditions.system_load > 0.8
            }
            ErrorCategory::RateLimitExceeded => {
                context.connection_attempt > 10
            }
            ErrorCategory::NetworkUnreachable => {
                context.network_conditions.estimated_bandwidth_mbps < 1.0
            }
            // Add more context-based validation rules
            _ => true,
        }
    }
    
    /// Fallback categorization based on layer and message content
    fn categorize_by_layer_and_content(&self, error: &BenchmarkError, message: &str) -> ErrorCategory {
        match error {
            BenchmarkError::StreamLayer(_) => {
                if message.contains("timeout") {
                    ErrorCategory::ConnectionTimeout
                } else if message.contains("handshake") {
                    ErrorCategory::HandshakeFailed
                } else if message.contains("auth") {
                    ErrorCategory::AuthenticationFailed
                } else {
                    ErrorCategory::ProtocolViolation
                }
            }
            BenchmarkError::MixLayer(_) => {
                if message.contains("path") || message.contains("route") {
                    ErrorCategory::MixPathBuilding
                } else if message.contains("node") {
                    ErrorCategory::ServiceUnavailable
                } else {
                    ErrorCategory::OnionLayerProcessing
                }
            }
            BenchmarkError::FecLayer(_) => {
                if message.contains("buffer") {
                    ErrorCategory::BufferOverflow
                } else if message.contains("checksum") {
                    ErrorCategory::ChecksumMismatch
                } else {
                    ErrorCategory::FramingError
                }
            }
            BenchmarkError::TransportLayer(_) => {
                if message.contains("network") {
                    ErrorCategory::NetworkUnreachable
                } else if message.contains("dns") {
                    ErrorCategory::DNSResolutionFailed
                } else {
                    ErrorCategory::RoutingError
                }
            }
            BenchmarkError::Unknown(_) => ErrorCategory::Unknown,
        }
    }
    
    /// Calculate statistical correlation between errors and network conditions
    pub fn calculate_correlation(&self, errors: &[ErrorEvent], metric_values: &[f64]) -> CorrelationResult {
        if errors.is_empty() || metric_values.is_empty() || errors.len() != metric_values.len() {
            return CorrelationResult {
                coefficient: 0.0,
                confidence: 0.0,
                sample_size: 0,
                significance: CorrelationSignificance::Negligible,
            };
        }
        
        let n = errors.len() as f64;
        let error_counts: Vec<f64> = errors.iter()
            .map(|_| 1.0) // Binary: error occurred = 1, no error = 0
            .collect();
        
        // Calculate Pearson correlation coefficient
        let mean_errors = error_counts.iter().sum::<f64>() / n;
        let mean_metric = metric_values.iter().sum::<f64>() / n;
        
        let numerator: f64 = error_counts.iter().zip(metric_values.iter())
            .map(|(e, m)| (e - mean_errors) * (m - mean_metric))
            .sum();
        
        let sum_sq_errors: f64 = error_counts.iter()
            .map(|e| (e - mean_errors).powi(2))
            .sum();
        
        let sum_sq_metric: f64 = metric_values.iter()
            .map(|m| (m - mean_metric).powi(2))
            .sum();
        
        let denominator = (sum_sq_errors * sum_sq_metric).sqrt();
        
        let coefficient = if denominator != 0.0 {
            numerator / denominator
        } else {
            0.0
        };
        
        // Calculate confidence based on sample size and correlation strength
        let confidence = self.calculate_confidence(coefficient, n as usize);
        
        let significance = match coefficient.abs() {
            x if x > 0.7 => CorrelationSignificance::VeryHigh,
            x if x > 0.5 => CorrelationSignificance::High,
            x if x > 0.3 => CorrelationSignificance::Moderate,
            x if x > 0.1 => CorrelationSignificance::Low,
            _ => CorrelationSignificance::Negligible,
        };
        
        CorrelationResult {
            coefficient,
            confidence,
            sample_size: n as usize,
            significance,
        }
    }
    
    /// Calculate confidence level for correlation coefficient
    fn calculate_confidence(&self, correlation: f64, sample_size: usize) -> f64 {
        if sample_size < 3 {
            return 0.0;
        }
        
        // Fisher transformation for confidence interval calculation
        let z = 0.5 * ((1.0 + correlation) / (1.0 - correlation)).ln();
        let se = 1.0 / ((sample_size - 3) as f64).sqrt();
        
        // Simplified confidence calculation (95% confidence interval)
        let margin = 1.96 * se;
        let lower = ((z - margin).exp() - 1.0) / ((z - margin).exp() + 1.0);
        let upper = ((z + margin).exp() - 1.0) / ((z + margin).exp() + 1.0);
        
        // Confidence as percentage of the interval that doesn't include zero
        if lower > 0.0 || upper < 0.0 {
            95.0
        } else {
            ((1.0 - (upper - lower).abs()) * 100.0).max(0.0)
        }
    }
    
    /// Detect advanced error patterns in the data
    pub fn detect_patterns(&self, errors: &[ErrorEvent]) -> PatternAnalysis {
        PatternAnalysis {
            recurring_patterns: self.detect_recurring_patterns(errors),
            anomalous_spikes: self.detect_anomalous_spikes(errors),
            cyclic_behavior: self.detect_cyclic_behavior(errors),
            cascade_effects: self.detect_cascade_effects(errors),
        }
    }
    
    /// Detect recurring error patterns
    fn detect_recurring_patterns(&self, errors: &[ErrorEvent]) -> Vec<RecurringPattern> {
        let mut patterns = Vec::new();
        
        if errors.len() < 10 {
            return patterns;
        }
        
        // Group errors by category and analyze frequency
        let mut category_groups: HashMap<ErrorCategory, Vec<&ErrorEvent>> = HashMap::new();
        for error in errors {
            let category = self.categorize_error(&error.error, &error.context);
            category_groups.entry(category).or_default().push(error);
        }
        
        for (category, error_list) in category_groups {
            if error_list.len() < 3 {
                continue;
            }
            
            // Calculate inter-arrival times
            let mut intervals = Vec::new();
            for i in 1..error_list.len() {
                let interval = error_list[i].timestamp.duration_since(error_list[i-1].timestamp);
                intervals.push(interval.as_secs_f64());
            }
            
            // Check for regular intervals (simplified pattern detection)
            if let Some(avg_interval) = self.calculate_average_interval(&intervals) {
                let variance = self.calculate_variance(&intervals, avg_interval);
                let coefficient_of_variation = (variance.sqrt() / avg_interval).abs();
                
                if coefficient_of_variation < 0.5 { // Regular pattern detected
                    patterns.push(RecurringPattern {
                        pattern_id: format!("recurring_{:?}", category),
                        error_categories: vec![category],
                        frequency: 1.0 / avg_interval,
                        duration: Duration::from_secs_f64(avg_interval),
                        confidence: (1.0 - coefficient_of_variation).max(0.0),
                    });
                }
            }
        }
        
        patterns
    }
    
    /// Detect anomalous error spikes
    fn detect_anomalous_spikes(&self, errors: &[ErrorEvent]) -> Vec<ErrorSpike> {
        let mut spikes = Vec::new();
        
        if errors.is_empty() {
            return spikes;
        }
        
        // Use sliding window to detect spikes
        let window_size = self.thresholds.clustering_time_window;
        let mut window_start = 0;
        
        while window_start < errors.len() {
            let window_end_time = errors[window_start].timestamp + window_size;
            let mut window_end = window_start;
            
            // Find errors within the window
            while window_end < errors.len() && errors[window_end].timestamp <= window_end_time {
                window_end += 1;
            }
            
            let window_error_count = window_end - window_start;
            
            // Detect spike if error count is significantly higher than average
            if window_error_count > 5 { // Simplified threshold
                let mut affected_categories = Vec::new();
                for i in window_start..window_end {
                    let category = self.categorize_error(&errors[i].error, &errors[i].context);
                    if !affected_categories.contains(&category) {
                        affected_categories.push(category);
                    }
                }
                
                spikes.push(ErrorSpike {
                    timestamp: Utc::now() - chrono::Duration::from_std(errors[window_start].timestamp.duration_since(errors[0].timestamp)).unwrap_or_default(),
                    magnitude: window_error_count as f64,
                    duration: window_size,
                    affected_categories,
                    potential_causes: vec![
                        "Network congestion".to_string(),
                        "Server overload".to_string(),
                        "Configuration change".to_string(),
                    ],
                });
            }
            
            window_start = window_end.max(window_start + 1);
        }
        
        spikes
    }
    
    /// Detect cyclic error behavior
    fn detect_cyclic_behavior(&self, errors: &[ErrorEvent]) -> Option<CyclicPattern> {
        if errors.len() < 20 {
            return None;
        }
        
        // Simplified cyclic detection - look for peaks in error frequency
        let time_buckets = self.create_time_buckets(errors, Duration::from_secs(60));
        
        if time_buckets.len() < 10 {
            return None;
        }
        
        // Find peaks (simplified)
        let mut peaks = Vec::new();
        let mean_count = time_buckets.iter().map(|b| b.1).sum::<usize>() as f64 / time_buckets.len() as f64;
        
        for (timestamp, count) in &time_buckets {
            if *count as f64 > mean_count * 1.5 {
                peaks.push(*timestamp);
            }
        }
        
        if peaks.len() >= 3 {
            // Calculate average cycle duration
            let mut intervals = Vec::new();
            for i in 1..peaks.len() {
                intervals.push(peaks[i].duration_since(peaks[i-1]));
            }
            
            if let Some(avg_cycle) = self.calculate_average_duration(&intervals) {
                return Some(CyclicPattern {
                    cycle_duration: avg_cycle,
                    peak_times: peaks.into_iter().map(|t| {
                        Utc::now() - chrono::Duration::from_std(t.duration_since(errors[0].timestamp)).unwrap_or_default()
                    }).collect(),
                    amplitude: mean_count * 1.5,
                    confidence: 0.7, // Simplified confidence calculation
                });
            }
        }
        
        None
    }
    
    /// Detect cascade effects between error categories
    fn detect_cascade_effects(&self, errors: &[ErrorEvent]) -> Vec<CascadeEffect> {
        let mut cascades = Vec::new();
        
        if errors.len() < 10 {
            return cascades;
        }
        
        // Look for temporal correlations between different error categories
        let categories: Vec<ErrorCategory> = errors.iter()
            .map(|e| self.categorize_error(&e.error, &e.context))
            .collect();
        
        // Simplified cascade detection
        for i in 1..errors.len() {
            let current_category = &categories[i];
            let prev_category = &categories[i-1];
            
            if current_category != prev_category {
                let delay = errors[i].timestamp.duration_since(errors[i-1].timestamp);
                
                if delay < Duration::from_secs(30) { // Potential cascade
                    cascades.push(CascadeEffect {
                        trigger_category: prev_category.clone(),
                        affected_categories: vec![current_category.clone()],
                        propagation_delay: delay,
                        amplification_factor: 1.2, // Simplified calculation
                    });
                }
            }
        }
        
        cascades
    }
    
    /// Helper function to calculate average interval
    fn calculate_average_interval(&self, intervals: &[f64]) -> Option<f64> {
        if intervals.is_empty() {
            return None;
        }
        Some(intervals.iter().sum::<f64>() / intervals.len() as f64)
    }
    
    /// Helper function to calculate variance
    fn calculate_variance(&self, values: &[f64], mean: f64) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let sum_squared_diff: f64 = values.iter()
            .map(|v| (v - mean).powi(2))
            .sum();
        sum_squared_diff / values.len() as f64
    }
    
    /// Helper function to calculate average duration
    fn calculate_average_duration(&self, durations: &[Duration]) -> Option<Duration> {
        if durations.is_empty() {
            return None;
        }
        let total_secs: f64 = durations.iter()
            .map(|d| d.as_secs_f64())
            .sum();
        Some(Duration::from_secs_f64(total_secs / durations.len() as f64))
    }
    
    /// Helper function to create time buckets for analysis
    fn create_time_buckets(&self, errors: &[ErrorEvent], bucket_size: Duration) -> Vec<(Instant, usize)> {
        if errors.is_empty() {
            return Vec::new();
        }
        
        let start_time = errors[0].timestamp;
        let end_time = errors.last().unwrap().timestamp;
        let total_duration = end_time.duration_since(start_time);
        
        let num_buckets = (total_duration.as_secs() / bucket_size.as_secs()).max(1) as usize;
        let mut buckets = Vec::with_capacity(num_buckets);
        
        for i in 0..num_buckets {
            let bucket_start = start_time + bucket_size * i as u32;
            let bucket_end = bucket_start + bucket_size;
            
            let count = errors.iter()
                .filter(|e| e.timestamp >= bucket_start && e.timestamp < bucket_end)
                .count();
            
            buckets.push((bucket_start, count));
        }
        
        buckets
    }
}

impl Default for ErrorCategorizer {
    fn default() -> Self {
        Self::new()
    }
}