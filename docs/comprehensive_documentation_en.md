# NyxNet - Comprehensive Project Documentation

This document provides a comprehensive documentation set for understanding the NyxNet project in its entirety.

## Table of Contents

1. [Project Overview](#project-overview)
2. [System Architecture Design](#system-architecture-design)
3. [Major Feature Details](#major-feature-details)
4. [API / External Interface Reference](#api--external-interface-reference)
5. [Development Environment Setup Guide](#development-environment-setup-guide)

---

## Project Overview

**NyxNet** is a next-generation anonymous communication protocol implementation that combines state-of-the-art cryptography with mixnet technology to provide high-performance, privacy-preserving network communication. Implemented in Rust, it achieves memory safety, quantum resistance, and cross-platform compatibility.

### Project Core

NyxNet solves the following challenges:

- **Protection from network surveillance and traffic analysis**
- **Robust defense against metadata correlation attacks**
- **Preparation for the post-quantum cryptography era**
- **Achieving both high performance and anonymity**

### Technology Stack Overview

- **Primary Language**: Rust (memory-safe, `#![forbid(unsafe_code)]`)
- **Cryptographic Libraries**: Pure Rust implementation (Kyber1024, X25519, ChaCha20Poly1305)
- **Networking**: libp2p, QUIC, UDP, TCP with fallback
- **Protocols**: Noise Protocol Framework, gRPC
- **Message Format**: Protocol Buffers
- **Configuration**: TOML
- **Telemetry**: Prometheus, OpenTelemetry
- **Platforms**: Windows, Linux, macOS, WebAssembly

### High-Level Directory Structure

- **`nyx-core/`** - Configuration management, error handling, type system, sandboxing
- **`nyx-crypto/`** - Noise Protocol, AEAD, post-quantum cryptography
- **`nyx-mix/`** - Mix routing, cover traffic, anonymization
- **`nyx-stream/`** - Stream multiplexing, flow control, frame processing
- **`nyx-transport/`** - Network I/O, packet handling, NAT traversal
- **`nyx-fec/`** - Reed-Solomon, RaptorQ forward error correction
- **`nyx-daemon/`** - Main service, gRPC API, layer integration
- **`nyx-cli/`** - Command-line management tool
- **`nyx-sdk/`** - Developer SDK
- **`formal/`** - TLA+ formal verification models
- **`docs/`** - Multi-language documentation

---

## System Architecture Design

### Major Components

#### 1. Application Layer

- **nyx-cli**: CLI tool (connection, status monitoring, benchmarking)
- **nyx-sdk**: Developer SDK (Rust, WASM, Mobile FFI)
- **Third-party Apps**: Custom application integration

#### 2. Control & Management Layer

- **nyx-daemon**: 
  - Session and stream management
  - gRPC API server (127.0.0.1:50051)
  - Metrics collection (Prometheus)
  - Inter-layer coordination

#### 3. Protocol Stack

```
┌─────────────────────────────────────┐
│           Application               │
├─────────────────────────────────────┤
│              Mix Layer              │ ← Anonymization, cover traffic
├─────────────────────────────────────┤
│             Stream Layer            │ ← Multiplexing, flow control
├─────────────────────────────────────┤
│             Crypto Layer            │ ← Encryption, key management
├─────────────────────────────────────┤
│              FEC Layer              │ ← Forward error correction
├─────────────────────────────────────┤
│           Transport Layer           │ ← UDP/QUIC/TCP
└─────────────────────────────────────┘
```

### Interactions and Data Flow

#### Data Transmission Flow

1. **Application** → Sends app data to `nyx-daemon`
2. **Stream Layer** → Framing, multiplexing, flow control
3. **Crypto Layer** → Encryption via Noise Protocol
4. **FEC Layer** → Reed-Solomon/RaptorQ error correction
5. **Mix Layer** → Mix routing, cover traffic
6. **Transport Layer** → Transmission via UDP/QUIC/TCP

#### Key Management Flow

- **Noise Handshake**: X25519 + Kyber1024 hybrid
- **Perfect Forward Secrecy**: Periodic key renewal
- **Post-Compromise Recovery**: HKDF-based key rekeying

### Architecture Patterns

#### Layered Architecture

Each layer is clearly separated, with upper layers only using services from lower layers:

- Clear dependency relationships (managed by `LayerManager`)
- Improved testability
- Enhanced maintainability

#### Event-Driven

- **Asynchronous messaging**: Tokio channels
- **Reactive processing**: Event-based state transitions
- **Metrics collection**: Real-time telemetry

#### Plugin Architecture

- **Dynamic plugins**: Sandboxed plugin system
- **Security**: seccomp/pledge/unveil restrictions
- **Extensibility**: Addition of custom features

### External System Integration

- **Bootstrap Nodes**: DHT peer discovery
- **Prometheus**: Metrics collection
- **gRPC**: Control API
- **libp2p**: P2P networking

---

## Major Feature Details

### 1. Mix Network Routing System

#### Feature Purpose and User Use Cases

Anonymizes communication paths and protects against traffic analysis attacks. User communications traverse multiple nodes to hide the relationship between sender and receiver.

#### Related Major Files/Modules

- **`nyx-mix/src/lib.rs`**: Core of mix routing
- **`nyx-mix/src/cmix.rs`**: cMix algorithm implementation
- **`nyx-mix/src/cover.rs`**: Cover traffic generation
- **`nyx-mix/src/larmix.rs`**: Latency-aware routing
- **`nyx-daemon/src/path_builder.rs`**: Path construction logic

#### Core Logic

1. **Path Selection Algorithm**:

```rust
pub struct WeightedPathBuilder {
    candidates: &[Candidate],
    alpha: f64,  // Balance between latency vs bandwidth
}
```

Weight calculation: `w = (1/latency_ms) * α + (bandwidth_mbps/MAX_BW) * (1-α)`

2. **Cover Traffic**:
   - Poisson distribution for transmission intervals
   - Adaptive generation rate (target utilization: 0.2-0.6)
   - Mobile optimization considering battery efficiency

3. **cMix Integration**:
   - Batch size 100 for collective encryption
   - Timing control via VDF (Verifiable Delay Function)
   - Proof via RSA accumulator

#### Data Model and Persistence

```rust
pub struct Candidate {
    pub id: NodeId,
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
}
```

Peer information is distributed via DHT, with local caching for fast access.

#### Error Handling

- **Path construction failure**: Alternative node selection, graceful degradation
- **Node failure**: Automatic rerouting, redundant paths
- **Network partition**: Recovery via DHT bootstrap nodes

#### Validation

- Node ID format validation (32-byte hash)
- Latency threshold check (maximum 10 seconds)
- Bandwidth limit verification

#### Testing

- **Unit tests**: Path selection algorithm accuracy
- **Integration tests**: Real network routing verification
- **Property tests**: Mathematical verification of anonymity guarantees

### 2. Post-Quantum Cryptography System

#### Feature Purpose and User Use Cases

Provides resistance against quantum computer attacks. Protects user data from future cryptographic threats.

#### Related Major Files/Modules

- **`nyx-crypto/src/noise.rs`**: Noise Protocol implementation
- **`nyx-crypto/src/kyber.rs`**: Kyber1024 implementation
- **`nyx-crypto/src/kdf.rs`**: Key derivation functions
- **`nyx-crypto/src/aead.rs`**: Authenticated encryption

#### Core Logic

1. **Hybrid Key Exchange**:

```
Handshake Pattern:
<- s
-> e, ee_dh25519, ee_kyber, s, ss
<- se_dh25519, se_kyber, es, ee_dh25519, ee_kyber
```

2. **Key Derivation**:
   - HKDF-Extract(SHA-512, concat(dh25519_secret, kyber_secret))
   - Perfect Forward Secrecy
   - Post-Compromise Recovery

3. **Encryption**:
   - ChaCha20Poly1305 AEAD
   - 32-byte session keys
   - Nonce management and replay attack prevention

#### Data Model and Persistence

```rust
pub struct SessionKey([u8; 32]);
pub struct NoiseSession {
    send_cipher: ChaCha20Poly1305,
    recv_cipher: ChaCha20Poly1305,
    send_nonce: u64,
    recv_nonce: u64,
}
```

Keys are stored only in volatile memory, zeroized on drop.

#### Error Handling

- **Key exchange failure**: No fallback to classical crypto (security first)
- **Decryption error**: Session reconstruction
- **Nonce exhaustion**: Automatic rekeying

#### Validation

- Public key format validation
- Ciphertext integrity checking
- Timing attack countermeasures

#### Testing

- RFC test vector compliance
- Property-based testing
- Cryptographic strength verification

### 3. High-Performance Streaming System

#### Feature Purpose and User Use Cases

Efficiently multiplexes multiple data streams and adaptively responds to network variations. Supports both real-time communication and file transfer.

#### Related Major Files/Modules

- **`nyx-stream/src/lib.rs`**: Stream layer core
- **`nyx-stream/src/congestion.rs`**: Congestion control
- **`nyx-stream/src/flow_controller.rs`**: Flow control
- **`nyx-stream/src/frame.rs`**: Frame processing
- **`nyx-stream/src/tx.rs`**: Transmission queue management

#### Core Logic

1. **Adaptive Congestion Control**:
   - BBR-like algorithm
   - RTT/bandwidth estimation
   - Multipath load balancing

2. **Flow Control**:
   - Stream-level back-pressure
   - Connection-level window management
   - Prioritization support

3. **Multipath Communication**:
   - Path ID routing (0-255)
   - Weighted Round Robin scheduling
   - Reordering buffer management

#### Data Model and Persistence

```rust
pub struct StreamFrame {
    pub stream_id: u64,
    pub flags: u8,
    pub data: Vec<u8>,
}

pub struct FrameHeader {
    pub connection_id: [u8; 12],
    pub frame_type: u8,
    pub flags: u8,
    pub path_id: u8,
    pub length: u16,
}
```

Frame header (14 bytes) + payload, supporting maximum MTU of 1280 bytes.

#### Error Handling

- **Packet loss**: Automatic retransmission, FEC recovery
- **Out-of-order**: Reordering buffer
- **Connection failure**: Multipath redundancy
- **Back-pressure**: Flow control adjustment

#### Validation

- Stream ID range checking
- Frame size limits
- Sequence number verification

#### Testing

- **Performance tests**: Throughput, latency measurement
- **Stress tests**: High connection count, high load environments
- **Network simulation**: Packet loss, delay variation

---

## API / External Interface Reference

### gRPC Service Definition

#### NyxControl Service

**Endpoint**: `127.0.0.1:50051` (TCP)

#### Major API Endpoints

##### 1. Node Information Retrieval

```protobuf
rpc GetInfo(Empty) returns (NodeInfo);
```

**Request Parameters**: None

**Response Structure**:
```json
{
  "node_id": "3a7b9c1d...",
  "version": "v0.1.0",
  "uptime_sec": 3600,
  "active_streams": 5,
  "bytes_in": 1048576,
  "bytes_out": 2097152
}
```

**HTTP Status Codes**:
- `200 OK`: Successful retrieval
- `500 Internal Server Error`: Node information retrieval failed

##### 2. Stream Management

```protobuf
rpc OpenStream(OpenRequest) returns (StreamResponse);
rpc CloseStream(StreamId) returns (Empty);
rpc GetStreamStats(StreamId) returns (StreamStats);
rpc ListStreams(Empty) returns (stream StreamStats);
```

**OpenRequest Structure**:
- `target` (string): Destination ("example.com:80")
- `multipath` (bool): Enable multipath
- `cover_traffic` (bool): Enable cover traffic
- `hop_count` (uint32): Number of hops (3-7, optional)
- `priority` (uint32): Stream priority (0-255)

**StreamResponse Structure**:
```json
{
  "stream_id": "12345678901234567890",
  "status": "ACTIVE",
  "path_info": [
    {"node_id": "abc123...", "latency_ms": 45.2},
    {"node_id": "def456...", "latency_ms": 67.8}
  ],
  "estimated_latency_ms": 113.0
}
```

##### 3. Real-time Statistics

```protobuf
rpc SubscribeStats(StatsRequest) returns (stream StatsUpdate);
```

**StatsRequest Structure**:
- `interval_ms` (uint32): Update interval (milliseconds)
- `metrics` (string[]): Array of metrics to monitor

**Supported Metrics**:
- `"stream_count"`: Number of active streams
- `"bandwidth_usage"`: Bandwidth usage
- `"latency_avg"`: Average latency
- `"error_rate"`: Error rate

##### 4. Health Check

```protobuf
rpc GetHealth(HealthRequest) returns (HealthResponse);
```

**HealthRequest Structure**:
- `include_details` (bool): Flag to include detailed information

**HealthResponse Structure**:
```json
{
  "status": "healthy",
  "checks": [
    {
      "name": "crypto_engine",
      "status": "healthy",
      "response_time_ms": 1.5
    },
    {
      "name": "dht_connectivity",
      "status": "degraded",
      "message": "Low peer count",
      "response_time_ms": 250.0
    }
  ],
  "checked_at": "2025-08-01T12:00:00Z"
}
```

##### 5. Path Management

```protobuf
rpc BuildPath(PathRequest) returns (PathResponse);
rpc GetPaths(Empty) returns (stream PathInfo);
```

**PathRequest Structure**:
- `target` (string): Destination host
- `hops` (uint32): Desired hop count
- `strategy` (string): "latency_optimized" | "bandwidth_optimized" | "reliability_optimized"

#### Authentication & Authorization

**Authentication Mechanisms**:
- **gRPC Metadata**: `authorization: Bearer <api_key>`
- **Unix Socket**: Permission control for local connections
- **mTLS**: Mutual authentication for production environments

**Authorization Levels**:
- `READ`: Information retrieval only
- `STREAM`: Stream operations
- `ADMIN`: Configuration changes, system control

#### Common Error Responses

```protobuf
// google.rpc.Status format
{
  "code": 3,  // INVALID_ARGUMENT
  "message": "Invalid stream configuration",
  "details": [
    {
      "@type": "type.googleapis.com/nyx.ErrorDetail",
      "error_code": "NYX_INVALID_HOP_COUNT",
      "suggestion": "Hop count must be between 3 and 7"
    }
  ]
}
```

**Nyx-Specific Error Codes**:
- `NYX_CRYPTO_ERROR` (100): Cryptography error
- `NYX_MIX_ROUTING_FAILED` (101): Routing failure
- `NYX_INSUFFICIENT_PEERS` (102): Insufficient peers
- `NYX_STREAM_LIMIT_EXCEEDED` (103): Stream count limit exceeded
- `NYX_INVALID_HOP_COUNT` (104): Hop count out of range

### SDK API Reference

#### Rust SDK Usage Example

```rust
use nyx_sdk::{NyxClient, StreamConfig, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Client connection
    let client = NyxClient::connect("http://127.0.0.1:50051").await?;
    
    // Stream configuration
    let config = StreamConfig {
        target: "example.com:80".to_string(),
        multipath: true,
        cover_traffic: true,
        hop_count: Some(5),
        priority: 128,
    };
    
    // Start stream
    let mut stream = client.open_stream(config).await?;
    
    // Send data
    stream.write(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await?;
    
    // Receive response
    let response = stream.read().await?;
    println!("Response: {}", String::from_utf8_lossy(&response));
    
    // Close stream
    stream.close().await?;
    
    Ok(())
}
```

#### JavaScript SDK Usage Example

```javascript
import { NyxClient } from '@nyx/sdk-js';

async function main() {
    const client = new NyxClient('ws://127.0.0.1:50051');
    await client.connect();
    
    const stream = await client.openStream({
        target: 'example.com:80',
        multipath: true,
        coverTraffic: true,
        hopCount: 5
    });
    
    await stream.write('GET / HTTP/1.1\r\n\r\n');
    const response = await stream.read();
    console.log('Response:', response);
    
    await stream.close();
}
```

#### Python SDK Usage Example

```python
import asyncio
from nyx_sdk import NyxClient, StreamConfig

async def main():
    client = NyxClient('http://127.0.0.1:50051')
    await client.connect()
    
    config = StreamConfig(
        target='example.com:80',
        multipath=True,
        cover_traffic=True,
        hop_count=5
    )
    
    stream = await client.open_stream(config)
    await stream.write(b'GET / HTTP/1.1\r\n\r\n')
    
    response = await stream.read()
    print(f'Response: {response.decode()}')
    
    await stream.close()

if __name__ == '__main__':
    asyncio.run(main())
```

---

## Development Environment Setup Guide

### Prerequisites

#### Required Software

**Rust Toolchain**: 1.70 or higher
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
rustup component add clippy rustfmt
```

**Protocol Buffers Compiler**: 3.20 or higher
- Linux: `sudo apt-get install protobuf-compiler`
- Windows: `choco install protoc`
- macOS: `brew install protobuf`

**Java**: 17 or higher (for TLA+ formal verification)
```bash
# Ubuntu
sudo apt-get install openjdk-17-jdk

# Windows
choco install openjdk17

# macOS
brew install openjdk@17
```

**Python**: 3.9 or higher (for verification scripts)
```bash
python3 --version  # Verify 3.9 or higher
pip3 install --upgrade pip
```

#### Platform-Specific Requirements

**Linux**:
```bash
sudo apt-get update
sudo apt-get install -y \
    libcap-dev \
    pkg-config \
    protobuf-compiler \
    build-essential \
    cmake
```

**Windows**:
```powershell
# Install via Chocolatey
choco install protoc cmake visualstudio2022buildtools

# Or manually download and place Protoc
# https://github.com/protocolbuffers/protobuf/releases
```

**macOS**:
```bash
brew install protobuf pkg-config cmake
xcode-select --install  # Xcode Command Line Tools
```

### Project Cloning

```bash
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# If there are submodules
git submodule update --init --recursive
```

### Dependency Installation

#### Rust Dependencies

```bash
# Pre-fetch dependencies (optional)
cargo fetch

# Add required Rust components
rustup component add clippy rustfmt rust-docs

# Add platform-specific targets (as needed)
rustup target add wasm32-unknown-unknown  # For WASM
rustup target add aarch64-apple-darwin   # For Apple Silicon
```

#### Verification Tools

```bash
# Install TLA+ tools
./scripts/install-tla-tools.sh

# Python verification dependencies
pip3 install -r scripts/requirements.txt
```

### Environment Configuration

#### Configuration File Creation

```bash
# Copy basic configuration file
cp nyx.toml.example nyx.toml

# Apply development configuration
cp docs/config/development.toml nyx.toml
```

**Basic Configuration** (`nyx.toml`):
```toml
# Basic network settings
listen_port = 43300
node_id = "auto"
log_level = "info"

# Development mode settings
[network]
bind_addr = "127.0.0.1:43300"
development = true

# Cryptography settings
[crypto]
post_quantum = true
kyber_enabled = true
bike_enabled = false

# Mix routing settings
[mix]
hop_count = 5
min_hops = 3
max_hops = 7
cover_traffic_rate = 10.0
adaptive_cover = true

# Transport settings
[transport]
quic_enabled = true
tcp_fallback = true
udp_buffer_size = 65536

# DHT settings
[dht]
enabled = true
port = 43301
bootstrap_peers = [
    "/dns4/testnet-validator1.nymtech.net/tcp/1789/p2p/12D3KooWNyxTestnet1"
]
```

#### Environment Variables Setup

**Linux/macOS** (`.bashrc` or `.zshrc`):
```bash
export RUST_LOG=info
export NYX_CONFIG_PATH=./nyx.toml
export NYX_DAEMON_ENDPOINT="127.0.0.1:50051"
export RUST_BACKTRACE=1

# Additional development settings
export CARGO_TARGET_DIR=./target
export RUSTFLAGS="-D warnings"
```

**Windows** (PowerShell Profile):
```powershell
$env:RUST_LOG = "info"
$env:NYX_CONFIG_PATH = ".\nyx.toml"
$env:NYX_DAEMON_ENDPOINT = "127.0.0.1:50051"
$env:RUST_BACKTRACE = "1"
```

### Development Server Startup

#### Initial Build

```bash
# Debug build (for development)
cargo build

# Release build (for performance testing)
cargo build --release

# Build with all features enabled
cargo build --all-features

# Build specific component only
cargo build -p nyx-daemon
```

#### Daemon Startup

**Terminal 1 (Daemon)**:
```bash
# Start in development mode
RUST_LOG=debug cargo run --bin nyx-daemon

# Or run pre-built binary
./target/debug/nyx-daemon

# Background execution
nohup ./target/release/nyx-daemon > daemon.log 2>&1 &
```

#### CLI Verification

**Terminal 2 (CLI verification)**:
```bash
# Check daemon status
cargo run --bin nyx-cli -- status

# Connection test
cargo run --bin nyx-cli -- connect example.com:80

# Benchmark test
cargo run --bin nyx-cli -- bench example.com:80 --duration 30
```

### Test Execution

#### Basic Tests

```bash
# Run all tests
cargo test --workspace --all-features

# Test specific crate
cargo test -p nyx-crypto
cargo test -p nyx-mix --test integration_tests

# Tests with detailed logging
RUST_LOG=debug cargo test test_name -- --nocapture
```

#### Integration Tests

```bash
# Conformance tests
cargo test --package nyx-conformance --all-features

# Extended property-based tests
PROPTEST_CASES=10000 cargo test -p nyx-conformance

# Network simulation tests
cargo test --test network_simulation --features network-testing
```

#### Performance Tests

```bash
# Run benchmarks
cargo bench

# Component-specific benchmarks
cargo bench -p nyx-crypto
cargo bench -p nyx-stream

# Benchmarks with profiling
cargo bench -- --profile-time=5
```

#### Formal Verification

```bash
# TLA+ model checking
./scripts/verify.py --timeout 600

# Comprehensive verification
./scripts/build-verify.sh

# Verification with specific configuration
./scripts/verify.py --config formal/basic.cfg
```

### Development Workflow

#### Daily Development

```bash
# 1. Pre-change verification
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 2. Post-change testing
cargo test --workspace
cargo build --release

# 3. Final pre-commit check
./scripts/pre-commit-check.sh
```

#### Debug Procedures

**Detailed Logging**:
```bash
# Module-specific log level settings
RUST_LOG=nyx_crypto=trace,nyx_mix=debug,nyx_daemon=info cargo run --bin nyx-daemon

# Debug specific functionality
RUST_LOG=nyx_stream::flow_controller=trace cargo test flow_control_test
```

**Profiling**:
```bash
# CPU usage analysis
cargo install cargo-profdata
cargo profdata -- --bench crypto_bench

# Memory usage analysis
cargo install cargo-bloat
cargo bloat --release --crates
```

**Performance Monitoring**:
```bash
# Check Prometheus metrics
curl http://127.0.0.1:9090/metrics

# Real-time statistics
cargo run --bin nyx-cli -- status --watch --format json
```

### Common Troubleshooting

#### Build Errors

**1. protoc not found**:
```bash
# Verify installation
which protoc
protoc --version

# Set path
export PATH=$PATH:/usr/local/bin
# For Windows: $env:PATH += ";C:\tools\protoc\bin"
```

**2. libcap not found (Linux)**:
```bash
sudo apt-get install libcap-dev
pkg-config --cflags --libs libcap
```

**3. Link errors**:
```bash
# Reinstall dependencies
cargo clean
cargo build

# Check system libraries
ldconfig -p | grep ssl  # Linux
otool -L binary_name   # macOS
```

#### Runtime Errors

**1. Port conflicts**:
```bash
# Check port usage
netstat -tlnp | grep :43300  # Linux
netstat -an | findstr :43300  # Windows

# Change port in configuration file
sed -i 's/listen_port = 43300/listen_port = 43301/' nyx.toml
```

**2. Permission errors (Linux)**:
```bash
# Check capabilities
getcap ./target/debug/nyx-daemon

# Set capability if needed
sudo setcap cap_net_bind_service=+ep ./target/debug/nyx-daemon

# Or use non-privileged port
echo "listen_port = 8080" >> nyx.toml
```

**3. DHT connection issues**:
```bash
# Check firewall
sudo ufw status                    # Linux
netsh advfirewall show allprofiles # Windows

# Verify bootstrap_peers configuration
grep -A 5 "bootstrap_peers" nyx.toml

# Use testnet bootstrap configuration
cp docs/config/testnet-bootstrap.toml nyx.toml
```

#### Debug Tips

**Memory leak detection**:
```bash
# Valgrind (Linux)
cargo build
valgrind --tool=memcheck ./target/debug/nyx-daemon

# AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" cargo run --bin nyx-daemon
```

**Network tracing**:
```bash
# Packet capture
sudo tcpdump -i lo port 43300

# Wireshark analysis
tshark -i lo -f "port 43300" -w nyx-traffic.pcap
```

**Log analysis**:
```bash
# Structured log output
RUST_LOG=debug cargo run --bin nyx-daemon 2>&1 | jq '.'

# Search for specific errors
grep -E "ERROR|WARN" daemon.log | tail -20
```

### IDE Configuration

#### Visual Studio Code

**Recommended Extensions**:
- rust-analyzer
- CodeLLDB (for debugging)
- Better TOML
- Protocol Buffer support

**Settings** (`.vscode/settings.json`):
```json
{
    "rust-analyzer.cargo.allFeatures": true,
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.target": null,
    "files.watcherExclude": {
        "**/target/**": true
    }
}
```

#### IntelliJ IDEA / CLion

**Rust Plugin Configuration**:
- Toolchain: `$HOME/.cargo/bin/rustc`
- Standard library: Auto-detect
- Cargo project: Select workspace root

### Continuous Integration

#### Full CI Environment Setup

```bash
# Check GitHub Actions workflow
cat .github/workflows/comprehensive_ci.yml

# Local CI testing
act -j test-linux  # Requires act tool
```

This setup guide enables developers to quickly establish a NyxNet development environment and contribute efficiently to the project.

---

**Last Updated**: August 1, 2025  
**Document Version**: v1.0  
**Compatible NyxNet Version**: v0.1+
