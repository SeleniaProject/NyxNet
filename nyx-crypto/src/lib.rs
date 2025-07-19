#![forbid(unsafe_code)]

//! Nyx cryptography engine.
//!
//! This crate provides:
//! 1. Noise_Nyx handshake implementation (see [`noise`] module).
//! 2. HKDF wrappers with misuse-resistant label semantics.
//! 3. Optional Kyber1024 Post-Quantum fallback when built with `--features pq`.

pub mod noise;
