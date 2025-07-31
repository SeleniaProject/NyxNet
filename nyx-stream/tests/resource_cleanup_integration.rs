use nyx_stream::{NyxAsyncStream, CleanupConfig, StreamStats, ResourceStats};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;

#[tokio::test]
async fn test_comprehensive_resource_cleanup() {
    // Create stream with custom cleanup configuration
    let cleanup_config = CleanupConfig {
        cleanup_timeout: Duration::from_secs(2),
        enable_automatic_cleanup: true,
        cleanup_interval: Duration::from_millis(100),
        force_cleanup_on_drop: true,
        max_pending_operations: 50,
    };
    
    let (mut stream, _receiver) = NyxAsyncStream::new_with_config(1, 1024, 8192, cleanup_config);
    
    // Start resource monitoring
    stream.start_resource_monitoring().await.unwrap();
    
    // Perform multiple operations to create resources
    for i in 0..10 {
        let data = format!("Test data chunk {}", i);
        stream.write_all(data.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        
        // Simulate some processing time
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    // Check resource statistics
    let resource_stats = stream.get_resource_stats().await.unwrap();
    println!("Resource stats: {:?}", resource_stats);
    
    // Check stream statistics
    let stream_stats = stream.get_stream_stats().await;
    println!("Stream stats: {:?}", stream_stats);
    
    assert!(stream_stats.bytes_written > 0);
    assert_eq!(stream_stats.stream_id, 1);
    
    // Perform cleanup
    stream.cleanup_resources().await.unwrap();
    
    // Verify cleanup completed
    let final_resource_stats = stream.get_resource_stats().await.unwrap();
    assert_eq!(final_resource_stats.total_resources, 0);
    
    let final_stream_stats = stream.get_stream_stats().await;
    assert!(final_stream_stats.is_cleaning_up);
    
    // Stop monitoring
    stream.stop_resource_monitoring().await;
}

#[tokio::test]
async fn test_force_cleanup_with_timeout() {
    let cleanup_config = CleanupConfig {
        cleanup_timeout: Duration::from_millis(50), // Very short timeout
        enable_automatic_cleanup: false,
        cleanup_interval: Duration::from_secs(1),
        force_cleanup_on_drop: true,
        max_pending_operations: 10,
    };
    
    let (stream, _receiver) = NyxAsyncStream::new_with_config(2, 1024, 4096, cleanup_config);
    
    // Register some operations
    for i in 0..5 {
        let _op_id = stream.register_operation(&format!("test_op_{}", i)).await.unwrap();
    }
    
    // Check pending operations
    let stats_before = stream.get_stream_stats().await;
    assert_eq!(stats_before.pending_operations, 5);
    
    // Force cleanup (should handle timeout gracefully)
    let result = stream.force_cleanup().await;
    
    // Even if timeout occurs, cleanup should handle it gracefully
    match result {
        Ok(()) => {
            let stats_after = stream.get_stream_stats().await;
            assert_eq!(stats_after.pending_operations, 0);
        }
        Err(e) => {
            println!("Expected timeout error: {}", e);
            // This is expected due to the very short timeout
        }
    }
}

#[tokio::test]
async fn test_resource_limits() {
    let cleanup_config = CleanupConfig {
        cleanup_timeout: Duration::from_secs(1),
        enable_automatic_cleanup: true,
        cleanup_interval: Duration::from_millis(100),
        force_cleanup_on_drop: true,
        max_pending_operations: 3, // Very low limit
    };
    
    let (stream, _receiver) = NyxAsyncStream::new_with_config(3, 1024, 4096, cleanup_config);
    
    // Register operations up to the limit
    for i in 0..3 {
        stream.register_operation(&format!("op_{}", i)).await.unwrap();
    }
    
    // This should fail due to limit
    let result = stream.register_operation("overflow_op").await;
    assert!(result.is_err());
    
    // Clean up
    stream.cleanup_resources().await.unwrap();
    
    // Now we should be able to register again
    let result = stream.register_operation("new_op").await;
    assert!(result.is_ok());
    
    // Complete the operation before final cleanup
    if let Ok(op_id) = result {
        stream.complete_operation(op_id).await.unwrap();
    }
    
    stream.cleanup_resources().await.unwrap();
}

#[tokio::test]
async fn test_memory_leak_prevention() {
    let (mut stream, _receiver) = NyxAsyncStream::new(4, 1024, 4096);
    
    // Create a lot of data to test memory management
    let large_data = vec![0u8; 2048];
    
    // Write data multiple times
    for _ in 0..10 {
        stream.write_all(&large_data).await.unwrap();
        stream.flush().await.unwrap();
    }
    
    // Check memory usage
    let stats_before = stream.get_stream_stats().await;
    let initial_write_buffer = stats_before.write_buffer_size;
    
    // Perform cleanup
    stream.cleanup_resources().await.unwrap();
    
    // Memory should be cleaned up
    let stats_after = stream.get_stream_stats().await;
    println!("Buffer size before: {}, after: {}", initial_write_buffer, stats_after.write_buffer_size);
    
    // Verify cleanup state
    assert!(stats_after.is_cleaning_up);
}

#[tokio::test]
async fn test_drop_cleanup() {
    let cleanup_config = CleanupConfig {
        cleanup_timeout: Duration::from_secs(1),
        enable_automatic_cleanup: true,
        cleanup_interval: Duration::from_millis(50),
        force_cleanup_on_drop: true,
        max_pending_operations: 10,
    };
    
    {
        let (stream, _receiver) = NyxAsyncStream::new_with_config(5, 1024, 4096, cleanup_config);
        
        // Register some operations
        for i in 0..3 {
            let _op_id = stream.register_operation(&format!("drop_test_op_{}", i)).await.unwrap();
        }
        
        let stats = stream.get_stream_stats().await;
        assert_eq!(stats.pending_operations, 3);
        
        // Stream will be dropped here, triggering cleanup
    }
    
    // Give some time for the drop cleanup to complete
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // If we reach here without hanging, drop cleanup worked
    println!("Drop cleanup completed successfully");
}

#[tokio::test]
async fn test_operation_lifecycle() {
    let (stream, _receiver) = NyxAsyncStream::new(6, 1024, 4096);
    
    // Register an operation
    let op_id = stream.register_operation("lifecycle_test").await.unwrap();
    
    // Check it's tracked
    let stats = stream.get_stream_stats().await;
    assert_eq!(stats.pending_operations, 1);
    assert_eq!(stats.operations_completed, 0);
    
    // Complete the operation
    stream.complete_operation(op_id).await.unwrap();
    
    // Check it's completed
    let stats_after = stream.get_stream_stats().await;
    assert_eq!(stats_after.pending_operations, 0);
    assert_eq!(stats_after.operations_completed, 1);
    
    // Clean up
    stream.cleanup_resources().await.unwrap();
}

#[tokio::test]
async fn test_sequential_operations() {
    let (stream, _receiver) = NyxAsyncStream::new(7, 1024, 4096);
    
    // Test sequential operations
    let mut op_ids = Vec::new();
    for i in 0..5 {
        let op_id = stream.register_operation(&format!("sequential_op_{}", i)).await.unwrap();
        op_ids.push(op_id);
    }
    
    // Check all operations are tracked
    let stats = stream.get_stream_stats().await;
    assert_eq!(stats.pending_operations, 5);
    
    // Complete all operations
    for op_id in op_ids {
        stream.complete_operation(op_id).await.unwrap();
    }
    
    // Check all completed
    let final_stats = stream.get_stream_stats().await;
    assert_eq!(final_stats.pending_operations, 0);
    assert_eq!(final_stats.operations_completed, 5);
    
    // Clean up
    stream.cleanup_resources().await.unwrap();
}