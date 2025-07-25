use std::collections::HashMap;
use std::time::Duration;
use chrono::{DateTime, Utc};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context, anyhow};
use tonic::Request;

use crate::{NyxControlClient, proto::Empty};
use tonic::transport::Channel;

/// Prometheus client for querying Nyx daemon metrics
#[derive(Debug, Clone)]
pub struct PrometheusClient {
    base_url: String,
    http_client: HttpClient,
    timeout: Duration,
    auth_token: Option<String>,
}

/// Configuration for Prometheus client
#[derive(Debug, Clone)]
pub struct PrometheusConfig {
    pub endpoint: String,
    pub timeout_seconds: u64,
    pub auth_token: Option<String>,
    pub enable_tls: bool,
    pub verify_tls: bool,
}

impl Default for PrometheusConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9090".to_string(),
            timeout_seconds: 30,
            auth_token: None,
            enable_tls: false,
            verify_tls: true,
        }
    }
}

/// Prometheus query parameters
#[derive(Debug, Clone)]
pub struct QueryParams {
    pub query: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub step: Option<Duration>,
    pub timeout: Option<Duration>,
}

/// Prometheus metric data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    pub timestamp: f64,
    pub value: String,
}

/// Prometheus metric series
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSeries {
    pub metric: HashMap<String, String>,
    pub values: Vec<MetricSample>,
}

/// Prometheus query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusResponse {
    pub status: String,
    pub data: PrometheusData,
    #[serde(default)]
    pub error_type: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Prometheus response data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrometheusData {
    #[serde(rename = "resultType")]
    pub result_type: String,
    pub result: Vec<MetricSeries>,
}

/// Nyx-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NyxMetrics {
    pub stream_count: f64,
    pub connection_count: f64,
    pub bytes_sent: f64,
    pub bytes_received: f64,
    pub error_count: f64,
    pub latency_p50: f64,
    pub latency_p95: f64,
    pub latency_p99: f64,
    pub throughput_mbps: f64,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub timestamp: DateTime<Utc>,
}

/// Metrics query filter
#[derive(Debug, Clone)]
pub struct MetricsFilter {
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub stream_ids: Vec<String>,
    pub error_types: Vec<String>,
    pub layer_filter: Option<String>,
}

impl Default for MetricsFilter {
    fn default() -> Self {
        Self {
            time_range: None,
            stream_ids: Vec::new(),
            error_types: Vec::new(),
            layer_filter: None,
        }
    }
}

impl PrometheusClient {
    /// Create a new Prometheus client
    pub fn new(config: PrometheusConfig) -> Result<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .danger_accept_invalid_certs(!config.verify_tls)
            .build()
            .context("Failed to create HTTP client")?;

        let base_url = if config.endpoint.ends_with('/') {
            config.endpoint.trim_end_matches('/').to_string()
        } else {
            config.endpoint
        };

        Ok(Self {
            base_url,
            http_client,
            timeout: Duration::from_secs(config.timeout_seconds),
            auth_token: config.auth_token,
        })
    }

    /// Test connection to Prometheus
    pub async fn test_connection(&self) -> Result<bool> {
        let url = format!("{}/api/v1/query", self.base_url);
        let mut request = self.http_client.get(&url)
            .query(&[("query", "up")])
            .timeout(Duration::from_secs(5));

        if let Some(ref token) = self.auth_token {
            request = request.bearer_auth(token);
        }

        match request.send().await {
            Ok(response) => {
                let success = response.status().is_success();
                Ok(success)
            }
            Err(_) => Ok(false),
        }
    }

    /// Execute a Prometheus query
    pub async fn query(&self, params: QueryParams) -> Result<PrometheusResponse> {
        let url = if params.start_time.is_some() && params.end_time.is_some() {
            format!("{}/api/v1/query_range", self.base_url)
        } else {
            format!("{}/api/v1/query", self.base_url)
        };

        let mut query_params = vec![("query", params.query.clone())];

        if let Some(start) = params.start_time {
            query_params.push(("start", start.timestamp().to_string()));
        }

        if let Some(end) = params.end_time {
            query_params.push(("end", end.timestamp().to_string()));
        }

        if let Some(step) = params.step {
            query_params.push(("step", format!("{}s", step.as_secs())));
        }

        if let Some(timeout) = params.timeout {
            query_params.push(("timeout", format!("{}s", timeout.as_secs())));
        }

        let mut request = self.http_client.get(&url)
            .query(&query_params)
            .timeout(self.timeout);

        if let Some(ref token) = self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await
            .context("Failed to send Prometheus query")?;

        if !response.status().is_success() {
            return Err(anyhow!("Prometheus query failed with status: {}", response.status()));
        }

        let prometheus_response: PrometheusResponse = response.json().await
            .context("Failed to parse Prometheus response")?;

        if prometheus_response.status != "success" {
            return Err(anyhow!(
                "Prometheus query failed: {} - {}",
                prometheus_response.error_type.unwrap_or_default(),
                prometheus_response.error.unwrap_or_default()
            ));
        }

        Ok(prometheus_response)
    }

    /// Query Nyx-specific metrics
    pub async fn query_nyx_metrics(&self, filter: MetricsFilter) -> Result<NyxMetrics> {
        let current_time = Utc::now();
        let (start_time, end_time) = filter.time_range
            .unwrap_or((current_time - chrono::Duration::minutes(5), current_time));

        // Build filter string for stream IDs if provided
        let stream_filter = if !filter.stream_ids.is_empty() {
            format!(r#"stream_id=~"{}""#, filter.stream_ids.join("|"))
        } else {
            String::new()
        };

        // Build filter string for error types if provided
        let error_filter = if !filter.error_types.is_empty() {
            format!(r#"error_type=~"{}""#, filter.error_types.join("|"))
        } else {
            String::new()
        };

        // Combine filters
        let base_filter = if stream_filter.is_empty() && error_filter.is_empty() {
            String::new()
        } else {
            let filters = [stream_filter, error_filter]
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", filters)
        };

        // Query individual metrics
        let mut metrics = NyxMetrics {
            stream_count: 0.0,
            connection_count: 0.0,
            bytes_sent: 0.0,
            bytes_received: 0.0,
            error_count: 0.0,
            latency_p50: 0.0,
            latency_p95: 0.0,
            latency_p99: 0.0,
            throughput_mbps: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0.0,
            timestamp: current_time,
        };

        // Query stream count
        if let Ok(response) = self.query(QueryParams {
            query: format!("nyx_active_streams{}", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.stream_count = self.extract_latest_value(&response)?;
        }

        // Query connection count
        if let Ok(response) = self.query(QueryParams {
            query: format!("nyx_active_connections{}", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.connection_count = self.extract_latest_value(&response)?;
        }

        // Query bytes sent
        if let Ok(response) = self.query(QueryParams {
            query: format!("rate(nyx_bytes_sent_total{}[5m])", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.bytes_sent = self.extract_latest_value(&response)?;
        }

        // Query bytes received
        if let Ok(response) = self.query(QueryParams {
            query: format!("rate(nyx_bytes_received_total{}[5m])", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.bytes_received = self.extract_latest_value(&response)?;
        }

        // Query error count
        if let Ok(response) = self.query(QueryParams {
            query: format!("rate(nyx_errors_total{}[5m])", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.error_count = self.extract_latest_value(&response)?;
        }

        // Query latency percentiles
        if let Ok(response) = self.query(QueryParams {
            query: format!("histogram_quantile(0.50, nyx_request_duration_seconds{})", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.latency_p50 = self.extract_latest_value(&response)? * 1000.0; // Convert to ms
        }

        if let Ok(response) = self.query(QueryParams {
            query: format!("histogram_quantile(0.95, nyx_request_duration_seconds{})", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.latency_p95 = self.extract_latest_value(&response)? * 1000.0; // Convert to ms
        }

        if let Ok(response) = self.query(QueryParams {
            query: format!("histogram_quantile(0.99, nyx_request_duration_seconds{})", base_filter),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.latency_p99 = self.extract_latest_value(&response)? * 1000.0; // Convert to ms
        }

        // Calculate throughput
        metrics.throughput_mbps = (metrics.bytes_sent + metrics.bytes_received) * 8.0 / 1_000_000.0;

        // Query system metrics
        if let Ok(response) = self.query(QueryParams {
            query: "rate(process_cpu_seconds_total[5m]) * 100".to_string(),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.cpu_usage_percent = self.extract_latest_value(&response)?;
        }

        if let Ok(response) = self.query(QueryParams {
            query: "process_resident_memory_bytes / 1024 / 1024".to_string(),
            start_time: Some(start_time),
            end_time: Some(end_time),
            step: Some(Duration::from_secs(60)),
            timeout: Some(Duration::from_secs(10)),
        }).await {
            metrics.memory_usage_mb = self.extract_latest_value(&response)?;
        }

        Ok(metrics)
    }

    /// Fallback to direct daemon gRPC queries when Prometheus is unavailable
    pub async fn query_daemon_fallback(
        &self,
        daemon_client: &mut NyxControlClient<Channel>,
    ) -> Result<NyxMetrics> {
        println!("⚠️  Prometheus unavailable, falling back to direct daemon queries");

        // Query daemon info via gRPC
        let daemon_info_response = daemon_client.get_info(Request::new(Empty {})).await
            .context("Failed to query daemon info")?;
        let daemon_info = daemon_info_response.into_inner();

        let current_time = Utc::now();
        let metrics = NyxMetrics {
            stream_count: daemon_info.active_streams as f64,
            connection_count: daemon_info.active_streams as f64, // Simplified
            bytes_sent: daemon_info.bytes_out as f64,
            bytes_received: daemon_info.bytes_in as f64,
            error_count: 0.0, // Not available via daemon info
            latency_p50: daemon_info.performance.as_ref().map(|p| p.avg_latency_ms).unwrap_or(0.0),
            latency_p95: daemon_info.performance.as_ref().map(|p| p.avg_latency_ms * 1.5).unwrap_or(0.0),
            latency_p99: daemon_info.performance.as_ref().map(|p| p.avg_latency_ms * 2.0).unwrap_or(0.0),
            throughput_mbps: (daemon_info.bytes_out + daemon_info.bytes_in) as f64 * 8.0 / 1_000_000.0 / daemon_info.uptime_sec as f64,
            cpu_usage_percent: daemon_info.performance.as_ref().map(|p| p.cpu_usage * 100.0).unwrap_or(0.0),
            memory_usage_mb: daemon_info.performance.as_ref().map(|p| p.memory_usage_mb).unwrap_or(0.0),
            timestamp: current_time,
        };

        Ok(metrics)
    }

    /// Extract the latest value from a Prometheus response
    fn extract_latest_value(&self, response: &PrometheusResponse) -> Result<f64> {
        if response.data.result.is_empty() {
            return Ok(0.0);
        }

        let series = &response.data.result[0];
        if series.values.is_empty() {
            return Ok(0.0);
        }

        // Get the latest value
        let latest_sample = &series.values[series.values.len() - 1];
        latest_sample.value.parse::<f64>()
            .context("Failed to parse metric value")
    }

    /// Validate metrics data
    pub fn validate_metrics(&self, metrics: &NyxMetrics) -> Result<()> {
        // Check for reasonable ranges
        if metrics.stream_count < 0.0 {
            return Err(anyhow!("Invalid stream count: {}", metrics.stream_count));
        }

        if metrics.connection_count < 0.0 {
            return Err(anyhow!("Invalid connection count: {}", metrics.connection_count));
        }

        if metrics.bytes_sent < 0.0 || metrics.bytes_received < 0.0 {
            return Err(anyhow!("Invalid byte counts: sent={}, received={}", 
                metrics.bytes_sent, metrics.bytes_received));
        }

        if metrics.latency_p50 < 0.0 || metrics.latency_p95 < 0.0 || metrics.latency_p99 < 0.0 {
            return Err(anyhow!("Invalid latency values: p50={}, p95={}, p99={}", 
                metrics.latency_p50, metrics.latency_p95, metrics.latency_p99));
        }

        if metrics.cpu_usage_percent < 0.0 || metrics.cpu_usage_percent > 100.0 {
            return Err(anyhow!("Invalid CPU usage: {}%", metrics.cpu_usage_percent));
        }

        if metrics.memory_usage_mb < 0.0 {
            return Err(anyhow!("Invalid memory usage: {} MB", metrics.memory_usage_mb));
        }

        // Check logical consistency
        if metrics.latency_p50 > metrics.latency_p95 || metrics.latency_p95 > metrics.latency_p99 {
            return Err(anyhow!("Inconsistent latency percentiles: p50={}, p95={}, p99={}", 
                metrics.latency_p50, metrics.latency_p95, metrics.latency_p99));
        }

        Ok(())
    }

    /// Format metrics for display
    pub fn format_metrics(&self, metrics: &NyxMetrics) -> String {
        format!(
            "Nyx Metrics ({})\n\
            Streams: {}\n\
            Connections: {}\n\
            Bytes Sent: {:.2} MB\n\
            Bytes Received: {:.2} MB\n\
            Error Rate: {:.2}/s\n\
            Latency P50: {:.1}ms\n\
            Latency P95: {:.1}ms\n\
            Latency P99: {:.1}ms\n\
            Throughput: {:.2} Mbps\n\
            CPU Usage: {:.1}%\n\
            Memory Usage: {:.1} MB",
            metrics.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            metrics.stream_count,
            metrics.connection_count,
            metrics.bytes_sent / 1_000_000.0,
            metrics.bytes_received / 1_000_000.0,
            metrics.error_count,
            metrics.latency_p50,
            metrics.latency_p95,
            metrics.latency_p99,
            metrics.throughput_mbps,
            metrics.cpu_usage_percent,
            metrics.memory_usage_mb
        )
    }
} 