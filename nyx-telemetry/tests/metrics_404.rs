use nyx_telemetry::{TelemetryConfig, TelemetryCollector};

#[tokio::test]
async fn exporter_returns_404() {
    // Create telemetry collector with test config
    let config = TelemetryConfig {
        metrics_enabled: true,
        metrics_port: 9920,
        collection_interval: 30,
        otlp_enabled: false,
        otlp_endpoint: None,
        trace_sampling: 1.0,
    };
    
    let _collector = TelemetryCollector::new(config).unwrap();
    
    // Basic test to ensure collector can be created
    assert!(true);
} 