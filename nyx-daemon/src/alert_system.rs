use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::metrics::{
    Alert, AlertSeverity, AlertThreshold, ThresholdComparison, AlertRoute, AlertHandler,
    SuppressionRule, AlertHistoryEntry, AlertAction, LayerType, MetricsSnapshot,
};

/// Comprehensive alert system for performance threshold monitoring and notification routing
pub struct AlertSystem {
    /// Alert threshold configurations
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
}

impl AlertSystem {
    /// Create a new alert system with default configurations
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
        };
        
        // Set up default thresholds
        system.setup_default_thresholds();
        
        // Set up default routes
        system.setup_default_routes();
        
        system
    }
    
    /// Set up default alert thresholds
    fn setup_default_thresholds(&self) {
        let mut thresholds = self.thresholds.write().unwrap();
        
        // CPU usage threshold
        thresholds.insert("cpu_usage".to_string(), AlertThreshold {
            metric: "cpu_usage".to_string(),
            threshold: 80.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(300), // 5 minutes
            last_triggered: None,
        });
        
        thresholds.insert("cpu_usage_critical".to_string(), AlertThreshold {
            metric: "cpu_usage".to_string(),
            threshold: 95.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: None,
            enabled: true,
            cooldown_duration: Duration::from_secs(60), // 1 minute
            last_triggered: None,
        });
        
        // Memory usage thresholds
        thresholds.insert("memory_usage".to_string(), AlertThreshold {
            metric: "memory_usage".to_string(),
            threshold: 85.0,
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
        
        // Error rate thresholds
        thresholds.insert("error_rate".to_string(), AlertThreshold {
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
        
        // Latency thresholds
        thresholds.insert("latency_high".to_string(), AlertThreshold {
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
        
        // Packet loss thresholds
        thresholds.insert("packet_loss".to_string(), AlertThreshold {
            metric: "packet_loss_rate".to_string(),
            threshold: 1.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
            layer: Some(LayerType::Transport),
            enabled: true,
            cooldown_duration: Duration::from_secs(180),
            last_triggered: None,
        });
        
        thresholds.insert("packet_loss_critical".to_string(), AlertThreshold {
            metric: "packet_loss_rate".to_string(),
            threshold: 5.0,
            severity: AlertSeverity::Critical,
            comparison: ThresholdComparison::GreaterThan,
            layer: Some(LayerType::Transport),
            enabled: true,
            cooldown_duration: Duration::from_secs(60),
            last_triggered: None,
        });
        
        // Connection failure thresholds
        thresholds.insert("connection_failure_rate".to_string(), AlertThreshold {
            metric: "connection_failure_rate".to_string(),
            threshold: 10.0,
            severity: AlertSeverity::Warning,
            comparison: ThresholdComparison::GreaterThan,
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
    }
    
    /// Check metrics against thresholds and generate alerts
    pub async fn check_thresholds(&self, snapshot: &MetricsSnapshot) -> Vec<Alert> {
        let mut new_alerts = Vec::new();
        let thresholds = self.thresholds.read().unwrap();
        
        for (threshold_id, threshold) in thresholds.iter() {
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
    
    /// Extract metric value from snapshot based on metric name and layer
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
            "packet_loss_rate" => Some(snapshot.performance_metrics.packet_loss_rate * 100.0), // Convert to percentage
            "connection_failure_rate" => {
                let total_connections = snapshot.network_metrics.total_connections;
                let failed_connections = snapshot.network_metrics.failed_connections;
                if total_connections > 0 {
                    Some((failed_connections as f64 / total_connections as f64) * 100.0)
                } else {
                    None
                }
            },
            _ => {
                // Check layer-specific metrics
                if let Some(layer_type) = layer {
                    if let Some(layer_metrics) = snapshot.layer_metrics.get(layer_type) {
                        match metric {
                            "throughput" => Some(layer_metrics.throughput),
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
        context.insert("threshold_id".to_string(), threshold.metric.clone());
        context.insert("comparison".to_string(), format!("{:?}", threshold.comparison));
        if let Some(layer) = &threshold.layer {
            context.insert("layer".to_string(), format!("{:?}", layer));
        }
        
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
    
    /// Check if an alert should be suppressed based on suppression rules
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
                        return true; // Suppress this alert
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
        
        // Update threshold last triggered time
        {
            let mut thresholds = self.thresholds.write().unwrap();
            for threshold in thresholds.values_mut() {
                if threshold.metric == alert.metric && threshold.layer == alert.layer {
                    threshold.last_triggered = Some(alert.timestamp);
                }
            }
        }
        
        // Route alert through configured handlers
        self.route_alert(&alert).await;
        
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
                    continue; // Alert has no layer but filter requires specific layers
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
                log::warn!("Alert: {} - {} - {}", alert.title, alert.description, alert.current_value);
            },
            AlertHandler::Email(email) => {
                // TODO: Implement email notification
                log::info!("Would send email alert to {}: {}", email, alert.title);
            },
            AlertHandler::Webhook(url) => {
                // TODO: Implement webhook notification
                log::info!("Would send webhook to {}: {}", url, alert.title);
            },
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
    
    /// Resolve an active alert
    pub async fn resolve_alert(&self, alert_id: &str) -> Result<(), String> {
        let mut active_alerts = self.active_alerts.write().unwrap();
        
        if let Some(mut alert) = active_alerts.remove(alert_id) {
            alert.resolved = true;
            alert.resolved_at = Some(SystemTime::now());
            
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
    
    /// Get alert history
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
        let active_alerts = self.active_alerts.read().unwrap();
        let history = self.alert_history.read().unwrap();
        
        let mut stats = AlertStatistics {
            total_active: active_alerts.len(),
            active_by_severity: HashMap::new(),
            total_resolved: 0,
            total_created_today: 0,
            most_frequent_metrics: HashMap::new(),
        };
        
        // Count active alerts by severity
        for alert in active_alerts.values() {
            *stats.active_by_severity.entry(alert.severity.clone()).or_insert(0) += 1;
        }
        
        // Analyze history
        let today = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
        
        for entry in history.iter() {
            match entry.action {
                AlertAction::Resolved => stats.total_resolved += 1,
                AlertAction::Created => {
                    let entry_day = entry.timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;
                    if entry_day == today {
                        stats.total_created_today += 1;
                    }
                    
                    *stats.most_frequent_metrics.entry(entry.alert.metric.clone()).or_insert(0) += 1;
                },
                _ => {},
            }
        }
        
        stats
    }
    
    /// Subscribe to alert notifications
    pub fn subscribe(&self) -> broadcast::Receiver<Alert> {
        self.alert_sender.subscribe()
    }
}

/// Alert system statistics
#[derive(Debug, Clone)]
pub struct AlertStatistics {
    pub total_active: usize,
    pub active_by_severity: HashMap<AlertSeverity, usize>,
    pub total_resolved: usize,
    pub total_created_today: usize,
    pub most_frequent_metrics: HashMap<String, usize>,
}

impl Default for AlertSystem {
    fn default() -> Self {
        Self::new()
    }
}