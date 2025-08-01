// frame_handler_tests.rs - Frame handler tests
#[cfg(test)]
mod tests {
    use super::super::frame_handler::*;
    use bytes::Bytes;

    #[tokio::test]
    async fn test_frame_creation() {
        let data = Bytes::from("test data");
        let frame = Frame::new(1, 0, data.clone());
        
        assert_eq!(frame.stream_id, 1);
        assert_eq!(frame.sequence, 0);
        assert_eq!(frame.data, data);
    }

    #[tokio::test]
    async fn test_frame_serialization() {
        let data = Bytes::from("test data");
        let frame = Frame::new(1, 0, data.clone());
        
        let serialized = frame.serialize();
        assert!(!serialized.is_empty());
        
        let deserialized = Frame::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.stream_id, frame.stream_id);
        assert_eq!(deserialized.sequence, frame.sequence);
        assert_eq!(deserialized.data, frame.data);
    }
}
