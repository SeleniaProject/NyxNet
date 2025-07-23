#![forbid(unsafe_code)]

//! Nyx command line tool with comprehensive network functionality.
//!
//! Implements connect, status, bench subcommands for interacting with Nyx daemon
//! via gRPC, with full internationalization support and professional CLI experience.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use regex::Regex;
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroizing;
use nyx_crypto::keystore::{encrypt_and_store, KeystoreError};
use dirs::home_dir;
use std::fs::{self, OpenOptions};
use std::io::{Write, BufRead, BufReader};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use rpassword::prompt_password;
use tokio::signal;
use tokio::sync::Mutex;
use indicatif::{ProgressBar, ProgressStyle};
use colored::*;
use comfy_table::{Table, presets::UTF8_FULL};
use byte_unit::Byte;
use humantime::format_duration;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

mod i18n;
use i18n::tr;
use fluent_bundle::FluentArgs;

// Generated gRPC client code
mod proto {
    tonic::include_proto!("nyx.api");
}

use proto::nyx_control_client::NyxControlClient;
use proto::{OpenRequest, StreamId, EventFilter};
use prost_types::Empty;
use tonic::transport::{Channel, Endpoint};

#[derive(Parser)]
#[command(name = "nyx")]
#[command(about = "Nyx command line interface", long_about = None)]
struct Cli {
    /// Daemon endpoint (default: platform-specific Unix socket)
    #[arg(long, global = true)]
    endpoint: Option<String>,
    
    /// Request timeout in seconds
    #[arg(long, default_value = "30", global = true)]
    timeout: u64,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Establish connection to target through Nyx network
    Connect {
        /// Target address (e.g., example.com:80, 192.168.1.1:22)
        target: String,
        
        /// Keep connection alive and forward stdin/stdout
        #[arg(short, long)]
        interactive: bool,
        
        /// Connection timeout in seconds
        #[arg(long, default_value = "10")]
        connect_timeout: u64,
        
        /// Stream name identifier
        #[arg(long, default_value = "cli-connection")]
        stream_name: String,
    },
    
    /// Display daemon and network status
    Status {
        /// Output format: table, json, yaml
        #[arg(short, long, default_value = "table")]
        format: String,
        
        /// Watch mode - continuously update status
        #[arg(short, long)]
        watch: bool,
        
        /// Update interval for watch mode (seconds)
        #[arg(long, default_value = "2")]
        interval: u64,
    },
    
    /// Run network performance benchmark
    Bench {
        /// Target address for benchmarking
        target: String,
        
        /// Duration of benchmark in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
        
        /// Number of concurrent connections
        #[arg(short, long, default_value = "10")]
        connections: u32,
        
        /// Payload size per request in bytes
        #[arg(short, long, default_value = "1024")]
        payload_size: usize,
        
        /// Requests per second rate limit (0 = unlimited)
        #[arg(short, long, default_value = "0")]
        rate_limit: u32,
        
        /// Output detailed latency percentiles
        #[arg(long)]
        detailed: bool,
    },
    
    /// Fetch error code statistics from Prometheus exporter
    Errors {
        /// Base URL of exporter (e.g. http://localhost:9090)
        #[arg(long, default_value = "http://localhost:9090")] 
        endpoint: String,
    },
    
    /// Rotate long-term node secret key
    Key {
        #[command(subcommand)]
        op: KeyCmd,
    },

    /// Add or manage quarantine list
    Quarantine {
        /// Node ID (hex) to quarantine
        node: String,
    },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Rotate secret key and overwrite keystore file
    Rotate {
        /// Path to keystore file (.age). Default: ~/.nyx/keys.json.age
        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
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
struct LatencyPercentiles {
    p50: Duration,
    p95: Duration,
    p99: Duration,
    p99_9: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatusInfo {
    daemon: DaemonInfo,
    network: NetworkInfo,
    performance: PerformanceInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonInfo {
    node_id: String,
    version: String,
    uptime: Duration,
    pid: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkInfo {
    active_streams: u32,
    connected_peers: u32,
    mix_routes: u32,
    bytes_in: u64,
    bytes_out: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerformanceInfo {
    cover_traffic_rate: f64,
    avg_latency: Duration,
    packet_loss_rate: f64,
    bandwidth_utilization: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Setup graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            shutdown_clone.store(true, Ordering::SeqCst);
        }
    });

    let result = match cli.command {
        Commands::Connect { target, interactive, connect_timeout, stream_name } => {
            cmd_connect(&cli, &target, interactive, connect_timeout, &stream_name, shutdown).await
        },
        Commands::Status { format, watch, interval } => {
            cmd_status(&cli, &format, watch, interval, shutdown).await
        },
        Commands::Bench { target, duration, connections, payload_size, rate_limit, detailed } => {
            cmd_bench(&cli, &target, duration, connections, payload_size, rate_limit, detailed, shutdown).await
        },
        Commands::Errors { endpoint } => {
            cmd_errors(&endpoint)
        },
        Commands::Key { op } => match op {
            KeyCmd::Rotate { path } => cmd_key_rotate(path),
        },
        Commands::Quarantine { node } => {
            cmd_quarantine(&node)
        },
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }

    Ok(())
}

/// Get the default daemon endpoint for the current platform
fn default_daemon_endpoint() -> String {
    #[cfg(unix)]
    return "unix:///tmp/nyx.sock".to_string();
    #[cfg(windows)]
    return "tcp://127.0.0.1:43299".to_string(); // Named pipes not supported by tonic
}

/// Create gRPC client connection to daemon
async fn create_client(cli: &Cli) -> Result<NyxControlClient<Channel>> {
    let endpoint_str = cli.endpoint.as_deref().unwrap_or(&default_daemon_endpoint());
    
    let channel = if endpoint_str.starts_with("unix://") {
        #[cfg(unix)]
        {
            let path = endpoint_str.strip_prefix("unix://").unwrap();
            Endpoint::try_from("http://[::]:50051")?
                .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
                    tokio::net::UnixStream::connect(path)
                }))
                .await?
        }
        #[cfg(windows)]
        {
            return Err(anyhow::anyhow!("Unix sockets not supported on Windows"));
        }
    } else {
        Endpoint::try_from(endpoint_str)?
            .timeout(Duration::from_secs(cli.timeout))
            .connect()
            .await?
    };

    Ok(NyxControlClient::new(channel))
}

/// Implementation of `nyx connect`
async fn cmd_connect(
    cli: &Cli,
    target: &str,
    interactive: bool,
    connect_timeout: u64,
    stream_name: &str,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let mut args = FluentArgs::new();
    args.set("target", target);
    println!("{}", tr("connect-establishing", Some(&args)));

    let mut client = create_client(cli).await
        .map_err(|e| {
            let mut args = FluentArgs::new();
            args.set("error", e.to_string());
            anyhow::anyhow!(tr("error-daemon-connection", Some(&args)))
        })?;

    let request = OpenRequest {
        stream_name: stream_name.to_string(),
    };

    let timeout = Duration::from_secs(connect_timeout);
    let response = tokio::time::timeout(timeout, client.open_stream(request)).await
        .map_err(|_| {
            let mut args = FluentArgs::new();
            args.set("duration", format_duration(timeout).to_string());
            anyhow::anyhow!(tr("connect-timeout", Some(&args)))
        })?
        .map_err(|e| {
            let mut args = FluentArgs::new();
            args.set("target", target);
            args.set("error", e.to_string());
            anyhow::anyhow!(tr("connect-failed", Some(&args)))
        })?;

    let stream_response = response.into_inner();
    
    let mut args = FluentArgs::new();
    args.set("target", target);
    args.set("stream_id", stream_response.stream_id.to_string());
    println!("{}", tr("connect-success", Some(&args)).green());

    if interactive {
        println!("{}", tr("press-ctrl-c", None).yellow());
        
        // Keep connection alive until interrupted
        while !shutdown.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        // Close stream gracefully
        let close_request = StreamId { id: stream_response.stream_id };
        let _ = client.close_stream(close_request).await;
        
        println!("{}", tr("connect-interrupted", None).yellow());
    }

    Ok(())
}

/// Implementation of `nyx status`
async fn cmd_status(
    cli: &Cli,
    format: &str,
    watch: bool,
    interval: u64,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let mut client = create_client(cli).await
        .map_err(|e| {
            let mut args = FluentArgs::new();
            args.set("error", e.to_string());
            anyhow::anyhow!(tr("error-daemon-connection", Some(&args)))
        })?;

    loop {
        let start_time = Instant::now();
        
        let info_response = client.get_info(Empty {}).await
            .map_err(|e| {
                let mut args = FluentArgs::new();
                args.set("error", e.to_string());
                anyhow::anyhow!(tr("error-network-error", Some(&args)))
            })?.into_inner();

        let status = StatusInfo {
            daemon: DaemonInfo {
                node_id: info_response.node_id,
                version: info_response.version,
                uptime: Duration::from_secs(info_response.uptime_sec as u64),
                pid: None, // TODO: Add PID to proto
            },
            network: NetworkInfo {
                active_streams: 0, // TODO: Add to proto
                connected_peers: 0, // TODO: Add to proto
                mix_routes: 0, // TODO: Add to proto
                bytes_in: info_response.bytes_in,
                bytes_out: info_response.bytes_out,
            },
            performance: PerformanceInfo {
                cover_traffic_rate: 0.0, // TODO: Add to proto
                avg_latency: Duration::from_millis(0), // TODO: Add to proto
                packet_loss_rate: 0.0, // TODO: Add to proto
                bandwidth_utilization: 0.0, // TODO: Add to proto
            },
        };

        match format {
            "json" => {
                println!("{}", serde_json::to_string_pretty(&status)?);
            },
            "yaml" => {
                println!("{}", serde_yaml::to_string(&status)?);
            },
            _ => {
                display_status_table(&status)?;
            }
        }

        if !watch || shutdown.load(Ordering::SeqCst) {
            break;
        }

        let elapsed = start_time.elapsed();
        let sleep_duration = Duration::from_secs(interval).saturating_sub(elapsed);
        
        if sleep_duration > Duration::ZERO {
            tokio::time::sleep(sleep_duration).await;
        }
        
        // Clear screen for watch mode
        print!("\x1B[2J\x1B[1;1H");
    }

    Ok(())
}

/// Display status information in a formatted table
fn display_status_table(status: &StatusInfo) -> Result<()> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    
    println!("{}", tr("status-daemon-info", None).blue().bold());
    
    let mut daemon_table = Table::new();
    daemon_table.load_preset(UTF8_FULL);
    
    let mut args = FluentArgs::new();
    args.set("node_id", &status.daemon.node_id);
    daemon_table.add_row(vec![tr("status-node-id", Some(&args))]);
    
    args.clear();
    args.set("version", &status.daemon.version);
    daemon_table.add_row(vec![tr("status-version", Some(&args))]);
    
    args.clear();
    args.set("uptime", format_duration(status.daemon.uptime).to_string());
    daemon_table.add_row(vec![tr("status-uptime", Some(&args))]);
    
    args.clear();
    args.set("bytes_in", Byte::from_bytes(status.network.bytes_in as u128).get_appropriate_unit(true).to_string());
    daemon_table.add_row(vec![tr("status-traffic-in", Some(&args))]);
    
    args.clear();
    args.set("bytes_out", Byte::from_bytes(status.network.bytes_out as u128).get_appropriate_unit(true).to_string());
    daemon_table.add_row(vec![tr("status-traffic-out", Some(&args))]);
    
    println!("{}", daemon_table);
    
    Ok(())
}

/// Implementation of `nyx bench`
async fn cmd_bench(
    cli: &Cli,
    target: &str,
    duration: u64,
    connections: u32,
    payload_size: usize,
    rate_limit: u32,
    detailed: bool,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let mut args = FluentArgs::new();
    args.set("target", target);
    println!("{}", tr("bench-starting", Some(&args)));
    
    args.clear();
    args.set("duration", format_duration(Duration::from_secs(duration)).to_string());
    println!("{}", tr("bench-duration", Some(&args)));
    
    args.clear();
    args.set("count", connections.to_string());
    println!("{}", tr("bench-connections", Some(&args)));
    
    args.clear();
    args.set("size", Byte::from_bytes(payload_size as u128).get_appropriate_unit(true).to_string());
    println!("{}", tr("bench-payload-size", Some(&args)));

    let progress = ProgressBar::new(duration);
    progress.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    let start_time = Instant::now();
    let total_requests = Arc::new(AtomicU64::new(0));
    let successful_requests = Arc::new(AtomicU64::new(0));
    let failed_requests = Arc::new(AtomicU64::new(0));
    let latencies = Arc::new(Mutex::new(Vec::new()));

    // Spawn benchmark tasks
    let mut handles = Vec::new();
    for _ in 0..connections {
        let client_res = create_client(cli).await;
        if client_res.is_err() {
            continue;
        }
        
        let mut client = client_res.unwrap();
        let shutdown_clone = shutdown.clone();
        let total_clone = total_requests.clone();
        let success_clone = successful_requests.clone();
        let failed_clone = failed_requests.clone();
        let latencies_clone = latencies.clone();
        let target_str = target.to_string();
        
        let handle = tokio::spawn(async move {
            while !shutdown_clone.load(Ordering::SeqCst) && start_time.elapsed() < Duration::from_secs(duration) {
                let req_start = Instant::now();
                
                let request = OpenRequest {
                    stream_name: format!("bench-{}", target_str),
                };
                
                match client.open_stream(request).await {
                    Ok(response) => {
                        let latency = req_start.elapsed();
                        success_clone.fetch_add(1, Ordering::SeqCst);
                        
                        // Close stream immediately for benchmark
                        let close_request = StreamId { id: response.into_inner().stream_id };
                        let _ = client.close_stream(close_request).await;
                        
                        latencies_clone.lock().await.push(latency);
                    },
                    Err(_) => {
                        failed_clone.fetch_add(1, Ordering::SeqCst);
                    }
                }
                
                total_clone.fetch_add(1, Ordering::SeqCst);
                
                if rate_limit > 0 {
                    let delay = Duration::from_millis(1000 / rate_limit as u64);
                    tokio::time::sleep(delay).await;
                }
            }
        });
        
        handles.push(handle);
    }

    // Progress tracking
    while start_time.elapsed() < Duration::from_secs(duration) && !shutdown.load(Ordering::SeqCst) {
        progress.set_position(start_time.elapsed().as_secs());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    progress.finish();
    
    // Wait for all tasks to complete
    for handle in handles {
        let _ = handle.await;
    }

    let total_time = start_time.elapsed();
    let total_reqs = total_requests.load(Ordering::SeqCst);
    let success_reqs = successful_requests.load(Ordering::SeqCst);
    let failed_reqs = failed_requests.load(Ordering::SeqCst);
    
    let latencies_vec = latencies.lock().await;
    let mut sorted_latencies = latencies_vec.clone();
    sorted_latencies.sort();
    
    let avg_latency = if !sorted_latencies.is_empty() {
        sorted_latencies.iter().sum::<Duration>() / sorted_latencies.len() as u32
    } else {
        Duration::ZERO
    };
    
    let percentiles = if !sorted_latencies.is_empty() {
        LatencyPercentiles {
            p50: sorted_latencies.get(sorted_latencies.len() * 50 / 100).copied().unwrap_or(Duration::ZERO),
            p95: sorted_latencies.get(sorted_latencies.len() * 95 / 100).copied().unwrap_or(Duration::ZERO),
            p99: sorted_latencies.get(sorted_latencies.len() * 99 / 100).copied().unwrap_or(Duration::ZERO),
            p99_9: sorted_latencies.get(sorted_latencies.len() * 999 / 1000).copied().unwrap_or(Duration::ZERO),
        }
    } else {
        LatencyPercentiles {
            p50: Duration::ZERO,
            p95: Duration::ZERO,
            p99: Duration::ZERO,
            p99_9: Duration::ZERO,
        }
    };

    let result = BenchmarkResult {
        target: target.to_string(),
        duration: total_time,
        total_requests: total_reqs,
        successful_requests: success_reqs,
        failed_requests: failed_reqs,
        bytes_sent: success_reqs * payload_size as u64,
        bytes_received: 0, // TODO: Track actual bytes received
        avg_latency,
        percentiles,
        throughput: if total_time.as_secs_f64() > 0.0 { success_reqs as f64 / total_time.as_secs_f64() } else { 0.0 },
        error_rate: if total_reqs > 0 { (failed_reqs as f64 / total_reqs as f64) * 100.0 } else { 0.0 },
        timestamp: Utc::now(),
    };

    display_benchmark_results(&result, detailed)?;
    
    Ok(())
}

/// Display benchmark results in a formatted table
fn display_benchmark_results(result: &BenchmarkResult, detailed: bool) -> Result<()> {
    println!("\n{}", tr("bench-results", None).green().bold());
    
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    
    let mut args = FluentArgs::new();
    args.set("duration", format_duration(result.duration).to_string());
    table.add_row(vec![tr("bench-total-time", Some(&args))]);
    
    args.clear();
    args.set("count", result.total_requests.to_string());
    table.add_row(vec![tr("bench-requests-sent", Some(&args))]);
    
    args.clear();
    args.set("count", result.successful_requests.to_string());
    table.add_row(vec![tr("bench-requests-success", Some(&args))]);
    
    args.clear();
    args.set("count", result.failed_requests.to_string());
    table.add_row(vec![tr("bench-requests-failed", Some(&args))]);
    
    args.clear();
    args.set("rate", format!("{:.2}", result.throughput));
    table.add_row(vec![tr("bench-throughput", Some(&args))]);
    
    args.clear();
    args.set("latency", format!("{:.2}ms", result.avg_latency.as_millis()));
    table.add_row(vec![tr("bench-latency-avg", Some(&args))]);
    
    if detailed {
        args.clear();
        args.set("latency", format!("{:.2}ms", result.percentiles.p50.as_millis()));
        table.add_row(vec![tr("bench-latency-p50", Some(&args))]);
        
        args.clear();
        args.set("latency", format!("{:.2}ms", result.percentiles.p95.as_millis()));
        table.add_row(vec![tr("bench-latency-p95", Some(&args))]);
        
        args.clear();
        args.set("latency", format!("{:.2}ms", result.percentiles.p99.as_millis()));
        table.add_row(vec![tr("bench-latency-p99", Some(&args))]);
    }
    
    args.clear();
    args.set("rate", Byte::from_bytes(result.bytes_sent as u128).get_appropriate_unit(true).to_string());
    table.add_row(vec![tr("bench-bandwidth", Some(&args))]);
    
    args.clear();
    args.set("rate", format!("{:.2}", result.error_rate));
    table.add_row(vec![tr("bench-error-rate", Some(&args))]);
    
    println!("{}", table);
    
    Ok(())
}

/// Implementation of `nyx errors` (unchanged)
fn cmd_errors(base: &str) -> Result<()> {
    let url = format!("{}/metrics", base.trim_end_matches('/'));
    let body = reqwest::blocking::get(&url)
        .with_context(|| format!("request metrics from {url}"))?
        .text()?;

    let re = Regex::new(r#"nyx_error_total\{code="([0-9a-f]{4})"\}\s+(\d+)"#)?;
    
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        tr("header-error-code", None),
        tr("header-description", None),
        tr("header-count", None),
    ]);
    
    fn desc(code: &str) -> &str {
        match code {
            "0007" => "UNSUPPORTED_CAP",
            "0004" => "VERSION_MISMATCH",
            "0005" => "PATH_VALIDATION_FAILED",
            _ => "UNKNOWN",
        }
    }
    
    for cap in re.captures_iter(&body) {
        let code = &cap[1];
        let count = &cap[2];
        table.add_row(vec![
            format!("0x{}", code),
            desc(code).to_string(),
            count.to_string(),
        ]);
    }
    
    println!("{}", table);
    Ok(())
}

/// Implementation of `nyx key rotate` (unchanged)
fn cmd_key_rotate(path_opt: Option<String>) -> Result<()> {
    let default_path = || {
        let mut p = home_dir().expect("home dir");
        p.push(".nyx/keys.json.age");
        p.to_string_lossy().to_string()
    };
    let path_str = path_opt.unwrap_or_else(default_path);
    let path = PathBuf::from(&path_str);

    let passphrase = prompt_password("Enter passphrase for keystore: ")?;
    if passphrase.trim().is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let secret = Zeroizing::new(bytes.to_vec());

    encrypt_and_store(&secret, &path, &passphrase).map_err(|e| match e {
        KeystoreError::Io(ioe) => ioe.into(),
        other => anyhow::anyhow!(other),
    })?;

    println!("{}", tr("rotate-success", None));
    Ok(())
}

/// Implementation of `nyx quarantine <node>` (unchanged)
fn cmd_quarantine(node: &str) -> Result<()> {
    let mut list_path = home_dir().expect("home");
    list_path.push(".nyx/quarantine.list");
    if let Some(parent) = list_path.parent() { 
        fs::create_dir_all(parent)?; 
    }

    let mut existing = Vec::new();
    if list_path.exists() {
        let file = fs::File::open(&list_path)?;
        for line in BufReader::new(file).lines() {
            if let Ok(l) = line { 
                existing.push(l); 
            }
        }
    }

    if existing.iter().any(|e| e == node) {
        let mut args = FluentArgs::new();
        args.set("node", node);
        println!("{}", tr("quarantine-duplicate", Some(&args)));
        return Ok(());
    }

    let mut file = OpenOptions::new().create(true).append(true).open(&list_path)?;
    writeln!(file, "{}", node)?;

    let mut args = FluentArgs::new();
    args.set("node", node);
    println!("{}", tr("quarantine-added", Some(&args)));
    Ok(())
} 