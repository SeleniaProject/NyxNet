# NyxNet

Reference implementation of the Nyx Protocol (v0.1) in Rust 2021.

## Crate Overview

| Crate | Description |
|-------|-------------|
| `nyx-core`        | Common types, configuration loader, i18n utilities |
| `nyx-crypto`      | Noise_Nyx handshake, HKDF wrappers, Kyber fallback |
| `nyx-stream`      | Secure Stream layer (framing, congestion control) |
| `nyx-fec`         | Reed-Solomon FEC (1280 B shard) |
| `nyx-mix`         | Mix-network routing & cover traffic |
| `nyx-transport`   | UDP + ICE-Lite traversal, STUN utilities |
| `nyx-control`     | Kademlia DHT & SETTINGS sync (`dht` feature) |
| `nyx-telemetry`   | Tracing + Prometheus exporter |
| `nyx-sdk-wasm`    | WASM bindings (WebTransport) |

## Building

```bash
# Default (no DHT, fastest compile)
cargo build --workspace

# Enable full DHT stack (libp2p)
cargo build --workspace --features nyx-control/dht
```

### Suggested CI Matrix

| Target            | Features            |
|-------------------|---------------------|
| `x86_64-unknown-linux-gnu` | default            |
| `x86_64-unknown-linux-musl`| default, `--release` |
| `wasm32-unknown-unknown`   | `nyx-sdk-wasm` only |
| `x86_64-unknown-linux-gnu` | `nyx-control/dht`   |

## Tests

```bash
# Run workspace unit + conformance tests
cargo test --workspace --all-features
```

## Contribution

* Commits follow **Conventional Commits**.
* All new code must pass `cargo clippy --deny warnings` and include unit tests.
* PR workflow: feature branch → draft PR → CI green → review.

---
MIT OR Apache-2.0 