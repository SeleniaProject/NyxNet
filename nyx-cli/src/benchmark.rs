use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tonic::Request;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::proto::nyx_control_client::NyxControlClient;
use crate::latency_collector::{LatencyCollector, LatencyStatistics};
use crate::throughput_measurer::{ThroughputMeasurer, ThroughputStatistics};
use crate::error_tracker::{ErrorTracker, ErrorStatistics, ErrorContext, NetworkConditions};
use tonic::transport::Channel;

/// Configuration for benchmark execution
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub target: String,
    pub duration: Duration,
    pub connections: u32,
    pub payload_size: usize,
    pub rate_limit: Option<u64>,
}

/// Comprehensive benchmark results with detailed statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub target: String,
    pub duration: Duration,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub avg_latency: Duration,
    pub percentiles: LatencyPercentiles,
    pub throughput_mbps: f64,
    pub error_rate: f64,
    pub layer_metrics: LayerMetrics,
    pub latency_statistics: LatencyStatistics,
    pub throughput_statistics: ThroughputStatistics,
    pub error_statistics: ErrorStatistics,
    pub timestamp: DateTime<Utc>,
}

/// Latency percentile breakdown for detailed analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p99_9: Duration,
}

/// Per-layer performance metrics for protocol stack analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMetrics {
    pub stream_layer: LayerPerformance,
    pub mix_layer: LayerPerformance,
    pub fec_layer: LayerPerformance,
    pub transport_layer: LayerPerformance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerPerformance {
    pub latency_ms: f64,
    pub throughput_mbps: f64,
    pub error_count: u64,
    pub success_rate: f64,
}

/// Error categorization by protocol layer
#[derive(Debug, Clone)]
pub enum BenchmarkError {
    StreamLayer(String),
    MixLayer(String),
    FecLayer(String),
    TransportLayer(String),
    Unknown(String),
}

impl BenchmarkError {
    pub fn layer(&self) -> &'static str {
        match self {
            BenchmarkError::StreamLayer(_) => "Stream",
            BenchmarkError::MixLayer(_) => "Mix",
            BenchmarkError::FecLayer(_) => "FEC",
            BenchmarkError::TransportLayer(_) => "Transport",
            BenchmarkError::Unknown(_) => "Unknown",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            BenchmarkError::StreamLayer(msg) => msg,
            BenchmarkError::MixLayer(msg) => msg,
            BenchmarkError::FecLayer(msg) => msg,
            BenchmarkError::TransportLayer(msg) => msg,
            BenchmarkError::Unknown(msg) => msg,
        }
    }

    /// Categorize gRPC error by analyzing error message and status
    pub fn from_grpc_error(error: &tonic::Status) -> Self {
        let message = error.message();
        let code = error.code();
        
        // Categorize based on error patterns
        if message.contains("stream") || message.contains("connection") {
            BenchmarkError::StreamLayer(format!("{}: {}", code, message))
        } else if message.contains("mix") || message.contains("routing") || message.contains("path") {
            BenchmarkError::MixLayer(format!("{}: {}", code, message))
        } else if message.contains("fec") || message.contains("forward error correction") {
            BenchmarkError::FecLayer(format!("{}: {}", code, message))
        } else if message.contains("transport") || message.contains("network") || message.contains("tcp") || message.contains("udp") {
            BenchmarkError::TransportLayer(format!("{}: {}", code, message))
        } else {
            BenchmarkError::Unknown(format!("{}: {}", code, message))
        }
    }
}

/// Main benchmark runner with concurrent stream management
pub struct BenchmarkRunner {
    client: NyxControlClient<Channel>,
    config: BenchmarkConfig,
    shutdown: Arc<AtomicBool>,
}

impl BenchmarkRunner {
    pub fn new(client: NyxControlClient<Channel>, config: BenchmarkConfig, shutdown: Arc<AtomicBool>) -> Self {
        Self {
            client,
            config,
            shutdown,
        }
    }

    /// Execute benchmark with actual Nyx stream establishment and data transmission
    pub async fn run(&mut self) -> Result<BenchmarkResult> {
        let config = self.config.clone();
        let start_time = Instant::now();
        
        // Shared counters for thread-safe statistics collection
        let total_requests = Arc::new(AtomicU64::new(0));
        let successful_requests = Arc::new(AtomicU64::new(0));
        let failed_requests = Arc::new(AtomicU64::new(0));
        let bytes_sent = Arc::new(AtomicU64::new(0));
        let bytes_received = Arc::new(AtomicU64::new(0));
        
        // Latency collection with thread-safe collector
        let latency_collector = Arc::new(tokio::sync::Mutex::new(LatencyCollector::new()));
        let throughput_measurer = Arc::new(tokio::sync::Mutex::new(ThroughputMeasurer::default()));
        let error_tracker = Arc::new(tokio::sync::Mutex::new(ErrorTracker::new()));
        let errors = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        
        // Semaphore to limit concurrent connections
        let semaphore = Arc::new(Semaphore::new(self.config.connections as usize));
        
        // Generate test payload
        let payload = vec![0u8; config.payload_size];
        
        // Calculate requests per second if rate limited
        let requests_per_second = if let Some(limit) = config.rate_limit {
            limit
        } else {
            // Default to aggressive testing without rate limiting
            config.connections as u64 * 100
        };
        
        let mut tasks = Vec::new();
        let duration_secs = config.duration.as_secs();
        
        // Spawn concurrent benchmark tasks
        for second in 0..duration_secs {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
            
            let requests_this_second = requests_per_second;
            
            for req_in_second in 0..requests_this_second {
                let client = self.client.clone();
                let target = config.target.clone();
                let payload = payload.clone();
                let semaphore = semaphore.clone();
                let shutdown = self.shutdown.clone();
                
                let total_requests = total_requests.clone();
                let successful_requests = successful_requests.clone();
                let failed_requests = failed_requests.clone();
                let bytes_sent = bytes_sent.clone();
                let bytes_received = bytes_received.clone();
                let latency_collector = latency_collector.clone();
                let throughput_measurer = throughput_measurer.clone();
                let error_tracker = error_tracker.clone();
                let errors = errors.clone();
                
                let task = tokio::spawn(async move {
                    // Wait for semaphore permit to limit concurrency
                    let _permit = semaphore.acquire().await.unwrap();
                    
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    
                    // Calculate delay for this request within the second
                    let delay_ms = (req_in_second * 1000) / requests_this_second;
                    let target_time = Duration::from_secs(second) + Duration::from_millis(delay_ms);
                    
                    // Sleep until it's time for this request
                    sleep(target_time.saturating_sub(start_time.elapsed())).await;
                    
                    let request_start = Instant::now();
                    total_requests.fetch_add(1, Ordering::Relaxed);
                    
                    // Execute actual stream establishment
                    match Self::execute_stream_request(client, &target, &payload).await {
                        Ok((sent, received, daemon_stats)) => {
                            successful_requests.fetch_add(1, Ordering::Relaxed);
                            bytes_sent.fetch_add(sent, Ordering::Relaxed);
                            bytes_received.fetch_add(received, Ordering::Relaxed);
                            
                            let latency = request_start.elapsed();
                            let request_id = format!("req_{}", total_requests.load(Ordering::Relaxed));
                            
                            // Use enhanced latency collection with daemon metrics
                            latency_collector.lock().await.record_measurement_with_daemon_metrics(
                                latency, 
                                request_id, 
                                daemon_stats.as_ref()
                            );
                            
                            // Record throughput measurement
                            throughput_measurer.lock().await.record_transfer(sent, received);
                            
                            // Record successful request for error rate calculation
                            error_tracker.lock().await.record_success();
                        }
                        Err(error) => {
                            failed_requests.fetch_add(1, Ordering::Relaxed);
                            
                            // Create error context for detailed analysis
                            let error_context = ErrorContext {
                                target_address: target.clone(),
                                payload_size: payload.len(),
                                connection_attempt: 1, // Simplified for now
                                network_conditions: NetworkConditions {
                                    estimated_latency_ms: 50.0, // Placeholder
                                    estimated_bandwidth_mbps: 10.0, // Placeholder
                                    connection_count: config.connections,
                                    system_load: 0.5, // Placeholder
                                },
                            };
                            
                            // Record error with detailed context
                            let request_id = format!("req_{}", total_requests.load(Ordering::Relaxed));
                            error_tracker.lock().await.record_error(error.clone(), request_id, error_context);
                            
                            errors.lock().await.push(error);
                        }
                    }
                });
                
                tasks.push(task);
            }
        }
        
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
        
        let elapsed = start_time.elapsed();
        
        // Collect final statistics
        let total_reqs = total_requests.load(Ordering::Relaxed);
        let successful_reqs = successful_requests.load(Ordering::Relaxed);
        let failed_reqs = failed_requests.load(Ordering::Relaxed);
        let total_bytes_sent = bytes_sent.load(Ordering::Relaxed);
        let total_bytes_received = bytes_received.load(Ordering::Relaxed);
        
        let latency_statistics = latency_collector.lock().await.calculate_statistics();
        let throughput_statistics = throughput_measurer.lock().await.calculate_statistics();
        
        // Set total requests for error rate calculation and get error statistics
        error_tracker.lock().await.set_total_requests(total_reqs);
        let error_statistics = error_tracker.lock().await.calculate_statistics();
        
        let errors_vec = errors.lock().await.clone();
        
        // Calculate basic statistics from latency collector
        let avg_latency = Duration::from_millis(latency_statistics.avg_latency_ms as u64);
        let percentiles = LatencyPercentiles {
            p50: Duration::from_millis(latency_statistics.percentiles.p50 as u64),
            p90: Duration::from_millis(latency_statistics.percentiles.p90 as u64),
            p95: Duration::from_millis(latency_statistics.percentiles.p95 as u64),
            p99: Duration::from_millis(latency_statistics.percentiles.p99 as u64),
            p99_9: Duration::from_millis(latency_statistics.percentiles.p99_9 as u64),
        };
        let error_rate = if total_reqs > 0 {
            (failed_reqs as f64 / total_reqs as f64) * 100.0
        } else {
            0.0
        };
        
        let throughput_mbps = if elapsed.as_secs_f64() > 0.0 {
            (total_bytes_sent as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0)
        } else {
            0.0
        };
        
        let layer_metrics = Self::analyze_layer_metrics(&errors_vec, &latency_statistics, throughput_mbps);
        
        Ok(BenchmarkResult {
            target: config.target.clone(),
            duration: elapsed,
            total_requests: total_reqs,
            successful_requests: successful_reqs,
            failed_requests: failed_reqs,
            bytes_sent: total_bytes_sent,
            bytes_received: total_bytes_received,
            avg_latency,
            percentiles,
            throughput_mbps,
            error_rate,
            layer_metrics,
            latency_statistics,
            throughput_statistics,
            error_statistics,
            timestamp: Utc::now(),
        })
    }
    
    /// Execute a single stream request with actual daemon communication and data transmission
    async fn execute_stream_request(
        mut client: NyxControlClient<Channel>,
        target: &str,
        payload: &[u8],
    ) -> Result<(u64, u64, Option<crate::proto::StreamStats>), BenchmarkError> {
        // Create stream options optimized for benchmarking
        let stream_options = crate::proto::StreamOptions {
            buffer_size: 65536, // 64KB buffer for optimal throughput
            timeout_ms: 30000,  // 30 second timeout
            multipath: true,    // Enable multipath for better performance
            max_paths: 3,       // Use up to 3 paths for redundancy
            path_strategy: "latency_weighted".to_string(), // Optimize for latency
            auto_reconnect: false, // Disable for benchmark accuracy
            max_retry_attempts: 0, // No retries for accurate error measurement
            compression: false, // Disable compression for raw performance
            cipher_suite: "ChaCha20Poly1305".to_string(), // Fast cipher
        };
        
        // Generate unique stream name for this benchmark request
        let stream_name = format!("bench_stream_{}", uuid::Uuid::new_v4());
        
        // Create stream establishment request
        let open_request = crate::proto::OpenRequest {
            stream_name: stream_name.clone(),
            target_address: target.to_string(),
            options: Some(stream_options),
        };
        
        // Execute actual stream establishment through daemon
        let stream_response = client
            .open_stream(Request::new(open_request))
            .await
            .map_err(|e| BenchmarkError::from_grpc_error(&e))?;
        
        let stream_info = stream_response.into_inner();
        
        // Verify stream was successfully established
        if !stream_info.success {
            return Err(BenchmarkError::StreamLayer(format!(
                "Stream establishment failed: {}", 
                stream_info.message
            )));
        }
        
        let stream_id = stream_info.stream_id;
        let mut bytes_sent = 0u64;
        let mut bytes_received = 0u64;
        
        // Perform actual data transmission through the established Nyx stream
        if !payload.is_empty() {
            // Send data through the stream
            let data_request = crate::proto::DataRequest {
                stream_id: stream_id.to_string(),
                data: payload.to_vec(),
            };
            
            let data_response = client
                .send_data(Request::new(data_request))
                .await
                .map_err(|e| BenchmarkError::from_grpc_error(&e))?;
            
            let data_result = data_response.into_inner();
            
            if data_result.success {
                bytes_sent = data_result.bytes_sent;
                // For benchmark purposes, assume echo response of same size
                bytes_received = bytes_sent;
            } else {
                return Err(BenchmarkError::StreamLayer(format!(
                    "Data transmission failed: {}", 
                    data_result.error
                )));
            }
        }
        
        // Get final stream statistics before closing for layer metrics
        let stats_request = crate::proto::StreamId { id: stream_id };
        let stream_stats_response = client
            .get_stream_stats(Request::new(stats_request))
            .await
            .map_err(|e| BenchmarkError::from_grpc_error(&e))?;
        
        let stream_stats = stream_stats_response.into_inner();
        
        // Clean up: close the stream to free resources
        let close_request = crate::proto::StreamId { id: stream_id };
        let _ = client.close_stream(Request::new(close_request)).await;
        
        Ok((bytes_sent, bytes_received, Some(stream_stats)))
    }
    

    
    /// Analyze performance metrics by protocol layer
    fn analyze_layer_metrics(
        errors: &[BenchmarkError],
        latency_stats: &LatencyStatistics,
        overall_throughput: f64,
    ) -> LayerMetrics {
        let total_errors = errors.len() as u64;
        let total_requests = latency_stats.total_measurements as u64 + total_errors;
        
        // Count errors by layer
        let mut layer_error_counts = HashMap::new();
        for error in errors {
            *layer_error_counts.entry(error.layer()).or_insert(0u64) += 1;
        }
        
        // Use latency statistics from collector
        let _avg_latency_ms = latency_stats.avg_latency_ms;
        
        // Distribute throughput across layers (simplified model)
        let layer_throughput = overall_throughput / 4.0;
        
        LayerMetrics {
            stream_layer: LayerPerformance {
                latency_ms: latency_stats.layer_statistics.stream_layer.avg_latency_ms,
                throughput_mbps: layer_throughput,
                error_count: *layer_error_counts.get("Stream").unwrap_or(&0),
                success_rate: Self::calculate_success_rate(total_requests, *layer_error_counts.get("Stream").unwrap_or(&0)),
            },
            mix_layer: LayerPerformance {
                latency_ms: latency_stats.layer_statistics.mix_layer.avg_latency_ms,
                throughput_mbps: layer_throughput,
                error_count: *layer_error_counts.get("Mix").unwrap_or(&0),
                success_rate: Self::calculate_success_rate(total_requests, *layer_error_counts.get("Mix").unwrap_or(&0)),
            },
            fec_layer: LayerPerformance {
                latency_ms: latency_stats.layer_statistics.fec_layer.avg_latency_ms,
                throughput_mbps: layer_throughput,
                error_count: *layer_error_counts.get("FEC").unwrap_or(&0),
                success_rate: Self::calculate_success_rate(total_requests, *layer_error_counts.get("FEC").unwrap_or(&0)),
            },
            transport_layer: LayerPerformance {
                latency_ms: latency_stats.layer_statistics.transport_layer.avg_latency_ms,
                throughput_mbps: layer_throughput,
                error_count: *layer_error_counts.get("Transport").unwrap_or(&0),
                success_rate: Self::calculate_success_rate(total_requests, *layer_error_counts.get("Transport").unwrap_or(&0)),
            },
        }
    }
    
    fn calculate_success_rate(total_requests: u64, error_count: u64) -> f64 {
        if total_requests > 0 {
            ((total_requests - error_count) as f64 / total_requests as f64) * 100.0
        } else {
            100.0
        }
    }
}