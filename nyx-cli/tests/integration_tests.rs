use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tempfile::TempDir;
use serde_json::Value;
use assert_cmd::prelude::*;
use predicates::prelude::*;

/// Test daemon connection and basic status functionality
#[tokio::test]
async fn test_status_command() {
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    
    // Test status command with table format
    cmd.arg("status")
        .arg("--format=table")
        .assert()
        .success()
        .stdout(predicate::str::contains("Version"));
    
    // Test status command with JSON format
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let output = cmd.arg("status")
        .arg("--format=json")
        .output()
        .expect("Failed to execute command");
    
    assert!(output.status.success());
    
    // Validate JSON output structure
    let json: Value = serde_json::from_slice(&output.stdout).expect("Invalid JSON output");
    assert!(json.get("version").is_some());
    assert!(json.get("uptime").is_some());
}

/// Test benchmark command with various configurations
#[tokio::test]
async fn test_benchmark_command() {
    let temp_dir = TempDir::new().unwrap();
    let results_file = temp_dir.path().join("benchmark_results.json");
    
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    
    // Test basic benchmark
    cmd.arg("bench")
        .arg("localhost:8080")
        .arg("--duration=10")
        .arg("--connections=5")
        .arg("--payload-size=512")
        .arg("--detailed")
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark Results"))
        .stdout(predicate::str::contains("Total Requests"))
        .stdout(predicate::str::contains("Latency Distribution"))
        .stdout(predicate::str::contains("Protocol Layer Performance"));
}

/// Test connection command with various options
#[tokio::test]
async fn test_connection_command() {
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    
    // Test connection establishment
    cmd.arg("connect")
        .arg("localhost:8080")
        .arg("--connect-timeout=10")
        .arg("--stream-name=test-stream")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nyx stream connection"))
        .stdout(predicate::str::contains("Stream ID"));
}

/// Test statistics command with various display options
#[tokio::test]
async fn test_statistics_command() {
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    
    // Test statistics with table format
    cmd.arg("statistics")
        .arg("--format=table")
        .arg("--layers")
        .arg("--percentiles")
        .arg("--analyze")
        .assert()
        .success()
        .stdout(predicate::str::contains("Network Statistics"))
        .stdout(predicate::str::contains("Latency"))
        .stdout(predicate::str::contains("Throughput"));
    
    // Test statistics with JSON format
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let output = cmd.arg("statistics")
        .arg("--format=json")
        .output()
        .expect("Failed to execute command");
    
    assert!(output.status.success());
    
    // Validate JSON structure
    let json: Value = serde_json::from_slice(&output.stdout).expect("Invalid JSON output");
    assert!(json.get("timestamp").is_some());
    assert!(json.get("summary").is_some());
}

/// Test metrics command with Prometheus integration
#[tokio::test]
async fn test_metrics_command() {
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    
    // Test metrics analysis
    cmd.arg("metrics")
        .arg("--prometheus-url=http://localhost:9090")
        .arg("--time-range=1h")
        .arg("--format=table")
        .arg("--detailed")
        .assert()
        .success()
        .stdout(predicate::str::contains("Metrics Analysis"))
        .stdout(predicate::str::contains("Latency Metrics"));
    
    // Test metrics with JSON output
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let output = cmd.arg("metrics")
        .arg("--prometheus-url=http://localhost:9090")
        .arg("--time-range=30m")
        .arg("--format=json")
        .output()
        .expect("Failed to execute command");
    
    // Should succeed or gracefully handle Prometheus unavailability
    if output.status.success() {
        let json: Value = serde_json::from_slice(&output.stdout).expect("Invalid JSON output");
        assert!(json.get("timestamp").is_some());
    }
}

/// Test internationalization support
#[tokio::test]
async fn test_i18n_support() {
    // Test English (default)
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("status")
        .arg("--language=en")
        .assert()
        .success();
    
    // Test Japanese
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("status")
        .arg("--language=ja")
        .assert()
        .success();
    
    // Test Chinese
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("status")
        .arg("--language=zh")
        .assert()
        .success();
}

/// Test error handling and edge cases
#[tokio::test]
async fn test_error_handling() {
    // Test invalid endpoint
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("status")
        .arg("--endpoint=invalid-endpoint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error").or(predicate::str::contains("failed")));
    
    // Test invalid command arguments
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("bench")
        .arg("invalid-target")
        .arg("--duration=0")
        .assert()
        .failure();
    
    // Test connection timeout
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("connect")
        .arg("192.0.2.1:12345") // Reserved test IP that should not respond
        .arg("--connect-timeout=1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("timeout").or(predicate::str::contains("failed")));
}

/// Test benchmark accuracy with known baselines
#[tokio::test]
async fn test_benchmark_accuracy() {
    // Test with localhost loopback for predictable performance
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let output = cmd.arg("bench")
        .arg("127.0.0.1:8080")
        .arg("--duration=5")
        .arg("--connections=1")
        .arg("--payload-size=100")
        .arg("--detailed")
        .output()
        .expect("Failed to execute benchmark");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Verify benchmark output contains expected metrics
    assert!(stdout.contains("Total Requests"));
    assert!(stdout.contains("Avg Latency"));
    assert!(stdout.contains("Throughput"));
    assert!(stdout.contains("Error Rate"));
    assert!(stdout.contains("50th"));
    assert!(stdout.contains("95th"));
    assert!(stdout.contains("99th"));
    
    // Performance baseline validation (should be reasonable for localhost)
    if output.status.success() {
        // Parse performance metrics from output
        if let Some(error_rate_line) = stdout.lines().find(|line| line.contains("Error Rate")) {
            if let Some(rate_str) = error_rate_line.split_whitespace().last() {
                if let Ok(error_rate) = rate_str.trim_end_matches('%').parse::<f64>() {
                    assert!(error_rate < 50.0, "Error rate too high: {}%", error_rate);
                }
            }
        }
    }
}

/// Test statistics display with various network conditions
#[tokio::test]
async fn test_statistics_display_scenarios() {
    // Test with mock data scenario
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("statistics")
        .arg("--format=compact")
        .assert()
        .success()
        .stdout(predicate::str::contains("Statistics"));
    
    // Test real-time mode for a short duration
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let child = cmd.arg("statistics")
        .arg("--realtime")
        .arg("--interval=1")
        .spawn()
        .expect("Failed to start real-time statistics");
    
    // Let it run for a few seconds then terminate
    sleep(Duration::from_secs(3)).await;
    // The process should handle Ctrl+C gracefully
    
    // Test distribution histogram
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("statistics")
        .arg("--distribution")
        .arg("--percentiles")
        .assert()
        .success()
        .stdout(predicate::str::contains("Distribution"));
}

/// Test connection functionality with various network scenarios
#[tokio::test]
async fn test_connection_network_conditions() {
    // Test normal connection
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("connect")
        .arg("localhost:8080")
        .arg("--connect-timeout=5")
        .assert()
        .success();
    
    // Test connection with custom stream name
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("connect")
        .arg("localhost:8080")
        .arg("--stream-name=custom-test-stream")
        .arg("--connect-timeout=5")
        .assert()
        .success();
}

/// Integration test for complete CLI workflow
#[tokio::test]
async fn test_complete_cli_workflow() {
    // 1. Check daemon status
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("status").assert().success();
    
    // 2. Run short benchmark
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("bench")
        .arg("localhost:8080")
        .arg("--duration=3")
        .arg("--connections=2")
        .assert()
        .success();
    
    // 3. Get statistics
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    cmd.arg("statistics")
        .arg("--format=summary")
        .assert()
        .success();
    
    // 4. Check metrics (should handle gracefully if Prometheus unavailable)
    let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
    let _output = cmd.arg("metrics")
        .arg("--time-range=5m")
        .output()
        .expect("Failed to execute metrics command");
    // Don't assert success as Prometheus may not be available
}

/// Test concurrent CLI operations
#[tokio::test]
async fn test_concurrent_operations() {
    use tokio::task::JoinSet;
    
    let mut set = JoinSet::new();
    
    // Spawn multiple concurrent operations
    for i in 0..3 {
        set.spawn(async move {
            let mut cmd = Command::cargo_bin("nyx-cli").unwrap();
            cmd.arg("status")
                .arg("--format=json")
                .output()
                .expect(&format!("Failed to execute concurrent command {}", i))
        });
    }
    
    // Wait for all to complete
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        let output = result.expect("Task failed");
        results.push(output);
    }
    
    // Verify all succeeded
    for (i, output) in results.iter().enumerate() {
        assert!(output.status.success(), "Concurrent operation {} failed", i);
    }
} 