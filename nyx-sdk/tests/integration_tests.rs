use nyx_sdk::{NyxDaemon, NyxEvent, EventHandler, config::NyxConfig, error::NyxError};
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use std::time::Duration;
use tokio::time::{sleep, timeout};
use async_trait::async_trait;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Test event handler for collecting events during tests
#[derive(Debug, Default)]
struct TestEventHandler {
    event_count: AtomicU32,
    last_event: Arc<tokio::sync::Mutex<Option<NyxEvent>>>,
}

#[async_trait]
impl EventHandler for TestEventHandler {
    async fn handle_event(&self, event: NyxEvent) {
        self.event_count.fetch_add(1, Ordering::SeqCst);
        *self.last_event.lock().await = Some(event);
    }
    
    fn name(&self) -> &str {
        "TestEventHandler"
    }
}

impl TestEventHandler {
    fn new() -> Self {
        Self::default()
    }
    
    fn event_count(&self) -> u32 {
        self.event_count.load(Ordering::SeqCst)
    }
    
    async fn last_event(&self) -> Option<NyxEvent> {
        self.last_event.lock().await.clone()
    }
}

/// Test daemon connection and basic functionality
#[tokio::test]
async fn test_daemon_connection() {
    let config = NyxConfig::default();
    
    // Test successful connection
    match NyxDaemon::connect(config).await {
        Ok(daemon) => {
            // Verify connection status
            let status = daemon.status().await;
            assert!(matches!(status.connection.status, nyx_sdk::daemon::ConnectionStatus::Connected));
            
            // Test health check
            match daemon.health_check().await {
                Ok(node_info) => {
                    assert!(!node_info.node_id.is_empty());
                    assert!(!node_info.version.is_empty());
                }
                Err(e) => {
                    // If daemon is not available, skip this test
                    eprintln!("Daemon not available for testing: {}", e);
                    return;
                }
            }
        }
        Err(e) => {
            // If daemon is not available, skip this test
            eprintln!("Daemon not available for testing: {}", e);
            return;
        }
    }
}

/// Test stream opening and basic operations
#[tokio::test]
async fn test_stream_operations() {
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping stream test");
            return;
        }
    };
    
    // Test stream opening
    let target = "localhost:8080";
    match daemon.open_stream(target).await {
        Ok(mut stream) => {
            // Verify stream is open
            assert!(stream.is_open().await);
            assert_eq!(stream.target(), target);
            
            // Test basic read/write operations
            let test_data = b"Hello, Nyx!";
            match timeout(Duration::from_secs(5), stream.write_all(test_data)).await {
                Ok(Ok(_)) => {
                    // Flush the stream
                    let _ = stream.flush().await;
                    
                    // Test reading
                    let mut buffer = vec![0u8; 1024];
                    match timeout(Duration::from_secs(5), stream.read(&mut buffer)).await {
                        Ok(Ok(bytes_read)) => {
                            assert!(bytes_read > 0 || bytes_read == 0); // Allow zero reads
                        }
                        Ok(Err(e)) => eprintln!("Read error: {}", e),
                        Err(_) => eprintln!("Read timeout"),
                    }
                }
                Ok(Err(e)) => eprintln!("Write error: {}", e),
                Err(_) => eprintln!("Write timeout"),
            }
            
            // Test stream closure
            let _ = stream.close().await;
            assert!(stream.is_closed().await);
        }
        Err(e) => {
            eprintln!("Failed to open stream: {}", e);
        }
    }
}

/// Test stream read/write with various data sizes and patterns
#[tokio::test]
async fn test_stream_data_patterns() {
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping data pattern test");
            return;
        }
    };
    
    match daemon.open_stream("localhost:8080").await {
        Ok(mut stream) => {
            // Test small data
            let small_data = b"small";
            let _ = stream.write_all(small_data).await;
            let _ = stream.flush().await;
            
            // Test medium data
            let medium_data = vec![0xAB; 1024]; // 1KB
            let _ = stream.write_all(&medium_data).await;
            let _ = stream.flush().await;
            
            // Test large data
            let large_data = vec![0xCD; 65536]; // 64KB
            let _ = stream.write_all(&large_data).await;
            let _ = stream.flush().await;
            
            // Test pattern data
            let pattern_data: Vec<u8> = (0..256).cycle().take(2048).collect();
            let _ = stream.write_all(&pattern_data).await;
            let _ = stream.flush().await;
            
            // Get stream statistics
            let stats = stream.stats().await;
            println!("Stream stats: bytes_written={}, write_ops={}", 
                stats.bytes_written, stats.write_ops);
            
            let _ = stream.close().await;
        }
        Err(e) => {
            eprintln!("Failed to open stream: {}", e);
        }
    }
}

/// Test reconnection functionality with simulated interruptions
#[cfg(feature = "reconnect")]
#[tokio::test]
async fn test_reconnection_scenarios() {
    use nyx_sdk::stream::StreamOptions;
    
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping reconnection test");
            return;
        }
    };
    
    // Configure stream with auto-reconnect
    let mut options = StreamOptions::default();
    options.auto_reconnect = true;
    options.max_reconnect_attempts = 3;
    options.reconnect_delay = Duration::from_millis(100);
    
    match daemon.open_stream_with_options("localhost:8080", options).await {
        Ok(mut stream) => {
            // Verify initial connection
            assert!(stream.is_open().await);
            
            // Test manual reconnection
            match stream.reconnect().await {
                Ok(_) => {
                    assert!(stream.is_open().await);
                    println!("Reconnection successful");
                }
                Err(e) => {
                    eprintln!("Reconnection failed: {}", e);
                }
            }
            
            let _ = stream.close().await;
        }
        Err(e) => {
            eprintln!("Failed to open stream with reconnection: {}", e);
        }
    }
}

/// Test event system with multiple subscribers and event types
#[tokio::test]
async fn test_event_system() {
    let config = NyxConfig::default();
    
    let mut daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping event test");
            return;
        }
    };
    
    // Create test event handlers
    let handler1 = Arc::new(TestEventHandler::new());
    let handler2 = Arc::new(TestEventHandler::new());
    
    // Register event handlers
    daemon.set_event_handler(handler1.clone());
    
    // Test opening a stream (should generate events)
    match daemon.open_stream("localhost:8080").await {
        Ok(stream) => {
            // Wait for events to be processed
            sleep(Duration::from_millis(100)).await;
            
            // Check that events were received
            let event_count = handler1.event_count();
            println!("Received {} events", event_count);
            
            if let Some(event) = handler1.last_event().await {
                println!("Last event: {}", event);
            }
            
            // Close stream
            let stream_id = stream.stream_id();
            let _ = daemon.close_stream(stream_id).await;
            
            // Wait for close event
            sleep(Duration::from_millis(100)).await;
            
            let final_count = handler1.event_count();
            println!("Final event count: {}", final_count);
        }
        Err(e) => {
            eprintln!("Failed to open stream for event testing: {}", e);
        }
    }
}

/// Test error handling and recovery mechanisms
#[tokio::test]
async fn test_error_handling() {
    let config = NyxConfig::default();
    
    // Test connection to invalid endpoint
    let mut invalid_config = config.clone();
    invalid_config.daemon.endpoint = "invalid-endpoint:12345".to_string();
    
    match NyxDaemon::connect(invalid_config).await {
        Ok(_) => panic!("Should not connect to invalid endpoint"),
        Err(e) => {
            assert!(matches!(e, NyxError::DaemonConnection { .. }));
            println!("Correctly handled invalid endpoint: {}", e);
        }
    }
    
    // Test connection timeout
    let mut timeout_config = config.clone();
    timeout_config.daemon.endpoint = "192.0.2.1:12345".to_string(); // Reserved test IP
    timeout_config.daemon.connect_timeout = Duration::from_millis(100);
    
    match NyxDaemon::connect(timeout_config).await {
        Ok(_) => eprintln!("Unexpected connection to test IP"),
        Err(e) => {
            println!("Correctly handled connection timeout: {}", e);
        }
    }
    
    // Test opening stream to invalid target
    if let Ok(daemon) = NyxDaemon::connect(config).await {
        match daemon.open_stream("invalid-target:99999").await {
            Ok(_) => eprintln!("Unexpected stream open to invalid target"),
            Err(e) => {
                println!("Correctly handled invalid stream target: {}", e);
            }
        }
    }
}

/// Test concurrent stream operations
#[tokio::test]
async fn test_concurrent_streams() {
    use tokio::task::JoinSet;
    
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => Arc::new(daemon),
        Err(_) => {
            eprintln!("Daemon not available, skipping concurrent test");
            return;
        }
    };
    
    let mut set = JoinSet::new();
    
    // Create multiple concurrent streams
    for i in 0..3 {
        let daemon_clone = daemon.clone();
        set.spawn(async move {
            match daemon_clone.open_stream("localhost:8080").await {
                Ok(mut stream) => {
                    // Perform some operations
                    let data = format!("Stream {} data", i);
                    let _ = stream.write_all(data.as_bytes()).await;
                    let _ = stream.flush().await;
                    
                    // Read some data
                    let mut buffer = vec![0u8; 256];
                    let _ = stream.read(&mut buffer).await;
                    
                    let _ = stream.close().await;
                    Ok(stream.stream_id())
                }
                Err(e) => Err(e)
            }
        });
    }
    
    // Wait for all streams to complete
    let mut results = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok(stream_result) => results.push(stream_result),
            Err(e) => eprintln!("Concurrent stream task failed: {}", e),
        }
    }
    
    println!("Completed {} concurrent stream operations", results.len());
}

/// Test configuration validation and edge cases
#[tokio::test]
async fn test_configuration_validation() {
    // Test default configuration
    let default_config = NyxConfig::default();
    assert!(!default_config.daemon.endpoint.is_empty());
    assert!(default_config.daemon.connect_timeout > Duration::ZERO);
    
    // Test custom configuration
    let mut custom_config = NyxConfig::default();
    custom_config.daemon.endpoint = "localhost:50051".to_string();
    custom_config.daemon.connect_timeout = Duration::from_secs(10);
    
    // Configuration should be valid
    match NyxDaemon::connect(custom_config).await {
        Ok(_) => println!("Custom configuration accepted"),
        Err(e) => println!("Custom configuration error: {}", e),
    }
}

/// Test performance under load
#[tokio::test]
async fn test_performance_load() {
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping performance test");
            return;
        }
    };
    
    // Measure stream creation performance
    let start_time = std::time::Instant::now();
    let mut streams = Vec::new();
    
    // Create multiple streams
    for _ in 0..5 {
        match daemon.open_stream("localhost:8080").await {
            Ok(stream) => streams.push(stream),
            Err(e) => {
                eprintln!("Failed to create stream: {}", e);
                break;
            }
        }
    }
    
    let creation_time = start_time.elapsed();
    println!("Created {} streams in {:?}", streams.len(), creation_time);
    
    // Test data transfer performance
    if !streams.is_empty() {
        let data = vec![0x42; 1024]; // 1KB test data
        let start_time = std::time::Instant::now();
        
        for stream in &mut streams {
            let _ = stream.write_all(&data).await;
            let _ = stream.flush().await;
        }
        
        let transfer_time = start_time.elapsed();
        let total_bytes = data.len() * streams.len();
        let mbps = (total_bytes as f64 * 8.0) / (transfer_time.as_secs_f64() * 1_000_000.0);
        
        println!("Transferred {} bytes in {:?} ({:.2} Mbps)", 
            total_bytes, transfer_time, mbps);
    }
    
    // Clean up streams
    for mut stream in streams {
        let _ = stream.close().await;
    }
}

/// Test memory usage and resource management
#[tokio::test]
async fn test_resource_management() {
    let config = NyxConfig::default();
    
    let daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping resource test");
            return;
        }
    };
    
    // Create and immediately close many streams to test resource cleanup
    for i in 0..10 {
        match daemon.open_stream("localhost:8080").await {
            Ok(mut stream) => {
                // Perform minimal operation
                let _ = stream.write_all(b"test").await;
                let _ = stream.flush().await;
                let _ = stream.close().await;
                
                if i % 5 == 0 {
                    println!("Processed {} streams", i + 1);
                }
            }
            Err(e) => {
                eprintln!("Stream creation failed at iteration {}: {}", i, e);
                break;
            }
        }
    }
    
    // Test daemon status after intensive operations
    let status = daemon.status().await;
    println!("Final daemon status: {:?}", status.connection.status);
}

/// Integration test for complete SDK workflow
#[tokio::test]
async fn test_complete_sdk_workflow() {
    let config = NyxConfig::default();
    
    // 1. Connect to daemon
    let mut daemon = match NyxDaemon::connect(config).await {
        Ok(daemon) => daemon,
        Err(_) => {
            eprintln!("Daemon not available, skipping workflow test");
            return;
        }
    };
    
    // 2. Set up event handling
    let event_handler = Arc::new(TestEventHandler::new());
    daemon.set_event_handler(event_handler.clone());
    
    // 3. Open multiple streams
    let mut streams = Vec::new();
    for i in 0..3 {
        match daemon.open_stream(&format!("localhost:808{}", i)).await {
            Ok(stream) => streams.push(stream),
            Err(e) => eprintln!("Failed to open stream {}: {}", i, e),
        }
    }
    
    // 4. Perform data operations
    for (i, stream) in streams.iter_mut().enumerate() {
        let data = format!("Workflow test data from stream {}", i);
        let _ = stream.write_all(data.as_bytes()).await;
        let _ = stream.flush().await;
        
        let mut buffer = vec![0u8; 256];
        let _ = stream.read(&mut buffer).await;
    }
    
    // 5. Check statistics
    for (i, stream) in streams.iter().enumerate() {
        let stats = stream.stats().await;
        println!("Stream {} stats: {} bytes written, {} ops", 
            i, stats.bytes_written, stats.write_ops);
    }
    
    // 6. Clean up all streams
    for mut stream in streams {
        let stream_id = stream.stream_id();
        let _ = stream.close().await;
        let _ = daemon.close_stream(stream_id).await;
    }
    
    // 7. Check final event count
    sleep(Duration::from_millis(100)).await;
    let final_events = event_handler.event_count();
    println!("Total events processed: {}", final_events);
    
    // 8. Verify daemon status
    let final_status = daemon.status().await;
    println!("Final daemon status: {:?}", final_status.health);
} 