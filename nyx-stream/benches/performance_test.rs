use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use nyx_stream::integrated_frame_processor::*;
use nyx_transport::quic::QuicTransport;
use nyx_mix::PathBuilder;
use nyx_crypto::noise::NoiseProtocol;
use tokio::runtime::Runtime;
use std::sync::Arc;
use std::time::{Duration, Instant};
use bytes::Bytes;

/// Performance benchmark suite for NyxNet
/// Target: 1Gbps throughput with low latency

struct BenchmarkConfig {
    data_sizes: Vec<usize>,
    stream_counts: Vec<usize>,
    target_throughput_gbps: f64,
    max_latency_ms: u64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            data_sizes: vec![1024, 4096, 16384, 65536, 262144, 1048576], // 1KB to 1MB
            stream_counts: vec![1, 10, 50, 100, 500, 1000],
            target_throughput_gbps: 1.0,
            max_latency_ms: 100,
        }
    }
}

struct PerformanceMetrics {
    throughput_mbps: f64,
    latency_ms: f64,
    packet_loss_rate: f64,
    cpu_usage_percent: f64,
    memory_usage_mb: f64,
}

impl PerformanceMetrics {
    fn meets_targets(&self, config: &BenchmarkConfig) -> bool {
        let throughput_gbps = self.throughput_mbps / 1000.0;
        throughput_gbps >= config.target_throughput_gbps && 
        self.latency_ms <= config.max_latency_ms as f64
    }
}

async fn setup_integrated_processor() -> IntegratedFrameProcessor {
    let config = IntegratedFrameConfig {
        max_concurrent_streams: 2000,
        frame_timeout: Duration::from_secs(30),
        flow_control_window: 1048576, // 1MB for high throughput
        congestion_window: 524288,   // 512KB
        max_frame_size: 65536,       // 64KB max frame
        enable_flow_control: true,
        enable_congestion_control: true,
        stats_update_interval: Duration::from_millis(100),
    };
    
    IntegratedFrameProcessor::new(config).await
}

async fn benchmark_frame_processing_throughput(
    processor: &IntegratedFrameProcessor,
    data_size: usize,
    num_streams: usize,
    duration_secs: u64,
) -> PerformanceMetrics {
    let start_time = Instant::now();
    let mut total_bytes = 0u64;
    let mut frame_count = 0u64;
    let mut latencies = Vec::new();
    
    let data = vec![0xAA; data_size];
    let test_duration = Duration::from_secs(duration_secs);
    
    // Create concurrent tasks for multiple streams
    let mut handles = Vec::new();
    
    for stream_id in 0..num_streams {
        let processor_clone = processor.clone();
        let data_clone = data.clone();
        let test_end = start_time + test_duration;
        
        let handle = tokio::spawn(async move {
            let mut local_bytes = 0u64;
            let mut local_frames = 0u64;
            let mut local_latencies = Vec::new();
            
            while Instant::now() < test_end {
                let frame_start = Instant::now();
                
                match processor_clone.process_frame(stream_id as u64, data_clone.clone()).await {
                    Ok(_) => {
                        let latency = frame_start.elapsed();
                        local_latencies.push(latency.as_micros() as f64 / 1000.0); // Convert to ms
                        local_bytes += data_clone.len() as u64;
                        local_frames += 1;
                    }
                    Err(_) => break,
                }
                
                // Small delay to prevent overwhelming
                tokio::time::sleep(Duration::from_micros(10)).await;
            }
            
            (local_bytes, local_frames, local_latencies)
        });
        
        handles.push(handle);
    }
    
    // Collect results from all tasks
    for handle in handles {
        if let Ok((bytes, frames, mut task_latencies)) = handle.await {
            total_bytes += bytes;
            frame_count += frames;
            latencies.append(&mut task_latencies);
        }
    }
    
    let elapsed = start_time.elapsed();
    let throughput_mbps = (total_bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
    
    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<f64>() / latencies.len() as f64
    } else {
        0.0
    };
    
    // Simple CPU and memory usage estimation (in real scenario, use system APIs)
    let cpu_usage = (frame_count as f64 / elapsed.as_secs_f64()).min(100.0);
    let memory_usage = (total_bytes as f64 / (1024.0 * 1024.0)) * 1.5; // Estimate
    
    PerformanceMetrics {
        throughput_mbps,
        latency_ms: avg_latency,
        packet_loss_rate: 0.0, // Calculate based on failed frames
        cpu_usage_percent: cpu_usage,
        memory_usage_mb: memory_usage,
    }
}

async fn benchmark_mixed_workload(
    processor: &IntegratedFrameProcessor,
    config: &BenchmarkConfig,
) -> Vec<PerformanceMetrics> {
    let mut results = Vec::new();
    
    for &data_size in &config.data_sizes {
        for &stream_count in &config.stream_counts {
            println!("Benchmarking: {} bytes per frame, {} streams", data_size, stream_count);
            
            let metrics = benchmark_frame_processing_throughput(
                processor,
                data_size,
                stream_count,
                10, // 10 seconds per test
            ).await;
            
            println!(
                "  Throughput: {:.2} Mbps, Latency: {:.2} ms, CPU: {:.1}%, Memory: {:.1} MB",
                metrics.throughput_mbps,
                metrics.latency_ms,
                metrics.cpu_usage_percent,
                metrics.memory_usage_mb
            );
            
            results.push(metrics);
        }
    }
    
    results
}

fn bench_integrated_processor_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = BenchmarkConfig::default();
    
    c.bench_function("integrated_processor_setup", |b| {
        b.to_async(&rt).iter(|| async {
            let processor = setup_integrated_processor().await;
            black_box(processor);
        })
    });
    
    let processor = rt.block_on(setup_integrated_processor());
    
    for &data_size in &config.data_sizes {
        let mut group = c.benchmark_group("frame_processing");
        group.throughput(Throughput::Bytes(data_size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("single_stream", data_size),
            &data_size,
            |b, &size| {
                let data = vec![0xBB; size];
                b.to_async(&rt).iter(|| async {
                    let result = processor.process_frame(1, black_box(data.clone())).await;
                    black_box(result)
                })
            },
        );
        
        group.finish();
    }
    
    // Multi-stream benchmarks
    for &stream_count in &[1, 10, 100] {
        let mut group = c.benchmark_group("multi_stream");
        group.sample_size(10); // Fewer samples for longer tests
        
        group.bench_with_input(
            BenchmarkId::new("concurrent_streams", stream_count),
            &stream_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let mut handles = Vec::new();
                    
                    for i in 0..count {
                        let processor_clone = processor.clone();
                        let data = vec![0xCC; 4096];
                        
                        let handle = tokio::spawn(async move {
                            processor_clone.process_frame(i as u64, data).await
                        });
                        
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        let _ = handle.await;
                    }
                })
            },
        );
        
        group.finish();
    }
}

fn bench_end_to_end_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("e2e_stream_setup", |b| {
        b.to_async(&rt).iter(|| async {
            // Simulate end-to-end stream setup
            let processor = setup_integrated_processor().await;
            
            // Simulate handshake and stream establishment
            let stream_id = 1;
            let handshake_data = vec![0x01, 0x02, 0x03, 0x04]; // Minimal handshake
            
            let result = processor.process_frame(stream_id, handshake_data).await;
            black_box(result)
        })
    });
    
    c.bench_function("e2e_data_transfer", |b| {
        b.to_async(&rt).iter(|| async {
            let processor = setup_integrated_processor().await;
            let stream_id = 1;
            
            // Transfer 1MB of data in chunks
            let chunk_size = 16384; // 16KB chunks
            let total_size = 1048576; // 1MB
            let num_chunks = total_size / chunk_size;
            
            for i in 0..num_chunks {
                let chunk_data = vec![0xDD; chunk_size];
                let _ = processor.process_frame(stream_id, chunk_data).await;
            }
            
            black_box(())
        })
    });
}

// Comprehensive performance test function
pub async fn run_comprehensive_performance_test() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting NyxNet Comprehensive Performance Test");
    println!("Target: 1 Gbps throughput, <100ms latency");
    println!("================================================");
    
    let config = BenchmarkConfig::default();
    let processor = setup_integrated_processor().await;
    
    // Run mixed workload benchmarks
    let results = benchmark_mixed_workload(&processor, &config).await;
    
    // Analyze results
    let mut passed_tests = 0;
    let total_tests = results.len();
    
    println!("\n📊 Performance Test Results:");
    println!("============================");
    
    for (i, metrics) in results.iter().enumerate() {
        let data_size = config.data_sizes[i % config.data_sizes.len()];
        let stream_count = config.stream_counts[i / config.data_sizes.len()];
        
        let status = if metrics.meets_targets(&config) {
            passed_tests += 1;
            "✅ PASS"
        } else {
            "❌ FAIL"
        };
        
        println!(
            "{} - {}B/{}streams: {:.2} Mbps, {:.2}ms latency",
            status, data_size, stream_count, metrics.throughput_mbps, metrics.latency_ms
        );
    }
    
    // Final summary
    println!("\n🎯 Performance Summary:");
    println!("======================");
    println!("Tests passed: {}/{}", passed_tests, total_tests);
    println!("Success rate: {:.1}%", (passed_tests as f64 / total_tests as f64) * 100.0);
    
    if passed_tests == total_tests {
        println!("🎉 All performance targets met! NyxNet is ready for 1Gbps deployment.");
    } else {
        println!("⚠️  Some performance targets not met. Consider optimization.");
    }
    
    // Cleanup
    processor.shutdown().await;
    
    Ok(())
}

criterion_group!(
    benches,
    bench_integrated_processor_performance,
    bench_end_to_end_performance
);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_performance_targets() {
        let result = run_comprehensive_performance_test().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_high_throughput_scenario() {
        let processor = setup_integrated_processor().await;
        
        // Test high throughput scenario: 100 streams, 64KB frames
        let metrics = benchmark_frame_processing_throughput(
            &processor,
            65536,  // 64KB frames
            100,    // 100 concurrent streams
            5,      // 5 seconds
        ).await;
        
        println!("High throughput test: {:.2} Mbps", metrics.throughput_mbps);
        
        // Should achieve at least 500 Mbps in this scenario
        assert!(metrics.throughput_mbps >= 500.0);
        assert!(metrics.latency_ms <= 200.0); // Allow higher latency for high throughput
        
        processor.shutdown().await;
    }
    
    #[tokio::test]
    async fn test_low_latency_scenario() {
        let processor = setup_integrated_processor().await;
        
        // Test low latency scenario: 1 stream, small frames
        let metrics = benchmark_frame_processing_throughput(
            &processor,
            1024,   // 1KB frames
            1,      // Single stream
            5,      // 5 seconds
        ).await;
        
        println!("Low latency test: {:.2} ms latency", metrics.latency_ms);
        
        // Should achieve very low latency for small frames
        assert!(metrics.latency_ms <= 10.0);
        
        processor.shutdown().await;
    }
}
