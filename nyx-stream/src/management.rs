#![forbid(unsafe_code)]

//! Management frame definitions (PING/PONG, SETTINGS, etc.) according to Nyx Protocol §16.
//! Currently implements PING (0x31) and PONG (0x32) frames.

use nom::{IResult, number::complete::be_u64};
use nom::{bytes::complete::take, number::complete::{be_u16, be_u32, u8 as parse_u8}};
use std::vec::Vec;

/// PING frame (Type=0x31).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingFrame {
    /// Random nonce echoed back by the peer.
    pub nonce: u64,
}

/// PONG frame (Type=0x32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PongFrame {
    /// Nonce copied from corresponding PING frame.
    pub nonce: u64,
}

/// Parse PING payload (8-byte big-endian nonce).
pub fn parse_ping_frame(input: &[u8]) -> IResult<&[u8], PingFrame> {
    let (input, nonce) = be_u64(input)?;
    Ok((input, PingFrame { nonce }))
}

/// Build PING payload (8-byte big-endian nonce).
pub fn build_ping_frame(frame: &PingFrame) -> [u8; 8] {
    frame.nonce.to_be_bytes()
}

/// Parse PONG payload (8-byte big-endian nonce).
pub fn parse_pong_frame(input: &[u8]) -> IResult<&[u8], PongFrame> {
    let (input, nonce) = be_u64(input)?;
    Ok((input, PongFrame { nonce }))
}

/// Build PONG payload (8-byte big-endian nonce).
pub fn build_pong_frame(frame: &PongFrame) -> [u8; 8] {
    frame.nonce.to_be_bytes()
}

/// CLOSE frame (Type=0x3F).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame<'a> {
    /// Application-defined error code.
    pub code: u16,
    /// Optional human-readable reason string (UTF-8).
    pub reason: &'a [u8],
}

/// Build CLOSE frame payload.
pub fn build_close_frame(code: u16, reason: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + reason.len());
    out.extend_from_slice(&code.to_be_bytes());
    out.push(reason.len() as u8);
    out.extend_from_slice(reason);
    out
}

/// Parse CLOSE frame payload.
pub fn parse_close_frame(input: &[u8]) -> IResult<&[u8], CloseFrame> {
    let (input, code) = be_u16(input)?;
    let (input, len) = parse_u8(input)?;
    let (input, reason_bytes) = take(len)(input)?;
    Ok((input, CloseFrame { code, reason: reason_bytes }))
}

/// PATH_CHALLENGE / PATH_RESPONSE token size (128-bit).
const TOKEN_LEN: usize = 16;

/// PATH_CHALLENGE frame (Type=0x33).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathChallengeFrame {
    pub token: [u8; TOKEN_LEN],
}

/// PATH_RESPONSE frame (Type=0x34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathResponseFrame {
    pub token: [u8; TOKEN_LEN],
}

/// Build PATH_CHALLENGE payload.
pub fn build_path_challenge_frame(token: &[u8; TOKEN_LEN]) -> [u8; TOKEN_LEN] { *token }

/// Build PATH_RESPONSE payload.
pub fn build_path_response_frame(token: &[u8; TOKEN_LEN]) -> [u8; TOKEN_LEN] { *token }

/// Parse PATH_CHALLENGE payload.
pub fn parse_path_challenge_frame(input: &[u8]) -> IResult<&[u8], PathChallengeFrame> {
    let (input, bytes) = take(TOKEN_LEN)(input)?;
    let mut token = [0u8; TOKEN_LEN];
    token.copy_from_slice(bytes);
    Ok((input, PathChallengeFrame { token }))
}

/// Parse PATH_RESPONSE payload.
pub fn parse_path_response_frame(input: &[u8]) -> IResult<&[u8], PathResponseFrame> {
    let (input, bytes) = take(TOKEN_LEN)(input)?;
    let mut token = [0u8; TOKEN_LEN];
    token.copy_from_slice(bytes);
    Ok((input, PathResponseFrame { token }))
}

/// SETTINGS frame (Type=0x30).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub id: u16,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFrame {
    pub settings: Vec<Setting>,
}

/// Build SETTINGS payload (concatenated TLVs).
pub fn build_settings_frame(settings: &[Setting]) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::with_capacity(settings.len() * 6);
    for s in settings {
        v.extend_from_slice(&s.id.to_be_bytes());
        v.extend_from_slice(&s.value.to_be_bytes());
    }
    v
}

/// Parse SETTINGS payload into vector.
pub fn parse_settings_frame(input: &[u8]) -> IResult<&[u8], SettingsFrame> {
    let mut rest = input;
    let mut list: Vec<Setting> = Vec::new();
    while !rest.is_empty() {
        let (i, id) = be_u16(rest)?;
        let (i, value) = be_u32(i)?;
        list.push(Setting { id, value });
        rest = i;
    }
    Ok((&[], SettingsFrame { settings: list }))
} 