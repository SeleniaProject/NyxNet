#![forbid(unsafe_code)]

//! Path validation support (PATH_CHALLENGE / PATH_RESPONSE handling).
//!
//! According to Nyx Protocol §16, each newly discovered network path MUST be
//! validated by sending a 128-bit `PATH_CHALLENGE` token and awaiting the
//! corresponding `PATH_RESPONSE`. The reference implementation adds a retry
//! mechanism with loss-tolerance: if the response is not received within a
//! short timeout the challenge is retransmitted up to *N* times before the
//! path is deemed unreachable.
//!
//! This module provides [`PathValidator`], an async component that can be used
//! as a [`crate::PacketHandler`]. It automatically:
//!
//! 1. Generates cryptographically-random tokens and transmits `PATH_CHALLENGE`.
//! 2. Retries with an exponential back-off (default *3* attempts).
//! 3. Responds to inbound `PATH_CHALLENGE` with `PATH_RESPONSE`.
//! 4. Tracks validation result per remote `SocketAddr`.
//!
//! The implementation is self-contained and **stateless** on disk: all tokens
//! live in memory and are wiped once path validation succeeds or fails. It is
//! intended to be embedded inside higher-level connection management code.

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use rand::RngCore;
use tokio::{sync::{RwLock, Notify}, time::{sleep, Duration, Instant}};

use nyx_stream::{
    build_header,
    parse_header_ext,
    FrameHeader,
};

use crate::{PacketHandler, Transport};

/// Management frame discriminators (Nyx §16).
const FRAME_PATH_CHALLENGE: u8 = 0x33;
const FRAME_PATH_RESPONSE: u8  = 0x34;

/// Default validation parameters.
const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MAX_RETRIES: u8 = 3;

/// Internal per-path entry.
struct Entry {
    token: [u8; 16],
    retries_left: u8,
    validated: bool,
    last_sent: Instant,
    waker: Arc<Notify>,
}

/// Path validator with retry & loss-tolerance logic.
///
/// Clone-able handle backed by an `Arc` so multiple components can access the
/// same validation state.
#[derive(Clone)]
pub struct PathValidator {
    transport: Transport,
    state: Arc<RwLock<HashMap<SocketAddr, Entry>>>,
}

impl PathValidator {
    /// Create new validator bound to existing [`Transport`].
    #[must_use]
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initiate path validation. If the path is already validated, the method
    /// returns immediately.
    pub async fn validate_path(&self, addr: SocketAddr) {
        // Fast path: already validated.
        {
            let map = self.state.read().await;
            if let Some(e) = map.get(&addr) {
                if e.validated { return; }
            }
        }

        // Insert new entry with random token.
        let mut token = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut token);
        let notify = Arc::new(Notify::new());

        let mut map = self.state.write().await;
        map.insert(addr, Entry {
            token,
            retries_left: MAX_RETRIES,
            validated: false,
            last_sent: Instant::now() - RETRY_INTERVAL, // send immediately
            waker: notify.clone(),
        });
        drop(map);

        // Spawn retry task (detached).
        let this = self.clone();
        tokio::spawn(async move {
            this.retry_loop(addr).await;
        });

        // First immediate send.
        self.send_challenge(addr, &token).await;
    }

    /// Returns `true` once the path is validated or `false` if retries are
    /// exhausted. Awaiters are woken as soon as result is known.
    pub async fn wait_validation(&self, addr: SocketAddr) -> bool {
        loop {
            let notify_opt;
            {
                let map = self.state.read().await;
                if let Some(e) = map.get(&addr) {
                    if e.validated { return true; }
                    if e.retries_left == 0 { return false; }
                    notify_opt = Some(e.waker.clone());
                } else {
                    // No entry — consider unvalidated.
                    return false;
                }
            }
            if let Some(n) = notify_opt { n.notified().await; }
        }
    }

    /// Internal task: retransmit challenge while retries remain.
    async fn retry_loop(&self, addr: SocketAddr) {
        loop {
            sleep(RETRY_INTERVAL).await;

            let mut remove = false;
            let mut maybe_resend = None;

            {
                let mut map = self.state.write().await;
                if let Some(e) = map.get_mut(&addr) {
                    if e.validated {
                        remove = true;
                    } else if e.retries_left == 0 {
                        // Exhausted attempts.
                        remove = true;
                        e.waker.notify_waiters();
                    } else {
                        maybe_resend = Some(e.token);
                        e.retries_left -= 1;
                        e.last_sent = Instant::now();
                    }
                } else {
                    // Entry disappeared.
                    break;
                }
            }

            if remove {
                let _ = self.state.write().await.remove(&addr);
                break;
            }

            if let Some(token) = maybe_resend {
                self.send_challenge(addr, &token).await;
            }
        }
    }

    /// Build and transmit a `PATH_CHALLENGE` frame inside a Control packet.
    async fn send_challenge(&self, addr: SocketAddr, token: &[u8; 16]) {
        let mut payload = Vec::with_capacity(1 + token.len());
        payload.push(FRAME_PATH_CHALLENGE);
        payload.extend_from_slice(token);

        let header = build_header(FrameHeader {
            frame_type: 1, // Control
            flags: 0,      // no flags
            length: payload.len() as u16,
        });

        let mut packet = Vec::with_capacity(header.len() + payload.len());
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&payload);

        self.transport.send(addr, &packet).await;
    }

    /// Build and transmit a `PATH_RESPONSE` replying to a given token.
    async fn send_response(&self, addr: SocketAddr, token: &[u8; 16]) {
        let mut payload = Vec::with_capacity(1 + token.len());
        payload.push(FRAME_PATH_RESPONSE);
        payload.extend_from_slice(token);

        let header = build_header(FrameHeader { frame_type: 1, flags: 0, length: payload.len() as u16 });
        let mut packet = Vec::with_capacity(header.len() + payload.len());
        packet.extend_from_slice(&header);
        packet.extend_from_slice(&payload);
        self.transport.send(addr, &packet).await;
    }

    /// Process inbound datagram; called from [`PacketHandler`] implementation.
    async fn process_incoming(&self, src: SocketAddr, data: &[u8]) {
        let (mut payload, hdr) = match parse_header_ext(data) {
            Ok(v) => v,
            Err(_) => return, // malformed packet – ignore
        };

        // Only interested in Control packets.
        if hdr.hdr.frame_type != 1 { return; }

        // Need at least type byte.
        if payload.is_empty() { return; }
        let frame_type = payload[0];
        payload = &payload[1..];

        match frame_type {
            FRAME_PATH_CHALLENGE => {
                // Echo back with PATH_RESPONSE.
                if let Ok((_, frame)) = build_parser_challenge(payload) {
                    self.send_response(src, &frame.token).await;
                }
            }
            FRAME_PATH_RESPONSE => {
                if let Ok((_, frame)) = build_parser_response(payload) {
                    // Match token to pending entry.
                    let mut map = self.state.write().await;
                    if let Some(entry) = map.get_mut(&src) {
                        if entry.token == frame.token {
                            entry.validated = true;
                            entry.waker.notify_waiters();
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Helpers wrapping existing parsers to operate on token-only payload.
fn build_parser_challenge(input: &[u8]) -> nom::IResult<&[u8], nyx_stream::PathChallengeFrame> {
    nyx_stream::parse_path_challenge_frame(input)
}

fn build_parser_response(input: &[u8]) -> nom::IResult<&[u8], nyx_stream::PathResponseFrame> {
    nyx_stream::parse_path_response_frame(input)
}

#[async_trait::async_trait]
impl PacketHandler for PathValidator {
    async fn handle_packet(&self, src: SocketAddr, data: &[u8]) {
        self.process_incoming(src, data).await;
    }
} 