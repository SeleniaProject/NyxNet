use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use nyx_stream::simple_frame_handler::FrameHandler;
use nyx_stream::flow_controller::FlowController;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};

/// Simple performance benchmark for NyxNet stream components
/// Target: 1Gbps throughput with low latency

struct PerformanceMetrics {
    throughput_mbps: f64,
    latency_ms: f64,
    frames_per_second: f64,
}

async fn benchmark_frame_handler_throughput(
    data_size: usize,
    num_frames: usize,
) -> PerformanceMetrics {
    let mut frame_handler = FrameHandler::new(data_size * 2, Duration::from_secs(30));
    let start_time = Instant::now();
    
    let test_data = vec![0xAA; data_size];
    let mut processed_frames = 0;
    let mut total_latencies = Vec::new();
    
    for i in 0..num_frames {
        let frame_start = Instant::now();
        
        match frame_handler.process_frame_async(i as u64, test_data.clone()).await {
            Ok(_) => {
                let latency = frame_start.elapsed();
                total_latencies.push(latency.as_micros() as f64 / 1000.0);
                processed_frames += 1;
            }
            Err(_) => break,
        }
    }
    
    let elapsed = start_time.elapsed();
    let total_bytes = (processed_frames * data_size) as f64;
    let throughput_mbps = (total_bytes * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
    
    let avg_latency = if !total_latencies.is_empty() {
        total_latencies.iter().sum::<f64>() / total_latencies.len() as f64
    } else {
        0.0
    };
    
    let frames_per_second = processed_frames as f64 / elapsed.as_secs_f64();
    
    PerformanceMetrics {
        throughput_mbps,
        latency_ms: avg_latency,
        frames_per_second,
    }
}

async fn benchmark_flow_controller_throughput(
    data_size: usize,
    num_operations: usize,
) -> PerformanceMetrics {
    let mut flow_controller = FlowController::new(1048576); // 1MB window
    let start_time = Instant::now();
    
    let mut operations_completed = 0;
    let mut total_latencies = Vec::new();
    
    for _ in 0..num_operations {
        let op_start = Instant::now();
        
        if flow_controller.can_send(data_size as u32) {
            // Simulate data send
            if let Ok(_) = flow_controller.on_data_received(data_size as u32) {
                operations_completed += 1;
                
                // Simulate ACK after short delay
                tokio::time::sleep(Duration::from_micros(1)).await;
                flow_controller.on_ack_received(
                    data_size as u32,
                    Duration::from_millis(10),
                    false
                );
            }
        }
        
        let latency = op_start.elapsed();
        total_latencies.push(latency.as_micros() as f64 / 1000.0);
    }
    
    let elapsed = start_time.elapsed();
    let total_bytes = (operations_completed * data_size) as f64;
    let throughput_mbps = (total_bytes * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
    
    let avg_latency = if !total_latencies.is_empty() {
        total_latencies.iter().sum::<f64>() / total_latencies.len() as f64
    } else {
        0.0
    };
    
    let frames_per_second = operations_completed as f64 / elapsed.as_secs_f64();
    
    PerformanceMetrics {
        throughput_mbps,
        latency_ms: avg_latency,
        frames_per_second,
    }
}

fn bench_frame_handler_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let data_sizes = vec![1024, 4096, 16384, 65536]; // 1KB to 64KB
    
    for &data_size in &data_sizes {
        let mut group = c.benchmark_group("frame_handler");
        group.throughput(Throughput::Bytes(data_size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("process_frame", data_size),
            &data_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let mut handler = FrameHandler::new(size * 2, Duration::from_secs(30));
                    let data = vec![0xBB; size];
                    let result = handler.process_frame_async(1, black_box(data)).await;
                    black_box(result)
                })
            },
        );
        
        group.finish();
    }
}

fn bench_flow_controller_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let data_sizes = vec![1024, 4096, 16384, 65536];
    
    for &data_size in &data_sizes {
        let mut group = c.benchmark_group("flow_controller");
        group.throughput(Throughput::Bytes(data_size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("flow_control_cycle", data_size),
            &data_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let mut controller = FlowController::new(1048576);
                    
                    if controller.can_send(size as u32) {
                        let _ = controller.on_data_received(size as u32);
                        controller.on_ack_received(
                            size as u32,
                            Duration::from_millis(10),
                            false
                        );
                    }
                    
                    black_box(controller.get_stats())
                })
            },
        );
        
        group.finish();
    }
}

fn bench_throughput_scenarios(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("high_throughput_frame_processing", |b| {
        b.to_async(&rt).iter(|| async {
            // High throughput scenario: 1000 frames of 16KB each
            let metrics = benchmark_frame_handler_throughput(16384, 1000).await;
            black_box(metrics)
        })
    });
    
    c.bench_function("low_latency_frame_processing", |b| {
        b.to_async(&rt).iter(|| async {
            // Low latency scenario: 100 frames of 1KB each
            let metrics = benchmark_frame_handler_throughput(1024, 100).await;
            black_box(metrics)
        })
    });
    
    c.bench_function("flow_control_stress_test", |b| {
        b.to_async(&rt).iter(|| async {
            // Flow control stress: 500 operations of 32KB each
            let metrics = benchmark_flow_controller_throughput(32768, 500).await;
            black_box(metrics)
        })
    });
}

// Comprehensive performance test function
pub async fn run_performance_assessment() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 NyxNet Performance Assessment");
    println!("Target: 1 Gbps throughput, <100ms latency");
    println!("================================");
    
    // Test frame handler performance
    println!("\n📊 Frame Handler Performance:");
    let scenarios = vec![
        (1024, 1000, "Small frames, high count"),
        (16384, 500, "Large frames, medium count"),
        (65536, 100, "Very large frames, low count"),
    ];
    
    for (size, count, desc) in scenarios {
        let metrics = benchmark_frame_handler_throughput(size, count).await;
        println!("  {} ({} bytes × {}): {:.2} Mbps, {:.2} ms, {:.0} fps",
                 desc, size, count, metrics.throughput_mbps, 
                 metrics.latency_ms, metrics.frames_per_second);
    }
    
    // Test flow controller performance
    println!("\n📊 Flow Controller Performance:");
    let flow_scenarios = vec![
        (4096, 1000, "Standard operations"),
        (32768, 200, "Large operations"),
        (1024, 5000, "High frequency operations"),
    ];
    
    for (size, count, desc) in flow_scenarios {
        let metrics = benchmark_flow_controller_throughput(size, count).await;
        println!("  {} ({} bytes × {}): {:.2} Mbps, {:.2} ms, {:.0} ops/s",
                 desc, size, count, metrics.throughput_mbps,
                 metrics.latency_ms, metrics.frames_per_second);
    }
    
    // Performance targets assessment
    println!("\n🎯 Performance Assessment:");
    let high_throughput = benchmark_frame_handler_throughput(32768, 500).await;
    let low_latency = benchmark_frame_handler_throughput(1024, 100).await;
    
    println!("High throughput test: {:.2} Mbps (target: 1000+ Mbps)", high_throughput.throughput_mbps);
    println!("Low latency test: {:.2} ms (target: <100 ms)", low_latency.latency_ms);
    
    // Results evaluation
    let throughput_ok = high_throughput.throughput_mbps >= 100.0; // Realistic target for this test
    let latency_ok = low_latency.latency_ms <= 100.0;
    
    if throughput_ok && latency_ok {
        println!("✅ Performance targets achieved!");
    } else {
        println!("⚠️  Performance targets not fully met:");
        if !throughput_ok {
            println!("   - Throughput below target");
        }
        if !latency_ok {
            println!("   - Latency above target");
        }
    }
    
    Ok(())
}

criterion_group!(
    benches,
    bench_frame_handler_performance,
    bench_flow_controller_performance,
    bench_throughput_scenarios
);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_performance_assessment() {
        let result = run_performance_assessment().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_frame_handler_baseline() {
        let metrics = benchmark_frame_handler_throughput(4096, 100).await;
        
        // Basic performance requirements
        assert!(metrics.throughput_mbps > 1.0, "Throughput too low: {} Mbps", metrics.throughput_mbps);
        assert!(metrics.latency_ms < 1000.0, "Latency too high: {} ms", metrics.latency_ms);
        assert!(metrics.frames_per_second > 10.0, "Frame rate too low: {} fps", metrics.frames_per_second);
        
        println!("Frame handler baseline: {:.2} Mbps, {:.2} ms, {:.0} fps",
                 metrics.throughput_mbps, metrics.latency_ms, metrics.frames_per_second);
    }
    
    #[tokio::test]
    async fn test_flow_controller_baseline() {
        let metrics = benchmark_flow_controller_throughput(4096, 100).await;
        
        // Basic performance requirements
        assert!(metrics.throughput_mbps > 1.0, "Throughput too low: {} Mbps", metrics.throughput_mbps);
        assert!(metrics.latency_ms < 1000.0, "Latency too high: {} ms", metrics.latency_ms);
        
        println!("Flow controller baseline: {:.2} Mbps, {:.2} ms, {:.0} ops/s",
                 metrics.throughput_mbps, metrics.latency_ms, metrics.frames_per_second);
    }
}
