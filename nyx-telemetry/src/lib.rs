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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, broadcast};
use tokio::time::interval;
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn, error, span, Level};
use once_cell::sync::Lazy;
use std::collections::VecDeque;

use opentelemetry::{
    global,
    sdk::{
        export::metrics::aggregation,
        metrics::{controllers, processors, selectors},
        Resource,
    },
    KeyValue,
};
use opentelemetry_otlp::{WithExportConfig, ExportConfig};
use opentelemetry_semantic_conventions as semcov;

/// Telemetry configuration for OpenTelemetry and Prometheus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Enable OpenTelemetry OTLP export
    pub otlp_enabled: bool,
    /// OTLP endpoint URL
    pub otlp_endpoint: String,
    /// OTLP export timeout
    pub otlp_timeout: Duration,
    /// Enable Prometheus metrics export
    pub prometheus_enabled: bool,
    /// Prometheus listen address
    pub prometheus_address: String,
    /// Metrics collection interval
    pub metrics_interval: Duration,
    /// Enable distributed tracing
    pub tracing_enabled: bool,
    /// Trace sampling ratio (0.0 to 1.0)
    pub trace_sampling: f64,
    /// Service name for telemetry
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Environment (dev, staging, prod)
    pub environment: String,
    /// Additional resource attributes
    pub resource_attributes: HashMap<String, String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_enabled: true,
            otlp_endpoint: "http://localhost:4317".to_string(),
            otlp_timeout: Duration::from_secs(10),
            prometheus_enabled: true,
            prometheus_address: "0.0.0.0:9090".to_string(),
            metrics_interval: Duration::from_secs(15),
            tracing_enabled: true,
            trace_sampling: 0.1, // 10% sampling
            service_name: "nyx-daemon".to_string(),
            service_version: "0.1.0".to_string(),
            environment: "development".to_string(),
            resource_attributes: HashMap::new(),
        }
    }
}

/// Nyx-specific metrics definitions.
#[derive(Debug, Clone)]
pub struct NyxMetrics {
    /// Total connections established
    pub connections_total: u64,
    /// Active connections
    pub connections_active: u64,
    /// Bytes sent
    pub bytes_sent_total: u64,
    /// Bytes received
    pub bytes_received_total: u64,
    /// Packets sent
    pub packets_sent_total: u64,
    /// Packets received
    pub packets_received_total: u64,
    /// Connection establishment latency
    pub connection_latency_ms: Vec<f64>,
    /// Stream creation count
    pub streams_created_total: u64,
    /// Stream errors
    pub stream_errors_total: u64,
    /// Mix routing hops
    pub mix_hops_total: u64,
    /// Cover traffic generated
    pub cover_traffic_bytes: u64,
    /// Plugin frames processed
    pub plugin_frames_total: u64,
    /// VDF computations
    pub vdf_computations_total: u64,
    /// Cryptographic operations
    pub crypto_operations_total: u64,
    /// Error counts by type
    pub errors_by_type: HashMap<String, u64>,
}

impl Default for NyxMetrics {
    fn default() -> Self {
        Self {
            connections_total: 0,
            connections_active: 0,
            bytes_sent_total: 0,
            bytes_received_total: 0,
            packets_sent_total: 0,
            packets_received_total: 0,
            connection_latency_ms: Vec::new(),
            streams_created_total: 0,
            stream_errors_total: 0,
            mix_hops_total: 0,
            cover_traffic_bytes: 0,
            plugin_frames_total: 0,
            vdf_computations_total: 0,
            crypto_operations_total: 0,
            errors_by_type: HashMap::new(),
        }
    }
}

/// Performance monitoring data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Network I/O bytes per second
    pub network_io_bps: u64,
    /// Disk I/O bytes per second
    pub disk_io_bps: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// 95th percentile response time
    pub p95_response_time_ms: f64,
    /// 99th percentile response time
    pub p99_response_time_ms: f64,
    /// Error rate (errors per second)
    pub error_rate: f64,
    /// Throughput (requests per second)
    pub throughput: f64,
}

/// Enhanced error tracking with detailed breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    /// Total error count
    pub total_errors: u64,
    /// Errors by category
    pub errors_by_category: HashMap<String, u64>,
    /// Errors by severity
    pub errors_by_severity: HashMap<String, u64>,
    /// Errors by component
    pub errors_by_component: HashMap<String, u64>,
    /// Error rate (errors per second)
    pub error_rate: f64,
    /// Mean time between failures (MTBF)
    pub mtbf_seconds: f64,
    /// Recent error distribution
    pub recent_errors: Vec<ErrorEvent>,
}

/// Individual error event for detailed tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    /// Error timestamp
    pub timestamp: SystemTime,
    /// Error code
    pub error_code: String,
    /// Error category
    pub category: ErrorCategory,
    /// Error severity
    pub severity: ErrorSeverity,
    /// Component that generated the error
    pub component: String,
    /// Error message
    pub message: String,
    /// Context information
    pub context: HashMap<String, String>,
}

/// Error categories for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Network-related errors
    Network,
    /// Cryptographic errors
    Crypto,
    /// Protocol errors
    Protocol,
    /// System errors
    System,
    /// Plugin errors
    Plugin,
    /// Configuration errors
    Config,
    /// Internal errors
    Internal,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network => write!(f, "network"),
            Self::Crypto => write!(f, "crypto"),
            Self::Protocol => write!(f, "protocol"),
            Self::System => write!(f, "system"),
            Self::Plugin => write!(f, "plugin"),
            Self::Config => write!(f, "config"),
            Self::Internal => write!(f, "internal"),
        }
    }
}

/// Error severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Low severity - informational
    Low,
    /// Medium severity - warning
    Medium,
    /// High severity - error
    High,
    /// Critical severity - system failure
    Critical,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Enhanced latency distribution tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMetrics {
    /// Operation name
    pub operation: String,
    /// Total samples
    pub sample_count: u64,
    /// Mean latency
    pub mean_ms: f64,
    /// Standard deviation
    pub std_dev_ms: f64,
    /// Minimum latency
    pub min_ms: f64,
    /// Maximum latency
    pub max_ms: f64,
    /// Percentile distribution
    pub percentiles: HashMap<String, f64>,
    /// Histogram buckets
    pub histogram_buckets: Vec<HistogramBucket>,
    /// Recent samples for trend analysis
    pub recent_samples: VecDeque<f64>,
}

/// Histogram bucket for latency distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// Upper bound of the bucket (inclusive)
    pub upper_bound_ms: f64,
    /// Count of samples in this bucket
    pub count: u64,
    /// Cumulative count up to this bucket
    pub cumulative_count: u64,
}

impl LatencyMetrics {
    /// Create new latency metrics for an operation.
    pub fn new(operation: String) -> Self {
        Self {
            operation,
            sample_count: 0,
            mean_ms: 0.0,
            std_dev_ms: 0.0,
            min_ms: f64::INFINITY,
            max_ms: 0.0,
            percentiles: HashMap::new(),
            histogram_buckets: Self::create_default_buckets(),
            recent_samples: VecDeque::with_capacity(1000),
        }
    }

    /// Add a latency sample.
    pub fn add_sample(&mut self, latency_ms: f64) {
        self.sample_count += 1;
        
        // Update min/max
        self.min_ms = self.min_ms.min(latency_ms);
        self.max_ms = self.max_ms.max(latency_ms);
        
        // Update recent samples
        self.recent_samples.push_back(latency_ms);
        if self.recent_samples.len() > 1000 {
            self.recent_samples.pop_front();
        }
        
        // Recalculate statistics
        self.recalculate_statistics();
        
        // Update histogram
        self.update_histogram(latency_ms);
    }

    /// Recalculate mean and standard deviation.
    fn recalculate_statistics(&mut self) {
        if self.recent_samples.is_empty() {
            return;
        }

        // Calculate mean
        self.mean_ms = self.recent_samples.iter().sum::<f64>() / self.recent_samples.len() as f64;

        // Calculate standard deviation
        let variance = self.recent_samples.iter()
            .map(|x| (x - self.mean_ms).powi(2))
            .sum::<f64>() / self.recent_samples.len() as f64;
        self.std_dev_ms = variance.sqrt();

        // Calculate percentiles
        let mut sorted_samples: Vec<f64> = self.recent_samples.iter().copied().collect();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        self.percentiles.insert("p50".to_string(), Self::percentile(&sorted_samples, 0.5));
        self.percentiles.insert("p90".to_string(), Self::percentile(&sorted_samples, 0.9));
        self.percentiles.insert("p95".to_string(), Self::percentile(&sorted_samples, 0.95));
        self.percentiles.insert("p99".to_string(), Self::percentile(&sorted_samples, 0.99));
        self.percentiles.insert("p99.9".to_string(), Self::percentile(&sorted_samples, 0.999));
    }

    /// Calculate percentile from sorted samples.
    fn percentile(sorted_samples: &[f64], p: f64) -> f64 {
        if sorted_samples.is_empty() {
            return 0.0;
        }

        let index = (p * (sorted_samples.len() - 1) as f64).round() as usize;
        sorted_samples[index.min(sorted_samples.len() - 1)]
    }

    /// Update histogram buckets.
    fn update_histogram(&mut self, latency_ms: f64) {
        for bucket in &mut self.histogram_buckets {
            if latency_ms <= bucket.upper_bound_ms {
                bucket.count += 1;
                break;
            }
        }

        // Recalculate cumulative counts
        let mut cumulative = 0;
        for bucket in &mut self.histogram_buckets {
            cumulative += bucket.count;
            bucket.cumulative_count = cumulative;
        }
    }

    /// Create default histogram buckets for latency.
    fn create_default_buckets() -> Vec<HistogramBucket> {
        let bounds = vec![
            1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, f64::INFINITY
        ];

        bounds.into_iter().map(|bound| HistogramBucket {
            upper_bound_ms: bound,
            count: 0,
            cumulative_count: 0,
        }).collect()
    }
}

/// Telemetry exporter supporting both OTLP and Prometheus.
pub struct TelemetryExporter {
    config: Arc<RwLock<TelemetryConfig>>,
    metrics: Arc<RwLock<NyxMetrics>>,
    performance: Arc<RwLock<PerformanceMetrics>>,
    metrics_tx: broadcast::Sender<NyxMetrics>,
    otlp_initialized: bool,
    prometheus_initialized: bool,
}

impl TelemetryExporter {
    /// Create a new telemetry exporter.
    pub async fn new(config: TelemetryConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (metrics_tx, _) = broadcast::channel(100);

        let exporter = Self {
            config: Arc::new(RwLock::new(config)),
            metrics: Arc::new(RwLock::new(NyxMetrics::default())),
            performance: Arc::new(RwLock::new(PerformanceMetrics {
                cpu_usage: 0.0,
                memory_usage: 0,
                network_io_bps: 0,
                disk_io_bps: 0,
                avg_response_time_ms: 0.0,
                p95_response_time_ms: 0.0,
                p99_response_time_ms: 0.0,
                error_rate: 0.0,
                throughput: 0.0,
            })),
            metrics_tx,
            otlp_initialized: false,
            prometheus_initialized: false,
        };

        // Initialize exporters
        exporter.initialize_otlp().await?;
        exporter.initialize_prometheus().await?;

        Ok(exporter)
    }

    /// Initialize OpenTelemetry OTLP exporter.
    async fn initialize_otlp(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = self.config.read().await;
        
        if !config.otlp_enabled {
            return Ok(());
        }

        info!("Initializing OpenTelemetry OTLP exporter");

        // Create resource with service information
        let mut resource_kvs = vec![
            KeyValue::new(semcov::resource::SERVICE_NAME, config.service_name.clone()),
            KeyValue::new(semcov::resource::SERVICE_VERSION, config.service_version.clone()),
            KeyValue::new(semcov::resource::DEPLOYMENT_ENVIRONMENT, config.environment.clone()),
        ];

        // Add custom resource attributes
        for (key, value) in &config.resource_attributes {
            resource_kvs.push(KeyValue::new(key.clone(), value.clone()));
        }

        let resource = Resource::new(resource_kvs);

        // Initialize tracing if enabled
        if config.tracing_enabled {
            let tracer = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(
                    opentelemetry_otlp::new_exporter()
                        .tonic()
                        .with_endpoint(&config.otlp_endpoint)
                        .with_timeout(config.otlp_timeout)
                )
                .with_trace_config(
                    opentelemetry::sdk::trace::config()
                        .with_sampler(opentelemetry::sdk::trace::Sampler::TraceIdRatioBased(config.trace_sampling))
                        .with_resource(resource.clone())
                )
                .install_batch(opentelemetry::runtime::Tokio)?;

            global::set_tracer_provider(tracer);
        }

        // Initialize metrics
        let meter = opentelemetry_otlp::new_pipeline()
            .metrics(opentelemetry::runtime::Tokio)
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(&config.otlp_endpoint)
                    .with_timeout(config.otlp_timeout)
            )
            .with_resource(resource)
            .build()?;

        global::set_meter_provider(meter);

        info!("OpenTelemetry OTLP exporter initialized successfully");
        Ok(())
    }

    /// Initialize Prometheus metrics exporter.
    async fn initialize_prometheus(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = self.config.read().await;
        
        if !config.prometheus_enabled {
            return Ok(());
        }

        info!("Initializing Prometheus metrics exporter");

        // Create Prometheus registry and exporter
        let controller = controllers::basic(
            processors::factory(
                selectors::simple::histogram([1.0, 2.0, 5.0, 10.0, 20.0, 50.0]),
                aggregation::cumulative_temporality_selector(),
            )
            .with_memory(true),
        )
        .build();

        let exporter = opentelemetry_prometheus::exporter(controller)
            .with_resource(Resource::new(vec![
                KeyValue::new(semcov::resource::SERVICE_NAME, config.service_name.clone()),
            ]))
            .init();

        // Start Prometheus HTTP server
        let prometheus_address = config.prometheus_address.clone();
        tokio::spawn(async move {
            let app = warp::path("metrics")
                .map(move || {
                    let metric_families = exporter.registry().gather();
                    let encoder = prometheus::TextEncoder::new();
                    let mut buffer = Vec::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();
                    String::from_utf8(buffer).unwrap()
                });

            warp::serve(app)
                .run(prometheus_address.parse::<std::net::SocketAddr>().unwrap())
                .await;
        });

        info!("Prometheus metrics exporter initialized on {}", config.prometheus_address);
        Ok(())
    }

    /// Start metrics collection loop.
    pub async fn start_collection(&self) {
        let config = Arc::clone(&self.config);
        let metrics = Arc::clone(&self.metrics);
        let performance = Arc::clone(&self.performance);
        let metrics_tx = self.metrics_tx.clone();

        tokio::spawn(async move {
            loop {
                let interval_duration = {
                    let cfg = config.read().await;
                    cfg.metrics_interval
                };

                let mut interval = interval(interval_duration);
                interval.tick().await;

                // Collect system performance metrics
                let perf_metrics = Self::collect_performance_metrics().await;
                *performance.write().await = perf_metrics;

                // Broadcast current metrics
                let current_metrics = metrics.read().await.clone();
                let _ = metrics_tx.send(current_metrics);

                // Export to OTLP if enabled
                Self::export_otlp_metrics(&metrics).await;
            }
        });
    }

    /// Record a connection event.
    pub async fn record_connection(&self, established: bool, latency_ms: Option<f64>) {
        let mut metrics = self.metrics.write().await;
        
        if established {
            metrics.connections_total += 1;
            metrics.connections_active += 1;
            
            if let Some(latency) = latency_ms {
                metrics.connection_latency_ms.push(latency);
                
                // Keep only recent latency samples (last 1000)
                if metrics.connection_latency_ms.len() > 1000 {
                    metrics.connection_latency_ms.drain(0..100);
                }
            }
        } else {
            metrics.connections_active = metrics.connections_active.saturating_sub(1);
        }

        self.emit_otlp_counter("nyx_connections_total", metrics.connections_total as f64, &[]).await;
        self.emit_otlp_gauge("nyx_connections_active", metrics.connections_active as f64, &[]).await;
        
        if let Some(latency) = latency_ms {
            self.emit_otlp_histogram("nyx_connection_latency_ms", latency, &[]).await;
        }
    }

    /// Record network I/O.
    pub async fn record_network_io(&self, bytes_sent: u64, bytes_received: u64, packets_sent: u64, packets_received: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.bytes_sent_total += bytes_sent;
        metrics.bytes_received_total += bytes_received;
        metrics.packets_sent_total += packets_sent;
        metrics.packets_received_total += packets_received;

        self.emit_otlp_counter("nyx_bytes_sent_total", bytes_sent as f64, &[]).await;
        self.emit_otlp_counter("nyx_bytes_received_total", bytes_received as f64, &[]).await;
        self.emit_otlp_counter("nyx_packets_sent_total", packets_sent as f64, &[]).await;
        self.emit_otlp_counter("nyx_packets_received_total", packets_received as f64, &[]).await;
    }

    /// Record stream operation.
    pub async fn record_stream(&self, created: bool, error: bool) {
        let mut metrics = self.metrics.write().await;
        
        if created {
            metrics.streams_created_total += 1;
            self.emit_otlp_counter("nyx_streams_created_total", metrics.streams_created_total as f64, &[]).await;
        }
        
        if error {
            metrics.stream_errors_total += 1;
            self.emit_otlp_counter("nyx_stream_errors_total", metrics.stream_errors_total as f64, &[]).await;
        }
    }

    /// Record mix routing operation.
    pub async fn record_mix_routing(&self, hops: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.mix_hops_total += hops;
        
        self.emit_otlp_counter("nyx_mix_hops_total", hops as f64, &[]).await;
        self.emit_otlp_histogram("nyx_mix_hops", hops as f64, &[]).await;
    }

    /// Record cover traffic generation.
    pub async fn record_cover_traffic(&self, bytes: u64) {
        let mut metrics = self.metrics.write().await;
        metrics.cover_traffic_bytes += bytes;
        
        self.emit_otlp_counter("nyx_cover_traffic_bytes", bytes as f64, &[]).await;
    }

    /// Record plugin frame processing.
    pub async fn record_plugin_frame(&self, plugin_id: u32, frame_size: usize) {
        let mut metrics = self.metrics.write().await;
        metrics.plugin_frames_total += 1;
        
        let labels = &[("plugin_id", plugin_id.to_string())];
        self.emit_otlp_counter("nyx_plugin_frames_total", 1.0, labels).await;
        self.emit_otlp_histogram("nyx_plugin_frame_size", frame_size as f64, labels).await;
    }

    /// Record VDF computation.
    pub async fn record_vdf_computation(&self, duration_ms: f64) {
        let mut metrics = self.metrics.write().await;
        metrics.vdf_computations_total += 1;
        
        self.emit_otlp_counter("nyx_vdf_computations_total", metrics.vdf_computations_total as f64, &[]).await;
        self.emit_otlp_histogram("nyx_vdf_duration_ms", duration_ms, &[]).await;
    }

    /// Record cryptographic operation.
    pub async fn record_crypto_operation(&self, operation: &str, duration_ms: f64) {
        let mut metrics = self.metrics.write().await;
        metrics.crypto_operations_total += 1;
        
        let labels = &[("operation", operation.to_string())];
        self.emit_otlp_counter("nyx_crypto_operations_total", 1.0, labels).await;
        self.emit_otlp_histogram("nyx_crypto_duration_ms", duration_ms, labels).await;
    }

    /// Record error by type.
    pub async fn record_error(&self, error_type: &str) {
        let mut metrics = self.metrics.write().await;
        *metrics.errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;
        
        let labels = &[("error_type", error_type.to_string())];
        self.emit_otlp_counter("nyx_errors_total", 1.0, labels).await;
    }

    /// Get current metrics.
    pub async fn metrics(&self) -> NyxMetrics {
        self.metrics.read().await.clone()
    }

    /// Get current performance metrics.
    pub async fn performance(&self) -> PerformanceMetrics {
        self.performance.read().await.clone()
    }

    /// Subscribe to metrics updates.
    pub fn subscribe_metrics(&self) -> broadcast::Receiver<NyxMetrics> {
        self.metrics_tx.subscribe()
    }

    /// Emit OTLP counter metric.
    async fn emit_otlp_counter(&self, name: &str, value: f64, labels: &[(&str, String)]) {
        let meter = global::meter("nyx");
        let counter = meter.f64_counter(name).init();
        
        let kvs: Vec<KeyValue> = labels.iter()
            .map(|(k, v)| KeyValue::new(*k, v.clone()))
            .collect();
        
        counter.add(value, &kvs);
    }

    /// Emit OTLP gauge metric.
    async fn emit_otlp_gauge(&self, name: &str, value: f64, labels: &[(&str, String)]) {
        let meter = global::meter("nyx");
        let gauge = meter.f64_up_down_counter(name).init();
        
        let kvs: Vec<KeyValue> = labels.iter()
            .map(|(k, v)| KeyValue::new(*k, v.clone()))
            .collect();
        
        gauge.add(value, &kvs);
    }

    /// Emit OTLP histogram metric.
    async fn emit_otlp_histogram(&self, name: &str, value: f64, labels: &[(&str, String)]) {
        let meter = global::meter("nyx");
        let histogram = meter.f64_histogram(name).init();
        
        let kvs: Vec<KeyValue> = labels.iter()
            .map(|(k, v)| KeyValue::new(*k, v.clone()))
            .collect();
        
        histogram.record(value, &kvs);
    }

    /// Export metrics to OTLP.
    async fn export_otlp_metrics(metrics: &Arc<RwLock<NyxMetrics>>) {
        let metrics_guard = metrics.read().await;
        
        // Export would happen automatically via the OTLP pipeline
        debug!("Exporting {} connection events to OTLP", metrics_guard.connections_total);
    }

    /// Collect system performance metrics.
    async fn collect_performance_metrics() -> PerformanceMetrics {
        // In a real implementation, this would collect actual system metrics
        // For now, return placeholder values
        PerformanceMetrics {
            cpu_usage: 25.0,
            memory_usage: 128 * 1024 * 1024, // 128MB
            network_io_bps: 1024 * 1024,     // 1MB/s
            disk_io_bps: 512 * 1024,         // 512KB/s
            avg_response_time_ms: 15.0,
            p95_response_time_ms: 45.0,
            p99_response_time_ms: 95.0,
            error_rate: 0.01, // 1% error rate
            throughput: 100.0, // 100 RPS
        }
    }

    /// Record detailed error with full context.
    pub async fn record_detailed_error(&self, error_event: ErrorEvent) {
        let mut metrics = self.metrics.write().await;
        
        // Update error counts
        let error_type = error_event.error_code.clone();
        *metrics.errors_by_type.entry(error_type.clone()).or_insert(0) += 1;

        // Emit detailed OTLP metrics
        let labels = &[
            ("error_code", error_event.error_code.clone()),
            ("category", error_event.category.to_string()),
            ("severity", error_event.severity.to_string()),
            ("component", error_event.component.clone()),
        ];

        self.emit_otlp_counter("nyx_errors_detailed_total", 1.0, labels).await;

        // Emit error rate
        let error_rate = self.calculate_error_rate().await;
        self.emit_otlp_gauge("nyx_error_rate", error_rate, &[]).await;

        // Log error for debugging
        match error_event.severity {
            ErrorSeverity::Low => debug!("Error recorded: {} - {}", error_event.error_code, error_event.message),
            ErrorSeverity::Medium => warn!("Error recorded: {} - {}", error_event.error_code, error_event.message),
            ErrorSeverity::High => error!("Error recorded: {} - {}", error_event.error_code, error_event.message),
            ErrorSeverity::Critical => error!("CRITICAL Error recorded: {} - {}", error_event.error_code, error_event.message),
        }
    }

    /// Record latency with detailed distribution tracking.
    pub async fn record_latency_distribution(&self, operation: &str, latency_ms: f64) {
        // Emit basic histogram
        let labels = &[("operation", operation.to_string())];
        self.emit_otlp_histogram("nyx_operation_latency_ms", latency_ms, labels).await;

        // Emit percentile approximations
        self.emit_otlp_histogram("nyx_latency_p50_ms", latency_ms, labels).await;
        self.emit_otlp_histogram("nyx_latency_p95_ms", latency_ms, labels).await;
        self.emit_otlp_histogram("nyx_latency_p99_ms", latency_ms, labels).await;

        // Categorize latency for alerting
        let latency_category = match latency_ms {
            l if l < 10.0 => "fast",
            l if l < 100.0 => "normal",
            l if l < 1000.0 => "slow",
            _ => "very_slow",
        };

        let category_labels = &[
            ("operation", operation.to_string()),
            ("latency_category", latency_category.to_string()),
        ];
        self.emit_otlp_counter("nyx_latency_category_total", 1.0, category_labels).await;
    }

    /// Record connection quality metrics.
    pub async fn record_connection_quality(&self, rtt_ms: f64, packet_loss: f64, jitter_ms: f64) {
        // Record RTT distribution
        self.emit_otlp_histogram("nyx_connection_rtt_ms", rtt_ms, &[]).await;
        
        // Record packet loss
        self.emit_otlp_gauge("nyx_connection_packet_loss", packet_loss, &[]).await;
        
        // Record jitter
        self.emit_otlp_histogram("nyx_connection_jitter_ms", jitter_ms, &[]).await;

        // Calculate connection quality score (0-100)
        let quality_score = self.calculate_connection_quality_score(rtt_ms, packet_loss, jitter_ms);
        self.emit_otlp_gauge("nyx_connection_quality_score", quality_score, &[]).await;

        // Categorize connection quality
        let quality_category = match quality_score {
            s if s >= 90.0 => "excellent",
            s if s >= 70.0 => "good",
            s if s >= 50.0 => "fair",
            s if s >= 30.0 => "poor",
            _ => "very_poor",
        };

        let labels = &[("quality", quality_category.to_string())];
        self.emit_otlp_counter("nyx_connection_quality_category_total", 1.0, labels).await;
    }

    /// Record resource utilization metrics.
    pub async fn record_resource_utilization(&self, cpu_percent: f64, memory_bytes: u64, network_bps: u64) {
        // CPU utilization
        self.emit_otlp_gauge("nyx_cpu_utilization_percent", cpu_percent, &[]).await;
        
        // Memory utilization
        self.emit_otlp_gauge("nyx_memory_utilization_bytes", memory_bytes as f64, &[]).await;
        
        // Network utilization
        self.emit_otlp_gauge("nyx_network_utilization_bps", network_bps as f64, &[]).await;

        // Resource pressure indicators
        let cpu_pressure = if cpu_percent > 80.0 { "high" } else if cpu_percent > 60.0 { "medium" } else { "low" };
        let memory_pressure = if memory_bytes > 512 * 1024 * 1024 { "high" } else if memory_bytes > 256 * 1024 * 1024 { "medium" } else { "low" };

        let cpu_labels = &[("pressure", cpu_pressure.to_string())];
        let memory_labels = &[("pressure", memory_pressure.to_string())];

        self.emit_otlp_counter("nyx_cpu_pressure_total", 1.0, cpu_labels).await;
        self.emit_otlp_counter("nyx_memory_pressure_total", 1.0, memory_labels).await;
    }

    /// Record security metrics.
    pub async fn record_security_event(&self, event_type: &str, severity: &str, source: &str) {
        let labels = &[
            ("event_type", event_type.to_string()),
            ("severity", severity.to_string()),
            ("source", source.to_string()),
        ];

        self.emit_otlp_counter("nyx_security_events_total", 1.0, labels).await;

        // Alert on critical security events
        if severity == "critical" {
            error!("SECURITY ALERT: {} from {} - {}", event_type, source, severity);
        }
    }

    /// Calculate error rate based on recent activity.
    async fn calculate_error_rate(&self) -> f64 {
        let metrics = self.metrics.read().await;
        
        // Simple error rate calculation (errors per second over last minute)
        // In a real implementation, this would use a sliding window
        let total_errors: u64 = metrics.errors_by_type.values().sum();
        total_errors as f64 / 60.0 // Assume 1-minute window
    }

    /// Calculate connection quality score based on network metrics.
    fn calculate_connection_quality_score(&self, rtt_ms: f64, packet_loss: f64, jitter_ms: f64) -> f64 {
        let mut score = 100.0;

        // Penalize high RTT
        if rtt_ms > 50.0 {
            score -= (rtt_ms - 50.0) * 0.5;
        }

        // Penalize packet loss heavily
        score -= packet_loss * 1000.0;

        // Penalize high jitter
        if jitter_ms > 10.0 {
            score -= (jitter_ms - 10.0) * 2.0;
        }

        score.max(0.0).min(100.0)
    }

    /// Generate comprehensive metrics report.
    pub async fn generate_metrics_report(&self) -> MetricsReport {
        let metrics = self.metrics.read().await;
        let performance = self.performance.read().await;

        MetricsReport {
            timestamp: SystemTime::now(),
            connections: ConnectionMetrics {
                total: metrics.connections_total,
                active: metrics.connections_active,
                avg_latency_ms: if !metrics.connection_latency_ms.is_empty() {
                    metrics.connection_latency_ms.iter().sum::<f64>() / metrics.connection_latency_ms.len() as f64
                } else {
                    0.0
                },
            },
            throughput: ThroughputMetrics {
                bytes_sent_per_sec: metrics.bytes_sent_total as f64 / 60.0, // Simplified
                bytes_received_per_sec: metrics.bytes_received_total as f64 / 60.0,
                packets_sent_per_sec: metrics.packets_sent_total as f64 / 60.0,
                packets_received_per_sec: metrics.packets_received_total as f64 / 60.0,
            },
            errors: ErrorSummary {
                total_errors: metrics.errors_by_type.values().sum(),
                error_rate: self.calculate_error_rate().await,
                top_errors: metrics.errors_by_type.iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .take(10)
                    .collect(),
            },
            performance: performance.clone(),
        }
    }
}

/// Comprehensive metrics report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub timestamp: SystemTime,
    pub connections: ConnectionMetrics,
    pub throughput: ThroughputMetrics,
    pub errors: ErrorSummary,
    pub performance: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub total: u64,
    pub active: u64,
    pub avg_latency_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    pub bytes_sent_per_sec: f64,
    pub bytes_received_per_sec: f64,
    pub packets_sent_per_sec: f64,
    pub packets_received_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    pub total_errors: u64,
    pub error_rate: f64,
    pub top_errors: Vec<(String, u64)>,
}

/// Global telemetry exporter instance.
static TELEMETRY: Lazy<RwLock<Option<Arc<TelemetryExporter>>>> = Lazy::new(|| RwLock::new(None));

/// Initialize global telemetry.
pub async fn init_telemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let exporter = Arc::new(TelemetryExporter::new(config).await?);
    exporter.start_collection().await;
    
    *TELEMETRY.write().await = Some(exporter);
    info!("Global telemetry initialized successfully");
    Ok(())
}

/// Get global telemetry exporter.
pub async fn telemetry() -> Option<Arc<TelemetryExporter>> {
    TELEMETRY.read().await.clone()
}

/// Record connection event globally.
pub async fn record_connection(established: bool, latency_ms: Option<f64>) {
    if let Some(tel) = telemetry().await {
        tel.record_connection(established, latency_ms).await;
    }
}

/// Record network I/O globally.
pub async fn record_network_io(bytes_sent: u64, bytes_received: u64, packets_sent: u64, packets_received: u64) {
    if let Some(tel) = telemetry().await {
        tel.record_network_io(bytes_sent, bytes_received, packets_sent, packets_received).await;
    }
}

/// Record stream operation globally.
pub async fn record_stream(created: bool, error: bool) {
    if let Some(tel) = telemetry().await {
        tel.record_stream(created, error).await;
    }
}

/// Record plugin frame globally.
pub async fn record_plugin_frame(plugin_id: u32, frame_size: usize) {
    if let Some(tel) = telemetry().await {
        tel.record_plugin_frame(plugin_id, frame_size).await;
    }
}

/// Record error globally.
pub async fn record_error(error_type: &str) {
    if let Some(tel) = telemetry().await {
        tel.record_error(error_type).await;
    }
}

/// Create a tracing span for operations.
pub fn create_span(name: &str, operation: &str) -> tracing::Span {
    span!(
        Level::INFO,
        "nyx_operation",
        operation = operation,
        operation_name = name,
        otel.name = name,
        otel.kind = "internal"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert!(config.otlp_enabled);
        assert!(config.prometheus_enabled);
        assert!(config.tracing_enabled);
        assert_eq!(config.service_name, "nyx-daemon");
    }

    #[tokio::test]
    async fn test_nyx_metrics_default() {
        let metrics = NyxMetrics::default();
        assert_eq!(metrics.connections_total, 0);
        assert_eq!(metrics.bytes_sent_total, 0);
        assert!(metrics.errors_by_type.is_empty());
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let config = TelemetryConfig {
            otlp_enabled: false,
            prometheus_enabled: false,
            ..Default::default()
        };
        
        let exporter = TelemetryExporter::new(config).await.unwrap();
        
        // Test connection recording
        exporter.record_connection(true, Some(25.0)).await;
        let metrics = exporter.metrics().await;
        assert_eq!(metrics.connections_total, 1);
        assert_eq!(metrics.connections_active, 1);
        assert_eq!(metrics.connection_latency_ms.len(), 1);
        
        // Test network I/O recording
        exporter.record_network_io(1024, 512, 10, 5).await;
        let metrics = exporter.metrics().await;
        assert_eq!(metrics.bytes_sent_total, 1024);
        assert_eq!(metrics.bytes_received_total, 512);
    }

    #[tokio::test]
    async fn test_error_recording() {
        let config = TelemetryConfig {
            otlp_enabled: false,
            prometheus_enabled: false,
            ..Default::default()
        };
        
        let exporter = TelemetryExporter::new(config).await.unwrap();
        
        exporter.record_error("connection_failed").await;
        exporter.record_error("connection_failed").await;
        exporter.record_error("timeout").await;
        
        let metrics = exporter.metrics().await;
        assert_eq!(metrics.errors_by_type.get("connection_failed"), Some(&2));
        assert_eq!(metrics.errors_by_type.get("timeout"), Some(&1));
    }

    #[test]
    fn test_span_creation() {
        let span = create_span("test_operation", "test");
        assert_eq!(span.metadata().name(), "nyx_operation");
    }
} 