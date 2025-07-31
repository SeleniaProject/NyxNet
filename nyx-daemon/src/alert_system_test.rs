use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::alert_system_enhanced::{EnhancedAlertSystem, PerformanceThresholds};
use crate::metrics::{
    MetricsSnapshot, SystemMetrics, NetworkMetrics, ErrorMetrics, LayerMetrics, LayerType,
    AlertSeverity, AlertThreshold, ThresholdComparison, SuppressionRule,
};

/// Test the enhanced alert system functionality
pub async fn test_alert_system() {
    println!("Testing Enhanced Alert System...");
    
    // Create alert system
    let alert_system = EnhancedAlertSystem::new();
    
    // Test 1: Basic threshold monitoring
    test_threshold_monitoring(&alert_system).await;
    
    // Test 2: Alert suppression
    test_alert_suppression(&alert_system).await;
    
    // Test 3: Alert routing and priority
    test_alert_routing(&alert_system).await;
    
    // Test 4: Statistics and analysis
    test_statistics_and_analysis(&alert_system).await;
    
    println!("Alert system tests completed successfully!");
}

async fn test_threshold_monitoring(alert_system: &EnhancedAlertSystem) {
    println!("  Testing threshold monitoring...");
    
    // Create a test metrics snapshot with high CPU usage
    let snapshot = create_test_snapshot(95.0, 50.0, 2.0, 500.0);
    
    // Check thresholds
    let alerts = alert_system.check_thresholds(&snapshot).await;
    
    // Should generate CPU critical alert
    assert!(!alerts.is_empty(), "Should generate alerts for high CPU usage");
    
    let cpu_alert = alerts.iter().find(|a| a.metric == "cpu_usage");
    assert!(cpu_alert.is_some(), "Should generate CPU usage alert");
    
    if let Some(alert) = cpu_alert {
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert!(alert.current_value > 90.0);
    }
    
    println!("    ✓ Threshold monitoring working correctly");
}

async fn test_alert_suppression(alert_system: &EnhancedAlertSystem) {
    println!("  Testing alert suppression...");
    
    // Add suppression rule
    let suppression_rule = SuppressionRule {
        id: "test_suppression".to_string(),
        metric_pattern: "cpu_usage".to_string(),
        layer: None,
        duration: Duration::from_secs(300),
        max_alerts: 1,
        created_at: SystemTime::now(),
    };
    
    alert_system.add_suppression_rule(suppression_rule);
    
    // Generate multiple alerts for the same metric
    let snapshot = create_test_snapshot(95.0, 50.0, 2.0, 500.0);
    
    // First alert should be generated
    let alerts1 = alert_system.check_thresholds(&snapshot).await;
    assert!(!alerts1.is_empty(), "First alert should be generated");
    
    // Wait a moment and try again - should be suppressed
    tokio::time::sleep(Duration::from_millis(100)).await;
    let alerts2 = alert_system.check_thresholds(&snapshot).await;
    
    // Check suppression statistics
    let stats = alert_system.get_alert_statistics();
    assert!(stats.suppression_count > 0, "Should have suppression count");
    
    println!("    ✓ Alert suppression working correctly");
}

async fn test_alert_routing(alert_system: &EnhancedAlertSystem) {
    println!("  Testing alert routing and priority...");
    
    // Generate different severity alerts
    let critical_snapshot = create_test_snapshot(95.0, 50.0, 2.0, 500.0);
    let warning_snapshot = create_test_snapshot(75.0, 50.0, 2.0, 500.0);
    
    let critical_alerts = alert_system.check_thresholds(&critical_snapshot).await;
    let warning_alerts = alert_system.check_thresholds(&warning_snapshot).await;
    
    // Check that alerts have different priorities
    for alert in &critical_alerts {
        if alert.severity == AlertSeverity::Critical {
            let priority = alert.context.get("priority_score")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            assert!(priority >= 80, "Critical alerts should have high priority");
        }
    }
    
    for alert in &warning_alerts {
        if alert.severity == AlertSeverity::Warning {
            let priority = alert.context.get("priority_score")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            assert!(priority >= 50 && priority < 80, "Warning alerts should have medium priority");
        }
    }
    
    println!("    ✓ Alert routing and priority working correctly");
}

async fn test_statistics_and_analysis(alert_system: &EnhancedAlertSystem) {
    println!("  Testing statistics and analysis...");
    
    // Generate some alerts
    let snapshot = create_test_snapshot(95.0, 85.0, 10.0, 2000.0);
    let alerts = alert_system.check_thresholds(&snapshot).await;
    
    // Get statistics
    let stats = alert_system.get_alert_statistics();
    assert!(stats.total_active > 0, "Should have active alerts");
    assert!(!stats.active_by_severity.is_empty(), "Should have severity breakdown");
    
    // Get analysis report
    let report = alert_system.generate_analysis_report();
    assert!(!report.metric_frequency.is_empty(), "Should have metric frequency data");
    assert!(!report.recommendations.is_empty(), "Should have recommendations");
    
    // Test alert resolution
    if let Some(alert) = alerts.first() {
        let result = alert_system.resolve_alert(&alert.id).await;
        assert!(result.is_ok(), "Should be able to resolve alert");
        
        let updated_stats = alert_system.get_alert_statistics();
        assert!(updated_stats.total_resolved > 0, "Should have resolved alerts count");
    }
    
    println!("    ✓ Statistics and analysis working correctly");
}

fn create_test_snapshot(cpu: f64, memory: f64, error_rate: f64, latency: f64) -> MetricsSnapshot {
    let mut layer_metrics = HashMap::new();
    
    layer_metrics.insert(LayerType::Transport, LayerMetrics {
        layer_type: LayerType::Transport,
        status: crate::metrics::LayerStatus::Active,
        throughput: 100.0,
        latency: Duration::from_millis(latency as u64),
        error_rate,
        resource_usage: crate::metrics::ResourceUsage::default(),
        packets_processed: 1000,
        bytes_processed: 50000,
        active_connections: 10,
        queue_depth: 5,
        last_updated: SystemTime::now(),
    });
    
    MetricsSnapshot {
        timestamp: SystemTime::now(),
        layer_metrics,
        system_metrics: SystemMetrics {
            cpu_usage_percent: cpu,
            memory_usage_bytes: (memory * 1024.0 * 1024.0 * 1024.0) as u64, // GB to bytes
            memory_total_bytes: 8 * 1024 * 1024 * 1024, // 8GB
            network_bytes_sent: 10000,
            network_bytes_received: 15000,
            open_file_descriptors: 100,
            thread_count: 20,
            disk_usage_bytes: 1000000,
            load_average: cpu / 100.0,
            uptime_seconds: 3600,
        },
        network_metrics: NetworkMetrics {
            active_connections: 10,
            total_connections: 100,
            failed_connections: 5,
            connection_success_rate: 0.95,
            average_latency: Duration::from_millis(latency as u64),
            packet_loss_rate: 0.01,
            bandwidth_utilization: 0.5,
            peer_count: 50,
            route_count: 10,
        },
        error_metrics: ErrorMetrics {
            total_errors: 10,
            errors_by_layer: HashMap::new(),
            errors_by_type: HashMap::new(),
            error_rate,
            recent_errors: Vec::new(),
            critical_errors: 2,
            warning_errors: 5,
        },
        performance_metrics: crate::proto::PerformanceMetrics {
            cover_traffic_rate: 10.0,
            avg_latency_ms: latency,
            packet_loss_rate: 0.01,
            bandwidth_utilization: 0.5,
            cpu_usage: cpu / 100.0,
            memory_usage_mb: memory * 1024.0,
            total_packets_sent: 1000,
            total_packets_received: 950,
            retransmissions: 10,
            connection_success_rate: 0.95,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_enhanced_alert_system() {
        test_alert_system().await;
    }
}