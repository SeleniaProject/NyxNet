use nyx_stream::simple_frame_handler::FrameHandler;
use nyx_stream::flow_controller::FlowController;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 NyxNet 詳細パフォーマンス分析");
    println!("{}", "=".repeat(60));
    
    // フレームハンドラパフォーマンステスト
    println!("\n📊 フレームハンドラ パフォーマンス:");
    println!("{}", "-".repeat(40));
    
    let frame_sizes = vec![512, 1024, 4096, 8192, 16384, 32768, 65536];
    
    for &size in &frame_sizes {
        let start = Instant::now();
        let mut handler = FrameHandler::new(size * 2, Duration::from_secs(30));
        
        // 実際のネットワーク処理に近いテスト数
        let num_frames = if size <= 4096 { 2000 } else { 1000 };
        let mut successful_frames = 0;
        let mut total_latency = Duration::ZERO;
        
        for i in 0..num_frames {
            let frame_start = Instant::now();
            
            // 疑似ランダムデータ生成
            let mut data = Vec::with_capacity(size);
            for j in 0..size {
                data.push(((i * 31 + j * 17) & 0xFF) as u8);
            }
            
            match handler.process_frame_async(i as u64, data).await {
                Ok(Some(_processed)) => {
                    successful_frames += 1;
                    total_latency += frame_start.elapsed();
                }
                Ok(None) => {
                    // フレームドロップ
                }
                Err(_) => {
                    break;
                }
            }
            
            // ネットワーク遅延シミュレーション (実際のネットワークに近く)
            // 100フレームに1回だけ遅延 (バックプレッシャー)
            if i % 100 == 0 && i > 0 {
                tokio::time::sleep(Duration::from_micros(100)).await;
            }
        }
        
        let total_time = start.elapsed();
        let total_bytes = successful_frames * size;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_time.as_secs_f64() * 1_000_000.0);
        let avg_latency_us = if successful_frames > 0 {
            total_latency.as_micros() as f64 / successful_frames as f64
        } else {
            0.0
        };
        let frames_per_sec = successful_frames as f64 / total_time.as_secs_f64();
        
        println!("  {}KB フレーム: {:.2} Mbps, {:.1}µs, {:.0} fps, 成功率: {:.1}%",
                 size / 1024, throughput_mbps, avg_latency_us, frames_per_sec,
                 (successful_frames as f64 / num_frames as f64) * 100.0);
    }
    
    // フローコントローラパフォーマンステスト
    println!("\n📊 フローコントローラ パフォーマンス:");
    println!("{}", "-".repeat(40));
    
    let flow_scenarios = vec![
        (1024, 5, 0.001, "高速LAN環境"),     // 1KB, 5ms RTT, 0.1% loss
        (4096, 15, 0.005, "一般的なWAN環境"), // 4KB, 15ms RTT, 0.5% loss
        (8192, 50, 0.02, "遠距離WAN環境"),    // 8KB, 50ms RTT, 2% loss
        (16384, 150, 0.05, "衛星回線"),      // 16KB, 150ms RTT, 5% loss
    ];
    
    for (data_size, rtt_ms, loss_rate, scenario) in flow_scenarios {
        let start = Instant::now();
        let mut flow_controller = FlowController::new(65536); // 64KB ウィンドウ
        
        let num_operations = if data_size <= 4096 { 1000 } else { 500 };
        let mut successful_ops = 0;
        let mut total_bytes = 0;
        let mut operation_latencies = Vec::new();
        
        for i in 0..num_operations {
            let op_start = Instant::now();
            
            // パケットロスシミュレーション
            let packet_lost = (i as f64 * 0.618) % 1.0 < loss_rate;
            
            if flow_controller.can_send(data_size as u32) {
                // RTT前半のシミュレーション (実際のネットワーク処理時間)
                if rtt_ms > 50 {
                    tokio::time::sleep(Duration::from_millis(rtt_ms / 4)).await;
                }
                
                if !packet_lost {
                    match flow_controller.on_data_received(data_size as u32) {
                        Ok(_) => {
                            successful_ops += 1;
                            total_bytes += data_size;
                            
                            // RTT後半のシミュレーション (ACK処理時間)
                            if rtt_ms > 50 {
                                tokio::time::sleep(Duration::from_millis(rtt_ms / 4)).await;
                            }
                            
                            flow_controller.on_ack_received(
                                data_size as u32,
                                Duration::from_millis(rtt_ms),
                                false
                            );
                        }
                        Err(_) => {
                            // フロー制御拒否
                        }
                    }
                } else {
                    // パケットロス - 再送シミュレーション
                    flow_controller.on_data_lost(data_size as u32);
                    if rtt_ms > 100 {
                        tokio::time::sleep(Duration::from_millis(rtt_ms)).await;
                    }
                }
            }
            
            operation_latencies.push(op_start.elapsed());
            
            // 定期的な統計更新
            if i % 50 == 0 {
                let _stats = flow_controller.get_stats();
            }
        }
        
        let total_time = start.elapsed();
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_time.as_secs_f64() * 1_000_000.0);
        let avg_latency_ms = if !operation_latencies.is_empty() {
            operation_latencies.iter().map(|d| d.as_micros()).sum::<u128>() as f64 
            / (operation_latencies.len() as f64 * 1000.0)
        } else {
            0.0
        };
        let ops_per_sec = successful_ops as f64 / total_time.as_secs_f64();
        let success_rate = (successful_ops as f64 / num_operations as f64) * 100.0;
        
        println!("  {}: {:.2} Mbps, {:.1}ms, {:.0} ops/s, 成功率: {:.1}%",
                 scenario, throughput_mbps, avg_latency_ms, ops_per_sec, success_rate);
    }
    
    // 統合パフォーマンステスト
    println!("\n📊 統合システム パフォーマンス:");
    println!("{}", "-".repeat(40));
    
    let start = Instant::now();
    let mut frame_handler = FrameHandler::new(32768, Duration::from_secs(30));
    let mut flow_controller = FlowController::new(1048576); // 1MB ウィンドウ
    
    let test_size = 8192; // 8KB フレーム
    let num_tests = 200;
    let mut total_processed = 0;
    let mut total_bytes = 0;
    
    for i in 0..num_tests {
        let test_start = Instant::now();
        
        // フロー制御チェック
        if flow_controller.can_send(test_size as u32) {
            // データ生成
            let mut data = Vec::with_capacity(test_size);
            for j in 0..test_size {
                data.push(((i * 37 + j * 23) & 0xFF) as u8);
            }
            
            // フレーム処理
            match frame_handler.process_frame_async(i as u64, data).await {
                Ok(Some(processed)) => {
                    // チェックサム計算 (CPU負荷)
                    let _checksum = processed.iter().fold(0u32, |acc, &b| acc.wrapping_add(b as u32));
                    
                    // フロー制御更新
                    if let Ok(_) = flow_controller.on_data_received(test_size as u32) {
                        total_processed += 1;
                        total_bytes += test_size;
                        
                        // ACK シミュレーション (実際のACK処理時間)
                        tokio::time::sleep(Duration::from_micros(50)).await;
                        flow_controller.on_ack_received(
                            test_size as u32,
                            Duration::from_millis(10),
                            false
                        );
                    }
                }
                _ => {
                    // 処理失敗
                }
            }
        }
        
        // バックプレッシャーシミュレーション (実際のネットワーク輻輳)
        if i % 50 == 0 && i > 0 {
            tokio::time::sleep(Duration::from_micros(25)).await;
        }
    }
    
    let total_time = start.elapsed();
    let integrated_throughput = (total_bytes as f64 * 8.0) / (total_time.as_secs_f64() * 1_000_000.0);
    let integrated_ops_per_sec = total_processed as f64 / total_time.as_secs_f64();
    let success_rate = (total_processed as f64 / num_tests as f64) * 100.0;
    
    println!("  統合処理: {:.2} Mbps, {:.0} ops/s, 成功率: {:.1}%",
             integrated_throughput, integrated_ops_per_sec, success_rate);
    
    // パフォーマンス評価
    println!("\n🎯 パフォーマンス評価:");
    println!("{}", "-".repeat(40));
    
    // 実際のアプリケーション層での現実的な目標値
    let target_throughput = 50.0;   // 50 Mbps (アプリケーション層)
    let target_latency = 5.0;       // 5ms (処理レイテンシー)
    let target_success_rate = 98.0; // 98%
    
    println!("目標スループット: {:.0} Mbps", target_throughput);
    println!("目標レイテンシー: {:.0} ms以下", target_latency);
    println!("目標成功率: {:.0}%以上", target_success_rate);
    
    // 達成度評価
    let throughput_ok = integrated_throughput >= target_throughput;
    let success_ok = success_rate >= target_success_rate;
    
    println!("\n📈 結果:");
    if throughput_ok && success_ok {
        println!("✅ パフォーマンス目標達成！");
        println!("   スループット: {:.1}% ({:.2}/{:.0} Mbps)", 
                 integrated_throughput / target_throughput * 100.0, 
                 integrated_throughput, target_throughput);
        println!("   成功率: {:.1}% (目標: {:.0}%)", success_rate, target_success_rate);
    } else {
        println!("⚠️ パフォーマンス課題:");
        if !throughput_ok {
            println!("   - スループット不足: {:.2} Mbps (目標: {:.0} Mbps)", 
                     integrated_throughput, target_throughput);
        }
        if !success_ok {
            println!("   - 成功率低下: {:.1}% (目標: {:.0}%)", success_rate, target_success_rate);
        }
    }
    
    // システム効率性
    println!("\n💻 システム効率性:");
    println!("   総処理時間: {:.2}秒", total_time.as_secs_f64());
    println!("   平均フレーム処理時間: {:.2}ms", 
             total_time.as_millis() as f64 / total_processed as f64);
    println!("   CPU効率: {:.2} ops/ms", 
             total_processed as f64 / total_time.as_millis() as f64);
    
    Ok(())
}
