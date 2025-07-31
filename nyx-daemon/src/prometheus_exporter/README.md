# Nyx Daemon Prometheus Exporter

This module provides Prometheus metrics export functionality for the Nyx daemon, enabling comprehensive monitoring and observability.

## Features

- **Comprehensive Metrics Export**: Exports all daemon metrics in Prometheus format with proper labels
- **HTTP Server**: Built-in HTTP server for metrics scraping
- **Grafana Compatibility**: Structured metrics for easy Grafana dashboard creation
- **Health Indicators**: Binary health indicators for alerting
- **Configurable Updates**: Customizable metrics collection intervals

## Exported Metrics

### Basic Counters
- `nyx_requests_total` - Total number of requests processed
- `nyx_successful_requests_total` - Number of successful requests
- `nyx_failed_requests_total` - Number of failed requests
- `nyx_bytes_sent_total` - Total bytes sent
- `nyx_bytes_received_total` - Total bytes received
- `nyx_packets_sent_total` - Total packets sent
- `nyx_packets_received_total` - Total packets received
- `nyx_retransmissions_total` - Total packet retransmissions

### Connection Metrics
- `nyx_active_streams` - Current number of active streams
- `nyx_connected_peers` - Current number of connected peers
- `nyx_total_connections` - Total connections established
- `nyx_failed_connections` - Total failed connection attempts

### Performance Metrics
- `nyx_cover_traffic_rate_pps` - Cover traffic rate in packets per second
- `nyx_average_latency_ms` - Average latency in milliseconds
- `nyx_packet_loss_rate` - Packet loss rate (0.0-1.0)
- `nyx_bandwidth_utilization` - Bandwidth utilization (0.0-1.0)
- `nyx_cpu_usage` - CPU usage (0.0-1.0)
- `nyx_memory_usage_mb` - Memory usage in megabytes
- `nyx_connection_success_rate` - Connection success rate (0.0-1.0)

### System Metrics
- `nyx_uptime_seconds` - Daemon uptime in seconds
- `nyx_memory_rss_bytes` - Resident set size memory usage
- `nyx_memory_vms_bytes` - Virtual memory size
- `nyx_cpu_percent` - CPU usage percentage
- `nyx_mix_routes_count` - Number of mix routes

### Dashboard Metrics
- `nyx_dashboard_health_score` - Overall system health score (0.0-1.0)
- `nyx_dashboard_connection_success_rate` - Connection success rate for dashboards

### Health Indicators (Binary 0/1)
- `nyx_health_high_cpu_usage` - 1 if CPU usage > 80%
- `nyx_health_high_latency` - 1 if latency > 1000ms
- `nyx_health_high_packet_loss` - 1 if packet loss > 5%
- `nyx_health_low_connection_success` - 1 if connection success < 90%

## Usage

### Basic Usage

```rust
use std::sync::Arc;
use nyx_daemon::metrics::MetricsCollector;
use nyx_daemon::prometheus_exporter::{PrometheusExporter, PrometheusExporterBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize metrics collector
    let metrics = Arc::new(MetricsCollector::new());
    let _metrics_task = Arc::clone(&metrics).start_collection();
    
    // Create Prometheus exporter
    let prometheus_addr = "127.0.0.1:9090".parse().unwrap();
    let exporter = PrometheusExporterBuilder::new()
        .with_server_addr(prometheus_addr)
        .with_update_interval(Duration::from_secs(15))
        .build(metrics)?;
    
    // Start server and collection
    exporter.start_server().await?;
    exporter.start_collection().await?;
    
    println!("Prometheus metrics available at http://{}/metrics", prometheus_addr);
    
    // Keep running
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Advanced Configuration

```rust
use std::time::Duration;

let exporter = PrometheusExporterBuilder::new()
    .with_server_addr("0.0.0.0:9090".parse().unwrap())
    .with_update_interval(Duration::from_secs(10))
    .with_custom_label("instance".to_string(), "nyx-node-1".to_string())
    .build(metrics_collector)?;
```

### Without HTTP Server (Manual Export)

```rust
let exporter = PrometheusExporterBuilder::new()
    .without_server()
    .build(metrics_collector)?;

// Start collection only
exporter.start_collection().await?;

// Manually get metrics
let metrics_text = exporter.render_metrics();
println!("{}", metrics_text);
```

## Grafana Integration

The exporter provides metrics structured for easy Grafana dashboard creation:

### Sample Grafana Queries

**System Health Overview:**
```promql
nyx_dashboard_health_score
```

**Connection Success Rate:**
```promql
rate(nyx_successful_requests_total[5m]) / rate(nyx_requests_total[5m])
```

**Network Throughput:**
```promql
rate(nyx_bytes_sent_total[5m]) + rate(nyx_bytes_received_total[5m])
```

**Latency Percentiles:**
```promql
histogram_quantile(0.95, rate(nyx_average_latency_ms[5m]))
```

### Alerting Rules

**High CPU Usage:**
```promql
nyx_health_high_cpu_usage == 1
```

**High Latency:**
```promql
nyx_health_high_latency == 1
```

**Low Connection Success:**
```promql
nyx_health_low_connection_success == 1
```

## Configuration

### Environment Variables

- `NYX_PROMETHEUS_ADDR` - Prometheus server address (default: "127.0.0.1:9090")
- `NYX_PROMETHEUS_INTERVAL` - Update interval in seconds (default: 15)

### Configuration File

```toml
[prometheus]
enabled = true
address = "0.0.0.0:9090"
update_interval = 15
enable_dashboard_metrics = true
enable_health_indicators = true

[prometheus.labels]
instance = "nyx-node-1"
environment = "production"
```

## Metrics Endpoint

Once running, metrics are available at:
- `http://localhost:9090/metrics` - Prometheus metrics
- `http://localhost:9090/health` - Health check endpoint

## Error Handling

The exporter includes comprehensive error handling:

```rust
match exporter.start_collection().await {
    Ok(_) => println!("Prometheus collection started"),
    Err(PrometheusError::AlreadyRunning) => {
        println!("Collection already running");
    }
    Err(PrometheusError::InitializationFailed(msg)) => {
        eprintln!("Failed to initialize: {}", msg);
    }
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

## Performance Considerations

- Default update interval is 15 seconds to balance freshness with performance
- Metrics are collected asynchronously to avoid blocking the main daemon
- HTTP server runs on a separate task
- Memory usage is bounded by the metrics history size

## Testing

```bash
# Check if metrics are being exported
curl http://localhost:9090/metrics

# Test health endpoint
curl http://localhost:9090/health

# Validate Prometheus format
promtool query instant 'nyx_dashboard_health_score'
```