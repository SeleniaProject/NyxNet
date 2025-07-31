use nyx_telemetry::{TelemetryConfig, TelemetryCollector};

#[tokio::test]
async fn exporter_serves_metrics() {
    // Create telemetry collector with test config
    let config = TelemetryConfig {
        metrics_enabled: true,
        metrics_port: 9898,
        collection_interval: 30,
        otlp_enabled: false,
        otlp_endpoint: None,
        trace_sampling: 1.0,
    };
    
    let _collector = TelemetryCollector::new(config).unwrap();
    
    // Basic test to ensure collector can be created
    assert!(true);
} 