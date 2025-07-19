#![forbid(unsafe_code)]

use nom::{number::complete::{be_u32, u8}, bytes::complete::take, IResult};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StreamFrame<'a> {
    pub stream_id: u32,
    pub offset: u32,
    pub fin: bool,
    pub data: &'a [u8],
}

pub fn parse_stream_frame<'a>(input: &'a [u8]) -> IResult<&'a [u8], StreamFrame<'a>> {
    let (input, stream_id) = be_u32(input)?;
    let (input, offset) = be_u32(input)?;
    let (input, fin_byte) = u8(input)?;
    let fin = fin_byte != 0;
    let (input, data_len) = be_u32(input)?;
    let (input, data) = take(data_len)(input)?;
    Ok((input, StreamFrame { stream_id, offset, fin, data }))
}

pub fn build_stream_frame(frame: &StreamFrame) -> Vec<u8> {
    let mut v = Vec::with_capacity(13 + frame.data.len());
    v.extend_from_slice(&frame.stream_id.to_be_bytes());
    v.extend_from_slice(&frame.offset.to_be_bytes());
    v.push(if frame.fin {1} else {0});
    v.extend_from_slice(&(frame.data.len() as u32).to_be_bytes());
    v.extend_from_slice(frame.data);
    v
} 