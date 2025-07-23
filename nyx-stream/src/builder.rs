#![forbid(unsafe_code)]

use super::frame::FrameHeader;

pub use super::frame::FLAG_HAS_PATH_ID;

pub fn build_header(hdr: FrameHeader) -> [u8; 4] {
    let mut out = [0u8; 4];
    out[0] = (hdr.frame_type << 6) | (hdr.flags & 0x3F);
    out[1] = ((hdr.length >> 8) & 0x3F) as u8; // high 6 bits
    out[2] = (hdr.length & 0xFF) as u8;
    out[3] = 0; // reserved
    out
}

/// Build header with optional PathID. If `path_id` is Some, sets FLAG_HAS_PATH_ID and
/// appends the additional byte. Returns a Vec<u8> containing the header bytes.
pub fn build_header_ext(hdr: FrameHeader, path_id: Option<u8>) -> Vec<u8> {
    let header = build_header(hdr);
    let mut out = Vec::with_capacity(5);
    out.extend_from_slice(&header);
    if let Some(pid) = path_id {
        out[0] |= FLAG_HAS_PATH_ID; // set flag
        out.push(pid);
    }
    out
} 