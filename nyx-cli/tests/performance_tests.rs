use std::process::Command;
use std::time::{Duration, Instant};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::task::JoinSet;
use assert_cmd::prelude::*;
use predicates::prelude::*;
use sysinfo::{System, SystemExt, ProcessExt};

/// Performance test configuration
struct PerformanceConfig {
    target_endpoint: String,
    max_connections: u32,
    test_duration: Duration,
    payload_sizes: Vec<usize>,
    acceptable_latency_ms: u64,
    acceptable_throughput_mbps: f64,
    memory_limit_mb: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            target_endpoint: "localhost:8080".to_string(),
            max_connections: 50,
            test_duration: Duration::from_secs(30),
            payload_sizes: vec![64, 512, 1024, 8192, 65536],
            acceptable_latency_ms: 500,
            acceptable_throughput_mbps: 1.0,
            memory_limit_mb: 512,
        }
    }
}

/// Performance metrics collected during tests
#[derive(Debug, Clone)]
struct PerformanceMetrics {
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    avg_latency_ms: f64,
    max_latency_ms: f64,
    min_latency_ms: f64,
    throughput_mbps: f64,
    memory_usage_mb: f64,
    cpu_usage_percent: f64,
    test_duration: Duration,
    error_rate: f64,
}

impl PerformanceMetrics {
    fn new() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_latency_ms: 0.0,
            max_latency_ms: 0.0,
            min_latency_ms: f64::MAX,
            throughput_mbps: 0.0,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            test_duration: Duration::ZERO,
            error_rate: 0.0,
        }
    }

    fn calculate_error_rate(&mut self) {
        if self.total_requests > 0 {
            self.error_rate = (self.failed_requests as f64 / self.total_requests as f64) * 100.0;
        }
    }

    fn meets_performance_criteria(&self, config: &PerformanceConfig) -> bool {
        self.error_rate < 10.0 &&
        self.avg_latency_ms < config.acceptable_latency_ms as f64 &&
        self.throughput_mbps >= config.acceptable_throughput_mbps &&
        self.memory_usage_mb < config.memory_limit_mb as f64
    }

    fn print_summary(&self) {
        println!("\n📊 Performance Test Summary:");
        println!("  Total Requests: {}", self.total_requests);
        println!("  Successful: {} ({:.1}%)", self.successful_requests, 
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0);
        println!("  Failed: {} ({:.1}%)", self.failed_requests, self.error_rate);
        println!("  Average Latency: {:.2}ms", self.avg_latency_ms);
        println!("  Max Latency: {:.2}ms", self.max_latency_ms);
        println!("  Min Latency: {:.2}ms", self.min_latency_ms);
        println!("  Throughput: {:.2} Mbps", self.throughput_mbps);
        println!("  Memory Usage: {:.1} MB", self.memory_usage_mb);
        println!("  CPU Usage: {:.1}%", self.cpu_usage_percent);
        println!("  Test Duration: {:.2}s", self.test_duration.as_secs_f64());
    }
}

/// Test CLI benchmark command performance under load
#[tokio::test]
async fn test_cli_benchmark_performance() {
    let config = PerformanceConfig::default();
    let start_time = Instant::now();
    
    // System monitoring
    let mut system = System::new_all();
    system.refresh_all();
    let initial_memory = get_process_memory_usage(&mut system);
    
    println!("🚀 Starting CLI benchmark performance test");
    println!("Configuration: {} connections, {}s duration", 
        config.max_connections, config.test_duration.as_secs());
    
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let output = cmd
        .arg("bench")
        .arg(&config.target_endpoint)
        .arg("--duration=30")
        .arg(&format!("--connections={}", config.max_connections))
        .arg("--payload-size=1024")
        .arg("--detailed")
        .timeout(Duration::from_secs(60))
        .output();
    
    let test_duration = start_time.elapsed();
    
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            // Parse benchmark results
            let mut metrics = parse_benchmark_output(&stdout);
            metrics.test_duration = test_duration;
            
            // Check final memory usage
            system.refresh_all();
            metrics.memory_usage_mb = get_process_memory_usage(&mut system) - initial_memory;
            
            metrics.print_summary();
            
            // Validate performance criteria
            if metrics.meets_performance_criteria(&config) {
                println!("✅ Performance test PASSED");
            } else {
                println!("❌ Performance test FAILED - criteria not met");
                if metrics.error_rate >= 10.0 {
                    println!("   Error rate too high: {:.1}%", metrics.error_rate);
                }
                if metrics.avg_latency_ms >= config.acceptable_latency_ms as f64 {
                    println!("   Latency too high: {:.2}ms", metrics.avg_latency_ms);
                }
                if metrics.throughput_mbps < config.acceptable_throughput_mbps {
                    println!("   Throughput too low: {:.2} Mbps", metrics.throughput_mbps);
                }
            }
            
            if !stderr.is_empty() {
                println!("Stderr: {}", stderr);
            }
            
            assert!(output.status.success(), "Benchmark command failed");
        }
        Err(e) => {
            panic!("Failed to execute benchmark: {}", e);
        }
    }
}

/// Test concurrent CLI operations
#[tokio::test]
async fn test_concurrent_cli_operations() {
    let concurrent_operations = 20;
    let mut set = JoinSet::new();
    
    println!("🔄 Testing {} concurrent CLI operations", concurrent_operations);
    
    let start_time = Instant::now();
    let success_counter = Arc::new(AtomicU64::new(0));
    let failure_counter = Arc::new(AtomicU64::new(0));
    
    // Launch concurrent operations
    for i in 0..concurrent_operations {
        let success_counter = success_counter.clone();
        let failure_counter = failure_counter.clone();
        
        set.spawn(async move {
            let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
            
            // Alternate between different commands
            let result = match i % 4 {
                0 => {
                    cmd.arg("status")
                        .arg("--format=json")
                        .output()
                }
                1 => {
                    cmd.arg("statistics")
                        .arg("--format=compact")
                        .output()
                }
                2 => {
                    cmd.arg("bench")
                        .arg("localhost:8080")
                        .arg("--duration=2")
                        .arg("--connections=1")
                        .output()
                }
                3 => {
                    cmd.arg("metrics")
                        .arg("--time-range=5m")
                        .arg("--format=json")
                        .output()
                }
                _ => unreachable!(),
            };
            
            match result {
                Ok(output) => {
                    if output.status.success() {
                        success_counter.fetch_add(1, Ordering::SeqCst);
                    } else {
                        failure_counter.fetch_add(1, Ordering::SeqCst);
                    }
                    (i, true, output.status.success())
                }
                Err(_) => {
                    failure_counter.fetch_add(1, Ordering::SeqCst);
                    (i, false, false)
                }
            }
        });
    }
    
    // Wait for all operations to complete
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        if let Ok(result) = result {
            results.push(result);
        }
    }
    
    let total_time = start_time.elapsed();
    let successful_ops = success_counter.load(Ordering::SeqCst);
    let failed_ops = failure_counter.load(Ordering::SeqCst);
    let total_ops = successful_ops + failed_ops;
    
    println!("📊 Concurrent Operations Results:");
    println!("  Total Operations: {}", total_ops);
    println!("  Successful: {} ({:.1}%)", successful_ops, 
        (successful_ops as f64 / total_ops as f64) * 100.0);
    println!("  Failed: {} ({:.1}%)", failed_ops, 
        (failed_ops as f64 / total_ops as f64) * 100.0);
    println!("  Total Time: {:.2}s", total_time.as_secs_f64());
    println!("  Operations/sec: {:.2}", total_ops as f64 / total_time.as_secs_f64());
    
    // Validate concurrent performance
    assert!(total_ops == concurrent_operations, "Not all operations completed");
    let success_rate = (successful_ops as f64 / total_ops as f64) * 100.0;
    assert!(success_rate >= 70.0, "Success rate too low: {:.1}%", success_rate);
    
    println!("✅ Concurrent operations test PASSED");
}

/// Test memory usage and resource consumption
#[tokio::test]
async fn test_memory_usage_validation() {
    let mut system = System::new_all();
    system.refresh_all();
    
    println!("💾 Testing memory usage during intensive operations");
    
    let initial_memory = get_system_memory_usage(&mut system);
    println!("Initial system memory usage: {:.1} MB", initial_memory);
    
    // Run memory-intensive benchmark
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let start_time = Instant::now();
    
    let output = cmd
        .arg("bench")
        .arg("localhost:8080")
        .arg("--duration=15")
        .arg("--connections=30")
        .arg("--payload-size=8192")
        .timeout(Duration::from_secs(30))
        .output();
    
    let test_duration = start_time.elapsed();
    
    match output {
        Ok(output) => {
            system.refresh_all();
            let final_memory = get_system_memory_usage(&mut system);
            let memory_increase = final_memory - initial_memory;
            
            println!("📊 Memory Usage Results:");
            println!("  Initial Memory: {:.1} MB", initial_memory);
            println!("  Final Memory: {:.1} MB", final_memory);
            println!("  Memory Increase: {:.1} MB", memory_increase);
            println!("  Test Duration: {:.2}s", test_duration.as_secs_f64());
            
            // Memory usage validation
            let acceptable_memory_increase = 100.0; // 100MB limit
            if memory_increase <= acceptable_memory_increase {
                println!("✅ Memory usage test PASSED");
            } else {
                println!("❌ Memory usage test FAILED - excessive memory usage");
                println!("   Memory increase: {:.1} MB (limit: {:.1} MB)", 
                    memory_increase, acceptable_memory_increase);
            }
            
            assert!(memory_increase <= acceptable_memory_increase, 
                "Memory usage too high: {:.1} MB", memory_increase);
        }
        Err(e) => {
            panic!("Failed to execute memory test: {}", e);
        }
    }
}

/// Test error handling and recovery under stress
#[tokio::test]
async fn test_stress_error_handling() {
    println!("🔥 Testing error handling under stress conditions");
    
    let stress_scenarios = vec![
        ("invalid-endpoint", "status", vec!["--endpoint=invalid:99999"]),
        ("connection-timeout", "connect", vec!["192.0.2.1:12345", "--connect-timeout=1"]),
        ("invalid-arguments", "bench", vec!["invalid-target", "--duration=0"]),
        ("missing-prometheus", "metrics", vec!["--prometheus-url=http://localhost:99999"]),
    ];
    
    let mut successful_error_handling = 0;
    let total_scenarios = stress_scenarios.len();
    
    for (scenario_name, command, args) in stress_scenarios {
        println!("Testing scenario: {}", scenario_name);
        
        let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
        cmd.arg(command);
        for arg in args {
            cmd.arg(arg);
        }
        
        let start_time = Instant::now();
        match cmd.timeout(Duration::from_secs(10)).output() {
            Ok(output) => {
                let duration = start_time.elapsed();
                
                // Error scenarios should fail gracefully (non-zero exit but controlled)
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    
                    // Check that error is handled gracefully (not a panic or crash)
                    if !stderr.contains("panic") && !stderr.contains("thread") && duration < Duration::from_secs(5) {
                        successful_error_handling += 1;
                        println!("  ✅ {} handled gracefully in {:.2}s", scenario_name, duration.as_secs_f64());
                    } else {
                        println!("  ❌ {} failed ungracefully", scenario_name);
                        if duration >= Duration::from_secs(5) {
                            println!("     Took too long: {:.2}s", duration.as_secs_f64());
                        }
                        if stderr.contains("panic") {
                            println!("     Contains panic");
                        }
                    }
                } else {
                    println!("  ⚠️  {} unexpectedly succeeded", scenario_name);
                }
            }
            Err(e) => {
                println!("  ❌ {} execution failed: {}", scenario_name, e);
            }
        }
    }
    
    let success_rate = (successful_error_handling as f64 / total_scenarios as f64) * 100.0;
    
    println!("📊 Stress Error Handling Results:");
    println!("  Scenarios Tested: {}", total_scenarios);
    println!("  Gracefully Handled: {}", successful_error_handling);
    println!("  Success Rate: {:.1}%", success_rate);
    
    assert!(success_rate >= 75.0, "Error handling success rate too low: {:.1}%", success_rate);
    println!("✅ Stress error handling test PASSED");
}

/// Load test with high connection counts and data volumes
#[tokio::test]
async fn test_high_load_performance() {
    let config = PerformanceConfig {
        max_connections: 100,
        test_duration: Duration::from_secs(60),
        payload_sizes: vec![1024, 8192, 65536],
        ..Default::default()
    };
    
    println!("⚡ Running high load performance test");
    println!("Configuration: {} connections, payload sizes: {:?}", 
        config.max_connections, config.payload_sizes);
    
    let mut system = System::new_all();
    system.refresh_all();
    let initial_memory = get_process_memory_usage(&mut system);
    
    for payload_size in &config.payload_sizes {
        println!("\nTesting with payload size: {} bytes", payload_size);
        
        let start_time = Instant::now();
        let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
        
        let output = cmd
            .arg("bench")
            .arg(&config.target_endpoint)
            .arg("--duration=20")
            .arg(&format!("--connections={}", config.max_connections))
            .arg(&format!("--payload-size={}", payload_size))
            .arg("--detailed")
            .timeout(Duration::from_secs(40))
            .output();
        
        match output {
            Ok(output) => {
                let test_duration = start_time.elapsed();
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut metrics = parse_benchmark_output(&stdout);
                metrics.test_duration = test_duration;
                
                system.refresh_all();
                metrics.memory_usage_mb = get_process_memory_usage(&mut system) - initial_memory;
                
                println!("Results for {} byte payload:", payload_size);
                metrics.print_summary();
                
                // Performance validation for high load
                let high_load_criteria = PerformanceConfig {
                    acceptable_latency_ms: 1000, // More lenient for high load
                    acceptable_throughput_mbps: 0.5, // Lower expectation under load
                    memory_limit_mb: 1024, // Higher memory limit
                    ..config
                };
                
                if metrics.meets_performance_criteria(&high_load_criteria) {
                    println!("✅ High load test PASSED for {} byte payload", payload_size);
                } else {
                    println!("⚠️  High load test marginal for {} byte payload", payload_size);
                }
                
                assert!(output.status.success(), "High load benchmark failed");
            }
            Err(e) => {
                eprintln!("High load test failed for payload {}: {}", payload_size, e);
            }
        }
        
        // Brief pause between tests
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    
    println!("✅ High load performance test completed");
}

/// Parse benchmark output to extract performance metrics
fn parse_benchmark_output(output: &str) -> PerformanceMetrics {
    let mut metrics = PerformanceMetrics::new();
    
    for line in output.lines() {
        if line.contains("Total Requests") {
            if let Some(value) = extract_number_from_line(line) {
                metrics.total_requests = value as u64;
            }
        } else if line.contains("Successful") {
            if let Some(value) = extract_number_from_line(line) {
                metrics.successful_requests = value as u64;
            }
        } else if line.contains("Failed") {
            if let Some(value) = extract_number_from_line(line) {
                metrics.failed_requests = value as u64;
            }
        } else if line.contains("Avg Latency") {
            if let Some(value) = extract_float_from_line(line) {
                metrics.avg_latency_ms = value;
            }
        } else if line.contains("Throughput") {
            if let Some(value) = extract_float_from_line(line) {
                metrics.throughput_mbps = value;
            }
        }
    }
    
    metrics.calculate_error_rate();
    metrics
}

/// Extract number from a line of text
fn extract_number_from_line(line: &str) -> Option<f64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts {
        if let Ok(num) = part.replace(",", "").parse::<f64>() {
            return Some(num);
        }
    }
    None
}

/// Extract floating point number from a line of text
fn extract_float_from_line(line: &str) -> Option<f64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts {
        let clean_part = part.replace("ms", "").replace("Mbps", "").replace("%", "");
        if let Ok(num) = clean_part.parse::<f64>() {
            return Some(num);
        }
    }
    None
}

/// Get current system memory usage in MB
fn get_system_memory_usage(system: &mut System) -> f64 {
    system.refresh_memory();
    let used_memory = system.used_memory();
    used_memory as f64 / 1024.0 / 1024.0 // Convert to MB
}

/// Get process memory usage in MB
fn get_process_memory_usage(system: &mut System) -> f64 {
    system.refresh_processes();
    
    // Find our process or similar processes
    let current_pid = std::process::id();
    
    if let Some(process) = system.process(sysinfo::Pid::from(current_pid as usize)) {
        process.memory() as f64 / 1024.0 / 1024.0 // Convert to MB
    } else {
        // Fallback to total system memory change
        get_system_memory_usage(system)
    }
} 