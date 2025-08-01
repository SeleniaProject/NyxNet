use super::super::integrated_frame_processor::*;
use super::super::frame_handler::{FrameHandler, StreamFrame};
use tokio::time::{sleep, Duration};
use std::sync::Arc;

#[tokio::test]
async fn test_integrated_processor_basic_functionality() {
    let config = IntegratedFrameConfig {
        max_concurrent_streams: 100,
        frame_timeout: Duration::from_secs(30),
        flow_control_window: 65536,
        congestion_window: 10240,
        max_frame_size: 16384,
        enable_flow_control: true,
        enable_congestion_control: true,
        stats_update_interval: Duration::from_secs(1),
    };

    let processor = IntegratedFrameProcessor::new(config).await;
    
    // Test basic frame processing
    let frame_data = vec![0x01, 0x02, 0x03, 0x04]; // Simple test frame
    let stream_id = 1;
    
    let result = processor.process_frame(stream_id, frame_data).await;
    assert!(result.is_ok());
    
    // Check stream context was created
    let contexts = processor.stream_contexts.read().await;
    assert!(contexts.contains_key(&stream_id));
    
    processor.shutdown().await;
}

#[tokio::test]
async fn test_integrated_processor_flow_control() {
    let config = IntegratedFrameConfig {
        max_concurrent_streams: 10,
        frame_timeout: Duration::from_secs(5),
        flow_control_window: 1024, // Small window for testing
        congestion_window: 512,
        max_frame_size: 256,
        enable_flow_control: true,
        enable_congestion_control: true,
        stats_update_interval: Duration::from_millis(100),
    };

    let processor = IntegratedFrameProcessor::new(config).await;
    let stream_id = 1;
    
    // Send multiple frames to test flow control
    let large_frame = vec![0xFF; 300]; // Larger than max_frame_size
    let result = processor.process_frame(stream_id, large_frame).await;
    assert!(result.is_err()); // Should fail due to size limit
    
    // Send valid frames up to window limit
    let frame = vec![0xAA; 200];
    for i in 0..5 {
        let result = processor.process_frame(stream_id + i, frame.clone()).await;
        assert!(result.is_ok());
    }
    
    processor.shutdown().await;
}

#[tokio::test]
async fn test_stream_context_management() {
    let config = IntegratedFrameConfig::default();
    let processor = IntegratedFrameProcessor::new(config).await;
    
    let stream_id = 42;
    let frame_data = vec![0x11, 0x22, 0x33];
    
    // Process frame to create context
    processor.process_frame(stream_id, frame_data).await.unwrap();
    
    // Verify context exists
    {
        let contexts = processor.stream_contexts.read().await;
        let context = contexts.get(&stream_id).unwrap();
        assert_eq!(context.stream_id, stream_id);
        assert_eq!(context.frame_count, 1);
        assert!(context.total_bytes > 0);
    }
    
    // Close stream
    processor.close_stream(stream_id).await.unwrap();
    
    // Verify context was removed
    let contexts = processor.stream_contexts.read().await;
    assert!(!contexts.contains_key(&stream_id));
    
    processor.shutdown().await;
}

#[tokio::test]
async fn test_statistics_collection() {
    let config = IntegratedFrameConfig {
        stats_update_interval: Duration::from_millis(50),
        ..IntegratedFrameConfig::default()
    };
    
    let processor = IntegratedFrameProcessor::new(config).await;
    
    // Process some frames
    for i in 0..10 {
        let frame_data = vec![0x55; 100];
        processor.process_frame(i, frame_data).await.unwrap();
    }
    
    // Wait for stats update
    sleep(Duration::from_millis(100)).await;
    
    // Check statistics
    let stats = processor.get_statistics().await;
    assert_eq!(stats.total_frames_processed, 10);
    assert_eq!(stats.active_streams, 10);
    assert!(stats.total_bytes_processed > 0);
    assert!(stats.average_frame_size > 0.0);
    
    processor.shutdown().await;
}

#[tokio::test]
async fn test_event_handling() {
    let config = IntegratedFrameConfig::default();
    let processor = IntegratedFrameProcessor::new(config).await;
    
    let mut event_receiver = processor.subscribe_events().await;
    
    // Process a frame to generate events
    let stream_id = 1;
    let frame_data = vec![0x77; 50];
    
    tokio::spawn(async move {
        sleep(Duration::from_millis(10)).await;
        processor.process_frame(stream_id, frame_data).await.unwrap();
    });
    
    // Wait for event
    let event = tokio::time::timeout(Duration::from_secs(1), event_receiver.recv()).await;
    assert!(event.is_ok());
    
    let received_event = event.unwrap().unwrap();
    match received_event {
        FrameProcessingEvent::FrameProcessed { stream_id: sid, .. } => {
            assert_eq!(sid, stream_id);
        }
        _ => panic!("Unexpected event type"),
    }
}

#[tokio::test]
async fn test_concurrent_stream_processing() {
    let config = IntegratedFrameConfig {
        max_concurrent_streams: 50,
        ..IntegratedFrameConfig::default()
    };
    
    let processor: Arc<IntegratedFrameProcessor> = Arc::new(IntegratedFrameProcessor::new(config).await);
    
    // Create multiple concurrent tasks
    let mut handles = Vec::new();
    
    for i in 0..20 {
        let processor_clone = Arc::clone(&processor);
        let handle = tokio::spawn(async move {
            let frame_data = vec![0x88; 75];
            for j in 0..5 {
                let stream_id = i * 5 + j;
                processor_clone.process_frame(stream_id, frame_data.clone()).await.unwrap();
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all streams were processed
    let contexts = processor.stream_contexts.read().await;
    assert_eq!(contexts.len(), 100); // 20 tasks * 5 streams each
    
    let stats = processor.get_statistics().await;
    assert_eq!(stats.total_frames_processed, 100);
    assert_eq!(stats.active_streams, 100);
    
    processor.shutdown().await;
}

#[tokio::test]
async fn test_processor_shutdown() {
    let config = IntegratedFrameConfig::default();
    let processor = IntegratedFrameProcessor::new(config).await;
    
    // Process some frames
    for i in 0..5 {
        let frame_data = vec![0x99; 25];
        processor.process_frame(i, frame_data).await.unwrap();
    }
    
    // Verify streams exist
    {
        let contexts = processor.stream_contexts.read().await;
        assert_eq!(contexts.len(), 5);
    }
    
    // Shutdown processor
    processor.shutdown().await;
    
    // Verify cleanup
    let contexts = processor.stream_contexts.read().await;
    assert_eq!(contexts.len(), 0);
    
    // Verify that new frame processing fails after shutdown
    let result = processor.process_frame(999, vec![0x00]).await;
    assert!(result.is_err());
}

#[test]
fn test_integrated_frame_config_default() {
    let config = IntegratedFrameConfig::default();
    
    assert_eq!(config.max_concurrent_streams, 1000);
    assert_eq!(config.frame_timeout, Duration::from_secs(60));
    assert_eq!(config.flow_control_window, 65536);
    assert_eq!(config.congestion_window, 10240);
    assert_eq!(config.max_frame_size, 16384);
    assert!(config.enable_flow_control);
    assert!(config.enable_congestion_control);
    assert_eq!(config.stats_update_interval, Duration::from_secs(5));
}

#[test]
fn test_processing_statistics_default() {
    let stats = ProcessingStatistics::default();
    
    assert_eq!(stats.total_frames_processed, 0);
    assert_eq!(stats.total_bytes_processed, 0);
    assert_eq!(stats.active_streams, 0);
    assert_eq!(stats.average_frame_size, 0.0);
    assert_eq!(stats.frames_per_second, 0.0);
    assert_eq!(stats.bytes_per_second, 0.0);
    assert!(stats.flow_control_events == 0);
    assert!(stats.congestion_events == 0);
}
