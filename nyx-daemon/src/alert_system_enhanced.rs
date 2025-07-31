use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::metrics::{
    Alert, AlertSeverity, AlertThreshold, ThresholdComparison, AlertRoute, AlertHandler,
    SuppressionRule, AlertHistoryEntry, AlertAction, LayerType, MetricsSnapshot,
};

/// Enhanced alert system with comprehensive threshold monitoring, routing, suppression, and analysis
pub struct EnhancedAlertSystem {
    /// Alert threshold configurations with full AlertThreshold objects
    thresholds: Arc<RwLock<HashMap<String, AlertThreshold>>>,
    
    /// Currently active alerts
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    
    /// Alert history for analysis and reporting
    alert_history: Arc<RwLock<Vec<AlertHistoryEntry>>>,
    
    /// Alert routing configurations
    routes: Arc<RwLock<Vec<AlertRoute>>>,
    
    /// Suppression rules for duplicate alert prevention
    suppression_rules: Arc<RwLock<Vec<SuppressionRule>>>,
    
    /// Alert broadcast channel
    alert_sender: broadcast::Sender<Alert>,
    
    /// Maximum history size to prevent memory growth
    max_history_size: usize,
    
    /// Alert statistics tracking
    alert_stats: Arc<RwLock<AlertStatistics>>,
    
    /// Performance threshold monitoring
    performance_thresholds: Arc<RwLock<PerformanceThresholds>>,
    
    /// Alert priority system
    priority_system: Arc<RwLock<AlertPrioritySystem>>,
}

/// Alert statistics for monitoring and analysis
#[derive(Debug, Clone, Default)]
pub struct AlertStatistics {
    pub total_active: usize,
    pub active_by_severity: HashMap<AlertSeverity, usize>,
    pub total_resolved: usize,
    pub total_created_today: usize,
    pub most_frequent_metrics: HashMap<String, usize>,
    pub avg_resolution_time: Duration,
    pub escalation_count: usize,
    pub suppression_count: usize,
}

/// Performance thresholds for different metrics
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub cpu_warning: f64,
    pub cpu_critical: f64,
    pub memory_warning: f64,
    pub memory_critical: f64,
    pub latency_warning: f64,
    pub latency_critical: f64,
    pub error_rate_warning: f64,
    pub error_rate_critical: f64,
    pub throughput_warning: f64,
    pub throughput_critical: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            cpu_warning: 70.0,
            cpu_critical: 90.0,
            memory_warning: 80.0,
            memory_critical: 95.0,
            latency_warning: 1000.0,
            latency_critical: 5000.0,
            error_rate_warning: 5.0,
            error_rate_critical: 15.0,
            throughput_warning: 100.0,
            throughput_critical: 50.0,
        }
    }
}

/// Alert priority system for routing and escalation
#[derive(Debug, Clone)]
pub struct AlertPrioritySystem {
    pub priority_rules: Vec<PriorityRule>,
    pub escalation_rules: Vec<EscalationRule>,
}

/// Priority rule for determining alert importance
#[derive(Debug, Clone)]
pub struct PriorityRule {
    pub metric_pattern: String,
    pub severity: AlertSeverity,
    pub priority_score: u32,
    pub auto_escalate: bool,
    pub escalation_delay: Duration,
}

/// Escalation rule for alert handling
#[derive(Debug, Clone)]
pub struct EscalationRule {
    pub trigger_conditions: Vec<EscalationCondition>,
    pub target_handlers: Vec<AlertHandler>,
    pub escalation_delay: Duration,
}

/// Conditions that trigger alert escalation
#[derive(Debug, Clone)]
pub enum EscalationCondition {
    UnresolvedDuration(Duration),
    SeverityLevel(AlertSeverity),
    FrequencyThreshold(u32, Duration),
    MetricPattern(String),
}

impl EnhancedAlertSystem {
    /// Create a new enhanced alert system
    pub fn new() -> Self {
        let (alert_sender, _) = broadcast::channel(1000);
        
        let mut system = Self {
            thresholds: Arc::new(RwLock::new(HashMap::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            routes: Arc::new(RwLock::new(Vec::new())),
            suppression_rules: Arc::new(RwLock::new(Vec::new())),
            alert_sender,
            max_history_size: 10000,
            alert_stats: Arc::new(RwLock::new(AlertStatistics::default())),
            performance_thresholds: Arc::new(RwLock::new(PerformanceThresholds::default())),
            priority_system: Arc::new(RwLock::new(AlertPrioritySystem {
                priority_rules: Vec::new(),
                escalation_rules: Vec::new(),
            })),
        };
        
        // Set up default thresholds and routes
        system.setup_default_configuration();
        
        system
    }
    
    /// Set up default alert thresholds and routes
    fn setup_default_configuration(&self) {
        self.setup_performance_thresholds();
        self.setup_default_routes();
        self.setup_priority_rules();
    }
    
    /// Set up performance threshold monitoring
    fn setup_performance_thresholds(&self) {
        let mut thresholds = self.thresholds.write().unwrap();
        
        // CPU usage thresholds
        thresholds.insert("cpu_usage_warning".to_string(), AlertThreshold {
            metric: "cpu_usage".to_string(),
            threshold: 70.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(300),
            last_triggered: None,
        });
        
        thresholds.insert("cpu_usage_critical".to_string(), AlertThreshold {
            metric: "cpu_usage".to_string(),
            threshold: 90.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(60),
            last_triggered: None,
        });
        
        // Memory usage thresholds
        thresholds.insert("memory_usage_warning".to_string(), AlertThreshold {
            metric: "memory_usage".to_string(),
            threshold: 80.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(300),
            last_triggered: None,
        });
        
        thresholds.insert("memory_usage_critical".to_string(), AlertThreshold {
            metric: "memory_usage".to_string(),
            threshold: 95.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(60),
            last_triggered: None,
        });
        
        // Latency thresholds
        thresholds.insert("latency_warning".to_string(), AlertThreshold {
            metric: "avg_latency_ms".to_string(),
            threshold: 1000.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: Some(LayerType::Transport),
            enabled: true,
            cooldown_duration: Duration::from_secs(120),
            last_triggered: None,
        });
        
        thresholds.insert("latency_critical".to_string(), AlertThreshold {
            metric: "avg_latency_ms".to_string(),
            threshold: 5000.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: Some(LayerType::Transport),
            enabled: true,
            cooldown_duration: Duration::from_secs(60),
            last_triggered: None,
        });
        
        // Error rate thresholds
        thresholds.insert("error_rate_warning".to_string(), AlertThreshold {
            metric: "error_rate".to_string(),
            threshold: 5.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(180),
            last_triggered: None,
        });
        
        thresholds.insert("error_rate_critical".to_string(), AlertThreshold {
            metric: "error_rate".to_string(),
            threshold: 15.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(60),
            last_triggered: None,
        });
        
        // Throughput thresholds
        thresholds.insert("throughput_warning".to_string(), AlertThreshold {
            metric: "throughput".to_string(),
            threshold: 100.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::LessThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(300),
            last_triggered: None,
        });
    }
    
    /// Set up default alert routes
    fn setup_default_routes(&self) {
        let mut routes = self.routes.write().unwrap();
        
        // Console logging for all alerts
        routes.push(AlertRoute {
            severity_filter: vec![AlertSeverity::Info, AlertSeverity::Warning, AlertSeverity::Critical],
            layer_filter: vec![], // All layers
            handler: AlertHandler::Console,
        });
        
        // Log file for all alerts
        routes.push(AlertRoute {
            severity_filter: vec![AlertSeverity::Info, AlertSeverity::Warning, AlertSeverity::Critical],
            layer_filter: vec![], // All layers
            handler: AlertHandler::Log,
        });
        
        // Critical alerts to webhook (if configured)
        routes.push(AlertRoute {
            severity_filter: vec![AlertSeverity::Critical],
            layer_filter: vec![],
            handler: AlertHandler::Webhook("http://localhost:8080/alerts".to_string()),
        });
    }
    
    /// Set up priority rules for alert handling
    fn setup_priority_rules(&self) {
        let mut priority_system = self.priority_system.write().unwrap();
        
        // High priority rules
        priority_system.priority_rules.push(PriorityRule {
            metric_pattern: "cpu_usage".to_string(),
            severity: AlertSeverity::Critical,
            priority_score: 100,
            auto_escalate: true,
            escalation_delay: Duration::from_secs(300),
        });
        
        priority_system.priority_rules.push(PriorityRule {
            metric_pattern: "memory_usage".to_string(),
            severity: AlertSeverity::Critical,
            priority_score: 95,
            auto_escalate: true,
            escalation_delay: Duration::from_secs(300),
        });
        
        // Medium priority rules
        priority_system.priority_rules.push(PriorityRule {
            metric_pattern: "error_rate".to_string(),
            severity: AlertSeverity::Warning,
            priority_score: 70,
            auto_escalate: false,
            escalation_delay: Duration::from_secs(600),
        });
        
        // Escalation rules
        priority_system.escalation_rules.push(EscalationRule {
            trigger_conditions: vec![
                EscalationCondition::UnresolvedDuration(Duration::from_secs(1800)), // 30 minutes
                EscalationCondition::SeverityLevel(AlertSeverity::Critical),
            ],
            target_handlers: vec![AlertHandler::Email("admin@example.com".to_string())],
            escalation_delay: Duration::from_secs(300),
        });
    }
    
    /// Check metrics against thresholds and generate alerts
    pub async fn check_thresholds(&self, snapshot: &MetricsSnapshot) -> Vec<Alert> {
        let mut new_alerts = Vec::new();
        let mut thresholds = self.thresholds.write().unwrap();
        
        for (threshold_id, threshold) in thresholds.iter_mut() {
            if !threshold.enabled {
                continue;
            }
            
            // Check cooldown period
            if let Some(last_triggered) = threshold.last_triggered {
                if SystemTime::now().duration_since(last_triggered).unwrap_or_default() < threshold.cooldown_duration {
                    continue;
                }
            }
            
            // Extract metric value based on threshold configuration
            let metric_value = self.extract_metric_value(snapshot, &threshold.metric, &threshold.layer);
            
            if let Some(value) = metric_value {
                let threshold_exceeded = match threshold.comparison {
                    ThresholdComparison::GreaterThan => value > threshold.threshold,
                    ThresholdComparison::LessThan => value < threshold.threshold,
                    ThresholdComparison::Equal => (value - threshold.threshold).abs() < f64::EPSILON,
                };
                
                if threshold_exceeded {
                    // Check if this alert should be suppressed
                    if !self.should_suppress_alert(&threshold.metric, &threshold.layer) {
                        let alert = self.create_alert(threshold, value).await;
                        threshold.last_triggered = Some(alert.timestamp);
                        new_alerts.push(alert);
                    }
                }
            }
        }
        
        // Process new alerts
        for alert in &new_alerts {
            self.process_alert(alert.clone()).await;
        }
        
        new_alerts
    }
    
    /// Extract metric value from snapshot
    fn extract_metric_value(&self, snapshot: &MetricsSnapshot, metric: &str, layer: &Option<LayerType>) -> Option<f64> {
        match metric {
            "cpu_usage" => Some(snapshot.system_metrics.cpu_usage_percent),
            "memory_usage" => {
                if snapshot.system_metrics.memory_total_bytes > 0 {
                    Some((snapshot.system_metrics.memory_usage_bytes as f64 / snapshot.system_metrics.memory_total_bytes as f64) * 100.0)
                } else {
                    None
                }
            },
            "error_rate" => Some(snapshot.error_metrics.error_rate),
            "avg_latency_ms" => Some(snapshot.performance_metrics.avg_latency_ms),
            "packet_loss_rate" => Some(snapshot.performance_metrics.packet_loss_rate * 100.0),
            "throughput" => {
                if let Some(layer_type) = layer {
                    if let Some(layer_metrics) = snapshot.layer_metrics.get(layer_type) {
                        Some(layer_metrics.throughput)
                    } else {
                        None
                    }
                } else {
                    // Calculate overall throughput
                    let total_throughput: f64 = snapshot.layer_metrics.values()
                        .map(|m| m.throughput)
                        .sum();
                    Some(total_throughput)
                }
            },
            _ => {
                // Check layer-specific metrics
                if let Some(layer_type) = layer {
                    if let Some(layer_metrics) = snapshot.layer_metrics.get(layer_type) {
                        match metric {
                            "layer_error_rate" => Some(layer_metrics.error_rate),
                            "queue_depth" => Some(layer_metrics.queue_depth as f64),
                            "active_connections" => Some(layer_metrics.active_connections as f64),
                            _ => None,
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }
    
    /// Create a new alert from threshold violation
    async fn create_alert(&self, threshold: &AlertThreshold, current_value: f64) -> Alert {
        let alert_id = Uuid::new_v4().to_string();
        let timestamp = SystemTime::now();
        
        let title = format!("{} threshold exceeded", threshold.metric);
        let description = format!(
            "Metric '{}' has exceeded the {} threshold. Current value: {:.2}, Threshold: {:.2}",
            threshold.metric,
            match threshold.severity {
                AlertSeverity::Info => "info",
                AlertSeverity::Warning => "warning",
                AlertSeverity::Critical => "critical",
            },
            current_value,
            threshold.threshold
        );
        
        let mut context = HashMap::new();
        context.insert("threshold_comparison".to_string(), format!("{:?}", threshold.comparison));
        if let Some(layer) = &threshold.layer {
            context.insert("layer".to_string(), format!("{:?}", layer));
        }
        
        // Add priority information
        let priority_score = self.calculate_alert_priority(&threshold.metric, &threshold.severity);
        context.insert("priority_score".to_string(), priority_score.to_string());
        
        Alert {
            id: alert_id,
            timestamp,
            severity: threshold.severity.clone(),
            title,
            description,
            metric: threshold.metric.clone(),
            current_value,
            threshold: threshold.threshold,
            layer: threshold.layer.clone(),
            context,
            resolved: false,
            resolved_at: None,
        }
    }
    
    /// Calculate alert priority score
    fn calculate_alert_priority(&self, metric: &str, severity: &AlertSeverity) -> u32 {
        let priority_system = self.priority_system.read().unwrap();
        
        for rule in &priority_system.priority_rules {
            if metric.contains(&rule.metric_pattern) && rule.severity == *severity {
                return rule.priority_score;
            }
        }
        
        // Default priority based on severity
        match severity {
            AlertSeverity::Critical => 80,
            AlertSeverity::Warning => 50,
            AlertSeverity::Info => 20,
        }
    }
    
    /// Check if an alert should be suppressed
    fn should_suppress_alert(&self, metric: &str, layer: &Option<LayerType>) -> bool {
        let suppression_rules = self.suppression_rules.read().unwrap();
        let now = SystemTime::now();
        
        for rule in suppression_rules.iter() {
            // Check if rule applies to this metric
            if metric.contains(&rule.metric_pattern) {
                // Check layer filter
                if rule.layer.is_some() && rule.layer != *layer {
                    continue;
                }
                
                // Check if rule is still active
                if now.duration_since(rule.created_at).unwrap_or_default() < rule.duration {
                    // Count recent alerts for this pattern
                    let active_alerts = self.active_alerts.read().unwrap();
                    let matching_alerts = active_alerts.values()
                        .filter(|alert| alert.metric.contains(&rule.metric_pattern))
                        .count();
                    
                    if matching_alerts >= rule.max_alerts as usize {
                        // Update suppression statistics
                        if let Ok(mut stats) = self.alert_stats.write() {
                            stats.suppression_count += 1;
                        }
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Process a new alert through the routing system
    async fn process_alert(&self, alert: Alert) {
        // Add to active alerts
        {
            let mut active_alerts = self.active_alerts.write().unwrap();
            active_alerts.insert(alert.id.clone(), alert.clone());
        }
        
        // Add to history
        self.add_to_history(alert.clone(), AlertAction::Created);
        
        // Update statistics
        self.update_alert_statistics(&alert, AlertAction::Created);
        
        // Route alert through configured handlers
        self.route_alert(&alert).await;
        
        // Check for escalation
        self.check_escalation(&alert).await;
        
        // Broadcast alert
        let _ = self.alert_sender.send(alert);
    }
    
    /// Route alert through configured handlers
    async fn route_alert(&self, alert: &Alert) {
        let routes = self.routes.read().unwrap();
        
        for route in routes.iter() {
            // Check severity filter
            if !route.severity_filter.is_empty() && !route.severity_filter.contains(&alert.severity) {
                continue;
            }
            
            // Check layer filter
            if !route.layer_filter.is_empty() {
                if let Some(alert_layer) = &alert.layer {
                    if !route.layer_filter.contains(alert_layer) {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            
            // Handle alert based on handler type
            self.handle_alert(alert, &route.handler).await;
        }
    }
    
    /// Handle alert based on handler type
    async fn handle_alert(&self, alert: &Alert, handler: &AlertHandler) {
        match handler {
            AlertHandler::Console => {
                println!("[ALERT] {} - {} - {}", 
                    match alert.severity {
                        AlertSeverity::Info => "INFO",
                        AlertSeverity::Warning => "WARN",
                        AlertSeverity::Critical => "CRIT",
                    },
                    alert.title,
                    alert.description
                );
            },
            AlertHandler::Log => {
                log::warn!("Alert: {} - {} - Current: {:.2}, Threshold: {:.2}", 
                    alert.title, alert.description, alert.current_value, alert.threshold);
            },
            AlertHandler::Email(email) => {
                // In a real implementation, this would send an actual email
                log::info!("Would send email alert to {}: {} (Priority: {})", 
                    email, alert.title, 
                    alert.context.get("priority_score").unwrap_or(&"0".to_string()));
            },
            AlertHandler::Webhook(url) => {
                // In a real implementation, this would make an HTTP request
                log::info!("Would send webhook to {}: {} (Severity: {:?})", 
                    url, alert.title, alert.severity);
            },
        }
    }
    
    /// Check if alert needs escalation
    async fn check_escalation(&self, alert: &Alert) {
        let priority_system = self.priority_system.read().unwrap();
        
        for rule in &priority_system.escalation_rules {
            let should_escalate = rule.trigger_conditions.iter().any(|condition| {
                match condition {
                    EscalationCondition::SeverityLevel(severity) => alert.severity == *severity,
                    EscalationCondition::MetricPattern(pattern) => alert.metric.contains(pattern),
                    EscalationCondition::UnresolvedDuration(_) => false, // Checked later
                    EscalationCondition::FrequencyThreshold(_, _) => false, // Checked later
                }
            });
            
            if should_escalate {
                // Schedule escalation (in a real implementation, this would use a timer)
                log::info!("Alert {} scheduled for escalation in {:?}", alert.id, rule.escalation_delay);
                
                // Update statistics
                if let Ok(mut stats) = self.alert_stats.write() {
                    stats.escalation_count += 1;
                }
            }
        }
    }
    
    /// Add alert to history
    fn add_to_history(&self, alert: Alert, action: AlertAction) {
        let mut history = self.alert_history.write().unwrap();
        
        history.push(AlertHistoryEntry {
            alert,
            action,
            timestamp: SystemTime::now(),
        });
        
        // Trim history if it exceeds maximum size
        if history.len() > self.max_history_size {
            history.remove(0);
        }
    }
    
    /// Update alert statistics
    fn update_alert_statistics(&self, alert: &Alert, action: AlertAction) {
        if let Ok(mut stats) = self.alert_stats.write() {
            match action {
                AlertAction::Created => {
                    stats.total_active += 1;
                    *stats.active_by_severity.entry(alert.severity.clone()).or_insert(0) += 1;
                    *stats.most_frequent_metrics.entry(alert.metric.clone()).or_insert(0) += 1;
                    
                    // Check if created today
                    let today = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
                    let alert_day = alert.timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
                    if alert_day == today {
                        stats.total_created_today += 1;
                    }
                },
                AlertAction::Resolved => {
                    if stats.total_active > 0 {
                        stats.total_active -= 1;
                    }
                    stats.total_resolved += 1;
                    
                    if let Some(count) = stats.active_by_severity.get_mut(&alert.severity) {
                        if *count > 0 {
                            *count -= 1;
                        }
                    }
                },
                _ => {}
            }
        }
    }
    
    /// Resolve an active alert
    pub async fn resolve_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_alerts = self.active_alerts.write().unwrap();
        
        if let Some(mut alert) = active_alerts.remove(alert_id) {
            alert.resolved = true;
            alert.resolved_at = Some(SystemTime::now());
            
            // Update statistics
            self.update_alert_statistics(&alert, AlertAction::Resolved);
            
            // Add to history
            self.add_to_history(alert, AlertAction::Resolved);
            
            Ok(())
        } else {
            Err(format!("Alert with ID {} not found", alert_id))
        }
    }
    
    /// Add a new alert threshold
    pub fn add_threshold(&self, threshold_id: String, threshold: AlertThreshold) {
        let mut thresholds = self.thresholds.write().unwrap();
        thresholds.insert(threshold_id, threshold);
    }
    
    /// Remove an alert threshold
    pub fn remove_threshold(&self, threshold_id: &str) -> bool {
        let mut thresholds = self.thresholds.write().unwrap();
        thresholds.remove(threshold_id).is_some()
    }
    
    /// Add a suppression rule
    pub fn add_suppression_rule(&self, rule: SuppressionRule) {
        let mut rules = self.suppression_rules.write().unwrap();
        rules.push(rule);
    }
    
    /// Add an alert route
    pub fn add_route(&self, route: AlertRoute) {
        let mut routes = self.routes.write().unwrap();
        routes.push(route);
    }
    
    /// Get active alerts
    pub fn get_active_alerts(&self) -> HashMap<String, Alert> {
        self.active_alerts.read().unwrap().clone()
    }
    
    /// Get alert history with optional limit
    pub fn get_alert_history(&self, limit: Option<usize>) -> Vec<AlertHistoryEntry> {
        let history = self.alert_history.read().unwrap();
        if let Some(limit) = limit {
            history.iter().rev().take(limit).cloned().collect()
        } else {
            history.clone()
        }
    }
    
    /// Get alert statistics
    pub fn get_alert_statistics(&self) -> AlertStatistics {
        self.alert_stats.read().unwrap().clone()
    }
    
    /// Generate alert analysis report
    pub fn generate_analysis_report(&self) -> AlertAnalysisReport {
        let stats = self.get_alert_statistics();
        let history = self.get_alert_history(Some(100)); // Last 100 alerts
        let active_alerts = self.get_active_alerts();
        
        // Analyze patterns
        let mut metric_frequency = HashMap::new();
        let mut severity_distribution = HashMap::new();
        let mut hourly_distribution = vec![0u32; 24];
        
        for entry in &history {
            *metric_frequency.entry(entry.alert.metric.clone()).or_insert(0) += 1;
            *severity_distribution.entry(entry.alert.severity.clone()).or_insert(0) += 1;
            
            // Calculate hour of day
            if let Ok(duration) = entry.alert.timestamp.duration_since(std::time::UNIX_EPOCH) {
                let hour = (duration.as_secs() / 3600) % 24;
                hourly_distribution[hour as usize] += 1;
            }
        }
        
        // Calculate trends
        let mut recommendations = Vec::new();
        
        if stats.total_active > 10 {
            recommendations.push("High number of active alerts - consider reviewing thresholds".to_string());
        }
        
        if stats.escalation_count > stats.total_resolved / 4 {
            recommendations.push("High escalation rate - review alert handling procedures".to_string());
        }
        
        if stats.suppression_count > stats.total_created_today / 2 {
            recommendations.push("High suppression rate - consider adjusting suppression rules".to_string());
        }
        
        AlertAnalysisReport {
            summary: stats,
            metric_frequency,
            severity_distribution,
            hourly_distribution,
            recommendations,
            active_alert_count: active_alerts.len(),
            oldest_unresolved: active_alerts.values()
                .min_by_key(|alert| alert.timestamp)
                .map(|alert| alert.timestamp),
        }
    }
    
    /// Subscribe to alert notifications
    pub fn subscribe(&self) -> broadcast::Receiver<Alert> {
        self.alert_sender.subscribe()
    }
}

/// Alert analysis report for monitoring and optimization
#[derive(Debug, Clone)]
pub struct AlertAnalysisReport {
    pub summary: AlertStatistics,
    pub metric_frequency: HashMap<String, u32>,
    pub severity_distribution: HashMap<AlertSeverity, u32>,
    pub hourly_distribution: Vec<u32>,
    pub recommendations: Vec<String>,
    pub active_alert_count: usize,
    pub oldest_unresolved: Option<SystemTime>,
}

impl Default for EnhancedAlertSystem {
    fn default() -> Self {
        Self::new()
    }
}