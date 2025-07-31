#![forbid(unsafe_code)]

//! Integrated Frame Handler with Flow Control
//! 
//! This module provides a unified interface for frame processing that combines
//! frame handling, reassembly, flow control, and congestion management.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing::{debug, info, warn, error, trace};
use thiserror::Error;

use crate::simple_frame_handler::{SimpleFrameHandler};
use crate::flow_controller::{FlowController, FlowControlError, FlowControlStats};
use crate::stream_frame::{StreamFrame, parse_stream_frame};

/// Data that has been successfully reassembled from frames
#[derive(Debug, Clone)]
pub struct ReassembledData {
    pub stream_id: u32,
    pub data: Vec<u8>,
    pub offset: u32,
    pub is_final: bool,
}

/// Errors that can occur in the integrated frame processing system
#[derive(Error, Debug)]
pub enum IntegratedFrameError {
    #[error("Frame parsing error: {0}")]
    FrameParsing(String),
    #[error("Flow control error: {0}")]
    FlowControl(#[from] FlowControlError),
    #[error("Stream {stream_id} is flow control blocked")]
    FlowControlBlocked { stream_id: u32 },
    #[error("Processing capacity exceeded for stream {stream_id}")]
    ProcessingCapacityExceeded { stream_id: u32 },
    #[error("Invalid stream state for stream {stream_id}: {state}")]
    InvalidStreamState { stream_id: u32, state: String },
    #[error("Network congestion detected")]
    NetworkCongestion,
    #[error("Backpressure threshold exceeded")]
    BackpressureExceeded,
}

/// Stream processing state
#[derive(Debug, Clone, PartialEq)]
pub enum StreamState {
    Active,
    FlowControlBlocked,
    CongestionBlocked,
    BackpressureBlocked,
    Draining,
    Closed,
}

/// Per-stream processing context
#[derive(Debug)]
pub struct StreamContext {
    pub stream_id: u32,
    pub state: StreamState,
    pub flow_controller: FlowController,
    pub last_activity: Instant,
    pub pending_frames: VecDeque<StreamFrame<'static>>,
    pub processing_queue: VecDeque<ReassembledData>,
    pub backlog_size: usize,
    pub priority: u8,
    pub max_backlog_size: usize,
}

impl StreamContext {
    pub fn new(stream_id: u32, initial_window: u32, max_backlog_size: usize) -> Self {
        Self {
            stream_id,
            state: StreamState::Active,
            flow_controller: FlowController::new(initial_window),
            last_activity: Instant::now(),
            pending_frames: VecDeque::new(),
            processing_queue: VecDeque::new(),
            backlog_size: 0,
            priority: 0,
            max_backlog_size,
        }
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self.state, 
            StreamState::FlowControlBlocked | 
            StreamState::CongestionBlocked | 
            StreamState::BackpressureBlocked
        )
    }

    pub fn can_accept_frame(&self) -> bool {
        self.state == StreamState::Active && 
        self.backlog_size < self.max_backlog_size
    }
}

/// Configuration for the integrated frame processor
#[derive(Debug, Clone)]
pub struct IntegratedFrameConfig {
    pub max_concurrent_streams: usize,
    pub default_stream_window: u32,
    pub max_frame_size: usize,
    pub reassembly_timeout: Duration,
    pub flow_control_update_interval: Duration,
    pub congestion_control_enabled: bool,
    pub backpressure_threshold: f64,
    pub max_processing_queue_size: usize,
    pub stream_cleanup_interval: Duration,
    pub priority_queue_enabled: bool,
}

impl Default for IntegratedFrameConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 1000,
            default_stream_window: 65536,
            max_frame_size: 1024 * 1024, // 1MB
            reassembly_timeout: Duration::from_secs(30),
            flow_control_update_interval: Duration::from_millis(100),
            congestion_control_enabled: true,
            backpressure_threshold: 0.8,
            max_processing_queue_size: 100,
            stream_cleanup_interval: Duration::from_secs(60),
            priority_queue_enabled: true,
        }
    }
}

/// Integrated statistics combining frame handling and flow control metrics
#[derive(Debug, Clone)]
pub struct IntegratedStats {
    pub total_frames_processed: u64,
    pub total_frames_reassembled: u64,
    pub total_bytes_processed: u64,
    pub active_streams: usize,
    pub blocked_streams: usize,
    pub avg_processing_latency_ms: f64,
    pub flow_control_stats: FlowControlStats,
    pub network_stats: NetworkStats,
    pub backpressure_events: u64,
    pub congestion_events: u64,
    pub stream_timeout_events: u64,
}

/// Event types for monitoring and alerting
#[derive(Debug, Clone)]
pub enum FrameProcessingEvent {
    StreamCreated { stream_id: u32 },
    StreamClosed { stream_id: u32, reason: String },
    FlowControlBlocked { stream_id: u32 },
    FlowControlUnblocked { stream_id: u32 },
    CongestionDetected { stream_id: u32, severity: f64 },
    BackpressureActivated { stream_id: u32 },
    FrameReassembled { stream_id: u32, data_size: usize },
    ProcessingLatencyHigh { stream_id: u32, latency_ms: f64 },
}

/// Main integrated frame processing system
pub struct IntegratedFrameProcessor {
    config: IntegratedFrameConfig,
    frame_handler: Arc<Mutex<FrameHandler>>,
    streams: Arc<RwLock<HashMap<u32, Arc<Mutex<StreamContext>>>>>,
    stats: Arc<RwLock<IntegratedStats>>,
    event_sender: mpsc::UnboundedSender<FrameProcessingEvent>,
    processing_tasks: Arc<Mutex<HashMap<u32, tokio::task::JoinHandle<()>>>>,
    global_flow_controller: Arc<Mutex<FlowController>>,
    shutdown_signal: Arc<Mutex<bool>>,
}

impl IntegratedFrameProcessor {
    /// Create a new integrated frame processor
    pub fn new(
        config: IntegratedFrameConfig,
        event_sender: mpsc::UnboundedSender<FrameProcessingEvent>
    ) -> Self {
        let frame_handler = Arc::new(Mutex::new(
            FrameHandler::new(config.max_frame_size, config.reassembly_timeout)
        ));
        
        let initial_stats = IntegratedStats {
            total_frames_processed: 0,
            total_frames_reassembled: 0,
            total_bytes_processed: 0,
            active_streams: 0,
            blocked_streams: 0,
            avg_processing_latency_ms: 0.0,
            flow_control_stats: FlowControlStats {
                flow_window_size: config.default_stream_window,
                bytes_in_flight: 0,
                congestion_window: config.default_stream_window,
                congestion_state: crate::flow_controller::CongestionState::SlowStart,
                rtt: None,
                rto: Duration::from_millis(1000),
                bytes_sent: 0,
                bytes_acked: 0,
                packets_lost: 0,
                send_buffer_size: 0,
                throughput: 0.0,
                backpressure_active: false,
            },
            network_stats: NetworkStats {
                bytes_sent: 0,
                bytes_acked: 0,
                packets_lost: 0,
                current_window: config.default_stream_window,
                congestion_window: config.default_stream_window,
                rtt: None,
                rto: Duration::from_millis(1000),
                throughput: 0.0,
                buffer_usage: 0.0,
            },
            backpressure_events: 0,
            congestion_events: 0,
            stream_timeout_events: 0,
        };

        Self {
            config: config.clone(),
            frame_handler,
            streams: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(initial_stats)),
            event_sender,
            processing_tasks: Arc::new(Mutex::new(HashMap::new())),
            global_flow_controller: Arc::new(Mutex::new(
                FlowController::new(config.default_stream_window)
            )),
            shutdown_signal: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the integrated frame processor
    pub async fn start(&self) -> Result<(), IntegratedFrameError> {
        info!("Starting integrated frame processor");
        
        // Start background tasks
        self.start_flow_control_updater().await;
        self.start_stream_cleanup_task().await;
        self.start_statistics_updater().await;
        
        info!("Integrated frame processor started successfully");
        Ok(())
    }

    /// Process an incoming frame with integrated flow control
    pub async fn process_frame(&self, frame_data: &[u8]) -> Result<Vec<ReassembledData>, IntegratedFrameError> {
        let start_time = Instant::now();
        
        // Parse frame
        let (_, frame) = match parse_stream_frame(frame_data) {
            Ok(result) => result,
            Err(e) => {
                error!("Failed to parse frame: {}", e);
                return Err(IntegratedFrameError::FrameParsing(e.to_string()));
            }
        };

        let stream_id = frame.stream_id;
        trace!("Processing frame for stream {}, offset {}, length {}", 
               stream_id, frame.offset, frame.data.len());

        // Get or create stream context
        let stream_context = self.get_or_create_stream(stream_id).await?;
        
        // Check if stream can accept more frames
        {
            let ctx = stream_context.lock().await;
            if !ctx.can_accept_frame() {
                warn!("Stream {} cannot accept frame - state: {:?}, backlog: {}", 
                      stream_id, ctx.state, ctx.backlog_size);
                return Err(IntegratedFrameError::FlowControlBlocked { stream_id });
            }
        }

        // Apply flow control
        self.apply_flow_control(stream_id, frame.data.len()).await?;

        // Process frame through frame handler
        let reassembled_data = {
            let mut handler = self.frame_handler.lock().await;
            handler.process_frame(frame)?
        };

        // Update stream context
        {
            let mut ctx = stream_context.lock().await;
            ctx.update_activity();
            ctx.backlog_size += frame.data.len();
            
            // Add reassembled data to processing queue
            for data in &reassembled_data {
                ctx.processing_queue.push_back(data.clone());
            }
        }

        // Update statistics
        self.update_processing_stats(start_time, frame.data.len(), reassembled_data.len()).await;

        // Emit events
        if !reassembled_data.is_empty() {
            for data in &reassembled_data {
                let _ = self.event_sender.send(FrameProcessingEvent::FrameReassembled {
                    stream_id: data.stream_id,
                    data_size: data.data.len(),
                });
            }
        }

        Ok(reassembled_data)
    }

    /// Apply flow control to incoming frame
    async fn apply_flow_control(&self, stream_id: u32, frame_size: usize) -> Result<(), IntegratedFrameError> {
        // Global flow control check
        {
            let mut global_fc = self.global_flow_controller.lock().await;
            if let Err(e) = global_fc.on_data_received(frame_size as u32) {
                warn!("Global flow control violation: {}", e);
                return Err(IntegratedFrameError::FlowControl(e));
            }
        }

        // Per-stream flow control
        let streams = self.streams.read().await;
        if let Some(stream_context) = streams.get(&stream_id) {
            let mut ctx = stream_context.lock().await;
            if let Err(e) = ctx.flow_controller.on_data_received(frame_size as u32) {
                ctx.state = StreamState::FlowControlBlocked;
                
                let _ = self.event_sender.send(FrameProcessingEvent::FlowControlBlocked { stream_id });
                
                warn!("Stream {} flow control blocked: {}", stream_id, e);
                return Err(IntegratedFrameError::FlowControl(e));
            }
        }

        Ok(())
    }

    /// Get or create stream context
    async fn get_or_create_stream(&self, stream_id: u32) -> Result<Arc<Mutex<StreamContext>>, IntegratedFrameError> {
        let streams = self.streams.read().await;
        
        if let Some(stream_context) = streams.get(&stream_id) {
            return Ok(Arc::clone(stream_context));
        }
        
        drop(streams);

        // Create new stream
        let mut streams = self.streams.write().await;
        
        // Double-check pattern
        if let Some(stream_context) = streams.get(&stream_id) {
            return Ok(Arc::clone(stream_context));
        }

        // Check concurrent stream limit
        if streams.len() >= self.config.max_concurrent_streams {
            error!("Maximum concurrent streams exceeded: {}", streams.len());
            return Err(IntegratedFrameError::ProcessingCapacityExceeded { stream_id });
        }

        // Create new stream context
        let stream_context = Arc::new(Mutex::new(StreamContext::new(
            stream_id,
            self.config.default_stream_window,
            self.config.max_processing_queue_size,
        )));

        streams.insert(stream_id, Arc::clone(&stream_context));
        
        // Emit stream created event
        let _ = self.event_sender.send(FrameProcessingEvent::StreamCreated { stream_id });
        
        info!("Created new stream context for stream {}", stream_id);
        Ok(stream_context)
    }

    /// Start flow control update task
    async fn start_flow_control_updater(&self) {
        let streams = Arc::clone(&self.streams);
        let global_fc = Arc::clone(&self.global_flow_controller);
        let event_sender = self.event_sender.clone();
        let update_interval = self.config.flow_control_update_interval;
        let shutdown_signal = Arc::clone(&self.shutdown_signal);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(update_interval);
            
            loop {
                interval.tick().await;
                
                // Check shutdown signal
                {
                    let shutdown = shutdown_signal.lock().await;
                    if *shutdown {
                        break;
                    }
                }

                // Update flow control for all streams
                let streams_guard = streams.read().await;
                for (stream_id, stream_context) in streams_guard.iter() {
                    let mut ctx = stream_context.lock().await;
                    
                    // Update flow control windows
                    if let stats = ctx.flow_controller.get_stats() {
                        // Check for flow control unblocking
                        if ctx.state == StreamState::FlowControlBlocked && stats.flow_window_size > 0 {
                            ctx.state = StreamState::Active;
                            let _ = event_sender.send(FrameProcessingEvent::FlowControlUnblocked { 
                                stream_id: *stream_id 
                            });
                        }
                        
                        // Check for congestion
                        if stats.congestion_window < stats.flow_window_size / 2 {
                            let _ = event_sender.send(FrameProcessingEvent::CongestionDetected {
                                stream_id: *stream_id,
                                severity: 1.0 - (stats.congestion_window as f64 / stats.flow_window_size as f64),
                            });
                        }
                    }
                }
            }
            
            debug!("Flow control updater task shutdown");
        });
    }

    /// Start stream cleanup task
    async fn start_stream_cleanup_task(&self) {
        let streams = Arc::clone(&self.streams);
        let event_sender = self.event_sender.clone();
        let cleanup_interval = self.config.stream_cleanup_interval;
        let reassembly_timeout = self.config.reassembly_timeout;
        let shutdown_signal = Arc::clone(&self.shutdown_signal);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            
            loop {
                interval.tick().await;
                
                // Check shutdown signal
                {
                    let shutdown = shutdown_signal.lock().await;
                    if *shutdown {
                        break;
                    }
                }

                let mut streams_to_remove = Vec::new();
                
                // Check for inactive streams
                {
                    let streams_guard = streams.read().await;
                    for (stream_id, stream_context) in streams_guard.iter() {
                        let ctx = stream_context.lock().await;
                        
                        if ctx.last_activity.elapsed() > reassembly_timeout {
                            streams_to_remove.push(*stream_id);
                        }
                    }
                }
                
                // Remove inactive streams
                if !streams_to_remove.is_empty() {
                    let mut streams_guard = streams.write().await;
                    for stream_id in streams_to_remove {
                        streams_guard.remove(&stream_id);
                        let _ = event_sender.send(FrameProcessingEvent::StreamClosed {
                            stream_id,
                            reason: "Timeout".to_string(),
                        });
                        debug!("Cleaned up inactive stream {}", stream_id);
                    }
                }
            }
            
            debug!("Stream cleanup task shutdown");
        });
    }

    /// Start statistics update task
    async fn start_statistics_updater(&self) {
        let stats = Arc::clone(&self.stats);
        let streams = Arc::clone(&self.streams);
        let global_fc = Arc::clone(&self.global_flow_controller);
        let shutdown_signal = Arc::clone(&self.shutdown_signal);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                // Check shutdown signal
                {
                    let shutdown = shutdown_signal.lock().await;
                    if *shutdown {
                        break;
                    }
                }

                // Update statistics
                let mut stats_guard = stats.write().await;
                let streams_guard = streams.read().await;
                
                stats_guard.active_streams = streams_guard.len();
                stats_guard.blocked_streams = streams_guard.values()
                    .filter(|ctx| {
                        if let Ok(ctx) = ctx.try_lock() {
                            ctx.is_blocked()
                        } else {
                            false
                        }
                    })
                    .count();

                // Update global flow control stats
                if let Ok(global_fc_guard) = global_fc.try_lock() {
                    if let fc_stats = global_fc_guard.get_stats() {
                        stats_guard.flow_control_stats = fc_stats;
                    }
                }
            }
            
            debug!("Statistics updater task shutdown");
        });
    }

    /// Update processing statistics
    async fn update_processing_stats(&self, start_time: Instant, frame_size: usize, reassembled_count: usize) {
        let processing_latency = start_time.elapsed().as_secs_f64() * 1000.0;
        
        let mut stats = self.stats.write().await;
        stats.total_frames_processed += 1;
        stats.total_frames_reassembled += reassembled_count as u64;
        stats.total_bytes_processed += frame_size as u64;
        
        // Update average processing latency (exponential moving average)
        stats.avg_processing_latency_ms = stats.avg_processing_latency_ms * 0.9 + processing_latency * 0.1;
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> IntegratedStats {
        self.stats.read().await.clone()
    }

    /// Shutdown the integrated frame processor
    pub async fn shutdown(&self) -> Result<(), IntegratedFrameError> {
        info!("Shutting down integrated frame processor");
        
        // Set shutdown signal
        {
            let mut shutdown = self.shutdown_signal.lock().await;
            *shutdown = true;
        }

        // Wait for background tasks to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Close all streams
        let mut streams = self.streams.write().await;
        for (stream_id, _) in streams.drain() {
            let _ = self.event_sender.send(FrameProcessingEvent::StreamClosed {
                stream_id,
                reason: "Shutdown".to_string(),
            });
        }

        info!("Integrated frame processor shutdown complete");
        Ok(())
    }

    /// Force close a specific stream
    pub async fn close_stream(&self, stream_id: u32, reason: String) -> Result<(), IntegratedFrameError> {
        let mut streams = self.streams.write().await;
        if streams.remove(&stream_id).is_some() {
            let _ = self.event_sender.send(FrameProcessingEvent::StreamClosed { stream_id, reason });
            info!("Closed stream {}", stream_id);
        }
        Ok(())
    }

    /// Get stream information
    pub async fn get_stream_info(&self, stream_id: u32) -> Option<(StreamState, FlowControlStats)> {
        let streams = self.streams.read().await;
        if let Some(stream_context) = streams.get(&stream_id) {
            if let Ok(ctx) = stream_context.try_lock() {
                if let fc_stats = ctx.flow_controller.get_stats() {
                    return Some((ctx.state.clone(), fc_stats));
                }
            }
        }
        None
    }

    /// Update stream priority
    pub async fn set_stream_priority(&self, stream_id: u32, priority: u8) -> Result<(), IntegratedFrameError> {
        let streams = self.streams.read().await;
        if let Some(stream_context) = streams.get(&stream_id) {
            let mut ctx = stream_context.lock().await;
            ctx.priority = priority;
            debug!("Updated stream {} priority to {}", stream_id, priority);
            Ok(())
        } else {
            Err(IntegratedFrameError::InvalidStreamState {
                stream_id,
                state: "Not found".to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_integrated_frame_processor_creation() {
        let config = IntegratedFrameConfig::default();
        let (event_sender, _event_receiver) = mpsc::unbounded_channel();
        
        let processor = IntegratedFrameProcessor::new(config, event_sender);
        assert!(processor.start().await.is_ok());
        assert!(processor.shutdown().await.is_ok());
    }

    #[tokio::test]
    async fn test_stream_creation_and_processing() {
        let config = IntegratedFrameConfig::default();
        let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
        
        let processor = IntegratedFrameProcessor::new(config, event_sender);
        assert!(processor.start().await.is_ok());

        // Create a test frame
        let test_frame_data = create_test_frame_data(1, 0, b"test data");
        
        // Process frame
        let result = processor.process_frame(&test_frame_data).await;
        assert!(result.is_ok());

        // Check for stream creation event
        let event = event_receiver.try_recv();
        assert!(matches!(event, Ok(FrameProcessingEvent::StreamCreated { stream_id: 1 })));

        assert!(processor.shutdown().await.is_ok());
    }

    fn create_test_frame_data(stream_id: u32, offset: u32, data: &[u8]) -> Vec<u8> {
        // Simple frame format: [stream_id][offset][data_len][data]
        let mut frame_data = Vec::new();
        frame_data.extend_from_slice(&stream_id.to_be_bytes());
        frame_data.extend_from_slice(&offset.to_be_bytes());
        frame_data.extend_from_slice(&(data.len() as u32).to_be_bytes());
        frame_data.extend_from_slice(data);
        frame_data
    }
}
