use std::time::Duration;

/// Simple frame handler for performance testing
#[derive(Debug)]
pub struct FrameHandler {
    pub max_frame_size: usize,
    pub timeout: Duration,
}

impl FrameHandler {
    pub fn new(max_frame_size: usize, timeout: Duration) -> Self {
        Self {
            max_frame_size,
            timeout,
        }
    }

    /// Process a frame for performance testing (async compatible)
    pub async fn process_frame_async(&mut self, _stream_id: u64, data: Vec<u8>) -> crate::errors::StreamResult<Option<Vec<u8>>> {
        if data.len() > self.max_frame_size {
            return Err(crate::errors::StreamError::InvalidFrame(
                format!("Frame size {} exceeds maximum {}", data.len(), self.max_frame_size)
            ));
        }

        // Simple processing for performance testing - just return the data
        Ok(Some(data))
    }

    /// Get number of active streams (always 0 for this simple implementation)
    pub fn active_streams(&self) -> usize {
        0
    }

    /// Clean up expired streams (no-op for this simple implementation)
    pub fn cleanup_expired_streams(&mut self) {
        // No-op
    }

    /// Close a stream (no-op for this simple implementation)
    pub fn close_stream(&mut self, _stream_id: u64) {
        // No-op
    }

    /// Get stream statistics (dummy implementation)
    pub fn get_stream_stats(&self, _stream_id: u64) -> Option<(u64, usize, usize)> {
        None
    }
}

impl Default for FrameHandler {
    fn default() -> Self {
        Self::new(16384, Duration::from_secs(60))
    }
}
