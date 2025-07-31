use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use metrics::{Counter, Gauge, Histogram, Key, Label, Unit};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::metrics::{MetricsCollector, LayerType, LayerStatus};

/// Prometheus exporter for Nyx daemon metrics
pub struct PrometheusExporter {
    handle: PrometheusHandle,
    metrics_collector: Arc<MetricsCollector>,
    server_addr: Option<SocketAddr>,
    update_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl PrometheusExporter {
    /// Create a new Prometheus exporter
    pub fn new(
        metrics_collector: Arc<MetricsCollector>,
        server_addr: Option<SocketAddr>,
    ) -> Result<Self, PrometheusError> {
        let builder = PrometheusBuilder::new();
        let recorder = builder.build_recorder();
        let handle = recorder.handle();
        
        // Install the recorder globally
        metrics::set_global_recorder(recorder)
            .map_err(|e| PrometheusError::InitializationFailed(e.to_string()))?;
        
        Ok(Self {
            handle,
            metrics_collector,
            server_addr,
            update_interval: Duration::from_secs(15), // Default 15 second updates
            running: Arc::new(RwLock::new(false)),
        })
    }
    
    /// Set the update interval for metrics collection
    pub fn with_update_interval(mut self, interval: Duration) -> Self {
        self.update_interval = interval;
        self
    }
    
    /// Start the Prometheus metrics server
    pub async fn start_server(&self) -> Result<(), PrometheusError> {
        if let Some(addr) = self.server_addr {
            let handle = self.handle.clone();
            
            tokio::spawn(async move {
                let app = axum::Router::new()
                    .route("/metrics", axum::routing::get(move || async move {
                        handle.render()
                    }))
                    .route("/health", axum::routing::get(|| async { "OK" }));
                
                let listener = tokio::net::TcpListener::bind(addr).await
                    .map_err(|e| {
                        error!("Failed to bind Prometheus server to {}: {}", addr, e);
                        e
                    })?;
                
                info!("Prometheus metrics server listening on {}", addr);
                
                axum::serve(listener, app).await
                    .map_err(|e| {
                        error!("Prometheus server error: {}", e);
                        e
                    })
            });
        }
        
        Ok(())
    }
    
    /// Start the metrics collection and export loop
    pub async fn start_collection(&self) -> Result<(), PrometheusError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(PrometheusError::AlreadyRunning);
        }
        *running = true;
        
        let metrics_collector = Arc::clone(&self.metrics_collector);
        let update_interval = self.update_interval;
        let running_flag = Arc::clone(&self.running);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(update_interval);
            
            while *running_flag.read().await {
                interval.tick().await;
                
                if let Err(e) = Self::export_basic_metrics(&metrics_collector).await {
                    error!("Failed to export metrics: {}", e);
                }
            }
            
            info!("Prometheus metrics collection stopped");
        });
        
        info!("Started Prometheus metrics collection with {}s interval", 
              update_interval.as_secs());
        
        Ok(())
    }
    
    /// Stop the metrics collection
    pub async fn stop_collection(&self) {
        let mut running = self.running.write().await;
        *running = false;
        info!("Stopping Prometheus metrics collection");
    }
    
    /// Export basic metrics from MetricsCollector to Prometheus format
    async fn export_basic_metrics(collector: &Arc<MetricsCollector>) -> Result<(), PrometheusError> {
        // Export basic counters
        metrics::counter!("nyx_requests_total")
            .absolute(collector.total_requests.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_successful_requests_total")
            .absolute(collector.successful_requests.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_failed_requests_total")
            .absolute(collector.failed_requests.load(std::sync::atomic::Ordering::Relaxed));
        
        // Export network metrics
        metrics::counter!("nyx_bytes_sent_total")
            .absolute(collector.bytes_sent.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_bytes_received_total")
            .absolute(collector.bytes_received.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_packets_sent_total")
            .absolute(collector.packets_sent.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_packets_received_total")
            .absolute(collector.packets_received.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_retransmissions_total")
            .absolute(collector.retransmissions.load(std::sync::atomic::Ordering::Relaxed));
        
        // Export connection metrics
        metrics::gauge!("nyx_active_streams")
            .set(collector.active_streams.load(std::sync::atomic::Ordering::Relaxed) as f64);
        metrics::gauge!("nyx_connected_peers")
            .set(collector.connected_peers.load(std::sync::atomic::Ordering::Relaxed) as f64);
        metrics::counter!("nyx_total_connections")
            .absolute(collector.total_connections.load(std::sync::atomic::Ordering::Relaxed));
        metrics::counter!("nyx_failed_connections")
            .absolute(collector.failed_connections.load(std::sync::atomic::Ordering::Relaxed));
        
        // Export performance metrics
        {
            let cover_traffic_rate = collector.cover_traffic_rate.read().await;
            metrics::gauge!("nyx_cover_traffic_rate_pps")
                .set(*cover_traffic_rate);
        }
        
        {
            let avg_latency = collector.avg_latency_ms.read().await;
            metrics::gauge!("nyx_average_latency_ms")
                .set(*avg_latency);
        }
        
        {
            let packet_loss_rate = collector.packet_loss_rate.read().await;
            metrics::gauge!("nyx_packet_loss_rate")
                .set(*packet_loss_rate);
        }
        
        {
            let bandwidth_utilization = collector.bandwidth_utilization.read().await;
            metrics::gauge!("nyx_bandwidth_utilization")
                .set(*bandwidth_utilization);
        }
        
        // Export uptime
        let uptime = collector.get_uptime();
        metrics::gauge!("nyx_uptime_seconds")
            .set(uptime.as_secs() as f64);
        
        // Export performance metrics from the collector
        let perf_metrics = collector.get_performance_metrics().await;
        metrics::gauge!("nyx_cpu_usage")
            .set(perf_metrics.cpu_usage);
        metrics::gauge!("nyx_memory_usage_mb")
            .set(perf_metrics.memory_usage_mb);
        metrics::gauge!("nyx_connection_success_rate")
            .set(perf_metrics.connection_success_rate);
        
        // Export resource usage if available
        if let Some(resource_usage) = collector.get_resource_usage().await {
            metrics::gauge!("nyx_memory_rss_bytes")
                .set(resource_usage.memory_rss_bytes as f64);
            metrics::gauge!("nyx_memory_vms_bytes")
                .set(resource_usage.memory_vms_bytes as f64);
            metrics::gauge!("nyx_cpu_percent")
                .set(resource_usage.cpu_percent);
            metrics::counter!("nyx_network_bytes_sent_total")
                .absolute(resource_usage.network_bytes_sent);
            metrics::counter!("nyx_network_bytes_received_total")
                .absolute(resource_usage.network_bytes_received);
        }
        
        // Export mix routes count
        let mix_routes = collector.get_mix_routes().await;
        metrics::gauge!("nyx_mix_routes_count")
            .set(mix_routes.len() as f64);
        
        // Export bandwidth, CPU, and memory samples for trend analysis
        let bandwidth_samples = collector.get_bandwidth_samples().await;
        if !bandwidth_samples.is_empty() {
            let avg_bandwidth = bandwidth_samples.iter().sum::<f64>() / bandwidth_samples.len() as f64;
            metrics::gauge!("nyx_bandwidth_average_mbps")
                .set(avg_bandwidth);
        }
        
        let cpu_samples = collector.get_cpu_samples().await;
        if !cpu_samples.is_empty() {
            let avg_cpu = cpu_samples.iter().sum::<f64>() / cpu_samples.len() as f64;
            metrics::gauge!("nyx_cpu_average_percent")
                .set(avg_cpu);
        }
        
        let memory_samples = collector.get_memory_samples().await;
        if !memory_samples.is_empty() {
            let avg_memory = memory_samples.iter().sum::<f64>() / memory_samples.len() as f64;
            metrics::gauge!("nyx_memory_average_percent")
                .set(avg_memory);
        }
        
        Ok(())
    }
    
    /// Export custom metrics for Grafana dashboard compatibility
    pub async fn export_grafana_metrics(&self) -> Result<(), PrometheusError> {
        // Export additional dashboard-friendly metrics
        self.export_dashboard_summary().await?;
        self.export_health_indicators().await?;
        
        Ok(())
    }
    
    /// Export dashboard summary metrics
    async fn export_dashboard_summary(&self) -> Result<(), PrometheusError> {
        let collector = &self.metrics_collector;
        
        // Calculate overall system health score (simplified)
        let mut health_score: f64 = 1.0;
        
        // Get performance metrics
        let perf_metrics = collector.get_performance_metrics().await;
        
        // Penalize high CPU usage
        if perf_metrics.cpu_usage > 0.8 {
            health_score -= 0.2;
        } else if perf_metrics.cpu_usage > 0.6 {
            health_score -= 0.1;
        }
        
        // Penalize high packet loss
        if perf_metrics.packet_loss_rate > 0.1 {
            health_score -= 0.3;
        } else if perf_metrics.packet_loss_rate > 0.05 {
            health_score -= 0.1;
        }
        
        // Penalize high latency
        if perf_metrics.avg_latency_ms > 1000.0 {
            health_score -= 0.2;
        } else if perf_metrics.avg_latency_ms > 500.0 {
            health_score -= 0.1;
        }
        
        metrics::gauge!("nyx_dashboard_health_score")
            .set(health_score.max(0.0).min(1.0));
        
        // Export connection success rate
        let total_attempts = collector.connection_attempts.load(std::sync::atomic::Ordering::Relaxed);
        let successful = collector.successful_connections.load(std::sync::atomic::Ordering::Relaxed);
        let success_rate = if total_attempts > 0 {
            successful as f64 / total_attempts as f64
        } else {
            1.0
        };
        
        metrics::gauge!("nyx_dashboard_connection_success_rate")
            .set(success_rate);
        
        Ok(())
    }
    
    /// Export health indicators for alerting
    async fn export_health_indicators(&self) -> Result<(), PrometheusError> {
        let perf_metrics = self.metrics_collector.get_performance_metrics().await;
        
        // Critical thresholds as binary indicators (0 or 1)
        let high_cpu = if perf_metrics.cpu_usage > 0.8 { 1.0 } else { 0.0 };
        metrics::gauge!("nyx_health_high_cpu_usage")
            .set(high_cpu);
        
        let high_latency = if perf_metrics.avg_latency_ms > 1000.0 { 1.0 } else { 0.0 };
        metrics::gauge!("nyx_health_high_latency")
            .set(high_latency);
        
        let high_packet_loss = if perf_metrics.packet_loss_rate > 0.05 { 1.0 } else { 0.0 };
        metrics::gauge!("nyx_health_high_packet_loss")
            .set(high_packet_loss);
        
        let low_connection_success = if perf_metrics.connection_success_rate < 0.9 { 1.0 } else { 0.0 };
        metrics::gauge!("nyx_health_low_connection_success")
            .set(low_connection_success);
        
        Ok(())
    }
    
    /// Get the current Prometheus metrics as a string
    pub fn render_metrics(&self) -> String {
        self.handle.render()
    }
    
    /// Helper function to convert LayerType to string
    fn layer_type_to_string(layer_type: &LayerType) -> String {
        match layer_type {
            LayerType::Crypto => "crypto".to_string(),
            LayerType::Stream => "stream".to_string(),
            LayerType::Mix => "mix".to_string(),
            LayerType::Fec => "fec".to_string(),
            LayerType::Transport => "transport".to_string(),
        }
    }
    
    /// Helper function to convert LayerStatus to string
    fn layer_status_to_string(status: &LayerStatus) -> String {
        match status {
            LayerStatus::Initializing => "initializing".to_string(),
            LayerStatus::Active => "active".to_string(),
            LayerStatus::Degraded => "degraded".to_string(),
            LayerStatus::Failed => "failed".to_string(),
            LayerStatus::Shutdown => "shutdown".to_string(),
        }
    }
}

/// Errors that can occur during Prometheus export
#[derive(Debug, thiserror::Error)]
pub enum PrometheusError {
    #[error("Prometheus exporter initialization failed: {0}")]
    InitializationFailed(String),
    
    #[error("Prometheus exporter is already running")]
    AlreadyRunning,
    
    #[error("Failed to export metrics: {0}")]
    ExportFailed(String),
    
    #[error("Server error: {0}")]
    ServerError(String),
    
    #[error("Metrics collection error: {0}")]
    CollectionError(String),
}

/// Configuration for Prometheus exporter
#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    pub server_addr: Option<SocketAddr>,
    pub update_interval: Duration,
    pub enable_dashboard_metrics: bool,
    pub enable_health_indicators: bool,
    pub enable_trend_metrics: bool,
    pub custom_labels: HashMap<String, String>,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            server_addr: Some("127.0.0.1:9090".parse().unwrap()),
            update_interval: Duration::from_secs(15),
            enable_dashboard_metrics: true,
            enable_health_indicators: true,
            enable_trend_metrics: true,
            custom_labels: HashMap::new(),
        }
    }
}

/// Builder for PrometheusExporter with configuration options
pub struct PrometheusExporterBuilder {
    config: PrometheusConfig,
}

impl PrometheusExporterBuilder {
    pub fn new() -> Self {
        Self {
            config: PrometheusConfig::default(),
        }
    }
    
    pub fn with_server_addr(mut self, addr: SocketAddr) -> Self {
        self.config.server_addr = Some(addr);
        self
    }
    
    pub fn without_server(mut self) -> Self {
        self.config.server_addr = None;
        self
    }
    
    pub fn with_update_interval(mut self, interval: Duration) -> Self {
        self.config.update_interval = interval;
        self
    }
    
    pub fn with_custom_label(mut self, key: String, value: String) -> Self {
        self.config.custom_labels.insert(key, value);
        self
    }
    
    pub fn build(
        self,
        metrics_collector: Arc<MetricsCollector>,
    ) -> Result<PrometheusExporter, PrometheusError> {
        PrometheusExporter::new(metrics_collector, self.config.server_addr)
            .map(|exporter| exporter.with_update_interval(self.config.update_interval))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricsCollector;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_prometheus_exporter_creation() {
        let metrics_collector = Arc::new(MetricsCollector::new());
        
        let exporter = PrometheusExporter::new(metrics_collector, None);
        assert!(exporter.is_ok());
    }
    
    #[tokio::test]
    async fn test_metrics_export() {
        let metrics_collector = Arc::new(MetricsCollector::new());
        
        let exporter = PrometheusExporter::new(metrics_collector, None).unwrap();
        
        // Test that we can render metrics without errors
        let metrics_output = exporter.render_metrics();
        assert!(!metrics_output.is_empty());
    }
    
    #[tokio::test]
    async fn test_builder_pattern() {
        let metrics_collector = Arc::new(MetricsCollector::new());
        
        let exporter = PrometheusExporterBuilder::new()
            .with_update_interval(Duration::from_secs(30))
            .with_custom_label("instance".to_string(), "test".to_string())
            .without_server()
            .build(metrics_collector);
        
        assert!(exporter.is_ok());
    }
}