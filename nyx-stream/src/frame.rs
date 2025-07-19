#![forbid(unsafe_code)]

use nom::{bytes::complete::take, number::complete::u8, IResult};

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