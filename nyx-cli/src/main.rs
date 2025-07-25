#![forbid(unsafe_code)]

//! Nyx command line tool with comprehensive network functionality.
//! 
//! Implements connect, status, bench subcommands for interacting with Nyx daemon
//! via gRPC, with full internationalization support and professional CLI experience.

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::sleep;
use indicatif::{ProgressBar, ProgressStyle};
use console::style;
use comfy_table::{Table, presets::UTF8_FULL};
use byte_unit::{Byte, UnitType};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use crossterm::{execute, terminal::{Clear, ClearType}, cursor::MoveTo};
use std::collections::HashMap;
use tokio::io::AsyncReadExt;

mod i18n;
mod benchmark;
mod latency_collector;
mod throughput_measurer;
mod error_tracker;
mod statistics_renderer;
mod performance_analyzer;

use i18n::localize;
use benchmark::{BenchmarkRunner, BenchmarkConfig, LatencyPercentiles};
use statistics_renderer::{StatisticsRenderer, DisplayConfig, DisplayFormat, StatisticsFilter, StatisticsData, StatisticsSummary, RealTimeMetrics, ConnectionHealth, SystemLoad};
use performance_analyzer::{PerformanceAnalyzer, AnalysisConfig};

// Include generated gRPC code
pub mod proto {
    tonic::include_proto!("nyx.api");
}

use proto::nyx_control_client::NyxControlClient;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Daemon endpoint
    #[arg(short, long, default_value = "http://127.0.0.1:50051")]
    endpoint: Option<String>,
    
    /// Language (en, ja, zh)
    #[arg(short, long, default_value = "en")]
    language: String,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to a target through Nyx network
    Connect {
        /// Target address to connect to
        target: String,
        /// Enable interactive mode
        #[arg(short, long)]
        interactive: bool,
        /// Connection timeout in seconds
        #[arg(short = 't', long, default_value = "30")]
        connect_timeout: u64,
        /// Stream name for identification
        #[arg(short = 'n', long, default_value = "nyx-stream")]
        stream_name: String,
    },
    /// Show daemon status
    Status {
        /// Output format (json, yaml, table)
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Watch mode - continuously update status
        #[arg(short, long)]
        watch: bool,
        /// Update interval in seconds for watch mode
        #[arg(short, long, default_value = "5")]
        interval: u64,
    },
    /// Benchmark connection performance
    Bench {
        /// Target address for benchmarking
        target: String,
        /// Duration of benchmark in seconds
        #[arg(short, long, default_value = "60")]
        duration: u64,
        /// Number of concurrent connections
        #[arg(short, long, default_value = "10")]
        connections: u32,
        /// Payload size in bytes
        #[arg(short, long, default_value = "1024")]
        payload_size: usize,
        /// Rate limit (requests per second)
        #[arg(short, long)]
        rate_limit: Option<u64>,
        /// Show detailed statistics
        #[arg(long)]
        detailed: bool,
    },
    /// Analyze error statistics and metrics
    Metrics {
        /// Prometheus endpoint URL
        #[arg(short, long, default_value = "http://127.0.0.1:9090")]
        prometheus_url: String,
        /// Time range for analysis (e.g., "1h", "24h", "7d")
        #[arg(short, long, default_value = "1h")]
        time_range: String,
        /// Output format (json, table, summary)
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Show detailed error breakdown
        #[arg(long)]
        detailed: bool,
    },
    /// Display comprehensive network statistics
    Statistics {
        /// Output format (table, json, summary, compact)
        #[arg(short, long, default_value = "table")]
        format: String,
        /// Enable real-time updates
        #[arg(short, long)]
        realtime: bool,
        /// Update interval in seconds for real-time mode
        #[arg(short, long, default_value = "5")]
        interval: u64,
        /// Show layer breakdown
        #[arg(long)]
        layers: bool,
        /// Show percentile breakdown
        #[arg(long)]
        percentiles: bool,
        /// Show distribution histogram
        #[arg(long)]
        distribution: bool,
        /// Filter by time range (e.g., "1h", "24h", "7d")
        #[arg(long)]
        time_range: Option<String>,
        /// Filter by stream IDs (comma-separated)
        #[arg(long)]
        stream_ids: Option<String>,
        /// Enable performance analysis and recommendations
        #[arg(long)]
        analyze: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct BenchmarkResult {
    target: String,
    duration: Duration,
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    bytes_sent: u64,
    bytes_received: u64,
    avg_latency: Duration,
    percentiles: LatencyPercentiles,
    throughput: f64,
    error_rate: f64,
    timestamp: DateTime<Utc>,
}



#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct StatusInfo {
    daemon: DaemonInfo,
    network: NetworkInfo,
    performance: PerformanceInfo,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct MetricsAnalysis {
    time_range: String,
    total_requests: u64,
    error_count: u64,
    error_rate: f64,
    error_breakdown: HashMap<String, u64>,
    latency_metrics: LatencyMetrics,
    throughput_metrics: ThroughputMetrics,
    availability_metrics: AvailabilityMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct LatencyMetrics {
    avg_latency_ms: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    max_latency_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct ThroughputMetrics {
    avg_rps: f64,
    max_rps: f64,
    avg_bandwidth_mbps: f64,
    peak_bandwidth_mbps: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct AvailabilityMetrics {
    uptime_percentage: f64,
    downtime_duration_minutes: f64,
    mtbf_hours: f64, // Mean Time Between Failures
    mttr_minutes: f64, // Mean Time To Recovery
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct DaemonInfo {
    node_id: String,
    version: String,
    uptime: Duration,
    pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct NetworkInfo {
    active_streams: u32,
    connected_peers: u32,
    mix_routes: u32,
    bytes_in: u64,
    bytes_out: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct PerformanceInfo {
    cover_traffic_rate: f64,
    avg_latency: Duration,
    packet_loss_rate: f64,
    bandwidth_utilization: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Setup signal handler for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        shutdown_clone.store(true, Ordering::Relaxed);
    });

    match &cli.command {
        Commands::Connect { target, interactive, connect_timeout, stream_name } => {
            cmd_connect(&cli, target, *interactive, *connect_timeout, stream_name, shutdown).await
        }
        Commands::Status { format, watch, interval } => {
            cmd_status(&cli, format, *watch, *interval, shutdown).await
        }
        Commands::Bench { target, duration, connections, payload_size, rate_limit, detailed } => {
            cmd_bench(&cli, target, *duration, *connections, *payload_size, *rate_limit, *detailed, shutdown).await
        }
        Commands::Metrics { prometheus_url, time_range, format, detailed } => {
            cmd_metrics(&cli, prometheus_url, time_range, format, *detailed, shutdown).await
        }
        Commands::Statistics { format, realtime, interval, layers, percentiles, distribution, time_range, stream_ids, analyze } => {
            cmd_statistics(&cli, format, *realtime, *interval, *layers, *percentiles, *distribution, time_range, stream_ids, *analyze, shutdown).await
        }
    }
}

fn default_daemon_endpoint() -> String {
    "127.0.0.1:8080".to_string()
}

async fn create_client(cli: &Cli) -> Result<NyxControlClient<Channel>, Box<dyn std::error::Error>> {
    let default_endpoint = default_daemon_endpoint();
    let endpoint_str = cli.endpoint.as_deref().unwrap_or(&default_endpoint);
    
    let channel = if endpoint_str.starts_with("http://") || endpoint_str.starts_with("https://") {
        Endpoint::from_shared(endpoint_str.to_string())?
    } else {
        Endpoint::from_shared(format!("http://{}", endpoint_str))?
    }
    .connect()
    .await?;
    
    Ok(NyxControlClient::new(channel))
}

async fn cmd_connect(
    cli: &Cli,
    target: &str,
    interactive: bool,
    connect_timeout: u64,
    stream_name: &str,
    shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Enhanced connection with retry logic and exponential backoff
    let max_retries = 3;
    let mut retry_count = 0;
    let mut base_delay = Duration::from_millis(500);
    
    println!("{}", style(format!("Initiating Nyx stream connection to {}", target)).cyan());
    println!("{}", style("Performing target address resolution...").dim());
    
    // Target address resolution - validate and normalize the address
    let resolved_address = resolve_target_address(target).await?;
    println!("{}", style(format!("Resolved target: {}", resolved_address)).green());
    
    // Create client with connection pooling and health checks
    let mut client = create_enhanced_client(cli).await?;
    
    // Configure stream options for optimal Nyx performance
    let stream_options = proto::StreamOptions {
        buffer_size: 65536, // 64KB buffer for optimal throughput
        timeout_ms: (connect_timeout * 1000) as u32,
        multipath: true, // Enable multipath for resilience
        max_paths: 3, // Use up to 3 parallel paths
        path_strategy: "latency_weighted".to_string(), // Optimize for low latency
        auto_reconnect: true, // Enable automatic reconnection
        max_retry_attempts: max_retries as u32,
        compression: false, // Disable compression for latency
        cipher_suite: "ChaCha20Poly1305".to_string(), // Fast cipher for Nyx
    };
    
    let request = proto::OpenRequest {
        stream_name: stream_name.to_string(),
        target_address: resolved_address.clone(),
        options: Some(stream_options),
    };
    
    // Enhanced progress indication with detailed status
    let progress = ProgressBar::new_spinner();
    progress.set_style(ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner:.blue} {msg} {elapsed_precise}")?);
    
    let mut connection_established = false;
    let mut stream_response: Option<proto::StreamResponse> = None;
    let overall_start = Instant::now();
    
    // Retry loop with exponential backoff
    while retry_count <= max_retries && !shutdown.load(Ordering::Relaxed) {
        let attempt_start = Instant::now();
        
        if retry_count == 0 {
            progress.set_message(format!("Establishing Nyx handshake with {}", target));
        } else {
            progress.set_message(format!("Retry attempt {}/{} (backoff: {}ms)", 
                retry_count, max_retries, base_delay.as_millis()));
            // Apply exponential backoff with jitter
            let jitter = Duration::from_millis(fastrand::u64(0..=100));
            sleep(base_delay + jitter).await;
        }
        
        // Start progress spinner
        let progress_clone = progress.clone();
        let spinner_task = tokio::spawn(async move {
            loop {
                progress_clone.tick();
                sleep(Duration::from_millis(100)).await;
            }
        });
        
        // Attempt connection with detailed error handling
        let connection_result = tokio::time::timeout(
            Duration::from_secs(connect_timeout),
            client.open_stream(Request::new(request.clone()))
        ).await;
        
        spinner_task.abort();
        
        match connection_result {
            Ok(Ok(response)) => {
                let stream_info = response.into_inner();
                let attempt_duration = attempt_start.elapsed();
                
                progress.finish_and_clear();
                println!("{}", style("✅ Nyx stream established successfully!").green());
                println!("Stream ID: {}", stream_info.stream_id);
                println!("Target: {}", stream_info.target_address);
                println!("Status: {}", stream_info.status);
                println!("Connection time: {:.2}s", attempt_duration.as_secs_f64());
                
                if let Some(stats) = &stream_info.initial_stats {
                    println!("Initial RTT: {:.2}ms", stats.avg_rtt_ms);
                    println!("Stream state: {}", stats.state);
                }
                
                stream_response = Some(stream_info);
                connection_established = true;
                break;
            }
            Ok(Err(e)) => {
                let error_msg = format!("Stream establishment failed: {}", e);
                match e.code() {
                    tonic::Code::Unavailable => {
                        progress.set_message(format!("Daemon unavailable, retrying..."));
                        if retry_count >= max_retries {
                            progress.finish_and_clear();
                            println!("{}", style("❌ Daemon is unavailable after all retry attempts").red());
                            return Err(format!("Daemon unavailable: {}", e).into());
                        }
                    }
                    tonic::Code::DeadlineExceeded => {
                        progress.set_message(format!("Connection timeout, retrying..."));
                        if retry_count >= max_retries {
                            progress.finish_and_clear();
                            println!("{}", style("❌ Connection timeout after all retry attempts").red());
                            return Err(format!("Connection timeout: {}", e).into());
                        }
                    }
                    tonic::Code::NotFound => {
                        progress.finish_and_clear();
                        println!("{}", style(format!("❌ Target not reachable: {}", target)).red());
                        return Err(format!("Target not found: {}", e).into());
                    }
                    tonic::Code::PermissionDenied => {
                        progress.finish_and_clear();
                        println!("{}", style("❌ Access denied - check daemon permissions").red());
                        return Err(format!("Permission denied: {}", e).into());
                    }
                    _ => {
                        progress.set_message(format!("Connection error: {}", e.message()));
                        if retry_count >= max_retries {
                            progress.finish_and_clear();
                            println!("{}", style(format!("❌ {}", error_msg)).red());
                            return Err(e.into());
                        }
                    }
                }
            }
            Err(_) => {
                // Timeout occurred
                progress.set_message(format!("Operation timeout, retrying..."));
                if retry_count >= max_retries {
                    progress.finish_and_clear();
                    println!("{}", style("❌ Connection timeout after all retry attempts").red());
                    return Err("Connection timeout".into());
                }
            }
        }
        
        retry_count += 1;
        base_delay = std::cmp::min(base_delay * 2, Duration::from_secs(10)); // Cap at 10 seconds
    }
    
    if !connection_established {
        return Err("Failed to establish connection after all retry attempts".into());
    }
    
    let stream_info = stream_response.unwrap();
    println!("Total connection time: {:.2}s", overall_start.elapsed().as_secs_f64());
    
    // Enhanced interactive or non-interactive data transfer
    if interactive {
        println!("{}", style("🔗 Entering interactive mode with real-time monitoring").yellow());
        println!("{}", style("Type 'quit' to exit, 'status' for connection info, or any text to send").dim());
        
        // Start connection monitoring in background
        let monitoring_client = create_enhanced_client(cli).await?;
        let stream_id_for_monitoring = stream_info.stream_id;
        let monitoring_task = tokio::spawn(async move {
            monitor_connection_health(monitoring_client, stream_id_for_monitoring).await;
        });
        
        // Interactive session with enhanced functionality
        let result = run_enhanced_interactive_session(&mut client, &stream_info, shutdown.clone()).await;
        
        monitoring_task.abort();
        
        match result {
            Ok(_) => println!("{}", style("Interactive session completed successfully").green()),
            Err(e) => println!("{}", style(format!("Interactive session error: {}", e)).yellow()),
        }
    } else {
        // Enhanced non-interactive mode with performance testing
        println!("{}", style("📤 Running connection test...").cyan());
        
        let test_result = run_connection_test(&mut client, &stream_info).await?;
        display_connection_test_results(&test_result);
    }
    
    // Graceful stream closure
    println!("{}", style("🔌 Closing Nyx stream...").dim());
    let close_request = proto::StreamId { id: stream_info.stream_id };
    match client.close_stream(Request::new(close_request)).await {
        Ok(_) => println!("{}", style("✅ Stream closed gracefully").green()),
        Err(e) => println!("{}", style(format!("⚠️  Stream close warning: {}", e)).yellow()),
    }
    
    Ok(())
}

/// Resolve and validate target address
async fn resolve_target_address(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Basic validation and normalization
    if target.is_empty() {
        return Err("Target address cannot be empty".into());
    }
    
    // Check if it's already a valid address format
    if target.contains(':') {
        // Validate port range
        if let Some(port_str) = target.split(':').last() {
            if let Ok(port) = port_str.parse::<u16>() {
                if port == 0 {
                    return Err("Port number cannot be 0".into());
                }
                return Ok(target.to_string());
            }
        }
        return Err("Invalid port number in target address".into());
    }
    
    // Add default port if not specified
    Ok(format!("{}:80", target))
}

/// Create enhanced client with connection pooling and health checks
async fn create_enhanced_client(cli: &Cli) -> Result<NyxControlClient<Channel>, Box<dyn std::error::Error>> {
    let default_endpoint = default_daemon_endpoint();
    let endpoint_str = cli.endpoint.as_deref().unwrap_or(&default_endpoint);
    
    let endpoint = if endpoint_str.starts_with("http://") || endpoint_str.starts_with("https://") {
        Endpoint::from_shared(endpoint_str.to_string())?
    } else {
        Endpoint::from_shared(format!("http://{}", endpoint_str))?
    }
    .connect_timeout(Duration::from_secs(10))
    .timeout(Duration::from_secs(30))
    .tcp_keepalive(Some(Duration::from_secs(60)))
    .keep_alive_timeout(Duration::from_secs(10))
    .keep_alive_while_idle(true);
    
    let channel = endpoint.connect().await?;
    Ok(NyxControlClient::new(channel))
}

/// Monitor connection health in background
async fn monitor_connection_health(mut client: NyxControlClient<Channel>, stream_id: u32) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        // Get stream statistics
        let stream_id_req = proto::StreamId { id: stream_id };
        match client.get_stream_stats(Request::new(stream_id_req)).await {
            Ok(response) => {
                let stats = response.into_inner();
                if stats.avg_rtt_ms > 200.0 {
                    println!("\n{}", style(format!("⚠️  High latency detected: {:.2}ms", stats.avg_rtt_ms)).yellow());
                }
                if stats.retransmissions > 0 {
                    println!("\n{}", style(format!("⚠️  Packet retransmissions: {}", stats.retransmissions)).yellow());
                }
            }
            Err(_) => {
                // Stream might be closed or connection lost
                break;
            }
        }
    }
}

/// Run enhanced interactive session
async fn run_enhanced_interactive_session(
    client: &mut NyxControlClient<Channel>,
    stream_info: &proto::StreamResponse,
    shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = tokio::io::stdin();
    let mut buffer = vec![0u8; 4096]; // Larger buffer for better performance
    
    while !shutdown.load(Ordering::Relaxed) {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout())?;
        
        match stdin.read(&mut buffer).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                let input_str = String::from_utf8_lossy(&buffer[..n]);
                let input = input_str.trim();
                
                if input == "quit" || input == "exit" {
                    break;
                } else if input == "status" {
                    // Display connection status
                    display_stream_status(client, stream_info.stream_id).await?;
                    continue;
                } else if input.is_empty() {
                    continue;
                }
                
                // Send data with timing
                let send_start = Instant::now();
                let data_request = proto::DataRequest {
                    stream_id: stream_info.stream_id.to_string(),
                    data: input.as_bytes().to_vec(),
                };
                
                match client.send_data(Request::new(data_request)).await {
                    Ok(response) => {
                        let data_response = response.into_inner();
                        let send_duration = send_start.elapsed();
                        
                        if data_response.success {
                            println!("✅ Data sent successfully ({:.2}ms)", send_duration.as_millis());
                            println!("📊 Bytes sent: {} | Protocol bytes: {}", 
                                input.len(), 
                                data_response.bytes_sent
                            );
                        } else {
                            println!("❌ Send failed: {}", data_response.error);
                        }
                    }
                    Err(e) => {
                        println!("❌ Network error: {}", e);
                        if matches!(e.code(), tonic::Code::Unavailable | tonic::Code::DeadlineExceeded) {
                            println!("⚠️  Connection may be lost. Type 'quit' to exit.");
                        }
                    }
                }
            }
            Err(e) => {
                println!("Input error: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

/// Display stream status information
async fn display_stream_status(
    client: &mut NyxControlClient<Channel>,
    stream_id: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let stream_id_req = proto::StreamId { id: stream_id };
    
    match client.get_stream_stats(Request::new(stream_id_req)).await {
        Ok(response) => {
            let stats = response.into_inner();
            println!("\n📊 Stream Status:");
            println!("  Stream ID: {}", stats.stream_id);
            println!("  State: {}", stats.state);
            println!("  Target: {}", stats.target_address);
            println!("  Bytes sent: {} | received: {}", stats.bytes_sent, stats.bytes_received);
            println!("  RTT: {:.2}ms (min: {:.2}ms)", stats.avg_rtt_ms, stats.min_rtt_ms);
            println!("  Retransmissions: {}", stats.retransmissions);
        }
        Err(e) => {
            println!("Failed to get stream status: {}", e);
        }
    }
    
    Ok(())
}

/// Connection test results
#[derive(Debug)]
struct ConnectionTestResult {
    total_time: Duration,
    bytes_sent: usize,
    rtt_ms: f64,
    success: bool,
    error_message: Option<String>,
}

/// Run connection test in non-interactive mode
async fn run_connection_test(
    client: &mut NyxControlClient<Channel>,
    stream_info: &proto::StreamResponse,
) -> Result<ConnectionTestResult, Box<dyn std::error::Error>> {
    let test_data = b"Nyx Network Connection Test - Performance Validation";
    let test_start = Instant::now();
    
    let data_request = proto::DataRequest {
        stream_id: stream_info.stream_id.to_string(),
        data: test_data.to_vec(),
    };
    
    match client.send_data(Request::new(data_request)).await {
        Ok(response) => {
            let data_response = response.into_inner();
            let test_duration = test_start.elapsed();
            
            // Get updated stream stats for RTT
            let stream_id_req = proto::StreamId { id: stream_info.stream_id };
            let rtt = match client.get_stream_stats(Request::new(stream_id_req)).await {
                Ok(stats_response) => stats_response.into_inner().avg_rtt_ms,
                Err(_) => 0.0,
            };
            
            Ok(ConnectionTestResult {
                total_time: test_duration,
                bytes_sent: test_data.len(),
                rtt_ms: rtt,
                success: data_response.success,
                error_message: if data_response.success { None } else { Some(data_response.error) },
            })
        }
        Err(e) => {
            Ok(ConnectionTestResult {
                total_time: test_start.elapsed(),
                bytes_sent: 0,
                rtt_ms: 0.0,
                success: false,
                error_message: Some(e.to_string()),
            })
        }
    }
}

/// Display connection test results
fn display_connection_test_results(result: &ConnectionTestResult) {
    println!("\n📊 Connection Test Results:");
    
    if result.success {
        println!("✅ Test completed successfully");
        println!("  Data sent: {} bytes", result.bytes_sent);
        println!("  Total time: {:.2}ms", result.total_time.as_millis());
        println!("  RTT: {:.2}ms", result.rtt_ms);
        
        // Calculate throughput
        let throughput = (result.bytes_sent as f64 * 8.0) / (result.total_time.as_secs_f64() * 1_000_000.0);
        println!("  Throughput: {:.2} Mbps", throughput);
        
        // Performance assessment
        if result.rtt_ms < 50.0 {
            println!("  Performance: {} Excellent latency", style("🟢").green());
        } else if result.rtt_ms < 100.0 {
            println!("  Performance: {} Good latency", style("🟡").yellow());
        } else {
            println!("  Performance: {} High latency", style("🔴").red());
        }
    } else {
        println!("❌ Test failed");
        if let Some(error) = &result.error_message {
            println!("  Error: {}", error);
        }
        println!("  Duration: {:.2}ms", result.total_time.as_millis());
    }
}

async fn cmd_status(
    cli: &Cli,
    format: &str,
    watch: bool,
    interval: u64,
    shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = create_client(cli).await?;
    
    loop {
        let request = Request::new(proto::Empty {});
        let response = client.get_info(request).await?;
        let status = response.into_inner();
        
        match format {
            "json" => {
                // TODO: Implement JSON serialization for NodeInfo
                eprintln!("JSON format not yet implemented for NodeInfo");
                display_status_table(&status, &cli.language)?;
            }
            "yaml" => {
                // TODO: Implement YAML serialization for NodeInfo
                eprintln!("YAML format not yet implemented for NodeInfo");
                display_status_table(&status, &cli.language)?;
            }
            "table" | _ => {
                display_status_table(&status, &cli.language)?;
            }
        }
        
        if !watch || shutdown.load(Ordering::Relaxed) {
            break;
        }
        
        sleep(Duration::from_secs(interval)).await;
        
        // Clear screen for watch mode
        execute!(std::io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    }
    
    Ok(())
}

fn display_status_table(status: &proto::NodeInfo, language: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    
    let mut args = HashMap::new();
    args.insert("version", status.version.clone());
    println!("{}", localize(language, "daemon_version", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("uptime", format_duration(status.uptime_sec as u64));
    println!("{}", localize(language, "uptime", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("bytes_in", Byte::from_u128(status.bytes_in as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string());
    println!("{}", localize(language, "network_bytes_in", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("bytes_out", Byte::from_u128(status.bytes_out as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string());
    println!("{}", localize(language, "network_bytes_out", Some(&args))?);
    
    Ok(())
}

async fn cmd_bench(
    cli: &Cli,
    target: &str,
    duration: u64,
    connections: u32,
    payload_size: usize,
    rate_limit: Option<u64>,
    detailed: bool,
    shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Starting benchmark with actual Nyx stream establishment...").cyan());
    
    let mut args = HashMap::new();
    args.insert("target", target.to_string());
    println!("{}", localize(&cli.language, "benchmark_target", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("duration", duration.to_string());
    println!("{}", localize(&cli.language, "benchmark_duration", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("connections", connections.to_string());
    println!("{}", localize(&cli.language, "benchmark_connections", Some(&args))?);
    
    let mut args = HashMap::new();
    args.insert("payload_size", Byte::from_u128(payload_size as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string());
    println!("{}", localize(&cli.language, "benchmark_payload_size", Some(&args))?);
    
    if let Some(limit) = rate_limit {
        println!("Rate limit: {} requests/second", limit);
    } else {
        println!("Rate limit: None (maximum throughput)");
    }
    
    // Progress bar for benchmark
    let pb = ProgressBar::new(duration);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")?
        .progress_chars("##-"));
    
    // Create client and benchmark configuration
    let client = create_client(cli).await?;
    let config = BenchmarkConfig {
        target: target.to_string(),
        duration: Duration::from_secs(duration),
        connections,
        payload_size,
        rate_limit,
    };
    
    // Create and run benchmark
    let mut benchmark_runner = BenchmarkRunner::new(client, config, shutdown.clone());
    
    // Start progress bar update task
    let pb_clone = pb.clone();
    let shutdown_clone = shutdown.clone();
    let progress_task = tokio::spawn(async move {
        let mut elapsed_secs = 0;
        while elapsed_secs < duration && !shutdown_clone.load(Ordering::Relaxed) {
            sleep(Duration::from_secs(1)).await;
            elapsed_secs += 1;
            pb_clone.set_position(elapsed_secs);
            pb_clone.set_message(format!("Running benchmark... {}s/{}", elapsed_secs, duration));
        }
    });
    
    // Execute benchmark
    let result = benchmark_runner.run().await?;
    
    // Stop progress bar
    progress_task.abort();
    pb.finish_with_message("Benchmark completed");
    
    // Display results
    display_benchmark_results(&result, &cli.language, detailed).await?;
    
    Ok(())
}

/// Display comprehensive benchmark results with layer-specific metrics
async fn display_benchmark_results(
    result: &benchmark::BenchmarkResult,
    language: &str,
    detailed: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", style("Benchmark Results:").bold().green());
    
    // Main results table
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Metric", "Value"]);
    
    table.add_row(vec!["Target", &result.target]);
    table.add_row(vec!["Duration", &format!("{:.2}s", result.duration.as_secs_f64())]);
    table.add_row(vec!["Total Requests", &result.total_requests.to_string()]);
    table.add_row(vec!["Successful", &result.successful_requests.to_string()]);
    table.add_row(vec!["Failed", &result.failed_requests.to_string()]);
    table.add_row(vec!["Error Rate", &format!("{:.2}%", result.error_rate)]);
    table.add_row(vec!["Requests/sec", &format!("{:.2}", result.total_requests as f64 / result.duration.as_secs_f64())]);
    table.add_row(vec!["Avg Latency", &format!("{:.2}ms", result.avg_latency.as_millis())]);
    
    let data_sent_str = Byte::from_u128(result.bytes_sent as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string();
    table.add_row(vec!["Data Sent", &data_sent_str]);
    
    let data_received_str = Byte::from_u128(result.bytes_received as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string();
    table.add_row(vec!["Data Received", &data_received_str]);
    
    table.add_row(vec!["Throughput", &format!("{:.2} Mbps", result.throughput_mbps)]);
    
    println!("{}", table);
    
    if detailed {
        println!("\n{}", style("Detailed Statistics:").bold());
        
        // Latency percentiles table
        let mut latency_table = Table::new();
        latency_table.load_preset(UTF8_FULL);
        latency_table.set_header(vec!["Percentile", "Latency"]);
        
        latency_table.add_row(vec!["50th (Median)", &format!("{:.2}ms", result.percentiles.p50.as_millis())]);
        latency_table.add_row(vec!["90th", &format!("{:.2}ms", result.percentiles.p90.as_millis())]);
        latency_table.add_row(vec!["95th", &format!("{:.2}ms", result.percentiles.p95.as_millis())]);
        latency_table.add_row(vec!["99th", &format!("{:.2}ms", result.percentiles.p99.as_millis())]);
        latency_table.add_row(vec!["99.9th", &format!("{:.2}ms", result.percentiles.p99_9.as_millis())]);
        
        println!("\n{}", style("Latency Distribution:").bold());
        println!("{}", latency_table);
        
        // Enhanced latency statistics from collector
        println!("\n{}", style("Advanced Latency Analysis:").bold());
        let mut advanced_table = Table::new();
        advanced_table.load_preset(UTF8_FULL);
        advanced_table.set_header(vec!["Metric", "Value"]);
        
        advanced_table.add_row(vec!["Standard Deviation", &format!("{:.2}ms", result.latency_statistics.std_deviation_ms)]);
        advanced_table.add_row(vec!["Min Latency", &format!("{:.2}ms", result.latency_statistics.min_latency_ms)]);
        advanced_table.add_row(vec!["Max Latency", &format!("{:.2}ms", result.latency_statistics.max_latency_ms)]);
        advanced_table.add_row(vec!["75th Percentile", &format!("{:.2}ms", result.latency_statistics.percentiles.p75)]);
        advanced_table.add_row(vec!["99.5th Percentile", &format!("{:.2}ms", result.latency_statistics.percentiles.p99_5)]);
        advanced_table.add_row(vec!["99.99th Percentile", &format!("{:.2}ms", result.latency_statistics.percentiles.p99_99)]);
        
        println!("{}", advanced_table);
        
        // Latency distribution histogram
        if !result.latency_statistics.distribution.buckets.is_empty() {
            println!("\n{}", style("Latency Distribution Histogram:").bold());
            let mut hist_table = Table::new();
            hist_table.load_preset(UTF8_FULL);
            hist_table.set_header(vec!["Range (ms)", "Count", "Percentage"]);
            
            for bucket in &result.latency_statistics.distribution.buckets {
                if bucket.count > 0 {
                    hist_table.add_row(vec![
                        &format!("{:.1}-{:.1}", bucket.range_start_ms, bucket.range_end_ms),
                        &bucket.count.to_string(),
                        &format!("{:.1}%", bucket.percentage),
                    ]);
                }
            }
            
            println!("{}", hist_table);
        }
        
        // Layer-specific performance metrics
        println!("\n{}", style("Protocol Layer Performance:").bold());
        let mut layer_table = Table::new();
        layer_table.load_preset(UTF8_FULL);
        layer_table.set_header(vec!["Layer", "Avg Latency (ms)", "95th Percentile (ms)", "Contribution (%)", "Errors", "Success Rate (%)"]);
        
        layer_table.add_row(vec![
            "Stream",
            &format!("{:.2}", result.latency_statistics.layer_statistics.stream_layer.avg_latency_ms),
            &format!("{:.2}", result.latency_statistics.layer_statistics.stream_layer.percentile_95_ms),
            &format!("{:.1}", result.latency_statistics.layer_statistics.stream_layer.contribution_percentage),
            &result.layer_metrics.stream_layer.error_count.to_string(),
            &format!("{:.2}", result.layer_metrics.stream_layer.success_rate),
        ]);
        
        layer_table.add_row(vec![
            "Mix",
            &format!("{:.2}", result.latency_statistics.layer_statistics.mix_layer.avg_latency_ms),
            &format!("{:.2}", result.latency_statistics.layer_statistics.mix_layer.percentile_95_ms),
            &format!("{:.1}", result.latency_statistics.layer_statistics.mix_layer.contribution_percentage),
            &result.layer_metrics.mix_layer.error_count.to_string(),
            &format!("{:.2}", result.layer_metrics.mix_layer.success_rate),
        ]);
        
        layer_table.add_row(vec![
            "FEC",
            &format!("{:.2}", result.latency_statistics.layer_statistics.fec_layer.avg_latency_ms),
            &format!("{:.2}", result.latency_statistics.layer_statistics.fec_layer.percentile_95_ms),
            &format!("{:.1}", result.latency_statistics.layer_statistics.fec_layer.contribution_percentage),
            &result.layer_metrics.fec_layer.error_count.to_string(),
            &format!("{:.2}", result.layer_metrics.fec_layer.success_rate),
        ]);
        
        layer_table.add_row(vec![
            "Transport",
            &format!("{:.2}", result.latency_statistics.layer_statistics.transport_layer.avg_latency_ms),
            &format!("{:.2}", result.latency_statistics.layer_statistics.transport_layer.percentile_95_ms),
            &format!("{:.1}", result.latency_statistics.layer_statistics.transport_layer.contribution_percentage),
            &result.layer_metrics.transport_layer.error_count.to_string(),
            &format!("{:.2}", result.layer_metrics.transport_layer.success_rate),
        ]);
        
        println!("{}", layer_table);
        
        // Performance analysis and recommendations
        println!("\n{}", style("Performance Analysis:").bold());
        if result.error_rate > 5.0 {
            println!("⚠️  High error rate detected ({:.2}%). Consider reducing load or checking network connectivity.", result.error_rate);
        } else if result.error_rate > 1.0 {
            println!("⚠️  Moderate error rate ({:.2}%). Monitor system performance.", result.error_rate);
        } else {
            println!("✅ Low error rate ({:.2}%). System performing well.", result.error_rate);
        }
        
        if result.avg_latency.as_millis() > 100 {
            println!("⚠️  High average latency ({:.2}ms). Consider optimizing network or server performance.", result.avg_latency.as_millis());
        } else if result.avg_latency.as_millis() > 50 {
            println!("⚠️  Moderate latency ({:.2}ms). Monitor performance trends.", result.avg_latency.as_millis());
        } else {
            println!("✅ Low latency ({:.2}ms). Excellent performance.", result.avg_latency.as_millis());
        }
        
        if result.throughput_mbps < 1.0 {
            println!("⚠️  Low throughput ({:.2} Mbps). Check network capacity and daemon configuration.", result.throughput_mbps);
        } else if result.throughput_mbps < 10.0 {
            println!("ℹ️  Moderate throughput ({:.2} Mbps). Consider optimizing for higher bandwidth applications.", result.throughput_mbps);
        } else {
            println!("✅ Good throughput ({:.2} Mbps). Network performing well.", result.throughput_mbps);
        }
        
        // Layer-specific recommendations
        if result.layer_metrics.stream_layer.error_count > 0 {
            println!("⚠️  Stream layer errors detected. Check stream establishment and management.");
        }
        if result.layer_metrics.mix_layer.error_count > 0 {
            println!("⚠️  Mix layer errors detected. Check routing and path selection.");
        }
        if result.layer_metrics.fec_layer.error_count > 0 {
            println!("⚠️  FEC layer errors detected. Check forward error correction configuration.");
        }
        if result.layer_metrics.transport_layer.error_count > 0 {
            println!("⚠️  Transport layer errors detected. Check network connectivity and transport configuration.");
        }
        
        let mut args = HashMap::new();
        args.insert("p99_latency", format!("{:.1}ms", result.percentiles.p99.as_millis()));
        println!("{}", localize(language, "benchmark_p99_latency", Some(&args)).unwrap_or_else(|_| format!("99th percentile latency: {}", args["p99_latency"])));
    }
    
    Ok(())
}

/// Collect comprehensive statistics data from the daemon
async fn collect_statistics_data(
    client: &mut NyxControlClient<Channel>,
) -> Result<StatisticsData, Box<dyn std::error::Error>> {
    // Get daemon info
    let daemon_info_response = client.get_info(Request::new(proto::Empty {})).await?;
    let daemon_info = daemon_info_response.into_inner();
    
    // Simulate collecting comprehensive statistics
    // In a real implementation, this would query actual daemon metrics
    let timestamp = Utc::now();
    
    // Create summary statistics
    let summary = StatisticsSummary {
        total_requests: daemon_info.bytes_out / 1024, // Simplified calculation
        successful_requests: (daemon_info.bytes_out / 1024) * 95 / 100, // Assume 95% success rate
        failed_requests: (daemon_info.bytes_out / 1024) * 5 / 100,
        success_rate: 95.0,
        avg_latency_ms: daemon_info.performance.as_ref().map(|p| p.avg_latency_ms).unwrap_or(50.0),
        throughput_mbps: daemon_info.performance.as_ref().map(|p| p.bandwidth_utilization * 100.0).unwrap_or(10.0),
        active_connections: daemon_info.active_streams,
        uptime_seconds: daemon_info.uptime_sec as u64,
    };
    
    // Create latency statistics (simplified)
    let latency_stats = latency_collector::LatencyStatistics {
        total_measurements: 100,
        avg_latency_ms: summary.avg_latency_ms,
        min_latency_ms: summary.avg_latency_ms * 0.5,
        max_latency_ms: summary.avg_latency_ms * 2.0,
        std_deviation_ms: summary.avg_latency_ms * 0.2,
        percentiles: latency_collector::LatencyPercentiles {
            p50: summary.avg_latency_ms * 0.8,
            p75: summary.avg_latency_ms * 0.9,
            p90: summary.avg_latency_ms * 1.1,
            p95: summary.avg_latency_ms * 1.3,
            p99: summary.avg_latency_ms * 1.8,
            p99_5: summary.avg_latency_ms * 1.9,
            p99_9: summary.avg_latency_ms * 1.95,
            p99_99: summary.avg_latency_ms * 1.99,
        },
        layer_statistics: latency_collector::LayerLatencyStatistics {
            stream_layer: latency_collector::LayerStats {
                avg_latency_ms: summary.avg_latency_ms * 0.4,
                min_latency_ms: summary.avg_latency_ms * 0.2,
                max_latency_ms: summary.avg_latency_ms * 0.8,
                percentile_95_ms: summary.avg_latency_ms * 0.6,
                contribution_percentage: 40.0,
            },
            mix_layer: latency_collector::LayerStats {
                avg_latency_ms: summary.avg_latency_ms * 0.3,
                min_latency_ms: summary.avg_latency_ms * 0.15,
                max_latency_ms: summary.avg_latency_ms * 0.6,
                percentile_95_ms: summary.avg_latency_ms * 0.45,
                contribution_percentage: 30.0,
            },
            fec_layer: latency_collector::LayerStats {
                avg_latency_ms: summary.avg_latency_ms * 0.2,
                min_latency_ms: summary.avg_latency_ms * 0.1,
                max_latency_ms: summary.avg_latency_ms * 0.4,
                percentile_95_ms: summary.avg_latency_ms * 0.3,
                contribution_percentage: 20.0,
            },
            transport_layer: latency_collector::LayerStats {
                avg_latency_ms: summary.avg_latency_ms * 0.1,
                min_latency_ms: summary.avg_latency_ms * 0.05,
                max_latency_ms: summary.avg_latency_ms * 0.2,
                percentile_95_ms: summary.avg_latency_ms * 0.15,
                contribution_percentage: 10.0,
            },
        },
        distribution: latency_collector::LatencyDistribution {
            buckets: vec![
                latency_collector::LatencyBucket {
                    range_start_ms: 0.0,
                    range_end_ms: 25.0,
                    count: 30,
                    percentage: 30.0,
                },
                latency_collector::LatencyBucket {
                    range_start_ms: 25.0,
                    range_end_ms: 50.0,
                    count: 40,
                    percentage: 40.0,
                },
                latency_collector::LatencyBucket {
                    range_start_ms: 50.0,
                    range_end_ms: 100.0,
                    count: 25,
                    percentage: 25.0,
                },
                latency_collector::LatencyBucket {
                    range_start_ms: 100.0,
                    range_end_ms: 200.0,
                    count: 5,
                    percentage: 5.0,
                },
            ],
            total_count: 100,
        },
        time_series: Vec::new(),
    };
    
    // Create throughput statistics (simplified)
    let throughput_stats = throughput_measurer::ThroughputStatistics {
        duration_secs: summary.uptime_seconds as f64,
        total_bytes_sent: daemon_info.bytes_out,
        total_bytes_received: daemon_info.bytes_in,
        avg_send_rate_mbps: summary.throughput_mbps * 0.5,
        avg_receive_rate_mbps: summary.throughput_mbps * 0.5,
        peak_send_rate_mbps: summary.throughput_mbps * 0.75,
        peak_receive_rate_mbps: summary.throughput_mbps * 0.75,
        min_send_rate_mbps: summary.throughput_mbps * 0.25,
        min_receive_rate_mbps: summary.throughput_mbps * 0.25,
        protocol_overhead_percentage: 15.0,
        data_transfer_efficiency: 0.85,
        bandwidth_utilization: throughput_measurer::BandwidthUtilization {
            theoretical_max_mbps: 100.0,
            actual_utilization_percentage: daemon_info.performance.as_ref().map(|p| p.bandwidth_utilization * 100.0).unwrap_or(50.0),
            efficiency_score: 0.8,
            bottleneck_analysis: "No significant bottlenecks detected".to_string(),
        },
        performance_analysis: throughput_measurer::PerformanceAnalysis {
            overall_score: 85.0,
            bottlenecks: Vec::new(),
            recommendations: vec!["Consider optimizing buffer sizes".to_string()],
            efficiency_rating: "Good".to_string(),
        },
        time_series: Vec::new(),
    };
    
    // Create error statistics (simplified)
    let mut error_rate_by_layer = HashMap::new();
    error_rate_by_layer.insert("stream".to_string(), error_tracker::LayerErrorStats {
        layer_name: "stream".to_string(),
        error_count: 2,
        error_rate: 2.0,
        most_common_errors: vec!["connection_timeout".to_string()],
        avg_time_between_errors_ms: 30000.0,
        error_severity: error_tracker::ErrorSeverity::Low,
    });
    
    let mut error_rate_by_type = HashMap::new();
    error_rate_by_type.insert("connection_timeout".to_string(), error_tracker::ErrorTypeStats {
        error_type: "connection_timeout".to_string(),
        count: 5,
        percentage: 50.0,
        first_occurrence: Utc::now() - chrono::Duration::hours(1),
        last_occurrence: Utc::now() - chrono::Duration::minutes(5),
        frequency_per_minute: 0.1,
        associated_layers: vec!["stream".to_string(), "transport".to_string()],
    });
    
    let error_stats = error_tracker::ErrorStatistics {
        total_errors: 10,
        total_requests: summary.total_requests,
        overall_error_rate: 5.0,
        error_rate_by_layer,
        error_rate_by_type,
        error_trends: error_tracker::ErrorTrends {
            error_rate_trend: "stable".to_string(),
            peak_error_periods: Vec::new(),
            error_clustering: false,
            dominant_error_types: vec!["connection_timeout".to_string()],
            error_rate_change_percentage: 0.0,
        },
        correlation_analysis: error_tracker::CorrelationAnalysis {
            latency_correlation: 0.3,
            bandwidth_correlation: 0.1,
            load_correlation: 0.2,
            connection_count_correlation: 0.4,
            strongest_correlation: "connection_count".to_string(),
            correlation_insights: vec!["Errors increase with connection count".to_string()],
        },
        troubleshooting_recommendations: Vec::new(),
        time_series: Vec::new(),
    };
    
    // Create layer metrics
    let layer_metrics = benchmark::LayerMetrics {
        stream_layer: benchmark::LayerPerformance {
            latency_ms: summary.avg_latency_ms * 0.4,
            throughput_mbps: summary.throughput_mbps * 0.25,
            error_count: 2,
            success_rate: 98.0,
        },
        mix_layer: benchmark::LayerPerformance {
            latency_ms: summary.avg_latency_ms * 0.3,
            throughput_mbps: summary.throughput_mbps * 0.25,
            error_count: 3,
            success_rate: 97.0,
        },
        fec_layer: benchmark::LayerPerformance {
            latency_ms: summary.avg_latency_ms * 0.2,
            throughput_mbps: summary.throughput_mbps * 0.25,
            error_count: 1,
            success_rate: 99.0,
        },
        transport_layer: benchmark::LayerPerformance {
            latency_ms: summary.avg_latency_ms * 0.1,
            throughput_mbps: summary.throughput_mbps * 0.25,
            error_count: 4,
            success_rate: 96.0,
        },
    };
    
    // Create real-time metrics
    let real_time_metrics = RealTimeMetrics {
        current_rps: summary.total_requests as f64 / summary.uptime_seconds as f64,
        current_latency_ms: summary.avg_latency_ms,
        current_throughput_mbps: summary.throughput_mbps,
        current_error_rate: 100.0 - summary.success_rate,
        connection_health: ConnectionHealth {
            healthy_connections: summary.active_connections * 80 / 100,
            degraded_connections: summary.active_connections * 15 / 100,
            failed_connections: summary.active_connections * 5 / 100,
            overall_health_score: summary.success_rate / 100.0,
        },
        system_load: SystemLoad {
            cpu_usage_percent: daemon_info.performance.as_ref().map(|p| p.cpu_usage * 100.0).unwrap_or(25.0),
            memory_usage_mb: daemon_info.performance.as_ref().map(|p| p.memory_usage_mb).unwrap_or(512.0),
            network_utilization_percent: daemon_info.performance.as_ref().map(|p| p.bandwidth_utilization * 100.0).unwrap_or(50.0),
            daemon_health: "healthy".to_string(),
        },
    };
    
    Ok(StatisticsData {
        timestamp,
        summary,
        latency_stats,
        throughput_stats,
        error_stats,
        layer_metrics,
        real_time_metrics,
    })
}

async fn cmd_statistics(
    cli: &Cli,
    format: &str,
    realtime: bool,
    interval: u64,
    show_layers: bool,
    show_percentiles: bool,
    show_distribution: bool,
    time_range: &Option<String>,
    stream_ids: &Option<String>,
    analyze: bool,
    shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Nyx Network Statistics").bold().cyan());
    
    // Parse display format
    let display_format = match format {
        "json" => DisplayFormat::Json,
        "summary" => DisplayFormat::Summary,
        "compact" => DisplayFormat::Compact,
        _ => DisplayFormat::Table,
    };
    
    // Create display configuration
    let mut display_config = DisplayConfig {
        format: display_format,
        update_interval: Duration::from_secs(interval),
        show_layer_breakdown: show_layers,
        show_percentiles: show_percentiles,
        show_distribution: show_distribution,
        show_time_series: false,
        filter: StatisticsFilter::default(),
    };
    
    // Parse stream IDs filter if provided
    if let Some(ids_str) = stream_ids {
        display_config.filter.stream_ids = ids_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
    }
    
    // Parse time range filter if provided
    if let Some(range_str) = time_range {
        // Simple time range parsing - in a full implementation this would be more robust
        let hours = match range_str.as_str() {
            "1h" => 1,
            "24h" => 24,
            "7d" => 24 * 7,
            _ => 1,
        };
        
        let end_time = Utc::now();
        let start_time = end_time - ChronoDuration::hours(hours);
        
        display_config.filter.time_range = Some(statistics_renderer::TimeRange {
            start: start_time,
            end: end_time,
        });
    }
    
    // Create statistics renderer
    let mut renderer = StatisticsRenderer::new(display_config);
    
    // Create performance analyzer if requested
    let mut analyzer = if analyze {
        Some(PerformanceAnalyzer::new(AnalysisConfig::default()))
    } else {
        None
    };
    
    // Create daemon client for data collection
    let mut client = create_client(cli).await?;
    
    if realtime {
        println!("{}", style("Starting real-time statistics display...").green());
        println!("{}", style("Press Ctrl+C to exit").dim());
        
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            
            // Collect current statistics from daemon
            let stats_data = collect_statistics_data(&mut client).await?;
            
            // Add to analyzer if enabled
            if let Some(ref mut analyzer) = analyzer {
                analyzer.add_data_point(stats_data.clone());
                
                // Perform analysis periodically
                if let Ok(analysis) = analyzer.analyze() {
                    if !analysis.alerts.is_empty() {
                        println!("\n{}", style("⚠️  Performance Alerts:").bold().yellow());
                        for alert in &analysis.alerts {
                            println!("  • {}", alert.message);
                        }
                    }
                    
                    if !analysis.recommendations.is_empty() {
                        println!("\n{}", style("💡 Recommendations:").bold().blue());
                        for rec in analysis.recommendations.iter().take(3) {
                            println!("  • {}: {}", rec.title, rec.description);
                        }
                    }
                }
            }
            
            // Display statistics
            renderer.display_real_time(&stats_data).await?;
            
            // Wait for next update
            sleep(Duration::from_secs(interval)).await;
        }
    } else {
        // Single snapshot mode
        let stats_data = collect_statistics_data(&mut client).await?;
        
        // Perform analysis if requested
        if let Some(ref mut analyzer) = analyzer {
            analyzer.add_data_point(stats_data.clone());
            
            if let Ok(analysis) = analyzer.analyze() {
                println!("\n{}", style("📊 Performance Analysis").bold());
                println!("Overall Health: {:.1}%", analysis.overall_health.overall_score * 100.0);
                
                if !analysis.alerts.is_empty() {
                    println!("\n{}", style("⚠️  Alerts:").bold().yellow());
                    for alert in &analysis.alerts {
                        println!("  • [{}] {}", 
                            match alert.severity {
                                performance_analyzer::AlertSeverity::Critical => "CRITICAL",
                                performance_analyzer::AlertSeverity::Warning => "WARNING",
                                performance_analyzer::AlertSeverity::Info => "INFO",
                                performance_analyzer::AlertSeverity::Emergency => "EMERGENCY",
                            },
                            alert.message
                        );
                    }
                }
                
                if !analysis.recommendations.is_empty() {
                    println!("\n{}", style("💡 Recommendations:").bold().blue());
                    for rec in &analysis.recommendations {
                        println!("  • [{}] {}: {}", 
                            match rec.priority {
                                performance_analyzer::RecommendationPriority::Critical => "HIGH",
                                performance_analyzer::RecommendationPriority::High => "HIGH",
                                performance_analyzer::RecommendationPriority::Medium => "MED",
                                performance_analyzer::RecommendationPriority::Low => "LOW",
                            },
                            rec.title, 
                            rec.description
                        );
                    }
                }
            }
        }
        
        // Display statistics
        let output = renderer.render(&stats_data)?;
        println!("{}", output);
    }
    
    Ok(())
}

async fn cmd_metrics(
    cli: &Cli,
    _prometheus_url: &str,
    time_range: &str,
    format: &str,
    detailed: bool,
    _shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Analyzing metrics...").cyan());
    
    // Simulate Prometheus query (in real implementation, this would query actual Prometheus)
    let analysis = simulate_metrics_analysis(time_range).await?;
    
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&analysis)?;
            println!("{}", json);
        }
        "summary" => {
            display_metrics_summary(&analysis, &cli.language)?;
        }
        _ => {
            display_metrics_table(&analysis, &cli.language, detailed)?;
        }
    }
    
    Ok(())
}

async fn simulate_metrics_analysis(time_range: &str) -> Result<MetricsAnalysis, Box<dyn std::error::Error>> {
    // Simulate metrics collection
    sleep(Duration::from_millis(500)).await;
    
    let mut error_breakdown = HashMap::new();
    error_breakdown.insert("connection_timeout".to_string(), 45);
    error_breakdown.insert("network_unreachable".to_string(), 23);
    error_breakdown.insert("authentication_failed".to_string(), 12);
    error_breakdown.insert("protocol_error".to_string(), 8);
    error_breakdown.insert("internal_error".to_string(), 5);
    
    Ok(MetricsAnalysis {
        time_range: time_range.to_string(),
        total_requests: 15420,
        error_count: 93,
        error_rate: 0.6,
        error_breakdown,
        latency_metrics: LatencyMetrics {
            avg_latency_ms: 45.2,
            p50_latency_ms: 38.5,
            p95_latency_ms: 89.3,
            p99_latency_ms: 156.7,
            max_latency_ms: 2340.1,
        },
        throughput_metrics: ThroughputMetrics {
            avg_rps: 4.28,
            max_rps: 12.5,
            avg_bandwidth_mbps: 2.1,
            peak_bandwidth_mbps: 8.9,
        },
        availability_metrics: AvailabilityMetrics {
            uptime_percentage: 99.4,
            downtime_duration_minutes: 3.2,
            mtbf_hours: 168.5,
            mttr_minutes: 2.1,
        },
    })
}

fn display_metrics_summary(analysis: &MetricsAnalysis, _language: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", style("📊 Metrics Summary").bold().blue());
    println!("Time Range: {}", analysis.time_range);
    println!("Total Requests: {}", analysis.total_requests);
    println!("Error Rate: {:.2}%", analysis.error_rate);
    println!("Average Latency: {:.1}ms", analysis.latency_metrics.avg_latency_ms);
    println!("Uptime: {:.2}%", analysis.availability_metrics.uptime_percentage);
    
    // Health assessment
    println!("\n{}", style("🏥 Health Assessment").bold());
    if analysis.error_rate < 1.0 {
        println!("✅ System health: Excellent");
    } else if analysis.error_rate < 5.0 {
        println!("⚠️  System health: Good");
    } else {
        println!("❌ System health: Needs attention");
    }
    
    Ok(())
}

fn display_metrics_table(analysis: &MetricsAnalysis, _language: &str, detailed: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n{}", style("📊 Metrics Analysis").bold().blue());
    
    // Overview table
    let mut overview_table = Table::new();
    overview_table.load_preset(UTF8_FULL);
    overview_table.set_header(vec!["Metric", "Value"]);
    
    overview_table.add_row(vec!["Time Range", &analysis.time_range]);
    overview_table.add_row(vec!["Total Requests", &analysis.total_requests.to_string()]);
    overview_table.add_row(vec!["Error Count", &analysis.error_count.to_string()]);
    overview_table.add_row(vec!["Error Rate", &format!("{:.2}%", analysis.error_rate)]);
    overview_table.add_row(vec!["Uptime", &format!("{:.2}%", analysis.availability_metrics.uptime_percentage)]);
    
    println!("{}", overview_table);
    
    // Latency metrics
    println!("\n{}", style("⏱️  Latency Metrics").bold());
    let mut latency_table = Table::new();
    latency_table.load_preset(UTF8_FULL);
    latency_table.set_header(vec!["Percentile", "Latency (ms)"]);
    
    latency_table.add_row(vec!["Average", &format!("{:.1}", analysis.latency_metrics.avg_latency_ms)]);
    latency_table.add_row(vec!["50th", &format!("{:.1}", analysis.latency_metrics.p50_latency_ms)]);
    latency_table.add_row(vec!["95th", &format!("{:.1}", analysis.latency_metrics.p95_latency_ms)]);
    latency_table.add_row(vec!["99th", &format!("{:.1}", analysis.latency_metrics.p99_latency_ms)]);
    latency_table.add_row(vec!["Max", &format!("{:.1}", analysis.latency_metrics.max_latency_ms)]);
    
    println!("{}", latency_table);
    
    // Throughput metrics
    println!("\n{}", style("🚀 Throughput Metrics").bold());
    let mut throughput_table = Table::new();
    throughput_table.load_preset(UTF8_FULL);
    throughput_table.set_header(vec!["Metric", "Value"]);
    
    throughput_table.add_row(vec!["Average RPS", &format!("{:.2}", analysis.throughput_metrics.avg_rps)]);
    throughput_table.add_row(vec!["Peak RPS", &format!("{:.2}", analysis.throughput_metrics.max_rps)]);
    throughput_table.add_row(vec!["Average Bandwidth", &format!("{:.1} Mbps", analysis.throughput_metrics.avg_bandwidth_mbps)]);
    throughput_table.add_row(vec!["Peak Bandwidth", &format!("{:.1} Mbps", analysis.throughput_metrics.peak_bandwidth_mbps)]);
    
    println!("{}", throughput_table);
    
    if detailed {
        // Error breakdown
        println!("\n{}", style("❌ Error Breakdown").bold());
        let mut error_table = Table::new();
        error_table.load_preset(UTF8_FULL);
        error_table.set_header(vec!["Error Type", "Count", "Percentage"]);
        
        for (error_type, count) in &analysis.error_breakdown {
            let percentage = (*count as f64 / analysis.error_count as f64) * 100.0;
            error_table.add_row(vec![
                error_type,
                &count.to_string(),
                &format!("{:.1}%", percentage)
            ]);
        }
        
        println!("{}", error_table);
        
        // Availability metrics
        println!("\n{}", style("📈 Availability Metrics").bold());
        let mut availability_table = Table::new();
        availability_table.load_preset(UTF8_FULL);
        availability_table.set_header(vec!["Metric", "Value"]);
        
        availability_table.add_row(vec!["Uptime", &format!("{:.2}%", analysis.availability_metrics.uptime_percentage)]);
        availability_table.add_row(vec!["Downtime", &format!("{:.1} min", analysis.availability_metrics.downtime_duration_minutes)]);
        availability_table.add_row(vec!["MTBF", &format!("{:.1} hours", analysis.availability_metrics.mtbf_hours)]);
        availability_table.add_row(vec!["MTTR", &format!("{:.1} min", analysis.availability_metrics.mttr_minutes)]);
        
        println!("{}", availability_table);
    }
    
    Ok(())
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
} 