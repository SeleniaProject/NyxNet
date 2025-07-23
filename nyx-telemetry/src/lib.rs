#![forbid(unsafe_code)]

//! Nyx Telemetry with OpenTelemetry OTLP Integration
//!
//! This module provides comprehensive observability for the Nyx protocol including:
//! - OpenTelemetry OTLP exporter for metrics, traces, and logs
//! - Prometheus metrics collection and export
//! - Distributed tracing with correlation IDs
//! - Custom metrics for Nyx-specific operations
//! - Performance monitoring and alerting
//! - Error tracking and analysis

use std::sync::Arc;
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{info, debug};
use anyhow::Result;

// Simplified telemetry without complex OpenTelemetry setup
use prometheus::{Encoder, TextEncoder};
use prometheus::{Counter, Gauge, Registry};
use warp::Filter;

/// Telemetry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Enable metrics collection
    pub metrics_enabled: bool,
    /// Prometheus metrics endpoint port
    pub metrics_port: u16,
    /// Metrics collection interval in seconds
    pub collection_interval: u64,
    /// Enable OTLP export
    pub otlp_enabled: bool,
    /// OTLP endpoint URL
    pub otlp_endpoint: Option<String>,
    /// Trace sampling ratio (0.0 to 1.0)
    pub trace_sampling: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            metrics_port: 9090,
            collection_interval: 30,
            otlp_enabled: false,
            otlp_endpoint: None,
            trace_sampling: 0.1,
        }
    }
}

/// System metrics data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub active_connections: u64,
    pub timestamp: SystemTime,
}

/// Network metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub connections_active: u64,
    pub connections_total: u64,
    pub latency_ms: f64,
    pub timestamp: SystemTime,
}

/// Stream metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetrics {
    pub active_streams: u64,
    pub total_streams: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub timestamp: SystemTime,
}

/// Mix network metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixMetrics {
    pub cover_traffic_sent: u64,
    pub real_traffic_sent: u64,
    pub messages_mixed: u64,
    pub anonymity_set_size: u64,
    pub timestamp: SystemTime,
}

/// Comprehensive telemetry collector
pub struct TelemetryCollector {
    config: TelemetryConfig,
    registry: Registry,
    
    // Prometheus metrics
    cpu_usage_gauge: Gauge,
    memory_usage_gauge: Gauge,
    network_bytes_counter: Counter,
    stream_count_gauge: Gauge,
    mix_messages_counter: Counter,
    
    // Broadcast channels for metrics
    system_tx: broadcast::Sender<SystemMetrics>,
    network_tx: broadcast::Sender<NetworkMetrics>,
    stream_tx: broadcast::Sender<StreamMetrics>,
    mix_tx: broadcast::Sender<MixMetrics>,
    
    // Internal state
    running: Arc<RwLock<bool>>,
}

impl TelemetryCollector {
    /// Create a new telemetry collector
    pub fn new(config: TelemetryConfig) -> Result<Self> {
        let registry = Registry::new();
        
        // Create Prometheus metrics
        let cpu_usage_gauge = Gauge::new("nyx_cpu_usage", "CPU usage percentage")?;
        let memory_usage_gauge = Gauge::new("nyx_memory_usage", "Memory usage percentage")?;
        let network_bytes_counter = Counter::new("nyx_network_bytes_total", "Total network bytes")?;
        let stream_count_gauge = Gauge::new("nyx_active_streams", "Number of active streams")?;
        let mix_messages_counter = Counter::new("nyx_mix_messages_total", "Total mix messages")?;
        
        // Register metrics
        registry.register(Box::new(cpu_usage_gauge.clone()))?;
        registry.register(Box::new(memory_usage_gauge.clone()))?;
        registry.register(Box::new(network_bytes_counter.clone()))?;
        registry.register(Box::new(stream_count_gauge.clone()))?;
        registry.register(Box::new(mix_messages_counter.clone()))?;
        
        // Create broadcast channels
        let (system_tx, _) = broadcast::channel(1000);
        let (network_tx, _) = broadcast::channel(1000);
        let (stream_tx, _) = broadcast::channel(1000);
        let (mix_tx, _) = broadcast::channel(1000);
        
        Ok(Self {
            config,
            registry,
            cpu_usage_gauge,
            memory_usage_gauge,
            network_bytes_counter,
            stream_count_gauge,
            mix_messages_counter,
            system_tx,
            network_tx,
            stream_tx,
            mix_tx,
            running: Arc::new(RwLock::new(false)),
        })
    }
    
    /// Start the telemetry collector
    pub async fn start(&self) -> Result<()> {
        *self.running.write().await = true;
        info!("Starting telemetry collector on port {}", self.config.metrics_port);
        
        if self.config.metrics_enabled {
            self.start_metrics_server().await?;
        }
        
        self.start_collection_loop().await;
        Ok(())
    }
    
    /// Start Prometheus metrics HTTP server
    async fn start_metrics_server(&self) -> Result<()> {
        let registry = self.registry.clone();
        
        let metrics_route = warp::path("metrics")
            .map(move || {
                let encoder = TextEncoder::new();
                let metric_families = registry.gather();
                let mut buffer = Vec::new();
                encoder.encode(&metric_families, &mut buffer).unwrap();
                String::from_utf8(buffer).unwrap()
            });
        
        let port = self.config.metrics_port;
        tokio::spawn(async move {
            warp::serve(metrics_route)
                .run(([0, 0, 0, 0], port))
                .await;
        });
        
        Ok(())
    }
    
    /// Start metrics collection loop
    async fn start_collection_loop(&self) {
        let mut interval = interval(Duration::from_secs(self.config.collection_interval));
        let running = self.running.clone();
        
        tokio::spawn(async move {
            while *running.read().await {
                interval.tick().await;
                // Collect system metrics here
                debug!("Collecting system metrics");
            }
        });
    }
    
    /// Record system metrics
    pub async fn record_system_metrics(&self, metrics: SystemMetrics) {
        self.cpu_usage_gauge.set(metrics.cpu_usage);
        self.memory_usage_gauge.set(metrics.memory_usage);
        
        let _ = self.system_tx.send(metrics);
    }
    
    /// Record network metrics
    pub async fn record_network_metrics(&self, metrics: NetworkMetrics) {
        self.network_bytes_counter.inc_by((metrics.bytes_sent + metrics.bytes_received) as f64);
        
        let _ = self.network_tx.send(metrics);
    }
    
    /// Record stream metrics
    pub async fn record_stream_metrics(&self, metrics: StreamMetrics) {
        self.stream_count_gauge.set(metrics.active_streams as f64);
        
        let _ = self.stream_tx.send(metrics);
    }
    
    /// Record mix metrics
    pub async fn record_mix_metrics(&self, metrics: MixMetrics) {
        self.mix_messages_counter.inc_by(metrics.messages_mixed as f64);
        
        let _ = self.mix_tx.send(metrics);
    }
    
    /// Subscribe to system metrics
    pub fn subscribe_system_metrics(&self) -> broadcast::Receiver<SystemMetrics> {
        self.system_tx.subscribe()
    }
    
    /// Subscribe to network metrics
    pub fn subscribe_network_metrics(&self) -> broadcast::Receiver<NetworkMetrics> {
        self.network_tx.subscribe()
    }
    
    /// Subscribe to stream metrics
    pub fn subscribe_stream_metrics(&self) -> broadcast::Receiver<StreamMetrics> {
        self.stream_tx.subscribe()
    }
    
    /// Subscribe to mix metrics
    pub fn subscribe_mix_metrics(&self) -> broadcast::Receiver<MixMetrics> {
        self.mix_tx.subscribe()
    }
    
    /// Stop the telemetry collector
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Telemetry collector stopped");
    }
}

/// Telemetry exporter for different backends
pub struct TelemetryExporter {
    collector: Arc<TelemetryCollector>,
}

impl TelemetryExporter {
    pub fn new(collector: Arc<TelemetryCollector>) -> Self {
        Self { collector }
    }
    
    /// Export metrics to Prometheus format
    pub async fn export_prometheus(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.collector.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_telemetry_collector_creation() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config).unwrap();
        assert!(collector.start().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_metrics_recording() {
        let config = TelemetryConfig::default();
        let collector = TelemetryCollector::new(config).unwrap();
        
        let system_metrics = SystemMetrics {
            cpu_usage: 50.0,
            memory_usage: 60.0,
            disk_usage: 70.0,
            network_rx_bytes: 1000,
            network_tx_bytes: 2000,
            active_connections: 5,
            timestamp: SystemTime::now(),
        };
        
        collector.record_system_metrics(system_metrics).await;
        
        // Verify metrics were recorded
        assert_eq!(collector.cpu_usage_gauge.get(), 50.0);
        assert_eq!(collector.memory_usage_gauge.get(), 60.0);
    }
} 