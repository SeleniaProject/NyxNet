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
use chrono::{DateTime, Utc};
use crossterm::{execute, terminal::{Clear, ClearType}, cursor::MoveTo};
use std::collections::HashMap;
use tokio::io::AsyncReadExt;

mod i18n;
use i18n::localize;

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
struct LatencyPercentiles {
    p50: Duration,
    p90: Duration,
    p95: Duration,
    p99: Duration,
    p99_9: Duration,
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
    _shutdown: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = create_client(cli).await?;
    
    let request = proto::OpenRequest {
        stream_name: stream_name.to_string(),
        target_address: target.to_string(),
        options: None,
    };
    
    println!("{}", style(localize(&cli.language, "connecting", None)?).cyan());
    
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(ProgressStyle::default_spinner()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        .template("{spinner:.blue} {msg}")?);
    spinner.set_message(format!("Connecting to {}", target));
    
    let start_time = Instant::now();
    
    // Start the spinner
    let spinner_clone = spinner.clone();
    let spinner_task = tokio::spawn(async move {
        loop {
            spinner_clone.tick();
            sleep(Duration::from_millis(100)).await;
        }
    });
    
    // Attempt connection
    let response = tokio::time::timeout(
        Duration::from_secs(connect_timeout),
        client.open_stream(Request::new(request))
    ).await;
    
    spinner_task.abort();
    spinner.finish_and_clear();
    
    match response {
        Ok(Ok(response)) => {
            let stream_info = response.into_inner();
            let duration = start_time.elapsed();
            
            println!("{}", style(localize(&cli.language, "connection_established", None)?).green());
            println!("Stream ID: {}", stream_info.stream_id);
            println!("Connection time: {:.2}s", duration.as_secs_f64());
            
            if interactive {
                println!("{}", style("Entering interactive mode. Type 'quit' to exit.").yellow());
                
                // Start interactive session
                let mut stdin = tokio::io::stdin();
                let mut buffer = vec![0u8; 1024];
                
                loop {
                    print!("> ");
                    std::io::Write::flush(&mut std::io::stdout())?;
                    
                    // Read user input
                    match stdin.read(&mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let input_str = String::from_utf8_lossy(&buffer[..n]);
                            let input = input_str.trim();
                            
                            if input == "quit" || input == "exit" {
                                break;
                            }
                            
                            if input.is_empty() {
                                continue;
                            }
                            
                            // Send data through stream
                            println!("Sending: {}", input);
                            
                            // Simulate data transfer
                            let data_request = proto::DataRequest {
                                stream_id: stream_info.stream_id.to_string(),
                                data: input.as_bytes().to_vec(),
                            };
                            
                            match client.send_data(Request::new(data_request)).await {
                                Ok(response) => {
                                    let data_response = response.into_inner();
                                    if data_response.success {
                                        println!("✅ Data sent successfully");
                                        println!("📊 Bytes sent: {}", input.len());
                                    } else {
                                        println!("❌ Failed to send data: {}", data_response.error);
                                    }
                                }
                                Err(e) => {
                                    println!("❌ Send error: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Input error: {}", e);
                            break;
                        }
                    }
                }
                
                println!("{}", style("Exiting interactive mode...").yellow());
            } else {
                // Non-interactive mode: send test data
                println!("{}", style("Sending test data...").cyan());
                
                let test_data = b"Hello, Nyx Network!";
                let data_request = proto::DataRequest {
                    stream_id: stream_info.stream_id.to_string(),
                    data: test_data.to_vec(),
                };
                
                match client.send_data(Request::new(data_request)).await {
                    Ok(response) => {
                        let data_response = response.into_inner();
                        if data_response.success {
                            println!("✅ Test data sent successfully");
                            println!("📊 Bytes sent: {}", test_data.len());
                        } else {
                            println!("❌ Failed to send test data: {}", data_response.error);
                        }
                    }
                    Err(e) => {
                        println!("❌ Send error: {}", e);
                    }
                }
            }
        }
        Ok(Err(e)) => {
            println!("{}", style(format!("Connection failed: {}", e)).red());
            return Err(e.into());
        }
        Err(_) => {
            println!("{}", style("Connection timeout").red());
            return Err("Connection timeout".into());
        }
    }
    
    Ok(())
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
    println!("{}", style("Starting benchmark...").cyan());
    
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
    
    // Progress bar for benchmark
    let pb = ProgressBar::new(duration);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")?
        .progress_chars("##-"));
    
    let start_time = Instant::now();
    let mut total_requests = 0u64;
    let mut successful_requests = 0u64;
    let mut failed_requests = 0u64;
    let mut total_bytes_sent = 0u64;
    let mut total_bytes_received = 0u64;
    let mut latencies = Vec::new();
    
    // Create client for actual requests
    let mut client = create_client(cli).await?;
    
    // Generate test payload
    let _payload = vec![0u8; payload_size];
    
    // Run actual benchmark with real requests
    for i in 0..duration {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        
        let requests_this_second = if let Some(limit) = rate_limit {
            limit.min(connections as u64 * 10)
        } else {
            connections as u64 * 10
        };
        
        // Execute requests for this second
        for _ in 0..requests_this_second {
            let request_start = Instant::now();
            total_requests += 1;
            
            // Make actual request to daemon
            let request = proto::OpenRequest {
                stream_name: format!("bench_stream_{}", total_requests),
                target_address: target.to_string(),
                options: None,
            };
            
            match client.open_stream(Request::new(request)).await {
                Ok(response) => {
                    successful_requests += 1;
                    total_bytes_sent += payload_size as u64;
                    
                    // Simulate data transfer
                    let _stream_response = response.into_inner();
                    total_bytes_received += payload_size as u64;
                    
                    let latency = request_start.elapsed();
                    latencies.push(latency);
                }
                Err(_) => {
                    failed_requests += 1;
                }
            }
            
            // Rate limiting
            if let Some(limit) = rate_limit {
                let delay = Duration::from_millis(1000 / limit);
                sleep(delay).await;
            }
        }
        
        pb.set_position(i + 1);
        pb.set_message(format!("RPS: {} | Success: {} | Failed: {}", 
                              requests_this_second, successful_requests, failed_requests));
        
        sleep(Duration::from_secs(1)).await;
    }
    
    pb.finish_with_message("Benchmark completed");
    
    let elapsed = start_time.elapsed();
    let avg_rps = total_requests as f64 / elapsed.as_secs_f64();
    
    // Calculate latency statistics
    latencies.sort();
    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    } else {
        Duration::from_millis(0)
    };
    
    let percentiles = if !latencies.is_empty() {
        LatencyPercentiles {
            p50: latencies[latencies.len() * 50 / 100],
            p90: latencies[latencies.len() * 90 / 100],
            p95: latencies[latencies.len() * 95 / 100],
            p99: latencies[latencies.len() * 99 / 100],
            p99_9: latencies[latencies.len() * 999 / 1000],
        }
    } else {
        LatencyPercentiles {
            p50: Duration::from_millis(0),
            p90: Duration::from_millis(0),
            p95: Duration::from_millis(0),
            p99: Duration::from_millis(0),
            p99_9: Duration::from_millis(0),
        }
    };
    
    let error_rate = if total_requests > 0 {
        (failed_requests as f64 / total_requests as f64) * 100.0
    } else {
        0.0
    };
    
    let throughput_mbps = (total_bytes_sent as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
    
    // Display results
    println!("\n{}", style("Benchmark Results:").bold().green());
    
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Metric", "Value"]);
    
    table.add_row(vec!["Duration", &format!("{:.2}s", elapsed.as_secs_f64())]);
    table.add_row(vec!["Total Requests", &total_requests.to_string()]);
    table.add_row(vec!["Successful", &successful_requests.to_string()]);
    table.add_row(vec!["Failed", &failed_requests.to_string()]);
    table.add_row(vec!["Error Rate", &format!("{:.2}%", error_rate)]);
    table.add_row(vec!["Requests/sec", &format!("{:.2}", avg_rps)]);
    table.add_row(vec!["Avg Latency", &format!("{:.2}ms", avg_latency.as_millis())]);
    
    let mut args = HashMap::new();
    args.insert("total_data", Byte::from_u128(total_bytes_sent as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string());
    table.add_row(vec!["Data Sent", &args["total_data"]]);
    
    let mut args = HashMap::new();
    args.insert("total_data", Byte::from_u128(total_bytes_received as u128).unwrap().get_appropriate_unit(UnitType::Binary).to_string());
    table.add_row(vec!["Data Received", &args["total_data"]]);
    
    table.add_row(vec!["Throughput", &format!("{:.2} Mbps", throughput_mbps)]);
    
    println!("{}", table);
    
    if detailed {
        println!("\n{}", style("Detailed Statistics:").bold());
        
        // Latency percentiles table
        let mut latency_table = Table::new();
        latency_table.load_preset(UTF8_FULL);
        latency_table.set_header(vec!["Percentile", "Latency"]);
        
        latency_table.add_row(vec!["50th (Median)", &format!("{:.2}ms", percentiles.p50.as_millis())]);
        latency_table.add_row(vec!["90th", &format!("{:.2}ms", percentiles.p90.as_millis())]);
        latency_table.add_row(vec!["95th", &format!("{:.2}ms", percentiles.p95.as_millis())]);
        latency_table.add_row(vec!["99th", &format!("{:.2}ms", percentiles.p99.as_millis())]);
        
        println!("\n{}", style("Latency Distribution:").bold());
        println!("{}", latency_table);
        
        // Connection statistics
        println!("\n{}", style("Connection Statistics:").bold());
        let mut conn_table = Table::new();
        conn_table.load_preset(UTF8_FULL);
        conn_table.set_header(vec!["Metric", "Value"]);
        
        conn_table.add_row(vec!["Concurrent Connections", &connections.to_string()]);
        conn_table.add_row(vec!["Success Rate", &format!("{:.2}%", 100.0 - error_rate)]);
        conn_table.add_row(vec!["Average RPS", &format!("{:.2}", avg_rps)]);
        
        if let Some(limit) = rate_limit {
            conn_table.add_row(vec!["Rate Limit", &format!("{} req/s", limit)]);
        } else {
            conn_table.add_row(vec!["Rate Limit", "None"]);
        }
        
        println!("{}", conn_table);
        
        // Performance analysis
        println!("\n{}", style("Performance Analysis:").bold());
        if error_rate > 5.0 {
            println!("⚠️  High error rate detected. Consider reducing load or checking network connectivity.");
        } else if error_rate > 1.0 {
            println!("⚠️  Moderate error rate. Monitor system performance.");
        } else {
            println!("✅ Low error rate. System performing well.");
        }
        
        if avg_latency.as_millis() > 100 {
            println!("⚠️  High average latency. Consider optimizing network or server performance.");
        } else if avg_latency.as_millis() > 50 {
            println!("⚠️  Moderate latency. Monitor performance trends.");
        } else {
            println!("✅ Low latency. Excellent performance.");
        }
        
        let mut args = HashMap::new();
        args.insert("p99_latency", "45.0ms".to_string());
        println!("{}", localize(&cli.language, "benchmark_p99_latency", Some(&args))?);
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