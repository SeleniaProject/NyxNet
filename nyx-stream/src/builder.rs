#![forbid(unsafe_code)]

use super::frame::FrameHeader;

pub fn build_header(hdr: FrameHeader) -> [u8; 4] {
    let mut out = [0u8; 4];
    out[0] = (hdr.frame_type << 6) | (hdr.flags & 0x3F);
    out[1] = ((hdr.length >> 8) & 0x3F) as u8; // high 6 bits
    out[2] = (hdr.length & 0xFF) as u8;
    out[3] = 0; // reserved
    out
} 