#![forbid(unsafe_code)]

//! Localized String Frame (Type=0x20) utilities as described in Nyx Protocol §12.
//! 
//! The payload layout is as follows:
//! ````text
//! +---------+----------------------+--------------------+
//! | TagLen | lang_tag (TagLen B) | UTF-8 text (...) |
//! +---------+----------------------+--------------------+
//!  ^ u8                                 variable
//! ````
//!
//! * `TagLen`: Length of the BCP-47 language tag in bytes (max 63 to fit Nyx flags field).
//! * `lang_tag`: ASCII language identifier (e.g. "en", "ja-JP").
//! * `text`: UTF-8 encoded string.
//!
//! The Nyx base header (2-bit type, flags, length) is handled by the caller. This module focuses
//! purely on encoding/decoding the frame payload.

use nom::{bytes::complete::{take, take_while}, number::complete::u8, IResult};

/// LOCALIZED_STRING frame representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedStringFrame<'a> {
    /// BCP-47 language tag (ASCII).
    pub lang_tag: &'a str,
    /// UTF-8 payload.
    pub text: &'a str,
}

/// Parse a LOCALIZED_STRING frame payload.
/// Returns the remaining slice and the parsed [`LocalizedStringFrame`].
///
/// # Errors
/// Returns a nom error if the input is malformed or violates UTF-8.
pub fn parse_localized_string_frame<'a>(input: &'a [u8]) -> IResult<&'a [u8], LocalizedStringFrame<'a>> {
    let (input, tag_len) = u8(input)?;
    let (input, tag_bytes) = take(tag_len)(input)?;
    let (input, text_bytes) = take_while(|_| true)(input)?; // take rest of input
    let lang_tag = std::str::from_utf8(tag_bytes).map_err(|_| nom::Err::Failure(nom::error::Error::new(tag_bytes, nom::error::ErrorKind::Fail)))?;
    let text = std::str::from_utf8(text_bytes).map_err(|_| nom::Err::Failure(nom::error::Error::new(text_bytes, nom::error::ErrorKind::Fail)))?;
    Ok((input, LocalizedStringFrame { lang_tag, text }))
}

/// Build a LOCALIZED_STRING frame payload.
/// Returns a `Vec<u8>` containing `TagLen || lang_tag || text`.
///
/// # Panics
/// * If `lang_tag.len()` exceeds 255 (u8)::MAX, or `lang_tag` contains non-ASCII bytes.
pub fn build_localized_string_frame(frame: &LocalizedStringFrame) -> Vec<u8> {
    assert!(frame.lang_tag.is_ascii(), "lang_tag must be ASCII");
    let tag_len = frame.lang_tag.len();
    assert!(tag_len <= u8::MAX as usize, "lang_tag too long");

    let mut v = Vec::with_capacity(1 + tag_len + frame.text.len());
    v.push(tag_len as u8);
    v.extend_from_slice(frame.lang_tag.as_bytes());
    v.extend_from_slice(frame.text.as_bytes());
    v
} 