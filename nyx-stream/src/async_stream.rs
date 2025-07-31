#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, warn, info};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::stream_frame::{StreamFrame, build_stream_frame};
use crate::resource_manager::{ResourceManager, ResourceInfo, ResourceType, ResourceError};
use crate::frame_handler::{FrameHandler, FrameHandlerError};
use crate::flow_controller::{FlowController, FlowControlError};

/// Errors that can occur during stream operations
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Stream closed")]
    StreamClosed,
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),
    #[error("Buffer overflow")]
    BufferOverflow,
    #[error("Flow control violation")]
    FlowControlViolation,
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Frame parsing error: {0}")]
    FrameParsing(String),
    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),
    #[error("Cleanup timeout")]
    CleanupTimeout,
    #[error("Operation cancelled")]
    OperationCancelled,
}

/// Stream state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    Open,
    HalfClosedLocal,
    HalfClosedRemote,
    Closed,
}

/// Stream statistics
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub stream_id: u32,
    pub state: StreamState,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub operations_completed: u64,
    pub operations_cancelled: u64,
    pub pending_operations: usize,
    pub read_buffer_size: usize,
    pub write_buffer_size: usize,
    pub is_cleaning_up: bool,
}

/// Flow control statistics
#[derive(Debug, Clone)]
pub struct FlowStats {
    pub available_window: u32,
    pub bytes_in_flight: u32,
    pub congestion_window: u32,
    pub send_buffer_size: usize,
}

/// Frame buffer for reassembly
#[derive(Debug)]
struct FrameBuffer {
    frames: VecDeque<StreamFrame<'static>>,
    expected_offset: u32,
    total_received: usize,
    max_buffer_size: usize,
}

impl FrameBuffer {
    fn new(max_buffer_size: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            expected_offset: 0,
            total_received: 0,
            max_buffer_size,
        }
    }

    fn add_frame(&mut self, frame: StreamFrame<'static>) -> Result<(), StreamError> {
        if self.total_received + frame.data.len() > self.max_buffer_size {
            return Err(StreamError::BufferOverflow);
        }

        // Insert frame in order by offset
        let mut insert_pos = self.frames.len();
        for (i, existing) in self.frames.iter().enumerate() {
            if frame.offset < existing.offset {
                insert_pos = i;
                break;
            } else if frame.offset == existing.offset {
                // Duplicate frame, ignore
                debug!("Ignoring duplicate frame at offset {}", frame.offset);
                return Ok(());
            }
        }

        self.total_received += frame.data.len();
        self.frames.insert(insert_pos, frame);
        Ok(())
    }

    fn get_contiguous_data(&mut self) -> Option<Bytes> {
        if self.frames.is_empty() {
            debug!("No frames available");
            return None;
        }

        let mut data = BytesMut::new();
        let mut current_offset = self.expected_offset;
        debug!("Getting contiguous data, expected_offset: {}, frames: {}", 
               current_offset, self.frames.len());

        while let Some(frame) = self.frames.front() {
            debug!("Checking frame: offset={}, expected={}", frame.offset, current_offset);
            if frame.offset != current_offset {
                break;
            }

            let frame = self.frames.pop_front().unwrap();
            debug!("Processing frame with {} bytes", frame.data.len());
            data.extend_from_slice(frame.data);
            current_offset += frame.data.len() as u32;
            self.total_received -= frame.data.len();
        }

        if data.is_empty() {
            debug!("No contiguous data available");
            None
        } else {
            self.expected_offset = current_offset;
            debug!("Returning {} bytes of data", data.len());
            Some(data.freeze())
        }
    }

    fn has_fin(&self) -> bool {
        self.frames.iter().any(|f| f.fin)
    }

    fn is_complete(&self) -> bool {
        // Check if we have received all data up to FIN
        if let Some(_fin_frame) = self.frames.iter().find(|f| f.fin) {
            // All data before FIN should be contiguous from expected_offset
            let mut current_offset = self.expected_offset;
            for frame in &self.frames {
                if frame.offset == current_offset {
                    current_offset += frame.data.len() as u32;
                    if frame.fin {
                        return true;
                    }
                } else if frame.offset > current_offset {
                    break;
                }
            }
        }
        false
    }
}

/// Resource cleanup configuration
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    pub cleanup_timeout: Duration,
    pub enable_automatic_cleanup: bool,
    pub cleanup_interval: Duration,
    pub force_cleanup_on_drop: bool,
    pub max_pending_operations: usize,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_timeout: Duration::from_secs(5),
            enable_automatic_cleanup: true,
            cleanup_interval: Duration::from_secs(30),
            force_cleanup_on_drop: true,
            max_pending_operations: 100,
        }
    }
}

/// Pending operation tracking
#[derive(Debug)]
struct PendingOperation {
    id: u64,
    operation_type: String,
    started_at: Instant,
    cancel_handle: Option<CancellationToken>,
}

/// Async stream implementation for Nyx protocol with comprehensive resource cleanup
pub struct NyxAsyncStream {
    stream_id: u32,
    state: StreamState,
    
    // Read side
    read_buffer: Arc<Mutex<FrameBuffer>>,
    read_data: BytesMut,
    read_waker: Option<std::task::Waker>,
    
    // Write side
    write_buffer: BytesMut,
    write_offset: u32,
    write_closed: bool,
    
    // Frame transmission
    frame_sender: mpsc::UnboundedSender<Vec<u8>>,
    frame_receiver: Arc<Mutex<mpsc::UnboundedReceiver<Vec<u8>>>>,
    
    // Frame and flow control
    frame_handler: Arc<Mutex<FrameHandler>>,
    flow_controller: Arc<Mutex<FlowController>>,
    
    // Configuration
    max_frame_size: usize,
    max_buffer_size: usize,
    
    // Resource management
    resource_manager: Arc<ResourceManager>,
    cleanup_config: CleanupConfig,
    
    // Operation tracking
    pending_operations: Arc<Mutex<Vec<PendingOperation>>>,
    next_operation_id: Arc<std::sync::atomic::AtomicU64>,
    
    // Cleanup state
    is_cleaning_up: Arc<std::sync::atomic::AtomicBool>,
    cleanup_completed: Arc<tokio::sync::Notify>,
    
    // Background tasks
    cleanup_task: Option<JoinHandle<()>>,
    monitoring_task: Option<JoinHandle<()>>,
    
    // Statistics
    created_at: Instant,
    last_activity: Arc<Mutex<Instant>>,
    bytes_read: Arc<std::sync::atomic::AtomicU64>,
    bytes_written: Arc<std::sync::atomic::AtomicU64>,
    operations_completed: Arc<std::sync::atomic::AtomicU64>,
    operations_cancelled: Arc<std::sync::atomic::AtomicU64>,
}

impl NyxAsyncStream {
    pub fn new(
        stream_id: u32,
        max_frame_size: usize,
        max_buffer_size: usize,
    ) -> (Self, mpsc::UnboundedReceiver<Vec<u8>>) {
        Self::new_with_config(stream_id, max_frame_size, max_buffer_size, CleanupConfig::default())
    }

    pub fn new_with_config(
        stream_id: u32,
        max_frame_size: usize,
        max_buffer_size: usize,
        cleanup_config: CleanupConfig,
    ) -> (Self, mpsc::UnboundedReceiver<Vec<u8>>) {
        let (frame_sender, frame_receiver) = mpsc::unbounded_channel();
        let frame_receiver = Arc::new(Mutex::new(frame_receiver));
        
        let resource_manager = Arc::new(ResourceManager::new(stream_id));
        let now = Instant::now();
        
        // Initialize frame handler and flow controller
        let frame_handler = Arc::new(Mutex::new(FrameHandler::new(
            1, // max_streams (single stream per handler)
            max_buffer_size,
            max_buffer_size * 2, // max_total_buffer
        )));
        
        let flow_controller = Arc::new(Mutex::new(FlowController::new(
            65536,    // initial_window (64KB)
            1048576,  // max_window (1MB)
            32768,    // initial_cwnd (32KB)
            max_buffer_size, // max_send_buffer
        )));
        
        let stream = Self {
            stream_id,
            state: StreamState::Open,
            read_buffer: Arc::new(Mutex::new(FrameBuffer::new(max_buffer_size))),
            read_data: BytesMut::new(),
            read_waker: None,
            write_buffer: BytesMut::new(),
            write_offset: 0,
            write_closed: false,
            frame_sender,
            frame_receiver: frame_receiver.clone(),
            frame_handler,
            flow_controller,
            max_frame_size,
            max_buffer_size,
            resource_manager,
            cleanup_config,
            pending_operations: Arc::new(Mutex::new(Vec::new())),
            next_operation_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            is_cleaning_up: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cleanup_completed: Arc::new(tokio::sync::Notify::new()),
            cleanup_task: None,
            monitoring_task: None,
            created_at: now,
            last_activity: Arc::new(Mutex::new(now)),
            bytes_read: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            bytes_written: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            operations_completed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            operations_cancelled: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        
        // Return a separate receiver for external frame handling
        let (_external_sender, external_receiver) = mpsc::unbounded_channel();
        
        (stream, external_receiver)
    }

    /// Process incoming frame data using FrameHandler
    pub async fn handle_incoming_frame(&mut self, frame_data: &[u8]) -> Result<(), StreamError> {
        // Register operation
        let operation_id = self.register_operation("handle_incoming_frame").await?;
        
        let result = async {
            // Register frame data as a resource
            let frame_resource = ResourceInfo::new(
                format!("frame_{}_{}", self.stream_id, frame_data.len()),
                ResourceType::Buffer,
                frame_data.len(),
            ).with_metadata("frame_type".to_string(), "incoming".to_string());
            
            self.resource_manager.register_resource(frame_resource).await?;

            // Use FrameHandler to process the frame
            let reassembled_data = {
                let mut frame_handler = self.frame_handler.lock().await;
                frame_handler.handle_frame(frame_data)
                    .map_err(|e| match e {
                        FrameHandlerError::FrameParsing(msg) => StreamError::FrameParsing(msg),
                        FrameHandlerError::InvalidFrame(msg) => StreamError::InvalidFrame(msg),
                        FrameHandlerError::BufferOverflow(_) => StreamError::BufferOverflow,
                        FrameHandlerError::OutOfOrder { .. } => StreamError::InvalidFrame("Out of order frame".to_string()),
                        FrameHandlerError::StreamNotFound(_) => StreamError::InvalidFrame("Stream not found".to_string()),
                        FrameHandlerError::DuplicateFrame(_, _) => StreamError::InvalidFrame("Duplicate frame".to_string()),
                    })?
            };

            // If we have reassembled data, add it to read buffer
            if let Some(data) = reassembled_data {
                debug!("Received reassembled data: {} bytes, complete: {}", 
                       data.data.len(), data.is_complete);
                
                // Add data to read buffer
                self.read_data.extend_from_slice(&data.data);
                
                // Update bytes read counter
                self.bytes_read.fetch_add(data.data.len() as u64, std::sync::atomic::Ordering::Relaxed);
                
                // Wake up any pending read operations
                if let Some(waker) = self.read_waker.take() {
                    waker.wake();
                }

                // Update stream state if complete
                if data.is_complete {
                    match self.state {
                        StreamState::Open => self.state = StreamState::HalfClosedRemote,
                        StreamState::HalfClosedLocal => self.state = StreamState::Closed,
                        _ => {}
                    }
                }
            }

            // Update activity
            self.update_activity().await;

            Ok(())
        }.await;
        
        // Complete operation
        if let Err(e) = self.complete_operation(operation_id).await {
            warn!("Failed to complete operation {}: {}", operation_id, e);
        }
        
        result
    }

    /// Get current stream state
    pub fn state(&self) -> StreamState {
        self.state
    }

    /// Check if stream is closed
    pub fn is_closed(&self) -> bool {
        self.state == StreamState::Closed
    }

    /// Close the write side of the stream
    pub fn close_write(&mut self) {
        self.write_closed = true;
        match self.state {
            StreamState::Open => self.state = StreamState::HalfClosedLocal,
            StreamState::HalfClosedRemote => self.state = StreamState::Closed,
            _ => {}
        }
    }

    /// Start resource monitoring and automatic cleanup
    pub async fn start_resource_monitoring(&mut self) -> Result<(), StreamError> {
        if self.cleanup_config.enable_automatic_cleanup {
            // Note: ResourceManager doesn't have mutable methods for start/stop
            // This would need to be implemented differently in the ResourceManager
            
            // Start monitoring task
            let monitoring_task = self.spawn_monitoring_task().await;
            self.monitoring_task = Some(monitoring_task);
            
            info!("Started resource monitoring for stream {}", self.stream_id);
        }
        Ok(())
    }

    /// Stop resource monitoring
    pub async fn stop_resource_monitoring(&mut self) {
        if let Some(task) = self.monitoring_task.take() {
            task.abort();
        }
        
        debug!("Stopped resource monitoring for stream {}", self.stream_id);
    }

    /// Register a new pending operation
    pub async fn register_operation(&self, operation_type: &str) -> Result<u64, StreamError> {
        let operation_id = self.next_operation_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        let mut pending_ops = self.pending_operations.lock().await;
        
        // Check limits
        if pending_ops.len() >= self.cleanup_config.max_pending_operations {
            return Err(StreamError::Resource(ResourceError::LimitExceeded {
                resource_type: ResourceType::Custom("pending_operation"),
                limit: self.cleanup_config.max_pending_operations,
                current: pending_ops.len(),
            }));
        }
        
        let operation = PendingOperation {
            id: operation_id,
            operation_type: operation_type.to_string(),
            started_at: Instant::now(),
            cancel_handle: Some(CancellationToken::new()),
        };
        
        pending_ops.push(operation);
        
        // Register with resource manager
        let resource_info = ResourceInfo::new(
            format!("operation_{}", operation_id),
            ResourceType::Custom("pending_operation"),
            std::mem::size_of::<PendingOperation>(),
        ).with_metadata("operation_type".to_string(), operation_type.to_string())
         .with_metadata("stream_id".to_string(), self.stream_id.to_string());
        
        self.resource_manager.register_resource(resource_info).await?;
        
        debug!("Registered operation {} ({})", operation_id, operation_type);
        Ok(operation_id)
    }

    /// Complete a pending operation
    pub async fn complete_operation(&self, operation_id: u64) -> Result<(), StreamError> {
        let mut pending_ops = self.pending_operations.lock().await;
        
        if let Some(pos) = pending_ops.iter().position(|op| op.id == operation_id) {
            let operation = pending_ops.remove(pos);
            
            // Clean up from resource manager
            self.resource_manager.cleanup_resource(&format!("operation_{}", operation_id)).await?;
            
            self.operations_completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            debug!("Completed operation {} ({})", operation_id, operation.operation_type);
        }
        
        Ok(())
    }

    /// Cancel a pending operation
    pub async fn cancel_operation(&self, operation_id: u64) -> Result<(), StreamError> {
        let mut pending_ops = self.pending_operations.lock().await;
        
        if let Some(pos) = pending_ops.iter().position(|op| op.id == operation_id) {
            let operation = pending_ops.remove(pos);
            
            // Cancel the operation if possible
            if let Some(cancel_handle) = operation.cancel_handle {
                cancel_handle.cancel();
            }
            
            // Clean up from resource manager
            self.resource_manager.cleanup_resource(&format!("operation_{}", operation_id)).await?;
            
            self.operations_cancelled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            debug!("Cancelled operation {} ({})", operation_id, operation.operation_type);
        }
        
        Ok(())
    }

    /// Cancel all pending operations
    async fn cancel_all_operations(&self) -> Result<(), StreamError> {
        let mut pending_ops = self.pending_operations.lock().await;
        let operation_ids: Vec<u64> = pending_ops.iter().map(|op| op.id).collect();
        
        for operation in pending_ops.drain(..) {
            if let Some(cancel_handle) = operation.cancel_handle {
                cancel_handle.cancel();
            }
            
            // Clean up from resource manager
            if let Err(e) = self.resource_manager.cleanup_resource(&format!("operation_{}", operation.id)).await {
                warn!("Failed to cleanup operation {}: {}", operation.id, e);
            }
            
            self.operations_cancelled.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        
        info!("Cancelled {} pending operations for stream {}", operation_ids.len(), self.stream_id);
        Ok(())
    }

    /// Perform complete resource cleanup
    pub async fn cleanup_resources(&self) -> Result<(), StreamError> {
        if self.is_cleaning_up.swap(true, std::sync::atomic::Ordering::SeqCst) {
            // Already cleaning up, wait for completion
            self.cleanup_completed.notified().await;
            return Ok(());
        }
        
        info!("Starting resource cleanup for stream {}", self.stream_id);
        
        // Cancel all pending operations
        if let Err(e) = self.cancel_all_operations().await {
            error!("Failed to cancel operations during cleanup: {}", e);
        }
        
        // Clean up frame buffer resources
        {
            let mut read_buffer = self.read_buffer.lock().await;
            
            // Clean up any leaked frame data
            for frame in &read_buffer.frames {
                // The frame data is leaked memory that needs to be reclaimed
                // Since we can't use unsafe code, we'll just clear the frames
                // The memory will be cleaned up when the frame is dropped
                debug!("Cleaning up frame data: {} bytes", frame.data.len());
            }
            read_buffer.frames.clear();
            read_buffer.total_received = 0;
        }
        
        // Register buffer cleanup
        let buffer_resource = ResourceInfo::new(
            format!("read_buffer_{}", self.stream_id),
            ResourceType::Buffer,
            self.read_data.len() + self.write_buffer.len(),
        ).with_cleanup_callback(Box::new(|| {
            debug!("Cleaned up stream buffers");
            Ok(())
        }));
        
        if let Err(e) = self.resource_manager.register_resource(buffer_resource).await {
            warn!("Failed to register buffer resource: {}", e);
        }
        
        // Clean up all resources through resource manager
        if let Err(errors) = self.resource_manager.cleanup_all().await {
            warn!("Some resources failed to clean up: {} errors", errors.len());
            for error in errors {
                error!("Resource cleanup error: {}", error);
            }
        }
        
        // Update activity timestamp
        {
            let mut last_activity = self.last_activity.lock().await;
            *last_activity = Instant::now();
        }
        
        info!("Completed resource cleanup for stream {}", self.stream_id);
        
        // Notify waiting tasks
        self.cleanup_completed.notify_waiters();
        
        Ok(())
    }

    /// Force cleanup with timeout
    pub async fn force_cleanup(&self) -> Result<(), StreamError> {
        let cleanup_future = self.cleanup_resources();
        
        match tokio::time::timeout(self.cleanup_config.cleanup_timeout, cleanup_future).await {
            Ok(result) => result,
            Err(_) => {
                error!("Cleanup timeout for stream {}", self.stream_id);
                
                // Force cleanup by aborting all tasks
                if let Err(e) = self.cancel_all_operations().await {
                    error!("Failed to force cancel operations: {}", e);
                }
                
                Err(StreamError::CleanupTimeout)
            }
        }
    }

    /// Get resource usage statistics
    pub async fn get_resource_stats(&self) -> Result<crate::resource_manager::ResourceStats, StreamError> {
        Ok(self.resource_manager.get_stats().await)
    }

    /// Get stream statistics
    pub async fn get_stream_stats(&self) -> StreamStats {
        let last_activity = *self.last_activity.lock().await;
        let pending_ops = self.pending_operations.lock().await.len();
        
        StreamStats {
            stream_id: self.stream_id,
            state: self.state,
            created_at: self.created_at,
            last_activity,
            bytes_read: self.bytes_read.load(std::sync::atomic::Ordering::SeqCst),
            bytes_written: self.bytes_written.load(std::sync::atomic::Ordering::SeqCst),
            operations_completed: self.operations_completed.load(std::sync::atomic::Ordering::SeqCst),
            operations_cancelled: self.operations_cancelled.load(std::sync::atomic::Ordering::SeqCst),
            pending_operations: pending_ops,
            read_buffer_size: self.read_data.len(),
            write_buffer_size: self.write_buffer.len(),
            is_cleaning_up: self.is_cleaning_up.load(std::sync::atomic::Ordering::SeqCst),
        }
    }

    /// Spawn monitoring task for resource usage
    async fn spawn_monitoring_task(&self) -> JoinHandle<()> {
        let resource_manager = Arc::clone(&self.resource_manager);
        let pending_operations = Arc::clone(&self.pending_operations);
        let cleanup_config = self.cleanup_config.clone();
        let stream_id = self.stream_id;
        let is_cleaning_up = Arc::clone(&self.is_cleaning_up);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_config.cleanup_interval);
            
            loop {
                interval.tick().await;
                
                if is_cleaning_up.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                
                // Check for stale operations
                let mut stale_operations = Vec::new();
                {
                    let pending_ops = pending_operations.lock().await;
                    let now = Instant::now();
                    
                    for operation in pending_ops.iter() {
                        if now.duration_since(operation.started_at) > cleanup_config.cleanup_timeout {
                            stale_operations.push(operation.id);
                        }
                    }
                }
                
                if !stale_operations.is_empty() {
                    warn!("Found {} stale operations for stream {}", stale_operations.len(), stream_id);
                    // Note: In a real implementation, we'd cancel these operations
                }
                
                // Get resource stats
                let stats = resource_manager.get_stats().await;
                if stats.total_resources > 0 {
                    debug!("Stream {} resource stats: {} resources, {} bytes", 
                           stream_id, stats.total_resources, stats.total_memory_bytes);
                }
            }
            
            debug!("Monitoring task completed for stream {}", stream_id);
        })
    }

    /// Update activity timestamp
    async fn update_activity(&self) {
        let mut last_activity = self.last_activity.lock().await;
        *last_activity = Instant::now();
    }

    /// Send buffered write data as frames using FlowController
    async fn flush_write_buffer(&mut self) -> Result<(), StreamError> {
        // Get sendable data from flow controller
        let sendable_data = {
            let mut flow_controller = self.flow_controller.lock().await;
            let available = flow_controller.available_to_send();
            if available == 0 && !self.write_closed {
                debug!("No data available to send due to flow control");
                return Ok(());
            }
            flow_controller.get_sendable_data(available)
        };

        if sendable_data.is_empty() && !self.write_closed {
            return Ok(());
        }

        let mut remaining = BytesMut::from(&sendable_data[..]);
        let mut current_offset = self.write_offset;

        while !remaining.is_empty() || self.write_closed {
            let chunk_size = std::cmp::min(remaining.len(), self.max_frame_size);
            let chunk = if chunk_size > 0 {
                remaining.split_to(chunk_size)
            } else {
                BytesMut::new()
            };

            let is_fin = self.write_closed && remaining.is_empty() && {
                // Check if flow controller has no more data
                let mut flow_controller = self.flow_controller.lock().await;
                flow_controller.get_sendable_data(1).is_empty()
            };
            
            let frame = StreamFrame {
                stream_id: self.stream_id,
                offset: current_offset,
                fin: is_fin,
                data: &chunk,
            };

            let frame_bytes = build_stream_frame(&frame);
            
            // Record data being sent in flow controller
            if !chunk.is_empty() {
                let mut flow_controller = self.flow_controller.lock().await;
                if let Err(e) = flow_controller.on_data_sent(chunk.len() as u32) {
                    warn!("Flow control error when sending data: {}", e);
                    return Err(StreamError::FlowControlViolation);
                }
            }
            
            if let Err(_) = self.frame_sender.send(frame_bytes) {
                return Err(StreamError::StreamClosed);
            }

            current_offset += chunk.len() as u32;

            if is_fin {
                break;
            }
        }

        self.write_offset = current_offset;
        Ok(())
    }

    /// Handle acknowledgment of sent data
    pub async fn handle_ack(&mut self, acked_bytes: u32, rtt: Duration) -> Result<(), StreamError> {
        let mut flow_controller = self.flow_controller.lock().await;
        flow_controller.on_ack_received(acked_bytes, rtt, false);
        debug!("Processed ACK for {} bytes with RTT {:?}", acked_bytes, rtt);
        Ok(())
    }

    /// Handle duplicate acknowledgment
    pub async fn handle_duplicate_ack(&mut self, acked_bytes: u32, rtt: Duration) -> Result<(), StreamError> {
        let mut flow_controller = self.flow_controller.lock().await;
        flow_controller.on_ack_received(acked_bytes, rtt, true);
        debug!("Processed duplicate ACK for {} bytes", acked_bytes);
        Ok(())
    }

    /// Handle packet loss notification
    pub async fn handle_packet_loss(&mut self, lost_bytes: u32) -> Result<(), StreamError> {
        let mut flow_controller = self.flow_controller.lock().await;
        flow_controller.on_packet_lost(lost_bytes);
        warn!("Handled packet loss of {} bytes", lost_bytes);
        Ok(())
    }

    /// Update flow control window
    pub async fn update_flow_window(&mut self, new_window: u32) -> Result<(), StreamError> {
        let mut flow_controller = self.flow_controller.lock().await;
        flow_controller.update_flow_window(new_window)
            .map_err(|e| StreamError::FlowControlViolation)?;
        debug!("Updated flow window to {} bytes", new_window);
        Ok(())
    }

    /// Get current flow control statistics
    pub async fn get_flow_stats(&self) -> FlowStats {
        let flow_controller = self.flow_controller.lock().await;
        let stats = flow_controller.get_stats();
        FlowStats {
            available_window: flow_controller.available_to_send(),
            bytes_in_flight: stats.bytes_in_flight,
            congestion_window: stats.congestion_window,
            send_buffer_size: stats.send_buffer_size,
        }
    }

    /// Check if backpressure should be applied
    pub async fn should_apply_backpressure(&self) -> bool {
        let flow_controller = self.flow_controller.lock().await;
        flow_controller.should_apply_backpressure()
    }
}

impl Drop for NyxAsyncStream {
    fn drop(&mut self) {
        if self.cleanup_config.force_cleanup_on_drop {
            // Spawn a cleanup task since we can't do async work in Drop
            let resource_manager = Arc::clone(&self.resource_manager);
            let pending_operations = Arc::clone(&self.pending_operations);
            let stream_id = self.stream_id;
            let cleanup_timeout = self.cleanup_config.cleanup_timeout;
            
            tokio::spawn(async move {
                info!("Performing cleanup on drop for stream {}", stream_id);
                
                // Cancel all pending operations
                let mut pending_ops = pending_operations.lock().await;
                for operation in pending_ops.drain(..) {
                    if let Some(cancel_handle) = operation.cancel_handle {
                        cancel_handle.cancel();
                    }
                    
                    if let Err(e) = resource_manager.cleanup_resource(&format!("operation_{}", operation.id)).await {
                        warn!("Failed to cleanup operation {} on drop: {}", operation.id, e);
                    }
                }
                
                // Clean up all resources with timeout
                let cleanup_future = resource_manager.cleanup_all();
                match tokio::time::timeout(cleanup_timeout, cleanup_future).await {
                    Ok(Ok(())) => {
                        debug!("Successfully cleaned up resources on drop for stream {}", stream_id);
                    }
                    Ok(Err(errors)) => {
                        warn!("Some resources failed to clean up on drop for stream {}: {} errors", 
                              stream_id, errors.len());
                    }
                    Err(_) => {
                        error!("Cleanup timeout on drop for stream {}", stream_id);
                    }
                }
            });
        }
        
        // Abort background tasks
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
        }
        if let Some(task) = self.monitoring_task.take() {
            task.abort();
        }
        
        warn!("NyxAsyncStream {} dropped", self.stream_id);
    }
}

impl AsyncRead for NyxAsyncStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Check if we're cleaning up
        if self.is_cleaning_up.load(std::sync::atomic::Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Stream is being cleaned up"
            )));
        }

        // First, try to read from existing buffered data
        if !self.read_data.is_empty() {
            let to_read = std::cmp::min(buf.remaining(), self.read_data.len());
            let data = self.read_data.split_to(to_read);
            buf.put_slice(&data);
            
            // Update statistics
            self.bytes_read.fetch_add(to_read as u64, std::sync::atomic::Ordering::SeqCst);
            
            // Update activity in background
            let last_activity = Arc::clone(&self.last_activity);
            tokio::spawn(async move {
                let mut activity = last_activity.lock().await;
                *activity = Instant::now();
            });
            
            return Poll::Ready(Ok(()));
        }

        // Try to get more data from frame buffer
        let buffer = self.read_buffer.clone();
        
        // Use a future to lock the buffer
        let lock_future = buffer.lock();
        let mut pinned_future = std::pin::pin!(lock_future);
        
        let mut buffer_guard = match pinned_future.as_mut().poll(cx) {
            Poll::Ready(guard) => guard,
            Poll::Pending => {
                self.read_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
        };

        if let Some(data) = buffer_guard.get_contiguous_data() {
            drop(buffer_guard);
            
            let to_read = std::cmp::min(buf.remaining(), data.len());
            buf.put_slice(&data[..to_read]);
            
            // Store remaining data for next read
            if to_read < data.len() {
                self.read_data.extend_from_slice(&data[to_read..]);
            }
            
            // Update statistics
            self.bytes_read.fetch_add(to_read as u64, std::sync::atomic::Ordering::SeqCst);
            
            // Update activity in background
            let last_activity = Arc::clone(&self.last_activity);
            tokio::spawn(async move {
                let mut activity = last_activity.lock().await;
                *activity = Instant::now();
            });
            
            return Poll::Ready(Ok(()));
        }

        // Check if stream is complete (FIN received and all data processed)
        if buffer_guard.is_complete() && buffer_guard.frames.is_empty() {
            drop(buffer_guard);
            return Poll::Ready(Ok(()));
        }

        // Check if stream is closed for reading and no more data available
        if matches!(self.state, StreamState::HalfClosedRemote | StreamState::Closed) {
            drop(buffer_guard);
            debug!("Stream {} closed and no more data, returning EOF", self.stream_id);
            return Poll::Ready(Ok(()));
        }

        // No data available, register waker
        self.read_waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl AsyncWrite for NyxAsyncStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        // Check if we're cleaning up
        if self.is_cleaning_up.load(std::sync::atomic::Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Stream is being cleaned up"
            )));
        }

        if self.write_closed || matches!(self.state, StreamState::HalfClosedLocal | StreamState::Closed) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Stream write side is closed"
            )));
        }

        // Use FlowController to check if we can send data
        let flow_controller = Arc::clone(&self.flow_controller);
        let can_send_future = async move {
            let mut controller = flow_controller.lock().await;
            
            // Try to buffer the data in flow controller
            match controller.buffer_data(buf) {
                Ok(buffered_bytes) => {
                    debug!("Buffered {} bytes in flow controller", buffered_bytes);
                    Ok(buffered_bytes)
                }
                Err(FlowControlError::BufferOverflow { available, .. }) => {
                    if available == 0 {
                        // No space available, would block
                        Err(io::Error::new(io::ErrorKind::WouldBlock, "Flow control buffer full"))
                    } else {
                        // Partial write possible
                        let partial_data = &buf[..available];
                        controller.buffer_data(partial_data).map_err(|e| {
                            io::Error::new(io::ErrorKind::Other, format!("Flow control error: {}", e))
                        })
                    }
                }
                Err(e) => Err(io::Error::new(io::ErrorKind::Other, format!("Flow control error: {}", e))),
            }
        };

        // Poll the future
        let mut pinned_future = std::pin::pin!(can_send_future);
        match pinned_future.as_mut().poll(cx) {
            Poll::Ready(Ok(bytes_written)) => {
                // Update statistics
                self.bytes_written.fetch_add(bytes_written as u64, std::sync::atomic::Ordering::SeqCst);
                
                // Update activity in background
                let last_activity = Arc::clone(&self.last_activity);
                tokio::spawn(async move {
                    let mut activity = last_activity.lock().await;
                    *activity = Instant::now();
                });
                
                Poll::Ready(Ok(bytes_written))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        // Check if we're cleaning up
        if self.is_cleaning_up.load(std::sync::atomic::Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Stream is being cleaned up"
            )));
        }

        // Extract last_activity before borrowing self mutably
        let last_activity = Arc::clone(&self.last_activity);
        
        let future = self.flush_write_buffer();
        let mut pinned = std::pin::pin!(future);
        
        match pinned.as_mut().poll(cx) {
            Poll::Ready(Ok(())) => {
                // Update activity in background
                tokio::spawn(async move {
                    let mut activity = last_activity.lock().await;
                    *activity = Instant::now();
                });
                Poll::Ready(Ok(()))
            },
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.close_write();
        
        // For shutdown, we'll just flush and let the Drop implementation handle cleanup
        self.poll_flush(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_basic_read_write() {
        let (mut stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Start resource monitoring
        stream.start_resource_monitoring().await.unwrap();
        
        // Test write
        let data = b"Hello, World!";
        stream.write_all(data).await.unwrap();
        stream.flush().await.unwrap();
        
        // Simulate receiving the frame back
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data,
        };
        let frame_bytes = build_stream_frame(&frame);
        stream.handle_incoming_frame(&frame_bytes).await.unwrap();
        
        // Test read
        let mut buf = vec![0u8; data.len()];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, data);
        
        // Check statistics
        let stats = stream.get_stream_stats().await;
        assert_eq!(stats.stream_id, 1);
        assert!(stats.bytes_read > 0);
        assert!(stats.bytes_written > 0);
        
        // Clean up
        stream.cleanup_resources().await.unwrap();
    }

    #[tokio::test]
    async fn test_frame_reassembly() {
        let (mut stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Send frames out of order
        let frame2 = StreamFrame {
            stream_id: 1,
            offset: 5,
            fin: false,
            data: b"World!",
        };
        let frame1 = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: b"Hello",
        };
        
        // Add second frame first
        let frame2_bytes = build_stream_frame(&frame2);
        stream.handle_incoming_frame(&frame2_bytes).await.unwrap();
        
        // Add first frame
        let frame1_bytes = build_stream_frame(&frame1);
        stream.handle_incoming_frame(&frame1_bytes).await.unwrap();
        
        // Read should get complete data
        let mut buf = vec![0u8; 11];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"HelloWorld!");
        
        // Clean up
        stream.cleanup_resources().await.unwrap();
    }

    #[tokio::test]
    async fn test_stream_close() {
        let (mut stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Send FIN frame
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: true,
            data: b"Final data",
        };
        let frame_bytes = build_stream_frame(&frame);
        stream.handle_incoming_frame(&frame_bytes).await.unwrap();
        
        // Read data
        let mut buf = vec![0u8; 20];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(&buf[..n], b"Final data");
        
        // Next read should return EOF
        let mut buf2 = vec![0u8; 10];
        let n2 = stream.read(&mut buf2).await.unwrap();
        assert_eq!(n2, 0);
        
        assert_eq!(stream.state(), StreamState::HalfClosedRemote);
        
        // Clean up
        stream.cleanup_resources().await.unwrap();
    }

    #[tokio::test]
    async fn test_resource_cleanup() {
        let (mut stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Start monitoring
        stream.start_resource_monitoring().await.unwrap();
        
        // Perform some operations
        let data = b"Test data";
        stream.write_all(data).await.unwrap();
        stream.flush().await.unwrap();
        
        // Check resource stats before cleanup
        let _stats_before = stream.get_resource_stats().await.unwrap();
        
        // Perform cleanup
        stream.cleanup_resources().await.unwrap();
        
        // Check resource stats after cleanup
        let stats_after = stream.get_resource_stats().await.unwrap();
        assert_eq!(stats_after.total_resources, 0);
        
        // Check stream stats
        let stream_stats = stream.get_stream_stats().await;
        assert!(stream_stats.is_cleaning_up);
    }

    #[tokio::test]
    async fn test_operation_tracking() {
        let (stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Register an operation
        let op_id = stream.register_operation("test_operation").await.unwrap();
        
        // Check pending operations
        let stats = stream.get_stream_stats().await;
        assert_eq!(stats.pending_operations, 1);
        
        // Complete the operation
        stream.complete_operation(op_id).await.unwrap();
        
        // Check operations completed
        let stats_after = stream.get_stream_stats().await;
        assert_eq!(stats_after.pending_operations, 0);
        assert_eq!(stats_after.operations_completed, 1);
    }

    #[tokio::test]
    async fn test_force_cleanup() {
        let (stream, _receiver) = NyxAsyncStream::new(1, 1024, 4096);
        
        // Register some operations
        let _op1 = stream.register_operation("op1").await.unwrap();
        let _op2 = stream.register_operation("op2").await.unwrap();
        
        // Force cleanup
        stream.force_cleanup().await.unwrap();
        
        // Check that operations were cancelled
        let stats = stream.get_stream_stats().await;
        assert_eq!(stats.pending_operations, 0);
        assert!(stats.operations_cancelled > 0);
    }

    #[tokio::test]
    async fn test_cleanup_config() {
        let config = CleanupConfig {
            cleanup_timeout: Duration::from_millis(100),
            enable_automatic_cleanup: false,
            cleanup_interval: Duration::from_secs(1),
            force_cleanup_on_drop: true,
            max_pending_operations: 10,
        };
        
        let (stream, _receiver) = NyxAsyncStream::new_with_config(1, 1024, 4096, config);
        
        // Test that we can't exceed max pending operations
        for i in 0..10 {
            stream.register_operation(&format!("op_{}", i)).await.unwrap();
        }
        
        // This should fail due to limit
        let result = stream.register_operation("overflow_op").await;
        assert!(result.is_err());
        
        // Clean up
        stream.cleanup_resources().await.unwrap();
    }
}