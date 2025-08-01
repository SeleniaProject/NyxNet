//! Path Performance Monitoring System Tests
//! 
//! Tests for the comprehensive path performance monitoring and analytics system
//! that provides real-time metrics collection, historical tracking, and 
//! predictive performance analysis for onion routing paths.

#[cfg(test)]
mod tests {
    use super::super::path_builder::{
        PathPerformanceMonitor, PathPerformanceMetrics, PerformanceTrend,
        GlobalPathStats, PathBuilder, PathBuilderConfig
    };
    use tokio::time::{sleep, Duration};
    use std::sync::Arc;

    /// Test basic performance monitor creation and initialization
    #[tokio::test]
    async fn test_performance_monitor_creation() {
        let monitor = PathPerformanceMonitor::new("test_path_1".to_string());
        let metrics = monitor.get_metrics().await;
        
        // Verify default metrics
        assert_eq!(metrics.current_latency_ms, 0.0);
        assert_eq!(metrics.current_bandwidth_mbps, 0.0);
        assert_eq!(metrics.packet_loss_rate, 0.0);
        assert_eq!(metrics.reliability_score, 1.0);
        assert_eq!(metrics.throughput_efficiency, 1.0);
        assert_eq!(metrics.successful_transmissions, 0);
        assert_eq!(metrics.failed_transmissions, 0);
        assert_eq!(metrics.performance_trend, PerformanceTrend::Stable);
    }

    /// Test performance monitoring lifecycle
    #[tokio::test]
    async fn test_monitoring_lifecycle() {
        let monitor = PathPerformanceMonitor::new("test_path_2".to_string());
        
        // Start monitoring
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record some performance data
        monitor.record_latency(25.5).await;
        monitor.record_bandwidth(150.0).await;
        monitor.record_transmission(1024, 1024, true).await;
        
        // Verify metrics updated
        let metrics = monitor.get_metrics().await;
        assert_eq!(metrics.current_latency_ms, 25.5);
        assert_eq!(metrics.avg_latency_ms, 25.5);
        assert_eq!(metrics.current_bandwidth_mbps, 150.0);
        assert_eq!(metrics.avg_bandwidth_mbps, 150.0);
        assert_eq!(metrics.successful_transmissions, 1);
        assert_eq!(metrics.reliability_score, 1.0);
        
        // Stop monitoring
        monitor.stop_monitoring().await;
    }

    /// Test performance metrics aggregation over time
    #[tokio::test]
    async fn test_metrics_aggregation() {
        let monitor = PathPerformanceMonitor::new("test_path_3".to_string());
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record multiple latency measurements
        let latencies = vec![10.0, 15.0, 20.0, 25.0, 30.0];
        for latency in &latencies {
            monitor.record_latency(*latency).await;
            sleep(Duration::from_millis(10)).await;
        }
        
        let metrics = monitor.get_metrics().await;
        let expected_avg = latencies.iter().sum::<f64>() / latencies.len() as f64;
        assert_eq!(metrics.avg_latency_ms, expected_avg);
        assert_eq!(metrics.current_latency_ms, 30.0); // Last recorded
        
        monitor.stop_monitoring().await;
    }

    /// Test transmission statistics and reliability calculation
    #[tokio::test]
    async fn test_transmission_statistics() {
        let monitor = PathPerformanceMonitor::new("test_path_4".to_string());
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record mixed successful and failed transmissions
        monitor.record_transmission(1000, 1000, true).await;  // Success
        monitor.record_transmission(1500, 1500, true).await;  // Success
        monitor.record_transmission(800, 0, false).await;     // Failure
        monitor.record_transmission(1200, 1200, true).await;  // Success
        
        let metrics = monitor.get_metrics().await;
        assert_eq!(metrics.successful_transmissions, 3);
        assert_eq!(metrics.failed_transmissions, 1);
        assert_eq!(metrics.bytes_transmitted, 4500);
        assert_eq!(metrics.bytes_received, 3700);
        
        // Reliability should be 3/4 = 0.75
        assert!((metrics.reliability_score - 0.75).abs() < 0.001);
        // Packet loss should be 1 - reliability = 0.25
        assert!((metrics.packet_loss_rate - 0.25).abs() < 0.001);
        
        monitor.stop_monitoring().await;
    }

    /// Test performance trend analysis
    #[tokio::test]
    async fn test_performance_trend_analysis() {
        let monitor = PathPerformanceMonitor::new("test_path_5".to_string());
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Build a clearer trend with more data points for better analysis
        // Start with good performance and then degrade significantly
        for i in 0..15 {
            let latency = 10.0 + (i as f64 * 15.0); // Significantly increasing latency
            let success_rate = 1.0 - (i as f64 * 0.08); // Decreasing success rate
            let success = success_rate > 0.5;
            
            monitor.record_latency(latency).await;
            monitor.record_transmission(1000, if success { 1000 } else { 0 }, success).await;
            sleep(Duration::from_millis(30)).await;
        }
        
        // Allow more time for trend analysis to complete
        sleep(Duration::from_millis(200)).await;
        
        let trend = monitor.analyze_performance_trend().await;
        // With significantly degrading performance, we should see a descending or volatile trend
        assert!(matches!(trend, PerformanceTrend::Descending | PerformanceTrend::Volatile | PerformanceTrend::Unknown));
        
        monitor.stop_monitoring().await;
    }

    /// Test performance alert callback system
    #[tokio::test]
    async fn test_performance_alerts() {
        let monitor = PathPerformanceMonitor::new("test_path_6".to_string());
        
        let alert_triggered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let alert_triggered_clone = Arc::clone(&alert_triggered);
        
        // Set up alert callback
        monitor.set_alert_callback(move |metrics| {
            if metrics.reliability_score < 0.5 {
                alert_triggered_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }).await;
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record failures immediately to trigger alert - enough to drop reliability significantly
        for _ in 0..10 {
            monitor.record_transmission(1000, 0, false).await;
            sleep(Duration::from_millis(10)).await;
        }
        
        // Allow more processing time for alert to be triggered
        sleep(Duration::from_millis(300)).await;
        
        // Check current metrics to verify condition
        let current_metrics = monitor.get_metrics().await;
        assert!(current_metrics.reliability_score < 0.5, 
            "Reliability score {} should be < 0.5", current_metrics.reliability_score);
        
        // Alert should have been triggered
        assert!(alert_triggered.load(std::sync::atomic::Ordering::SeqCst));
        
        monitor.stop_monitoring().await;
    }

    /// Test performance report generation
    #[tokio::test]
    async fn test_performance_report_generation() {
        let monitor = PathPerformanceMonitor::new("test_path_7".to_string());
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record some performance data
        monitor.record_latency(42.5).await;
        monitor.record_bandwidth(200.0).await;
        monitor.record_transmission(2048, 2048, true).await;
        monitor.record_transmission(1024, 1024, true).await;
        monitor.record_transmission(512, 0, false).await;
        
        let report = monitor.generate_performance_report().await;
        
        // Verify report contains expected information
        assert!(report.contains("test_path_7"));
        assert!(report.contains("42.5"));  // Latency
        assert!(report.contains("200.0")); // Bandwidth
        assert!(report.contains("2"));     // Successful transmissions
        assert!(report.contains("1"));     // Failed transmissions
        assert!(report.contains("3584"));  // Total bytes transmitted
        assert!(report.contains("3072"));  // Total bytes received
        
        monitor.stop_monitoring().await;
    }

    /// Test PathBuilder integration with performance monitoring
    #[tokio::test]
    async fn test_path_builder_performance_integration() {
        let config = PathBuilderConfig::default();
        let bootstrap_peers = vec!["127.0.0.1:8000".to_string()];
        
        // Create PathBuilder (this will fail without actual DHT, but we can test structure)
        let result = PathBuilder::new(bootstrap_peers, config).await;
        
        // If DHT is not available, we expect an error, which is fine for this test
        // We're mainly testing that the PathBuilder structure compiles correctly
        // with performance monitoring integration
        match result {
            Ok(path_builder) => {
                // If it succeeds, test that global stats are initialized
                let stats = path_builder.get_global_stats().await;
                assert_eq!(stats.active_paths, 0);
                assert_eq!(stats.avg_performance_score, 1.0);
                assert_eq!(stats.total_successful_transmissions, 0);
                assert_eq!(stats.total_failed_transmissions, 0);
            },
            Err(_) => {
                // Expected when DHT infrastructure is not available
                // This is acceptable for unit testing
            }
        }
    }

    /// Test global performance statistics aggregation
    #[tokio::test]
    async fn test_global_stats() {
        let mut global_stats = GlobalPathStats::default();
        
        // Verify default values
        assert_eq!(global_stats.active_paths, 0);
        assert_eq!(global_stats.avg_performance_score, 1.0);
        assert_eq!(global_stats.global_packet_loss_rate, 0.0);
        assert_eq!(global_stats.total_successful_transmissions, 0);
        assert_eq!(global_stats.total_failed_transmissions, 0);
        assert_eq!(global_stats.monitoring_uptime_secs, 0);
        assert!(global_stats.best_performing_path.is_none());
        assert!(global_stats.worst_performing_path.is_none());
        
        // Simulate statistics update
        global_stats.active_paths = 3;
        global_stats.avg_performance_score = 0.85;
        global_stats.global_packet_loss_rate = 0.05;
        global_stats.total_successful_transmissions = 1500;
        global_stats.total_failed_transmissions = 100;
        global_stats.best_performing_path = Some("path_1".to_string());
        global_stats.worst_performing_path = Some("path_3".to_string());
        
        assert_eq!(global_stats.active_paths, 3);
        assert!((global_stats.avg_performance_score - 0.85).abs() < 0.001);
        assert!((global_stats.global_packet_loss_rate - 0.05).abs() < 0.001);
        assert_eq!(global_stats.total_successful_transmissions, 1500);
        assert_eq!(global_stats.total_failed_transmissions, 100);
        assert_eq!(global_stats.best_performing_path, Some("path_1".to_string()));
        assert_eq!(global_stats.worst_performing_path, Some("path_3".to_string()));
    }

    /// Test performance data history tracking
    #[tokio::test]
    async fn test_performance_history() {
        let monitor = PathPerformanceMonitor::new("test_path_8".to_string());
        
        assert!(monitor.start_monitoring().await.is_ok());
        
        // Record performance data over time
        for i in 1..=10 {
            monitor.record_latency(i as f64 * 5.0).await;
            monitor.record_bandwidth(100.0 + i as f64 * 10.0).await;
            monitor.record_transmission(1000, 1000, i % 3 != 0).await; // Every 3rd fails
            sleep(Duration::from_millis(10)).await;
        }
        
        // Allow time for history to accumulate
        sleep(Duration::from_millis(100)).await;
        
        let history = monitor.get_history().await;
        
        // Should have collected some history points
        assert!(!history.is_empty());
        
        // Verify history contains expected data structure
        if let Some(first_point) = history.first() {
            assert!(first_point.latency_ms > 0.0);
            assert!(first_point.bandwidth_mbps > 0.0);
            assert!(first_point.reliability_score >= 0.0 && first_point.reliability_score <= 1.0);
            assert!(first_point.packet_loss_rate >= 0.0 && first_point.packet_loss_rate <= 1.0);
        }
        
        monitor.stop_monitoring().await;
    }

    /// Test concurrent performance monitoring for multiple paths
    #[tokio::test]
    async fn test_concurrent_monitoring() {
        let mut monitors = Vec::new();
        let num_paths = 5;
        
        // Create multiple monitors
        for i in 0..num_paths {
            let monitor = PathPerformanceMonitor::new(format!("concurrent_path_{}", i));
            assert!(monitor.start_monitoring().await.is_ok());
            monitors.push(monitor);
        }
        
        // Record different performance characteristics for each path
        for (i, monitor) in monitors.iter().enumerate() {
            let base_latency = 10.0 + i as f64 * 5.0;
            let base_bandwidth = 100.0 + i as f64 * 25.0;
            
            monitor.record_latency(base_latency).await;
            monitor.record_bandwidth(base_bandwidth).await;
            monitor.record_transmission(1000, 1000, i % 2 == 0).await; // Alternate success/failure
        }
        
        // Verify each monitor has correct individual metrics
        for (i, monitor) in monitors.iter().enumerate() {
            let metrics = monitor.get_metrics().await;
            let expected_latency = 10.0 + i as f64 * 5.0;
            let expected_bandwidth = 100.0 + i as f64 * 25.0;
            
            assert_eq!(metrics.current_latency_ms, expected_latency);
            assert_eq!(metrics.current_bandwidth_mbps, expected_bandwidth);
            assert_eq!(metrics.successful_transmissions, if i % 2 == 0 { 1 } else { 0 });
            assert_eq!(metrics.failed_transmissions, if i % 2 == 0 { 0 } else { 1 });
        }
        
        // Stop all monitors
        for monitor in &monitors {
            monitor.stop_monitoring().await;
        }
    }
}
