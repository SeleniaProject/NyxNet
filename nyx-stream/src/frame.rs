#![forbid(unsafe_code)]

use nom::{bytes::complete::take, number::complete::u8, IResult};

/// Flag bit (within the 6-bit flags field) indicating the presence of an 8-bit `PathID`
/// immediately following the standard 4-byte header. This implements the Multipath
/// extension described in the design document §14.
pub const FLAG_HAS_PATH_ID: u8 = 0x20; // 0b100000

/// Parsed header including optional `path_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedHeader {
    pub hdr: FrameHeader,
    pub path_id: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_type: u8, // 2 bits
    pub flags: u8,      // 6 bits
    pub length: u16,    // 14 bits
}

pub fn parse_header(input: &[u8]) -> IResult<&[u8], FrameHeader> {
    let (input, byte0) = u8(input)?;
    let (input, byte1) = u8(input)?;
    let (input, len_bytes) = take(2u8)(input)?; // length high & low
    let byte0_type = byte0 >> 6;
    let byte0_flags = byte0 & 0x3F;
    let len_high = ((byte1 as u16) << 8) | (len_bytes[0] as u16);
    let length = len_high & 0x3FFF; // 14 bits
    Ok((input, FrameHeader { frame_type: byte0_type, flags: byte0_flags, length }))
}

/// Parse header and optional `PathID` if `FLAG_HAS_PATH_ID` bit set.
pub fn parse_header_ext(input: &[u8]) -> IResult<&[u8], ParsedHeader> {
    let (input, hdr) = parse_header(input)?;
    if hdr.flags & FLAG_HAS_PATH_ID != 0 {
        let (input, pid) = u8(input)?;
        Ok((input, ParsedHeader { hdr, path_id: Some(pid) }))
    } else {
        Ok((input, ParsedHeader { hdr, path_id: None }))
    }
}

#[cfg(test)]
mod tests_ext {
    use super::*;

    #[test]
    fn path_id_parsed_when_flag_set() {
        // frame_type=2 (0b10), flags=0x25 (0x20 path flag + 0x05), length=50, path_id=7
        let bytes = [0xA5u8, 0x00u8, 0x32u8, 0x00u8, 0x07u8];
        let (_, parsed) = parse_header_ext(&bytes).expect("parse");
        assert_eq!(parsed.hdr.frame_type, 2);
        assert_eq!(parsed.hdr.flags & FLAG_HAS_PATH_ID, FLAG_HAS_PATH_ID);
        assert_eq!(parsed.hdr.length, 50);
        assert_eq!(parsed.path_id, Some(7));
    }

    #[test]
    fn no_path_id_when_flag_clear() {
        let bytes = [0x55u8, 0x00u8, 0x64u8, 0x00u8];
        let (_, parsed) = parse_header_ext(&bytes).expect("parse");
        assert!(parsed.path_id.is_none());
    }
} 