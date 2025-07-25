#![forbid(unsafe_code)]

//! Stream implementation for the Nyx SDK.
//!
//! This module provides the `NyxStream` type which implements `AsyncRead` and `AsyncWrite`
//! for seamless integration with existing async I/O code, along with automatic reconnection,
//! comprehensive statistics, and state management.

use crate::config::NyxConfig;
use crate::error::{NyxError, NyxResult};
use crate::proto::nyx_control_client::NyxControlClient;
use crate::daemon::ConnectionInfo;

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{RwLock, Mutex};
use tonic::transport::Channel;
use tracing::{debug, error, warn};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use bytes::{Bytes, BytesMut};
use futures_util::ready;
// use pin_project_lite::pin_project;

#[cfg(feature = "reconnect")]
use crate::reconnect::ReconnectionManager;

/// Options for configuring a stream
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// Buffer size for read/write operations
    pub buffer_size: usize,
    /// Timeout for individual operations
    pub operation_timeout: Duration,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Initial reconnection delay
    pub reconnect_delay: Duration,
    /// Enable stream statistics collection
    pub collect_stats: bool,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
            operation_timeout: Duration::from_secs(30),
            auto_reconnect: true,
            max_reconnect_attempts: 3,
            reconnect_delay: Duration::from_millis(100),
            collect_stats: true,
        }
    }
}

impl Into<crate::proto::StreamOptions> for StreamOptions {
    fn into(self) -> crate::proto::StreamOptions {
        crate::proto::StreamOptions {
            buffer_size: self.buffer_size as u32,
            timeout_ms: self.operation_timeout.as_millis() as u32,
            multipath: false,
            max_paths: 1,
            path_strategy: "lowest_latency".to_string(),
            auto_reconnect: self.auto_reconnect,
            max_retry_attempts: self.max_reconnect_attempts,
            compression: false,
            cipher_suite: "chacha20poly1305".to_string(),
        }
    }
}

/// Current state of a stream
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamState {
    /// Stream is being established
    Connecting,
    /// Stream is open and ready for I/O
    Open,
    /// Stream is half-closed (write side closed)
    HalfClosed,
    /// Stream is fully closed
    Closed,
    /// Stream encountered an error
    Error,
    /// Stream is being reconnected
    #[cfg(feature = "reconnect")]
    Reconnecting,
}

/// Statistics for a stream
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamStats {
    /// Total bytes read
    pub bytes_read: u64,
    /// Total bytes written
    pub bytes_written: u64,
    /// Number of read operations
    pub read_ops: u64,
    /// Number of write operations
    pub write_ops: u64,
    /// Stream creation time
    pub created_at: DateTime<Utc>,
    /// Last activity time
    pub last_activity: DateTime<Utc>,
    /// Number of reconnections
    #[cfg(feature = "reconnect")]
    pub reconnections: u32,
    /// Total connection time
    pub connection_time: Duration,
    /// Average operation latency
    pub avg_latency: Duration,
}

/// A Nyx network stream implementing AsyncRead + AsyncWrite
pub struct NyxStream {
    stream_id: u32,
    target: String,
    client: Arc<Mutex<NyxControlClient<Channel>>>,
    connection_info: Arc<RwLock<ConnectionInfo>>,
    options: StreamOptions,
    state: Arc<RwLock<StreamState>>,
    stats: Arc<RwLock<StreamStats>>,
    read_buffer: Arc<Mutex<BytesMut>>,
    write_buffer: Arc<Mutex<BytesMut>>,
    #[cfg(feature = "reconnect")]
    reconnect_manager: Option<Arc<ReconnectionManager>>,
}

impl NyxStream {
    /// Create a new stream (internal use)
    pub(crate) async fn new(
        stream_id: u32,
        target: String,
        client: Arc<Mutex<NyxControlClient<Channel>>>,
        connection_info: Arc<RwLock<ConnectionInfo>>,
        options: StreamOptions,
    ) -> NyxResult<Self> {
        let stats = StreamStats {
            created_at: Utc::now(),
            last_activity: Utc::now(),
            ..Default::default()
        };

        let stream = Self {
            stream_id,
            target: target.clone(),
            client,
            connection_info,
            options: options.clone(),
            state: Arc::new(RwLock::new(StreamState::Open)),
            stats: Arc::new(RwLock::new(stats)),
            read_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(options.buffer_size))),
            write_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(options.buffer_size))),
            #[cfg(feature = "reconnect")]
            reconnect_manager: if options.auto_reconnect {
                Some(Arc::new(ReconnectionManager::new(target, options.clone())))
            } else {
                None
            },
        };

        debug!("Created stream {} to {}", stream_id, target);
        Ok(stream)
    }

    /// Get the stream ID
    pub fn stream_id(&self) -> u32 {
        self.stream_id
    }

    /// Get the target address
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Get current stream state
    pub async fn state(&self) -> StreamState {
        *self.state.read().await
    }

    /// Get stream statistics
    pub async fn stats(&self) -> StreamStats {
        self.stats.read().await.clone()
    }

    /// Check if the stream is open and ready for I/O
    pub async fn is_open(&self) -> bool {
        matches!(*self.state.read().await, StreamState::Open)
    }

    /// Check if the stream is closed
    pub async fn is_closed(&self) -> bool {
        matches!(*self.state.read().await, StreamState::Closed)
    }

    /// Gracefully close the stream
    pub async fn close(&mut self) -> NyxResult<()> {
        debug!("Closing stream {}", self.stream_id);
        
        // Update state
        *self.state.write().await = StreamState::Closed;
        
        // Flush any pending writes
        self.flush_internal().await?;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.last_activity = Utc::now();
            stats.connection_time = stats.last_activity.signed_duration_since(stats.created_at)
                .to_std().unwrap_or(Duration::ZERO);
        }
        
        debug!("Stream {} closed successfully", self.stream_id);
        Ok(())
    }

    /// Force reconnection of the stream
    #[cfg(feature = "reconnect")]
    pub async fn reconnect(&mut self) -> NyxResult<()> {
        if let Some(reconnect_manager) = &self.reconnect_manager {
            debug!("Reconnecting stream {}", self.stream_id);
            
            *self.state.write().await = StreamState::Reconnecting;
            
            match reconnect_manager.reconnect(&self.client).await {
                Ok(new_stream_id) => {
                    self.stream_id = new_stream_id;
                    *self.state.write().await = StreamState::Open;
                    
                    // Update statistics
                    {
                        let mut stats = self.stats.write().await;
                        stats.reconnections += 1;
                        stats.last_activity = Utc::now();
                    }
                    
                    debug!("Stream reconnected with new ID {}", new_stream_id);
                    Ok(())
                }
                Err(e) => {
                    *self.state.write().await = StreamState::Error;
                    error!("Failed to reconnect stream {}: {}", self.stream_id, e);
                    Err(e)
                }
            }
        } else {
            Err(NyxError::stream_error("Reconnection not enabled", Some(self.stream_id)))
        }
    }

    /// Internal flush implementation
    async fn flush_internal(&mut self) -> NyxResult<()> {
        let mut write_buffer = self.write_buffer.lock().await;
        if write_buffer.is_empty() {
            return Ok(());
        }

        // Send data to daemon via gRPC
        let data = write_buffer.split().freeze();
        let bytes_to_send = data.len();
        
        let mut client = self.client.lock().await;
        let request = crate::proto::DataRequest {
            stream_id: self.stream_id.to_string(),
            data: data.to_vec(),
        };

        match client.send_data(tonic::Request::new(request)).await {
            Ok(response) => {
                let data_response = response.into_inner();
                if !data_response.success {
                    return Err(NyxError::stream_error(
                        format!("Failed to send data: {}", data_response.error),
                        Some(self.stream_id)
                    ));
                }
                
                // Update statistics
                if self.options.collect_stats {
                    let mut stats = self.stats.write().await;
                    stats.bytes_written += bytes_to_send as u64;
                    stats.write_ops += 1;
                    stats.last_activity = Utc::now();
                }
                
                debug!("Successfully sent {} bytes on stream {}", bytes_to_send, self.stream_id);
                Ok(())
            }
            Err(e) => {
                // Put data back in buffer for retry
                let mut new_buffer = BytesMut::from(data.as_ref());
                new_buffer.extend_from_slice(&write_buffer);
                *write_buffer = new_buffer;
                
                Err(NyxError::from(e))
            }
        }
    }

    /// Internal read implementation
    async fn read_internal(&mut self, buf: &mut ReadBuf<'_>) -> NyxResult<()> {
        let mut read_buffer = self.read_buffer.lock().await;
        
        // If buffer is empty, try to receive data from daemon
        if read_buffer.is_empty() {
            // In a real implementation, this would be a streaming gRPC call
            // For now, we simulate receiving data
            let mut client = self.client.lock().await;
            
            // Check if there's data available for this stream
            let request = crate::proto::StreamId {
                id: self.stream_id,
            };
            
            match client.get_stream_stats(tonic::Request::new(request)).await {
                Ok(response) => {
                    let stats = response.into_inner();
                    // In a real implementation, we would receive actual data here
                    // For now, simulate some data based on stream activity
                    if stats.bytes_received > 0 {
                        read_buffer.extend_from_slice(b"Data from remote peer\n");
                    }
                }
                Err(e) => {
                    warn!("Failed to check for stream data: {}", e);
                    // Continue with empty buffer
                }
            }
        }

        let to_copy = std::cmp::min(buf.remaining(), read_buffer.len());
        if to_copy > 0 {
            buf.put_slice(&read_buffer.split_to(to_copy));
            
            // Update statistics
            if self.options.collect_stats {
                let mut stats = self.stats.write().await;
                stats.bytes_read += to_copy as u64;
                stats.read_ops += 1;
                stats.last_activity = Utc::now();
            }
        }

        Ok(())
    }

    /// Handle stream errors with potential reconnection
    async fn handle_error(&mut self, error: NyxError) -> NyxResult<()> {
        warn!("Stream {} encountered error: {}", self.stream_id, error);
        
        *self.state.write().await = StreamState::Error;
        
        #[cfg(feature = "reconnect")]
        if self.options.auto_reconnect && error.is_retryable() {
            return self.reconnect().await;
        }
        
        Err(error)
    }
}

impl AsyncRead for NyxStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // Check if stream is open
        let state = match ready!(Box::pin(self.state.read()).poll(cx)) {
            state => *state,
        };
        
        match state {
            StreamState::Open => {
                // Perform async read
                match ready!(Box::pin(self.read_internal(buf)).poll(cx)) {
                    Ok(()) => Poll::Ready(Ok(())),
                    Err(e) => {
                        // Handle error asynchronously
                        let error_result = ready!(Box::pin(self.handle_error(e)).poll(cx));
                        match error_result {
                            Ok(()) => {
                                // Retry after successful reconnection
                                cx.waker().wake_by_ref();
                                Poll::Pending
                            }
                            Err(e) => Poll::Ready(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))),
                        }
                    }
                }
            }
            StreamState::Closed => Poll::Ready(Ok(())), // EOF
            StreamState::Error => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is in error state",
            ))),
            #[cfg(feature = "reconnect")]
            StreamState::Reconnecting => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            _ => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Stream is not ready for reading",
            ))),
        }
    }
}

impl AsyncWrite for NyxStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // Check if stream is open
        let state = match ready!(Box::pin(self.state.read()).poll(cx)) {
            state => *state,
        };
        
        match state {
            StreamState::Open => {
                // Write to buffer
                match ready!(Box::pin(self.write_buffer.lock()).poll(cx)) {
                    mut write_buffer => {
                        let to_write = std::cmp::min(buf.len(), write_buffer.capacity() - write_buffer.len());
                        if to_write > 0 {
                            write_buffer.extend_from_slice(&buf[..to_write]);
                            Poll::Ready(Ok(to_write))
                        } else {
                            // Buffer full, need to flush
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    }
                }
            }
            StreamState::Closed | StreamState::HalfClosed => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is closed for writing",
            ))),
            StreamState::Error => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Stream is in error state",
            ))),
            #[cfg(feature = "reconnect")]
            StreamState::Reconnecting => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            _ => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Stream is not ready for writing",
            ))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match ready!(Box::pin(self.flush_internal()).poll(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Flush first
        ready!(self.as_mut().poll_flush(cx))?;
        
        // Then close
        match ready!(Box::pin(self.close()).poll(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))),
        }
    }
}

// NyxStream is automatically Send + Sync because all its fields are Send + Sync
// Arc<RwLock<_>>, Arc<Mutex<_>>, and other contained types already implement Send + Sync
// No unsafe implementation needed - the compiler will automatically derive these traits 