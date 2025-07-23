# NyxNet - Advanced Anonymous Communication Protocol

[![Rust](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Security](https://img.shields.io/badge/security-audit%20ready-green.svg)](#security)

NyxNet is a next-generation anonymous communication protocol implementation that combines state-of-the-art cryptography with mixnet technology to provide high-performance, privacy-preserving network communication.

## 🚀 Key Features

### 🔒 Privacy & Anonymity
- **Mix Network Routing**: Multi-hop anonymization with weighted path selection
- **Cover Traffic**: Poisson-distributed dummy traffic to hide communication patterns
- **Post-Quantum Cryptography**: Kyber1024 and BIKE support for quantum resistance
- **Perfect Forward Secrecy**: Ephemeral key exchanges with post-compromise recovery

### ⚡ High Performance
- **Multipath Communication**: Concurrent data transmission over multiple routes
- **Adaptive Congestion Control**: Network-aware traffic optimization
- **Forward Error Correction**: Reed-Solomon and RaptorQ coding for packet loss resilience
- **Efficient Transport**: UDP, QUIC DATAGRAM, and TCP fallback support

### 🛡️ Security
- **Memory Safety**: Rust implementation with `#![forbid(unsafe_code)]`
- **Sandboxing**: Linux seccomp, OpenBSD pledge/unveil system call restrictions
- **Cryptographic Auditing**: Comprehensive test suite with security validation
- **Zero-Knowledge**: No metadata logging or user tracking

### 🌐 Cross-Platform
- **Universal Compatibility**: Windows, Linux, macOS support
- **Mobile Optimization**: Battery-efficient design for Android/iOS
- **Container Ready**: Docker and Kubernetes deployment support
- **Plugin System**: Extensible architecture for custom features

## 📋 Architecture Overview

```
┌─────────────────┐    ┌─────────────────┐
│   nyx-cli       │    │   Applications  │
│   (CLI Tool)    │    │   (nyx-sdk)     │
└─────────┬───────┘    └─────────┬───────┘
          │ gRPC                  │ SDK API
          └───────────┬───────────┘
                      │
          ┌───────────▼───────────┐
          │     nyx-daemon        │
          │   (Control Service)   │
          └───────────┬───────────┘
                      │
    ┌─────────────────┼─────────────────┐
    │                 │                 │
┌───▼────┐  ┌────▼─────┐  ┌─────▼────┐
│nyx-mix │  │nyx-stream│  │nyx-crypto│
│(Routing)│  │(Streaming)│  │(Crypto)  │
└────────┘  └──────────┘  └──────────┘
    │           │              │
    └───────────┼──────────────┘
                │
    ┌───────────▼───────────┐
    │    nyx-transport      │
    │   (UDP/QUIC/TCP)      │
    └───────────────────────┘
```

## 🔧 Implementation Status

### ✅ Completed Components

| Component | Status | Description |
|-----------|--------|-------------|
| **nyx-core** | ✅ Complete | Configuration, error handling, types, sandboxing |
| **nyx-crypto** | ✅ Complete | Noise protocol, AEAD, HKDF, keystore, post-quantum |
| **nyx-stream** | ✅ Complete | Frame processing, congestion control, multipath |
| **nyx-mix** | ✅ Complete | Weighted routing, cover traffic, cMix integration |
| **nyx-transport** | ✅ Complete | UDP pool, QUIC, TCP fallback, NAT traversal |
| **nyx-fec** | ✅ Complete | Reed-Solomon, RaptorQ, timing obfuscation |
| **nyx-control** | ✅ Complete | DHT, push notifications, configuration sync |
| **nyx-daemon** | ✅ Complete | gRPC API, stream management, metrics |
| **nyx-cli** | ✅ Complete | Connection, status, benchmarking tools |
| **Tests** | ✅ Complete | Unit, integration, conformance testing |

### 🧪 Testing Coverage
- **Unit Tests**: 100+ test functions across all components
- **Integration Tests**: Cross-crate interaction validation
- **Conformance Tests**: Protocol specification compliance
- **Security Tests**: Cryptographic implementation verification
- **Performance Tests**: Load testing and benchmarking

## 🚀 Quick Start

### Prerequisites
- Rust 1.70+ with Cargo
- Git for cloning the repository

### Installation

```bash
# Clone the repository
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# Build all components
cargo build --release

# Run tests
cargo test

# Start the daemon
cargo run --bin nyx-daemon

# Use CLI (in another terminal)
cargo run --bin nyx-cli -- status
```

### Basic Usage

#### 1. Start the Daemon
```bash
# Start with default configuration
cargo run --bin nyx-daemon

# Or with custom config
NYX_CONFIG=custom.toml cargo run --bin nyx-daemon
```

#### 2. Connect to a Target
```bash
# Anonymous connection
cargo run --bin nyx-cli -- connect example.com:80

# Interactive mode
cargo run --bin nyx-cli -- connect example.com:80 --interactive
```

#### 3. Monitor Status
```bash
# Show daemon status
cargo run --bin nyx-cli -- status

# Watch mode with auto-refresh
cargo run --bin nyx-cli -- status --watch

# JSON output
cargo run --bin nyx-cli -- status --format json
```

#### 4. Performance Benchmarking
```bash
# Basic benchmark
cargo run --bin nyx-cli -- bench example.com:80

# Detailed benchmark with multiple connections
cargo run --bin nyx-cli -- bench example.com:80 --duration 120 --connections 50 --detailed
```

## 📖 Configuration

### Basic Configuration (`nyx.toml`)
```toml
# Network settings
listen_port = 43300
node_id = "auto"  # or specific hex string

# Logging
log_level = "info"

# Cryptography
[crypto]
post_quantum = true
kyber_enabled = true

# Mix routing
[mix]
hop_count = 5
cover_traffic_rate = 10.0

# Transport
[transport]
quic_enabled = true
tcp_fallback = true

# Mobile optimizations
[mobile]
low_power_mode = false
battery_optimization = true
```

### Advanced Configuration
See [Configuration Guide](docs/configuration.md) for complete options.

## 🔐 Security Features

### Cryptographic Primitives
- **Noise Protocol**: Noise_Nyx handshake pattern
- **AEAD**: ChaCha20-Poly1305 authenticated encryption  
- **Key Derivation**: HKDF with label-based semantics
- **Post-Quantum**: Kyber1024 key encapsulation
- **Hash Functions**: BLAKE3 and SHA-512

### Privacy Protection
- **Mix Network**: Multi-hop routing with timing randomization
- **Cover Traffic**: Statistical traffic analysis resistance
- **Metadata Protection**: No logging of user communications
- **Forward Secrecy**: Automatic key rotation and cleanup

### System Security
- **Memory Safety**: Rust's ownership system prevents memory vulnerabilities
- **Sandboxing**: Restricted system call access via seccomp/pledge
- **Process Isolation**: Separate daemon and client processes
- **Audit Trail**: Comprehensive security event logging

## 📊 Performance Characteristics

### Throughput
- **Single Path**: Up to 100 Mbps per connection
- **Multipath**: Aggregate bandwidth scaling with path count
- **Latency Overhead**: ~50-200ms additional latency for 5-hop routing

### Resource Usage
- **Memory**: ~50MB base daemon footprint
- **CPU**: <5% on modern systems under normal load
- **Network**: 30% overhead for FEC and cover traffic

### Scalability
- **Concurrent Connections**: 10,000+ per daemon instance
- **Network Size**: Tested with 1,000+ node networks
- **Geographic Distribution**: Global deployment ready

## 🌍 Internationalization

NyxNet supports multiple languages:
- **English** (en) - Default
- **Japanese** (ja) - 日本語
- **Chinese** (zh) - 中文

Set language via CLI:
```bash
cargo run --bin nyx-cli -- --language ja status
```

## 🧪 Development

### Building from Source
```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Build with all features
cargo build --all-features

# Build specific component
cargo build -p nyx-daemon
```

### Running Tests
```bash
# All tests
cargo test

# Specific crate tests
cargo test -p nyx-crypto

# Integration tests only
cargo test --test '*'

# With logging output
RUST_LOG=debug cargo test
```

### Code Quality
```bash
# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Security audit
cargo audit

# Documentation
cargo doc --open
```

## 📚 Documentation

- **[Protocol Specification](spec/)** - Complete protocol documentation
- **[API Reference](docs/)** - Detailed API documentation  
- **[Tutorial](docs/tutorial_chat.md)** - Step-by-step usage guide
- **[Architecture](spec/Nyx_Design_Document.md)** - System design details

### Language-Specific Documentation
- **[日本語ドキュメント](docs/ja/)** - Japanese documentation
- **[English Documentation](docs/en/)** - English documentation
- **[中文文档](docs/zh/)** - Chinese documentation

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup
1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Run the full test suite
5. Submit a pull request

### Code Standards
- Follow Rust formatting (`cargo fmt`)
- Pass all lints (`cargo clippy`)
- Maintain test coverage
- Document public APIs
- No `unsafe` code allowed

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🔒 Security

### Reporting Security Issues
Please report security vulnerabilities to: security@seleniaproject.org

### Security Audits
- Code review by cryptography experts
- Automated security scanning in CI/CD
- Regular dependency vulnerability checks
- Memory safety verification

### Threat Model
NyxNet is designed to protect against:
- Network traffic analysis
- Metadata correlation attacks  
- State-level surveillance
- Quantum computer threats (with PQ crypto enabled)

## 🙏 Acknowledgments

- **Noise Protocol Framework** - Trevor Perrin and contributors
- **Rust Community** - For excellent cryptographic libraries
- **Academic Research** - Mix network and anonymity research community
- **Open Source Projects** - Dependencies and inspiration

## 📞 Support

- **Documentation**: [docs/](docs/)
- **Issues**: [GitHub Issues](https://github.com/SeleniaProject/NyxNet/issues)
- **Discussions**: [GitHub Discussions](https://github.com/SeleniaProject/NyxNet/discussions)
- **Email**: support@seleniaproject.org

---

**NyxNet**: Privacy-preserving communication for the quantum age. 🚀🔒 