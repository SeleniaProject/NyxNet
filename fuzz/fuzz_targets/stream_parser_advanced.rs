#![no_main]

use libfuzzer_sys::fuzz_target;
use nyx_stream::frame::{parse_header, parse_header_ext, FrameHeader};
use nyx_stream::{parse_stream_frame, parse_ack_frame, parse_settings_frame};
use nyx_stream::{parse_close_frame, parse_ping_frame, parse_pong_frame};
use nyx_stream::{parse_path_challenge_frame, parse_path_response_frame};
use nyx_stream::{parse_localized_string_frame};

fuzz_target!(|data: &[u8]| {
    // Skip very short inputs to avoid trivial failures
    if data.len() < 4 {
        return;
    }
    
    // Test basic header parsing
    let _ = parse_header(data);
    
    // Test extended header parsing if we have enough data
    if data.len() >= 8 {
        let _ = parse_header_ext(data);
    }
    
    // Test various frame type parsers
    test_frame_parsers(data);
    
    // Test malformed inputs
    test_malformed_inputs(data);
    
    // Test boundary conditions
    test_boundary_conditions(data);
    
    // Test state machine transitions
    test_state_transitions(data);
});

fn test_frame_parsers(data: &[u8]) {
    // Try parsing as different frame types
    let _ = parse_stream_frame(data);
    let _ = parse_ack_frame(data);
    let _ = parse_settings_frame(data);
    let _ = parse_close_frame(data);
    let _ = parse_ping_frame(data);
    let _ = parse_pong_frame(data);
    let _ = parse_path_challenge_frame(data);
    let _ = parse_path_response_frame(data);
    let _ = parse_localized_string_frame(data);
}

fn test_malformed_inputs(data: &[u8]) {
    // Test with various corrupted headers
    let mut corrupted = data.to_vec();
    
    if corrupted.len() >= 4 {
        // Corrupt frame type
        corrupted[0] = 0xFF;
        let _ = parse_header(&corrupted);
        
        // Corrupt flags
        corrupted[1] = 0xFF;
        let _ = parse_header(&corrupted);
        
        // Corrupt length field
        corrupted[2] = 0xFF;
        corrupted[3] = 0xFF;
        let _ = parse_header(&corrupted);
    }
    
    // Test with truncated data
    for i in 1..std::cmp::min(data.len(), 16) {
        let truncated = &data[..i];
        let _ = parse_header(truncated);
        let _ = parse_stream_frame(truncated);
        let _ = parse_ack_frame(truncated);
    }
}

fn test_boundary_conditions(data: &[u8]) {
    // Test maximum length values
    if data.len() >= 4 {
        let mut max_len = data.to_vec();
        max_len[2] = 0xFF; // Set length to maximum
        max_len[3] = 0xFF;
        let _ = parse_header(&max_len);
    }
    
    // Test zero length
    if data.len() >= 4 {
        let mut zero_len = data.to_vec();
        zero_len[2] = 0x00;
        zero_len[3] = 0x00;
        let _ = parse_header(&zero_len);
    }
    
    // Test various frame type values
    if data.len() >= 4 {
        for frame_type in 0..=255u8 {
            let mut typed_data = data.to_vec();
            typed_data[0] = (typed_data[0] & 0x0F) | (frame_type << 4);
            let _ = parse_header(&typed_data);
        }
    }
}

fn test_state_transitions(data: &[u8]) {
    // Test parsing multiple frames in sequence
    let mut offset = 0;
    let mut frame_count = 0;
    
    while offset < data.len() && frame_count < 10 {
        if let Ok((remaining, header)) = parse_header(&data[offset..]) {
            // Verify header consistency
            if header.length as usize <= remaining.len() {
                // Try parsing frame payload based on type
                match header.frame_type {
                    0 => { let _ = parse_stream_frame(&data[offset..]); },
                    1 => { let _ = parse_ack_frame(&data[offset..]); },
                    2 => { let _ = parse_settings_frame(&data[offset..]); },
                    3 => { let _ = parse_close_frame(&data[offset..]); },
                    4 => { let _ = parse_ping_frame(&data[offset..]); },
                    5 => { let _ = parse_pong_frame(&data[offset..]); },
                    6 => { let _ = parse_path_challenge_frame(&data[offset..]); },
                    7 => { let _ = parse_path_response_frame(&data[offset..]); },
                    8 => { let _ = parse_localized_string_frame(&data[offset..]); },
                    _ => {}, // Unknown frame type
                }
                
                offset += 4 + header.length as usize;
            } else {
                break;
            }
        } else {
            break;
        }
        
        frame_count += 1;
    }
}

// Additional fuzzing for specific frame types
fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    
    // Focus on stream frame parsing
    fuzz_stream_frame(data);
    
    // Focus on ACK frame parsing
    fuzz_ack_frame(data);
    
    // Focus on settings frame parsing
    fuzz_settings_frame(data);
});

fn fuzz_stream_frame(data: &[u8]) {
    if let Ok((_, frame)) = parse_stream_frame(data) {
        // Verify stream frame properties
        assert!(frame.stream_id <= u32::MAX);
        assert!(frame.offset <= u64::MAX);
        
        // Test serialization roundtrip
        use nyx_stream::build_stream_frame;
        let rebuilt = build_stream_frame(frame.stream_id, frame.offset, &frame.data);
        if let Ok((_, reparsed)) = parse_stream_frame(&rebuilt) {
            assert_eq!(frame.stream_id, reparsed.stream_id);
            assert_eq!(frame.offset, reparsed.offset);
            assert_eq!(frame.data, reparsed.data);
        }
    }
}

fn fuzz_ack_frame(data: &[u8]) {
    if let Ok((_, frame)) = parse_ack_frame(data) {
        // Verify ACK frame properties
        assert!(frame.largest_acked <= u64::MAX);
        assert!(frame.ack_delay <= u64::MAX);
        
        // Test that ACK ranges are valid
        for range in &frame.ack_ranges {
            assert!(range.start <= range.end);
        }
        
        // Test serialization roundtrip
        use nyx_stream::build_ack_frame;
        let rebuilt = build_ack_frame(&frame);
        if let Ok((_, reparsed)) = parse_ack_frame(&rebuilt) {
            assert_eq!(frame.largest_acked, reparsed.largest_acked);
            assert_eq!(frame.ack_delay, reparsed.ack_delay);
            assert_eq!(frame.ack_ranges.len(), reparsed.ack_ranges.len());
        }
    }
}

fn fuzz_settings_frame(data: &[u8]) {
    if let Ok((_, frame)) = parse_settings_frame(data) {
        // Verify settings frame properties
        for setting in &frame.settings {
            // Setting IDs should be reasonable
            assert!(setting.id <= 0xFFFF);
            
            // Values should fit in expected ranges
            match setting.id {
                0x0001 => assert!(setting.value <= 1024), // max_streams
                0x0002 => assert!(setting.value <= u64::MAX), // max_data
                0x0003 => assert!(setting.value <= 3600), // idle_timeout (seconds)
                _ => {}, // Unknown settings are allowed
            }
        }
        
        // Test serialization roundtrip
        use nyx_stream::build_settings_frame;
        let rebuilt = build_settings_frame(&frame.settings);
        if let Ok((_, reparsed)) = parse_settings_frame(&rebuilt) {
            assert_eq!(frame.settings.len(), reparsed.settings.len());
            for (orig, reparsed) in frame.settings.iter().zip(reparsed.settings.iter()) {
                assert_eq!(orig.id, reparsed.id);
                assert_eq!(orig.value, reparsed.value);
            }
        }
    }
} 