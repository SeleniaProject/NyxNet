#![forbid(unsafe_code)]

use std::collections::{HashMap, VecDeque};
use bytes::{Bytes, BytesMut};
use tracing::{debug, warn, error};

use crate::stream_frame::{StreamFrame, parse_stream_frame};

/// Errors that can occur during frame handling
#[derive(Debug, thiserror::Error)]
pub enum FrameHandlerError {
    #[error("Invalid frame: {0}")]
    InvalidFrame(String),
    #[error("Frame parsing error: {0}")]
    FrameParsing(String),
    #[error("Duplicate frame detected: stream_id={0}, offset={1}")]
    DuplicateFrame(u32, u32),
    #[error("Frame out of order: expected offset {expected}, got {actual}")]
    OutOfOrder { expected: u32, actual: u32 },
    #[error("Buffer overflow for stream {0}")]
    BufferOverflow(u32),
    #[error("Stream {0} not found")]
    StreamNotFound(u32),
}

/// Frame validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameValidation {
    Valid,
    Duplicate,
    OutOfOrder,
    Invalid(String),
}

/// Reassembled frame data with metadata
#[derive(Debug, Clone)]
pub struct ReassembledData {
    pub stream_id: u32,
    pub data: Bytes,
    pub is_complete: bool,
    pub has_fin: bool,
}

/// Per-stream frame reassembly state
#[derive(Debug)]
struct StreamReassembly {
    stream_id: u32,
    expected_offset: u32,
    frames: VecDeque<StreamFrame<'static>>,
    total_buffered: usize,
    max_buffer_size: usize,
    has_fin: bool,
    fin_offset: Option<u32>,
}

impl StreamReassembly {
    fn new(stream_id: u32, max_buffer_size: usize) -> Self {
        Self {
            stream_id,
            expected_offset: 0,
            frames: VecDeque::new(),
            total_buffered: 0,
            max_buffer_size,
            has_fin: false,
            fin_offset: None,
        }
    }

    fn add_frame(&mut self, frame: StreamFrame<'static>) -> Result<(), FrameHandlerError> {
        // Check buffer overflow
        if self.total_buffered + frame.data.len() > self.max_buffer_size {
            return Err(FrameHandlerError::BufferOverflow(self.stream_id));
        }

        // Check for duplicate frames
        if self.frames.iter().any(|f| f.offset == frame.offset) {
            return Err(FrameHandlerError::DuplicateFrame(self.stream_id, frame.offset));
        }

        // Handle FIN frame
        if frame.fin {
            if self.has_fin {
                // Multiple FIN frames - this is an error
                return Err(FrameHandlerError::InvalidFrame(
                    format!("Multiple FIN frames for stream {}", self.stream_id)
                ));
            }
            self.has_fin = true;
            self.fin_offset = Some(frame.offset + frame.data.len() as u32);
        }

        // Insert frame in order by offset
        let mut insert_pos = self.frames.len();
        for (i, existing) in self.frames.iter().enumerate() {
            if frame.offset < existing.offset {
                insert_pos = i;
                break;
            }
        }

        let data_len = frame.data.len();
        let offset = frame.offset;
        self.total_buffered += data_len;
        self.frames.insert(insert_pos, frame);
        
        debug!("Added frame to stream {}: offset={}, data_len={}, total_frames={}", 
               self.stream_id, offset, data_len, self.frames.len());

        Ok(())
    }

    fn get_contiguous_data(&mut self) -> Option<ReassembledData> {
        if self.frames.is_empty() {
            return None;
        }

        let mut data = BytesMut::new();
        let mut current_offset = self.expected_offset;
        let mut has_fin_in_data = false;

        // Collect contiguous frames
        while let Some(frame) = self.frames.front() {
            if frame.offset != current_offset {
                break;
            }

            let frame = self.frames.pop_front().unwrap();
            data.extend_from_slice(frame.data);
            current_offset += frame.data.len() as u32;
            self.total_buffered -= frame.data.len();

            if frame.fin {
                has_fin_in_data = true;
                break;
            }
        }

        if data.is_empty() {
            return None;
        }

        self.expected_offset = current_offset;
        
        // Check if stream is complete
        let is_complete = has_fin_in_data || 
            (self.has_fin && self.fin_offset == Some(current_offset));

        debug!("Reassembled {} bytes for stream {}, complete: {}, has_fin: {}", 
               data.len(), self.stream_id, is_complete, has_fin_in_data);

        Some(ReassembledData {
            stream_id: self.stream_id,
            data: data.freeze(),
            is_complete,
            has_fin: has_fin_in_data,
        })
    }

    fn is_complete(&self) -> bool {
        self.has_fin && 
        self.frames.is_empty() && 
        self.fin_offset == Some(self.expected_offset)
    }

    fn validate_frame(&self, frame: &StreamFrame) -> FrameValidation {
        // Check for duplicates
        if self.frames.iter().any(|f| f.offset == frame.offset) {
            return FrameValidation::Duplicate;
        }

        // Check if frame is too far ahead (potential out of order)
        if frame.offset > self.expected_offset + (self.max_buffer_size as u32) {
            return FrameValidation::OutOfOrder;
        }

        // Check for multiple FIN frames
        if frame.fin && self.has_fin {
            return FrameValidation::Invalid("Multiple FIN frames".to_string());
        }

        // Check frame integrity
        if frame.data.is_empty() && !frame.fin {
            return FrameValidation::Invalid("Empty non-FIN frame".to_string());
        }

        FrameValidation::Valid
    }
}

/// Frame handler for STREAM frame processing
pub struct FrameHandler {
    streams: HashMap<u32, StreamReassembly>,
    max_streams: usize,
    max_buffer_per_stream: usize,
    total_buffered: usize,
    max_total_buffer: usize,
}

impl FrameHandler {
    pub fn new(
        max_streams: usize,
        max_buffer_per_stream: usize,
        max_total_buffer: usize,
    ) -> Self {
        Self {
            streams: HashMap::new(),
            max_streams,
            max_buffer_per_stream,
            total_buffered: 0,
            max_total_buffer,
        }
    }

    /// Process incoming frame data and return reassembled data if available
    pub fn handle_frame(&mut self, frame_data: &[u8]) -> Result<Option<ReassembledData>, FrameHandlerError> {
        // Parse the frame
        let frame = parse_stream_frame(frame_data)
            .map_err(|e| FrameHandlerError::FrameParsing(format!("Failed to parse frame: {:?}", e)))?
            .1;

        debug!("Handling frame: stream_id={}, offset={}, fin={}, data_len={}", 
               frame.stream_id, frame.offset, frame.fin, frame.data.len());

        // Convert to owned frame with 'static lifetime
        let static_frame = StreamFrame {
            stream_id: frame.stream_id,
            offset: frame.offset,
            fin: frame.fin,
            data: Box::leak(frame.data.to_vec().into_boxed_slice()),
        };

        self.process_frame(static_frame)
    }

    /// Process a parsed frame
    pub fn process_frame(&mut self, frame: StreamFrame<'static>) -> Result<Option<ReassembledData>, FrameHandlerError> {
        let stream_id = frame.stream_id;

        // Check global buffer limits
        if self.total_buffered + frame.data.len() > self.max_total_buffer {
            return Err(FrameHandlerError::BufferOverflow(stream_id));
        }

        // Get or create stream reassembly state
        if !self.streams.contains_key(&stream_id) {
            if self.streams.len() >= self.max_streams {
                return Err(FrameHandlerError::InvalidFrame(
                    format!("Too many streams, max: {}", self.max_streams)
                ));
            }
            self.streams.insert(stream_id, StreamReassembly::new(stream_id, self.max_buffer_per_stream));
        }

        let stream = self.streams.get_mut(&stream_id).unwrap();

        // Validate frame
        match stream.validate_frame(&frame) {
            FrameValidation::Valid => {},
            FrameValidation::Duplicate => {
                warn!("Ignoring duplicate frame: stream_id={}, offset={}", stream_id, frame.offset);
                return Ok(None);
            },
            FrameValidation::OutOfOrder => {
                return Err(FrameHandlerError::OutOfOrder {
                    expected: stream.expected_offset,
                    actual: frame.offset,
                });
            },
            FrameValidation::Invalid(reason) => {
                return Err(FrameHandlerError::InvalidFrame(reason));
            },
        }

        // Add frame to stream
        let data_len = frame.data.len();
        stream.add_frame(frame)?;
        self.total_buffered += data_len;

        // Try to get reassembled data
        let result = stream.get_contiguous_data();
        
        // Update total buffered count
        if let Some(ref data) = result {
            self.total_buffered = self.total_buffered.saturating_sub(data.data.len());
        }

        // Clean up completed streams
        if stream.is_complete() {
            debug!("Stream {} is complete, removing", stream_id);
            self.streams.remove(&stream_id);
        }

        Ok(result)
    }

    /// Get reassembled data for a specific stream if available
    pub fn get_stream_data(&mut self, stream_id: u32) -> Result<Option<ReassembledData>, FrameHandlerError> {
        let stream = self.streams.get_mut(&stream_id)
            .ok_or(FrameHandlerError::StreamNotFound(stream_id))?;

        let result = stream.get_contiguous_data();
        
        // Update total buffered count
        if let Some(ref data) = result {
            self.total_buffered = self.total_buffered.saturating_sub(data.data.len());
        }

        // Clean up completed streams
        if stream.is_complete() {
            debug!("Stream {} is complete, removing", stream_id);
            self.streams.remove(&stream_id);
        }

        Ok(result)
    }

    /// Check if a stream is complete
    pub fn is_stream_complete(&self, stream_id: u32) -> bool {
        self.streams.get(&stream_id)
            .map(|s| s.is_complete())
            .unwrap_or(false)
    }

    /// Get the number of active streams
    pub fn active_streams(&self) -> usize {
        self.streams.len()
    }

    /// Get total buffered data size
    pub fn total_buffered(&self) -> usize {
        self.total_buffered
    }

    /// Validate frame integrity without processing
    pub fn validate_frame_data(&self, frame_data: &[u8]) -> Result<FrameValidation, FrameHandlerError> {
        let frame = parse_stream_frame(frame_data)
            .map_err(|e| FrameHandlerError::FrameParsing(format!("Failed to parse frame: {:?}", e)))?
            .1;

        if let Some(stream) = self.streams.get(&frame.stream_id) {
            Ok(stream.validate_frame(&frame))
        } else {
            // New stream
            if frame.offset != 0 {
                Ok(FrameValidation::OutOfOrder)
            } else {
                Ok(FrameValidation::Valid)
            }
        }
    }

    /// Remove a stream and free its resources
    pub fn remove_stream(&mut self, stream_id: u32) -> bool {
        if let Some(stream) = self.streams.remove(&stream_id) {
            self.total_buffered = self.total_buffered.saturating_sub(stream.total_buffered);
            debug!("Removed stream {}, freed {} bytes", stream_id, stream.total_buffered);
            true
        } else {
            false
        }
    }

    /// Get statistics about frame handling
    pub fn get_stats(&self) -> FrameHandlerStats {
        let mut total_frames = 0;
        let mut total_pending = 0;
        
        for stream in self.streams.values() {
            total_frames += stream.frames.len();
            total_pending += stream.total_buffered;
        }

        FrameHandlerStats {
            active_streams: self.streams.len(),
            total_frames,
            total_buffered: self.total_buffered,
            total_pending,
        }
    }
}

/// Statistics about frame handler state
#[derive(Debug, Clone)]
pub struct FrameHandlerStats {
    pub active_streams: usize,
    pub total_frames: usize,
    pub total_buffered: usize,
    pub total_pending: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_frame::build_stream_frame;

    #[test]
    fn test_frame_handler_basic() {
        let mut handler = FrameHandler::new(10, 4096, 40960);
        
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: b"Hello",
        };
        let frame_bytes = build_stream_frame(&frame);
        
        let result = handler.handle_frame(&frame_bytes).unwrap();
        assert!(result.is_some());
        
        let data = result.unwrap();
        assert_eq!(data.stream_id, 1);
        assert_eq!(data.data, b"Hello"[..]);
        assert!(!data.is_complete);
        assert!(!data.has_fin);
    }

    #[test]
    fn test_frame_reassembly() {
        let mut handler = FrameHandler::new(10, 4096, 40960);
        
        // Send frames out of order
        let frame2 = StreamFrame {
            stream_id: 1,
            offset: 5,
            fin: false,
            data: b"World",
        };
        let frame2_bytes = build_stream_frame(&frame2);
        
        // First frame should not produce data (out of order)
        let result = handler.handle_frame(&frame2_bytes).unwrap();
        assert!(result.is_none());
        
        let frame1 = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: b"Hello",
        };
        let frame1_bytes = build_stream_frame(&frame1);
        
        // Second frame should produce reassembled data
        let result = handler.handle_frame(&frame1_bytes).unwrap();
        assert!(result.is_some());
        
        let data = result.unwrap();
        assert_eq!(data.data, b"HelloWorld"[..]);
    }

    #[test]
    fn test_duplicate_frame_detection() {
        let mut handler = FrameHandler::new(10, 4096, 40960);
        
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: b"Hello",
        };
        let frame_bytes = build_stream_frame(&frame);
        
        // First frame should succeed
        let result = handler.handle_frame(&frame_bytes).unwrap();
        assert!(result.is_some());
        
        // Duplicate frame should be ignored
        let result = handler.handle_frame(&frame_bytes).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_fin_frame_handling() {
        let mut handler = FrameHandler::new(10, 4096, 40960);
        
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: true,
            data: b"Final",
        };
        let frame_bytes = build_stream_frame(&frame);
        
        let result = handler.handle_frame(&frame_bytes).unwrap();
        assert!(result.is_some());
        
        let data = result.unwrap();
        assert_eq!(data.data, b"Final"[..]);
        assert!(data.is_complete);
        assert!(data.has_fin);
        
        // Stream should be removed after completion
        assert_eq!(handler.active_streams(), 0);
    }

    #[test]
    fn test_buffer_overflow() {
        let mut handler = FrameHandler::new(10, 100, 1000);
        
        let large_data = vec![b'A'; 200];
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: &large_data,
        };
        let frame_bytes = build_stream_frame(&frame);
        
        let result = handler.handle_frame(&frame_bytes);
        assert!(matches!(result, Err(FrameHandlerError::BufferOverflow(_))));
    }

    #[test]
    fn test_frame_validation() {
        let handler = FrameHandler::new(10, 4096, 40960);
        
        let frame = StreamFrame {
            stream_id: 1,
            offset: 0,
            fin: false,
            data: b"Hello",
        };
        let frame_bytes = build_stream_frame(&frame);
        
        let validation = handler.validate_frame_data(&frame_bytes).unwrap();
        assert_eq!(validation, FrameValidation::Valid);
        
        // Test out of order frame for new stream
        let frame_ooo = StreamFrame {
            stream_id: 2,
            offset: 100,
            fin: false,
            data: b"Hello",
        };
        let frame_ooo_bytes = build_stream_frame(&frame_ooo);
        
        let validation = handler.validate_frame_data(&frame_ooo_bytes).unwrap();
        assert_eq!(validation, FrameValidation::OutOfOrder);
    }
}