use anyhow::Result;
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

use crate::benchmark::LayerMetrics;
use crate::statistics_renderer::StatisticsData;

/// Performance analysis configuration
#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub trend_window_size: usize,
    pub alert_thresholds: AlertThresholds,
    pub analysis_interval: Duration,
    pub enable_predictive_analysis: bool,
    pub enable_anomaly_detection: bool,
}

/// Configurable alert thresholds
#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub max_latency_ms: f64,
    pub min_success_rate: f64,
    pub max_error_rate: f64,
    pub min_throughput_mbps: f64,
    pub max_cpu_usage: f64,
    pub max_memory_usage_mb: f64,
    pub min_connection_health: f64,
}

/// Performance analysis results
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub timestamp: DateTime<Utc>,
    pub overall_health: HealthAssessment,
    pub trend_analysis: TrendAnalysis,
    pub alerts: Vec<PerformanceAlert>,
    pub recommendations: Vec<Recommendation>,
    pub predictions: Option<PerformancePrediction>,
    pub anomalies: Vec<AnomalyDetection>,
    pub system_assessment: SystemAssessment,
}

/// Overall system health assessment
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthAssessment {
    pub overall_score: f64, // 0.0-1.0
    pub status: HealthStatus,
    pub critical_issues: u32,
    pub warnings: u32,
    pub component_health: ComponentHealth,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HealthStatus {
    Excellent,
    Good,
    Fair,
    Poor,
    Critical,
}

/// Health assessment for different system components
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub network_layer: f64,
    pub stream_layer: f64,
    pub mix_layer: f64,
    pub fec_layer: f64,
    pub transport_layer: f64,
    pub daemon_health: f64,
}

/// Trend analysis over time
#[derive(Debug, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub latency_trend: TrendDirection,
    pub throughput_trend: TrendDirection,
    pub error_rate_trend: TrendDirection,
    pub connection_trend: TrendDirection,
    pub performance_degradation: Option<DegradationPattern>,
    pub improvement_areas: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TrendDirection {
    Improving,
    Stable,
    Degrading,
    Volatile,
}

/// Performance degradation pattern detection
#[derive(Debug, Serialize, Deserialize)]
pub struct DegradationPattern {
    pub pattern_type: DegradationType,
    pub severity: f64, // 0.0-1.0
    pub affected_components: Vec<String>,
    pub estimated_impact: String,
    pub time_detected: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum DegradationType {
    GradualDegradation,
    SuddenDrop,
    PeriodicSpikes,
    ResourceExhaustion,
    NetworkCongestion,
}

/// Performance alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAlert {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub affected_component: String,
    pub current_value: f64,
    pub threshold_value: f64,
    pub timestamp: DateTime<Utc>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    HighLatency,
    LowThroughput,
    HighErrorRate,
    LowSuccessRate,
    ResourceExhaustion,
    ConnectionIssues,
    SystemOverload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Performance recommendation
#[derive(Debug, Serialize, Deserialize)]
pub struct Recommendation {
    pub category: RecommendationCategory,
    pub priority: RecommendationPriority,
    pub title: String,
    pub description: String,
    pub expected_impact: String,
    pub implementation_effort: ImplementationEffort,
    pub specific_actions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RecommendationCategory {
    Configuration,
    Infrastructure,
    NetworkOptimization,
    ResourceManagement,
    Monitoring,
    Security,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ImplementationEffort {
    Low,
    Medium,
    High,
}

/// Performance prediction based on trends
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformancePrediction {
    pub prediction_horizon: ChronoDuration,
    pub predicted_latency_ms: f64,
    pub predicted_throughput_mbps: f64,
    pub predicted_error_rate: f64,
    pub confidence_level: f64, // 0.0-1.0
    pub potential_issues: Vec<String>,
}

/// Anomaly detection result
#[derive(Debug, Serialize, Deserialize)]
pub struct AnomalyDetection {
    pub anomaly_type: AnomalyType,
    pub metric_name: String,
    pub expected_value: f64,
    pub actual_value: f64,
    pub deviation_score: f64, // How far from normal (standard deviations)
    pub timestamp: DateTime<Utc>,
    pub context: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AnomalyType {
    Spike,
    Drop,
    Plateau,
    Oscillation,
}

/// System-wide assessment
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemAssessment {
    pub capacity_utilization: f64, // 0.0-1.0
    pub bottleneck_analysis: Vec<Bottleneck>,
    pub scalability_assessment: ScalabilityAssessment,
    pub reliability_metrics: ReliabilityMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bottleneck {
    pub component: String,
    pub utilization: f64,
    pub impact_score: f64,
    pub resolution_suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScalabilityAssessment {
    pub current_load_factor: f64,
    pub estimated_max_capacity: f64,
    pub scaling_recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReliabilityMetrics {
    pub mtbf_hours: f64, // Mean Time Between Failures
    pub mttr_minutes: f64, // Mean Time To Recovery
    pub availability_percentage: f64,
    pub error_budget_remaining: f64,
}

/// Main performance analyzer
pub struct PerformanceAnalyzer {
    config: AnalysisConfig,
    historical_data: VecDeque<StatisticsData>,
    baseline_metrics: Option<BaselineMetrics>,
    alert_history: VecDeque<PerformanceAlert>,
}

/// Baseline metrics for comparison
#[derive(Debug, Clone)]
struct BaselineMetrics {
    avg_latency_ms: f64,
    avg_throughput_mbps: f64,
    avg_success_rate: f64,
    avg_error_rate: f64,
    established_at: DateTime<Utc>,
}

impl PerformanceAnalyzer {
    /// Create a new performance analyzer
    pub fn new(config: AnalysisConfig) -> Self {
        let trend_window_size = config.trend_window_size;
        Self {
            config,
            historical_data: VecDeque::with_capacity(trend_window_size),
            baseline_metrics: None,
            alert_history: VecDeque::with_capacity(100), // Keep last 100 alerts
        }
    }

    /// Add new statistics data for analysis
    pub fn add_data_point(&mut self, data: StatisticsData) {
        if self.historical_data.len() >= self.config.trend_window_size {
            self.historical_data.pop_front();
        }
        self.historical_data.push_back(data);

        // Establish baseline if we have enough data and no baseline exists
        if self.baseline_metrics.is_none() && self.historical_data.len() >= 10 {
            self.establish_baseline();
        }
    }

    /// Perform comprehensive performance analysis
    pub fn analyze(&mut self) -> Result<PerformanceAnalysis> {
        if self.historical_data.is_empty() {
            return Err(anyhow::anyhow!("No data available for analysis"));
        }

        let current_data = self.historical_data.back().unwrap();
        
        let overall_health = self.assess_overall_health(current_data);
        let trend_analysis = self.analyze_trends();
        let alerts = self.generate_alerts(current_data);
        let recommendations = self.generate_recommendations(&overall_health, &trend_analysis, &alerts);
        let predictions = if self.config.enable_predictive_analysis {
            Some(self.predict_performance())
        } else {
            None
        };
        let anomalies = if self.config.enable_anomaly_detection {
            self.detect_anomalies(current_data)
        } else {
            Vec::new()
        };
        let system_assessment = self.assess_system_performance(current_data);

        // Store alerts in history
        for alert in &alerts {
            if self.alert_history.len() >= 100 {
                self.alert_history.pop_front();
            }
            self.alert_history.push_back(alert.clone());
        }

        Ok(PerformanceAnalysis {
            timestamp: Utc::now(),
            overall_health,
            trend_analysis,
            alerts,
            recommendations,
            predictions,
            anomalies,
            system_assessment,
        })
    }

    /// Assess overall system health
    fn assess_overall_health(&self, data: &StatisticsData) -> HealthAssessment {
        let mut score: f64 = 1.0;
        let mut critical_issues = 0;
        let mut warnings = 0;

        // Latency assessment
        if data.summary.avg_latency_ms > self.config.alert_thresholds.max_latency_ms {
            score -= 0.3;
            if data.summary.avg_latency_ms > self.config.alert_thresholds.max_latency_ms * 2.0 {
                critical_issues += 1;
            } else {
                warnings += 1;
            }
        }

        // Success rate assessment
        if data.summary.success_rate < self.config.alert_thresholds.min_success_rate {
            score -= 0.4;
            if data.summary.success_rate < self.config.alert_thresholds.min_success_rate * 0.9 {
                critical_issues += 1;
            } else {
                warnings += 1;
            }
        }

        // Error rate assessment
        let error_rate = if data.summary.total_requests > 0 {
            (data.summary.failed_requests as f64 / data.summary.total_requests as f64) * 100.0
        } else {
            0.0
        };

        if error_rate > self.config.alert_thresholds.max_error_rate {
            score -= 0.2;
            warnings += 1;
        }

        // Throughput assessment
        if data.summary.throughput_mbps < self.config.alert_thresholds.min_throughput_mbps {
            score -= 0.1;
            warnings += 1;
        }

        score = score.max(0.0);

        let status = match score {
            s if s >= 0.9 => HealthStatus::Excellent,
            s if s >= 0.7 => HealthStatus::Good,
            s if s >= 0.5 => HealthStatus::Fair,
            s if s >= 0.3 => HealthStatus::Poor,
            _ => HealthStatus::Critical,
        };

        let component_health = self.assess_component_health(&data.layer_metrics);

        HealthAssessment {
            overall_score: score,
            status,
            critical_issues,
            warnings,
            component_health,
        }
    }

    /// Assess health of individual components
    fn assess_component_health(&self, layer_metrics: &LayerMetrics) -> ComponentHealth {
        let layers = [
            &layer_metrics.stream_layer,
            &layer_metrics.mix_layer,
            &layer_metrics.fec_layer,
            &layer_metrics.transport_layer,
        ];

        let layer_scores: Vec<f64> = layers.iter().map(|layer| {
            let mut score: f64 = 1.0;
            
            // Latency impact
            if layer.latency_ms > 100.0 {
                score -= 0.3;
            } else if layer.latency_ms > 50.0 {
                score -= 0.1;
            }
            
            // Success rate impact
            if layer.success_rate < 95.0 {
                score -= 0.4;
            } else if layer.success_rate < 99.0 {
                score -= 0.2;
            }
            
            // Error count impact
            if layer.error_count > 10 {
                score -= 0.3;
            } else if layer.error_count > 0 {
                score -= 0.1;
            }
            
            score.max(0.0)
        }).collect();

        ComponentHealth {
            network_layer: (layer_scores.iter().sum::<f64>() / layer_scores.len() as f64),
            stream_layer: layer_scores[0],
            mix_layer: layer_scores[1],
            fec_layer: layer_scores[2],
            transport_layer: layer_scores[3],
            daemon_health: 0.95, // Placeholder - would be derived from daemon metrics
        }
    }

    /// Analyze performance trends
    fn analyze_trends(&self) -> TrendAnalysis {
        if self.historical_data.len() < 3 {
            return TrendAnalysis {
                latency_trend: TrendDirection::Stable,
                throughput_trend: TrendDirection::Stable,
                error_rate_trend: TrendDirection::Stable,
                connection_trend: TrendDirection::Stable,
                performance_degradation: None,
                improvement_areas: Vec::new(),
            };
        }

        let latency_trend = self.analyze_metric_trend(|data| data.summary.avg_latency_ms);
        let throughput_trend = self.analyze_metric_trend(|data| data.summary.throughput_mbps);
        let error_rate_trend = self.analyze_metric_trend(|data| {
            if data.summary.total_requests > 0 {
                (data.summary.failed_requests as f64 / data.summary.total_requests as f64) * 100.0
            } else {
                0.0
            }
        });
        let connection_trend = self.analyze_metric_trend(|data| data.summary.active_connections as f64);

        let performance_degradation = self.detect_degradation_pattern();
        let improvement_areas = self.identify_improvement_areas();

        TrendAnalysis {
            latency_trend,
            throughput_trend,
            error_rate_trend,
            connection_trend,
            performance_degradation,
            improvement_areas,
        }
    }

    /// Analyze trend for a specific metric
    fn analyze_metric_trend<F>(&self, metric_extractor: F) -> TrendDirection
    where
        F: Fn(&StatisticsData) -> f64,
    {
        let values: Vec<f64> = self.historical_data.iter().map(metric_extractor).collect();
        
        if values.len() < 3 {
            return TrendDirection::Stable;
        }

        // Simple linear regression to determine trend
        let n = values.len() as f64;
        let x_sum: f64 = (0..values.len()).map(|i| i as f64).sum();
        let y_sum: f64 = values.iter().sum();
        let xy_sum: f64 = values.iter().enumerate().map(|(i, &y)| i as f64 * y).sum();
        let x_squared_sum: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();

        let slope = (n * xy_sum - x_sum * y_sum) / (n * x_squared_sum - x_sum.powi(2));
        
        // Calculate volatility
        let mean = y_sum / n;
        let variance = values.iter().map(|&y| (y - mean).powi(2)).sum::<f64>() / n;
        let std_dev = variance.sqrt();
        let coefficient_of_variation = if mean != 0.0 { std_dev / mean.abs() } else { 0.0 };

        if coefficient_of_variation > 0.3 {
            TrendDirection::Volatile
        } else if slope.abs() < 0.01 {
            TrendDirection::Stable
        } else if slope > 0.0 {
            TrendDirection::Improving // For throughput, higher is better
        } else {
            TrendDirection::Degrading
        }
    }

    /// Detect performance degradation patterns
    fn detect_degradation_pattern(&self) -> Option<DegradationPattern> {
        if self.historical_data.len() < 5 {
            return None;
        }

        // Check for gradual degradation in latency
        let recent_latencies: Vec<f64> = self.historical_data
            .iter()
            .rev()
            .take(5)
            .map(|data| data.summary.avg_latency_ms)
            .collect();

        let is_gradual_degradation = recent_latencies.windows(2)
            .all(|window| window[0] >= window[1]); // Each value is worse than the previous

        if is_gradual_degradation {
            let severity = (recent_latencies[0] - recent_latencies[4]) / recent_latencies[4];
            
            if severity > 0.2 {
                return Some(DegradationPattern {
                    pattern_type: DegradationType::GradualDegradation,
                    severity: severity.min(1.0),
                    affected_components: vec!["Latency".to_string()],
                    estimated_impact: format!("Latency increased by {:.1}%", severity * 100.0),
                    time_detected: Utc::now(),
                });
            }
        }

        None
    }

    /// Identify areas for improvement
    fn identify_improvement_areas(&self) -> Vec<String> {
        let mut areas = Vec::new();
        
        if let Some(current_data) = self.historical_data.back() {
            // Check each layer for improvement opportunities
            if current_data.layer_metrics.stream_layer.latency_ms > 50.0 {
                areas.push("Stream layer latency optimization".to_string());
            }
            
            if current_data.layer_metrics.mix_layer.error_count > 0 {
                areas.push("Mix routing reliability improvement".to_string());
            }
            
            if current_data.summary.throughput_mbps < 5.0 {
                areas.push("Throughput optimization".to_string());
            }
            
            if current_data.real_time_metrics.connection_health.overall_health_score < 0.8 {
                areas.push("Connection stability enhancement".to_string());
            }
        }

        areas
    }

    /// Generate performance alerts
    fn generate_alerts(&self, data: &StatisticsData) -> Vec<PerformanceAlert> {
        let mut alerts = Vec::new();

        // High latency alert
        if data.summary.avg_latency_ms > self.config.alert_thresholds.max_latency_ms {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::HighLatency,
                severity: if data.summary.avg_latency_ms > self.config.alert_thresholds.max_latency_ms * 2.0 {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::Warning
                },
                message: format!("Average latency ({:.2}ms) exceeds threshold ({:.2}ms)", 
                    data.summary.avg_latency_ms, self.config.alert_thresholds.max_latency_ms),
                affected_component: "Network".to_string(),
                current_value: data.summary.avg_latency_ms,
                threshold_value: self.config.alert_thresholds.max_latency_ms,
                timestamp: Utc::now(),
                suggested_actions: vec![
                    "Check network connectivity".to_string(),
                    "Analyze mix routing paths".to_string(),
                    "Review daemon configuration".to_string(),
                ],
            });
        }

        // Low success rate alert
        if data.summary.success_rate < self.config.alert_thresholds.min_success_rate {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::LowSuccessRate,
                severity: if data.summary.success_rate < self.config.alert_thresholds.min_success_rate * 0.9 {
                    AlertSeverity::Critical
                } else {
                    AlertSeverity::Warning
                },
                message: format!("Success rate ({:.2}%) below threshold ({:.2}%)", 
                    data.summary.success_rate, self.config.alert_thresholds.min_success_rate),
                affected_component: "System".to_string(),
                current_value: data.summary.success_rate,
                threshold_value: self.config.alert_thresholds.min_success_rate,
                timestamp: Utc::now(),
                suggested_actions: vec![
                    "Investigate error patterns".to_string(),
                    "Check daemon logs".to_string(),
                    "Verify network stability".to_string(),
                ],
            });
        }

        // Low throughput alert
        if data.summary.throughput_mbps < self.config.alert_thresholds.min_throughput_mbps {
            alerts.push(PerformanceAlert {
                alert_type: AlertType::LowThroughput,
                severity: AlertSeverity::Warning,
                message: format!("Throughput ({:.2} Mbps) below threshold ({:.2} Mbps)", 
                    data.summary.throughput_mbps, self.config.alert_thresholds.min_throughput_mbps),
                affected_component: "Network".to_string(),
                current_value: data.summary.throughput_mbps,
                threshold_value: self.config.alert_thresholds.min_throughput_mbps,
                timestamp: Utc::now(),
                suggested_actions: vec![
                    "Optimize buffer sizes".to_string(),
                    "Check bandwidth limitations".to_string(),
                    "Review FEC configuration".to_string(),
                ],
            });
        }

        alerts
    }

    /// Generate performance recommendations
    fn generate_recommendations(
        &self,
        health: &HealthAssessment,
        trends: &TrendAnalysis,
        alerts: &[PerformanceAlert],
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Critical health recommendations
        if health.overall_score < 0.5 {
            recommendations.push(Recommendation {
                category: RecommendationCategory::Infrastructure,
                priority: RecommendationPriority::Critical,
                title: "System Health Critical".to_string(),
                description: "Overall system health is critically low. Immediate action required.".to_string(),
                expected_impact: "Prevent system failure and restore normal operation".to_string(),
                implementation_effort: ImplementationEffort::High,
                specific_actions: vec![
                    "Review all active alerts".to_string(),
                    "Check daemon status and logs".to_string(),
                    "Verify network connectivity".to_string(),
                    "Consider scaling resources".to_string(),
                ],
            });
        }

        // Trend-based recommendations
        match trends.latency_trend {
            TrendDirection::Degrading => {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::NetworkOptimization,
                    priority: RecommendationPriority::High,
                    title: "Address Latency Degradation".to_string(),
                    description: "Latency is showing a degrading trend over time".to_string(),
                    expected_impact: "Improve response times and user experience".to_string(),
                    implementation_effort: ImplementationEffort::Medium,
                    specific_actions: vec![
                        "Analyze network paths for bottlenecks".to_string(),
                        "Optimize mix routing configuration".to_string(),
                        "Consider geographic distribution of nodes".to_string(),
                    ],
                });
            }
            TrendDirection::Volatile => {
                recommendations.push(Recommendation {
                    category: RecommendationCategory::Configuration,
                    priority: RecommendationPriority::Medium,
                    title: "Stabilize Latency Variance".to_string(),
                    description: "Latency shows high volatility indicating instability".to_string(),
                    expected_impact: "More predictable performance".to_string(),
                    implementation_effort: ImplementationEffort::Low,
                    specific_actions: vec![
                        "Review connection pooling settings".to_string(),
                        "Adjust timeout configurations".to_string(),
                        "Enable connection keep-alive".to_string(),
                    ],
                });
            }
            _ => {}
        }

        // Alert-based recommendations
        for alert in alerts {
            match alert.alert_type {
                AlertType::HighLatency => {
                    recommendations.push(Recommendation {
                        category: RecommendationCategory::NetworkOptimization,
                        priority: RecommendationPriority::High,
                        title: "Optimize Network Latency".to_string(),
                        description: "Current latency exceeds acceptable thresholds".to_string(),
                        expected_impact: format!("Reduce latency from {:.2}ms to target {:.2}ms", 
                            alert.current_value, alert.threshold_value),
                        implementation_effort: ImplementationEffort::Medium,
                        specific_actions: alert.suggested_actions.clone(),
                    });
                }
                AlertType::LowThroughput => {
                    recommendations.push(Recommendation {
                        category: RecommendationCategory::Configuration,
                        priority: RecommendationPriority::Medium,
                        title: "Improve Throughput Performance".to_string(),
                        description: "System throughput is below optimal levels".to_string(),
                        expected_impact: format!("Increase throughput from {:.2} to {:.2} Mbps", 
                            alert.current_value, alert.threshold_value),
                        implementation_effort: ImplementationEffort::Low,
                        specific_actions: alert.suggested_actions.clone(),
                    });
                }
                _ => {}
            }
        }

        recommendations
    }

    /// Predict future performance based on trends
    fn predict_performance(&self) -> PerformancePrediction {
        if self.historical_data.len() < 5 {
            return PerformancePrediction {
                prediction_horizon: ChronoDuration::hours(1),
                predicted_latency_ms: 0.0,
                predicted_throughput_mbps: 0.0,
                predicted_error_rate: 0.0,
                confidence_level: 0.0,
                potential_issues: vec!["Insufficient data for prediction".to_string()],
            };
        }

        // Simple linear extrapolation for prediction
        let current_data = self.historical_data.back().unwrap();
        let latency_values: Vec<f64> = self.historical_data.iter()
            .map(|data| data.summary.avg_latency_ms).collect();
        
        let predicted_latency = self.extrapolate_trend(&latency_values, 12); // 1 hour ahead
        let predicted_throughput = current_data.summary.throughput_mbps; // Assume stable
        let predicted_error_rate = if current_data.summary.total_requests > 0 {
            (current_data.summary.failed_requests as f64 / current_data.summary.total_requests as f64) * 100.0
        } else {
            0.0
        };

        let mut potential_issues = Vec::new();
        if predicted_latency > self.config.alert_thresholds.max_latency_ms {
            potential_issues.push("Latency may exceed thresholds".to_string());
        }

        PerformancePrediction {
            prediction_horizon: ChronoDuration::hours(1),
            predicted_latency_ms: predicted_latency,
            predicted_throughput_mbps: predicted_throughput,
            predicted_error_rate: predicted_error_rate,
            confidence_level: 0.7, // Simplified confidence calculation
            potential_issues,
        }
    }

    /// Detect performance anomalies
    fn detect_anomalies(&self, data: &StatisticsData) -> Vec<AnomalyDetection> {
        let mut anomalies = Vec::new();

        if let Some(baseline) = &self.baseline_metrics {
            // Check for latency anomalies
            let latency_deviation = (data.summary.avg_latency_ms - baseline.avg_latency_ms) / baseline.avg_latency_ms;
            if latency_deviation.abs() > 0.5 { // 50% deviation
                anomalies.push(AnomalyDetection {
                    anomaly_type: if latency_deviation > 0.0 { AnomalyType::Spike } else { AnomalyType::Drop },
                    metric_name: "Average Latency".to_string(),
                    expected_value: baseline.avg_latency_ms,
                    actual_value: data.summary.avg_latency_ms,
                    deviation_score: latency_deviation.abs(),
                    timestamp: Utc::now(),
                    context: "Significant deviation from baseline performance".to_string(),
                });
            }

            // Check for throughput anomalies
            let throughput_deviation = (data.summary.throughput_mbps - baseline.avg_throughput_mbps) / baseline.avg_throughput_mbps;
            if throughput_deviation.abs() > 0.3 { // 30% deviation
                anomalies.push(AnomalyDetection {
                    anomaly_type: if throughput_deviation > 0.0 { AnomalyType::Spike } else { AnomalyType::Drop },
                    metric_name: "Throughput".to_string(),
                    expected_value: baseline.avg_throughput_mbps,
                    actual_value: data.summary.throughput_mbps,
                    deviation_score: throughput_deviation.abs(),
                    timestamp: Utc::now(),
                    context: "Unusual throughput pattern detected".to_string(),
                });
            }
        }

        anomalies
    }

    /// Assess overall system performance
    fn assess_system_performance(&self, data: &StatisticsData) -> SystemAssessment {
        let capacity_utilization = self.calculate_capacity_utilization(data);
        let bottlenecks = self.identify_bottlenecks(&data.layer_metrics);
        let scalability_assessment = self.assess_scalability(data);
        let reliability_metrics = self.calculate_reliability_metrics(data);

        SystemAssessment {
            capacity_utilization,
            bottleneck_analysis: bottlenecks,
            scalability_assessment,
            reliability_metrics,
        }
    }

    /// Calculate system capacity utilization
    fn calculate_capacity_utilization(&self, data: &StatisticsData) -> f64 {
        // Simplified calculation based on active connections and throughput
        let connection_utilization = (data.summary.active_connections as f64) / 1000.0; // Assume max 1000 connections
        let throughput_utilization = data.summary.throughput_mbps / 100.0; // Assume max 100 Mbps
        
        (connection_utilization + throughput_utilization) / 2.0
    }

    /// Identify system bottlenecks
    fn identify_bottlenecks(&self, layer_metrics: &LayerMetrics) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        let layers = [
            ("Stream Layer", &layer_metrics.stream_layer),
            ("Mix Layer", &layer_metrics.mix_layer),
            ("FEC Layer", &layer_metrics.fec_layer),
            ("Transport Layer", &layer_metrics.transport_layer),
        ];

        for (name, layer) in &layers {
            let utilization = (layer.latency_ms / 200.0).min(1.0); // Normalize to 0-1
            let impact_score = if layer.error_count > 0 {
                utilization * 1.5
            } else {
                utilization
            };

            if impact_score > 0.7 {
                bottlenecks.push(Bottleneck {
                    component: name.to_string(),
                    utilization,
                    impact_score,
                    resolution_suggestions: vec![
                        format!("Optimize {} configuration", name.to_lowercase()),
                        "Monitor resource usage".to_string(),
                        "Consider scaling resources".to_string(),
                    ],
                });
            }
        }

        bottlenecks
    }

    /// Assess system scalability
    fn assess_scalability(&self, data: &StatisticsData) -> ScalabilityAssessment {
        let current_load_factor = (data.summary.active_connections as f64) / 100.0; // Simplified
        let estimated_max_capacity = 1000.0; // Placeholder

        let scaling_recommendations = if current_load_factor > 0.8 {
            vec![
                "Consider horizontal scaling".to_string(),
                "Optimize resource allocation".to_string(),
                "Implement load balancing".to_string(),
            ]
        } else {
            vec!["Current capacity is sufficient".to_string()]
        };

        ScalabilityAssessment {
            current_load_factor,
            estimated_max_capacity,
            scaling_recommendations,
        }
    }

    /// Calculate reliability metrics
    fn calculate_reliability_metrics(&self, data: &StatisticsData) -> ReliabilityMetrics {
        let availability_percentage = data.summary.success_rate;
        let error_budget_remaining = (100.0 - availability_percentage) / 1.0; // 1% error budget

        ReliabilityMetrics {
            mtbf_hours: 24.0, // Placeholder
            mttr_minutes: 5.0, // Placeholder
            availability_percentage,
            error_budget_remaining,
        }
    }

    /// Establish baseline metrics from historical data
    fn establish_baseline(&mut self) {
        if self.historical_data.len() < 10 {
            return;
        }

        let avg_latency_ms = self.historical_data.iter()
            .map(|data| data.summary.avg_latency_ms)
            .sum::<f64>() / self.historical_data.len() as f64;

        let avg_throughput_mbps = self.historical_data.iter()
            .map(|data| data.summary.throughput_mbps)
            .sum::<f64>() / self.historical_data.len() as f64;

        let avg_success_rate = self.historical_data.iter()
            .map(|data| data.summary.success_rate)
            .sum::<f64>() / self.historical_data.len() as f64;

        let avg_error_rate = self.historical_data.iter()
            .map(|data| {
                if data.summary.total_requests > 0 {
                    (data.summary.failed_requests as f64 / data.summary.total_requests as f64) * 100.0
                } else {
                    0.0
                }
            })
            .sum::<f64>() / self.historical_data.len() as f64;

        self.baseline_metrics = Some(BaselineMetrics {
            avg_latency_ms,
            avg_throughput_mbps,
            avg_success_rate,
            avg_error_rate,
            established_at: Utc::now(),
        });
    }

    /// Simple linear extrapolation for trend prediction
    fn extrapolate_trend(&self, values: &[f64], steps_ahead: usize) -> f64 {
        if values.len() < 2 {
            return values.last().copied().unwrap_or(0.0);
        }

        // Simple linear regression
        let n = values.len() as f64;
        let x_sum: f64 = (0..values.len()).map(|i| i as f64).sum();
        let y_sum: f64 = values.iter().sum();
        let xy_sum: f64 = values.iter().enumerate().map(|(i, &y)| i as f64 * y).sum();
        let x_squared_sum: f64 = (0..values.len()).map(|i| (i as f64).powi(2)).sum();

        let slope = (n * xy_sum - x_sum * y_sum) / (n * x_squared_sum - x_sum.powi(2));
        let intercept = (y_sum - slope * x_sum) / n;

        let future_x = (values.len() + steps_ahead - 1) as f64;
        slope * future_x + intercept
    }
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            trend_window_size: 50,
            alert_thresholds: AlertThresholds::default(),
            analysis_interval: Duration::from_secs(30),
            enable_predictive_analysis: true,
            enable_anomaly_detection: true,
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_latency_ms: 100.0,
            min_success_rate: 95.0,
            max_error_rate: 5.0,
            min_throughput_mbps: 1.0,
            max_cpu_usage: 80.0,
            max_memory_usage_mb: 1024.0,
            min_connection_health: 0.8,
        }
    }
}