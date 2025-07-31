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

/// Comprehensive benchmark analysis results
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkAnalysis {
    pub overall_performance: OverallPerformance,
    pub latency_analysis: LatencyAnalysis,
    pub throughput_analysis: ThroughputAnalysis,
    pub error_analysis: ErrorAnalysis,
    pub recommendations: Vec<String>,
}

/// Overall performance metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct OverallPerformance {
    pub success_rate: f64,
    pub avg_throughput_mbps: f64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub efficiency_score: f64,
}

/// Detailed latency analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyAnalysis {
    pub distribution: LatencyDistribution,
    pub consistency_score: f64,
    pub outlier_percentage: f64,
}

/// Latency distribution metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyDistribution {
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p99_9_ms: f64,
}

/// Throughput analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct ThroughputAnalysis {
    pub peak_mbps: f64,
    pub sustained_mbps: f64,
    pub efficiency: f64,
    pub bottleneck_indicators: Vec<String>,
}

/// Error analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorAnalysis {
    pub error_distribution: HashMap<String, u64>,
    pub critical_errors: u64,
    pub recoverable_errors: u64,
    pub error_patterns: Vec<String>,
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
                                    estimated_latency_ms: Self::estimate_network_latency(&target).await,
                                    estimated_bandwidth_mbps: Self::estimate_network_bandwidth().await,
                                    connection_count: config.connections,
                                    system_load: Self::get_system_load(),
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
    
    /// Enhanced benchmark analysis with detailed performance breakdown
    pub fn analyze_benchmark_results(result: &BenchmarkResult) -> BenchmarkAnalysis {
        let mut analysis = BenchmarkAnalysis {
            overall_performance: OverallPerformance {
                success_rate: (result.successful_requests as f64 / result.total_requests as f64) * 100.0,
                avg_throughput_mbps: result.throughput_mbps,
                avg_latency_ms: result.avg_latency.as_millis() as f64,
                error_rate: result.error_rate,
                efficiency_score: 0.0, // Will be calculated
            },
            latency_analysis: LatencyAnalysis {
                distribution: LatencyDistribution {
                    p50_ms: result.percentiles.p50.as_millis() as f64,
                    p90_ms: result.percentiles.p90.as_millis() as f64,
                    p95_ms: result.percentiles.p95.as_millis() as f64,
                    p99_ms: result.percentiles.p99.as_millis() as f64,
                    p99_9_ms: result.percentiles.p99_9.as_millis() as f64,
                },
                consistency_score: 0.0, // Will be calculated
                outlier_percentage: 0.0, // Will be calculated
            },
            throughput_analysis: ThroughputAnalysis {
                peak_mbps: result.throughput_statistics.peak_send_rate_mbps,
                sustained_mbps: result.throughput_statistics.avg_send_rate_mbps,
                efficiency: 0.0, // Will be calculated
                bottleneck_indicators: Vec::new(),
            },
            error_analysis: ErrorAnalysis {
                error_distribution: result.error_statistics.error_rate_by_type
                    .iter()
                    .map(|(k, v)| (k.clone(), v.count))
                    .collect(),
                critical_errors: result.error_statistics.total_errors / 10, // Estimate critical errors
                recoverable_errors: result.error_statistics.total_errors - (result.error_statistics.total_errors / 10),
                error_patterns: Vec::new(), // Will be analyzed
            },
            recommendations: Vec::new(), // Will be generated
        };
        
        // Calculate efficiency score (0-100)
        analysis.overall_performance.efficiency_score = Self::calculate_efficiency_score(result);
        
        // Calculate latency consistency score
        analysis.latency_analysis.consistency_score = Self::calculate_latency_consistency(result);
        
        // Calculate outlier percentage
        analysis.latency_analysis.outlier_percentage = Self::calculate_outlier_percentage(result);
        
        // Calculate throughput efficiency
        analysis.throughput_analysis.efficiency = Self::calculate_throughput_efficiency(result);
        
        // Identify bottlenecks
        analysis.throughput_analysis.bottleneck_indicators = Self::identify_bottlenecks(result);
        
        // Analyze error patterns
        analysis.error_analysis.error_patterns = Self::analyze_error_patterns(result);
        
        // Generate recommendations
        analysis.recommendations = Self::generate_recommendations(result, &analysis);
        
        analysis
    }
    
    /// Calculate overall efficiency score based on multiple factors
    fn calculate_efficiency_score(result: &BenchmarkResult) -> f64 {
        let success_weight = 0.4;
        let latency_weight = 0.3;
        let throughput_weight = 0.3;
        
        // Success rate component (0-100)
        let success_score = (result.successful_requests as f64 / result.total_requests as f64) * 100.0;
        
        // Latency component (lower is better, normalize to 0-100)
        let latency_ms = result.avg_latency.as_millis() as f64;
        let latency_score = if latency_ms > 0.0 {
            (1000.0 / (latency_ms + 10.0)) * 100.0 // Normalize with offset
        } else {
            100.0
        };
        
        // Throughput component (higher is better, normalize to 0-100)
        let throughput_score = if result.throughput_mbps > 0.0 {
            (result.throughput_mbps / 100.0).min(1.0) * 100.0 // Normalize to 100 Mbps max
        } else {
            0.0
        };
        
        (success_score * success_weight + latency_score * latency_weight + throughput_score * throughput_weight)
            .min(100.0)
    }
    
    /// Calculate latency consistency score
    fn calculate_latency_consistency(result: &BenchmarkResult) -> f64 {
        let p50 = result.percentiles.p50.as_millis() as f64;
        let p99 = result.percentiles.p99.as_millis() as f64;
        
        if p50 > 0.0 {
            // Lower ratio means more consistent latency
            let ratio = p99 / p50;
            (10.0 / ratio).min(10.0) * 10.0 // Normalize to 0-100
        } else {
            100.0
        }
    }
    
    /// Calculate percentage of outlier requests
    fn calculate_outlier_percentage(result: &BenchmarkResult) -> f64 {
        // Define outliers as requests taking more than 3x the median latency
        let p50_ms = result.percentiles.p50.as_millis() as f64;
        let p99_ms = result.percentiles.p99.as_millis() as f64;
        
        if p50_ms > 0.0 && p99_ms > p50_ms * 3.0 {
            // Rough estimation based on percentile distribution
            1.0 // Approximately 1% are outliers if p99 > 3*p50
        } else {
            0.1 // Very few outliers
        }
    }
    
    /// Calculate throughput efficiency
    fn calculate_throughput_efficiency(result: &BenchmarkResult) -> f64 {
        let peak = result.throughput_statistics.peak_send_rate_mbps;
        let sustained = result.throughput_statistics.avg_send_rate_mbps;
        
        if peak > 0.0 {
            (sustained / peak) * 100.0
        } else {
            0.0
        }
    }
    
    /// Identify performance bottlenecks
    fn identify_bottlenecks(result: &BenchmarkResult) -> Vec<String> {
        let mut bottlenecks = Vec::new();
        
        // High latency bottleneck
        if result.avg_latency.as_millis() > 500 {
            bottlenecks.push("High average latency detected - possible network or processing bottleneck".to_string());
        }
        
        // Low throughput bottleneck
        if result.throughput_mbps < 1.0 {
            bottlenecks.push("Low throughput detected - possible bandwidth or processing limitation".to_string());
        }
        
        // High error rate bottleneck
        if result.error_rate > 5.0 {
            bottlenecks.push("High error rate detected - possible reliability or capacity issue".to_string());
        }
        
        // Latency inconsistency bottleneck
        let p50 = result.percentiles.p50.as_millis() as f64;
        let p99 = result.percentiles.p99.as_millis() as f64;
        if p50 > 0.0 && p99 / p50 > 5.0 {
            bottlenecks.push("High latency variance detected - possible intermittent bottleneck".to_string());
        }
        
        bottlenecks
    }
    
    /// Analyze error patterns for insights
    fn analyze_error_patterns(result: &BenchmarkResult) -> Vec<String> {
        let mut patterns = Vec::new();
        
        // Analyze error distribution
        for (error_type, stats) in &result.error_statistics.error_rate_by_type {
            let percentage = (stats.count as f64 / result.total_requests as f64) * 100.0;
            if percentage > 1.0 {
                patterns.push(format!("{}: {:.1}% of requests", error_type, percentage));
            }
        }
        
        patterns
    }
    
    /// Generate performance recommendations
    fn generate_recommendations(result: &BenchmarkResult, analysis: &BenchmarkAnalysis) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        // Success rate recommendations
        if analysis.overall_performance.success_rate < 95.0 {
            recommendations.push("Consider investigating error causes and implementing retry mechanisms".to_string());
        }
        
        // Latency recommendations
        if analysis.overall_performance.avg_latency_ms > 200.0 {
            recommendations.push("High latency detected - consider optimizing network paths or reducing processing overhead".to_string());
        }
        
        // Throughput recommendations
        if analysis.overall_performance.avg_throughput_mbps < 10.0 {
            recommendations.push("Low throughput - consider increasing buffer sizes or enabling compression".to_string());
        }
        
        // Consistency recommendations
        if analysis.latency_analysis.consistency_score < 70.0 {
            recommendations.push("Inconsistent latency - investigate intermittent bottlenecks or resource contention".to_string());
        }
        
        // Error-specific recommendations
        if result.error_rate > 2.0 {
            recommendations.push("Consider implementing circuit breakers and graceful degradation".to_string());
        }
        
        recommendations
    }
    

    
    /// Estimate network latency to target using ping-like measurement
    async fn estimate_network_latency(target: &str) -> f64 {
        // Extract hostname from target address
        let hostname = if let Some(colon_pos) = target.find(':') {
            &target[..colon_pos]
        } else {
            target
        };
        
        // Perform simple TCP connection test to estimate latency
        let start = Instant::now();
        match tokio::net::TcpStream::connect(format!("{}:80", hostname)).await {
            Ok(_) => start.elapsed().as_millis() as f64,
            Err(_) => {
                // Fallback: try DNS resolution time as rough latency estimate
                match tokio::net::lookup_host(format!("{}:80", hostname)).await {
                    Ok(_) => start.elapsed().as_millis() as f64,
                    Err(_) => 50.0, // Default fallback
                }
            }
        }
    }
    
    /// Estimate available network bandwidth using system information
    async fn estimate_network_bandwidth() -> f64 {
        // Use system information to estimate bandwidth
        // This is a simplified estimation - in production, you might use more sophisticated methods
        use sysinfo::{System, Networks};
        
        let networks = Networks::new_with_refreshed_list();
        
        let mut max_bandwidth: f64 = 10.0; // Default 10 Mbps
        
        for (interface_name, network) in &networks {
            // Skip loopback and virtual interfaces
            if interface_name.contains("lo") || interface_name.contains("docker") || interface_name.contains("veth") {
                continue;
            }
            
            // Estimate bandwidth based on interface statistics
            // This is a rough estimation based on recent activity
            let received_bytes = network.total_received();
            let transmitted_bytes = network.total_transmitted();
            
            if received_bytes > 0 || transmitted_bytes > 0 {
                // Rough estimation: assume gigabit ethernet for active interfaces
                max_bandwidth = max_bandwidth.max(100.0); // 100 Mbps
                
                // If we see significant traffic, assume higher bandwidth
                if received_bytes > 1_000_000 || transmitted_bytes > 1_000_000 {
                    max_bandwidth = max_bandwidth.max(1000.0); // 1 Gbps
                }
            }
        }
        
        max_bandwidth
    }
    
    /// Get current system load as a factor affecting network performance
    fn get_system_load() -> f64 {
        use sysinfo::System;
        
        let mut sys = System::new_all();
        sys.refresh_cpu();
        
        // Calculate average CPU usage
        let cpu_usage: f64 = sys.cpus().iter().map(|cpu| cpu.cpu_usage() as f64).sum::<f64>() / sys.cpus().len() as f64;
        
        // Convert to load factor (0.0 = no load, 1.0 = full load)
        cpu_usage / 100.0
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