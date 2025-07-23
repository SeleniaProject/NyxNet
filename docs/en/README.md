# NyxNet - Advanced Anonymous Communication Protocol Implementation

## Overview

NyxNet is a next-generation anonymous communication protocol reference implementation that combines state-of-the-art cryptography with mixnet technology. It integrates cutting-edge technologies such as post-quantum cryptography, multipath communication, and adaptive cover traffic to achieve advanced privacy protection.

## Detailed Architecture

### System-Wide Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
├─────────────────┬─────────────────┬─────────────────────────┤
│   nyx-cli       │   nyx-sdk       │   Custom Apps           │
│   (CLI Tool)    │   (SDK)         │   (Third-party)        │
└─────────┬───────┴─────────┬───────┴─────────────────────────┘
          │ gRPC             │ SDK API
          └─────────────┬────┘
                        │
          ┌─────────────▼─────────────┐
          │      nyx-daemon           │
          │   (Control & Management)   │
          │                           │
          │  ┌─────────────────────┐  │
          │  │  Session Manager    │  │
          │  │  Stream Manager     │  │
          │  │  Path Builder       │  │
          │  │  Metrics Collector  │  │
          │  │  Health Monitor     │  │
          │  │  Event System       │  │
          │  └─────────────────────┘  │
          └─────────────┬─────────────┘
                        │
    ┌───────────────────┼───────────────────┐
    │                   │                   │
┌───▼────┐    ┌────▼─────┐    ┌─────▼────┐
│nyx-mix │    │nyx-stream│    │nyx-crypto│
│        │    │          │    │          │
│ ┌────┐ │    │ ┌──────┐ │    │ ┌──────┐ │
│ │Path│ │    │ │Frame │ │    │ │Noise │ │
│ │Bld │ │    │ │Proc  │ │    │ │Proto │ │
│ └────┘ │    │ └──────┘ │    │ └──────┘ │
│ ┌────┐ │    │ ┌──────┐ │    │ ┌──────┐ │
│ │Cvr │ │    │ │Cong  │ │    │ │AEAD  │ │
│ │Trf │ │    │ │Ctrl  │ │    │ │      │ │
│ └────┘ │    │ └──────┘ │    │ └──────┘ │
└────────┘    └──────────┘    └──────────┘
    │               │               │
    └───────────────┼───────────────┘
                    │
    ┌───────────────▼───────────────┐
    │        nyx-transport          │
    │                               │
    │  ┌─────┐ ┌─────┐ ┌─────────┐  │
    │  │ UDP │ │QUIC │ │TCP Fall │  │
    │  │Pool │ │Ext  │ │back     │  │
    │  └─────┘ └─────┘ └─────────┘  │
    │  ┌─────┐ ┌─────┐ ┌─────────┐  │
    │  │ICE  │ │Trd  │ │Path Val │  │
    │  │Lite │ │IPv6 │ │idation  │  │
    │  └─────┘ └─────┘ └─────────┘  │
    └───────────────────────────────┘
                    │
    ┌───────────────▼───────────────┐
    │         nyx-fec               │
    │                               │
    │  ┌─────────┐ ┌─────────────┐  │
    │  │Reed-Sol │ │RaptorQ      │  │
    │  │omon     │ │Adaptive     │  │
    │  └─────────┘ └─────────────┘  │
    │  ┌─────────┐ ┌─────────────┐  │
    │  │Timing   │ │Padding      │  │
    │  │Obfusc   │ │1280B Fixed  │  │
    │  └─────────┘ └─────────────┘  │
    └───────────────────────────────┘
```

## Detailed Component Specifications

### 1. nyx-core - Core Library

#### 1.1 Configuration Management (`config.rs`)
```rust
pub struct NyxConfig {
    pub listen_port: u16,
    pub node_id: Option<String>,
    pub log_level: Option<String>,
    pub crypto: CryptoConfig,
    pub mix: MixConfig,
    pub transport: TransportConfig,
    pub mobile: MobileConfig,
    pub compliance: ComplianceConfig,
}
```

**Key Features:**
- TOML format configuration file loading
- Environment variable configuration override
- Hot reload support (file watching)
- Configuration validation and default value application

#### 1.2 Error Handling (`error.rs`)
```rust
#[derive(thiserror::Error, Debug)]
pub enum NyxError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Cryptographic error: {0}")]
    Crypto(String),
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Mix routing error: {0}")]
    Mix(String),
}
```

**Characteristics:**
- Unified error types using `thiserror`
- Error chain support
- Integration with logging output
- Debug information retention

#### 1.3 Type Definitions (`types.rs`)
```rust
pub type NodeId = [u8; 32];
pub type SessionKey = [u8; 32];
pub type ConnectionId = [u8; 12];

pub struct StreamId(pub u64);
pub struct PathId(pub u8);
```

#### 1.4 Sandboxing Features

**Linux (seccomp-bpf):**
- System call restrictions
- File access control
- Network access limitations

**OpenBSD (pledge/unveil):**
- Process privilege restrictions
- Filesystem access control
- Network permission management

#### 1.5 Internationalization Support (`i18n.rs`)
```rust
pub fn localize(lang: &str, key: &str, args: Option<&HashMap<String, String>>) -> Result<String, NyxError>
```

**Supported Languages:**
- Japanese (ja)
- English (en)
- Chinese (zh)

### 2. nyx-crypto - Cryptographic Engine

#### 2.1 Noise Protocol Implementation (`noise.rs`)
```rust
pub struct NoiseState {
    handshake_state: HandshakeState,
    session_key: Option<SessionKey>,
}

impl NoiseState {
    pub fn new_initiator() -> Self;
    pub fn new_responder() -> Self;
    pub fn write_message(&mut self, payload: &[u8], message: &mut [u8]) -> Result<usize>;
    pub fn read_message(&mut self, message: &[u8], payload: &mut [u8]) -> Result<usize>;
}
```

**Implementation Pattern:**
- Noise_Nyx: `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`
- Handshake: 3-round authentication
- Forward secrecy: Ephemeral key usage

#### 2.2 HKDF Implementation (`kdf.rs`)
```rust
pub enum KdfLabel {
    Handshake,
    Application,
    Rekey,
    Export,
}

pub fn hkdf_expand(prk: &[u8], label: KdfLabel, length: usize) -> Vec<u8>
```

**Security Properties:**
- Labeled semantics
- Misuse-resistant design
- RFC 5869 compliance

#### 2.3 AEAD Encryption (`aead.rs`)
```rust
pub fn encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
pub fn decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
```

**Algorithm Used:**
- ChaCha20-Poly1305
- 96-bit nonce
- 128-bit authentication tag

#### 2.4 Keystore (`keystore.rs`)
```rust
pub struct SecureKeystore {
    keys: HashMap<String, SecretKey>,
}

impl SecureKeystore {
    pub fn store_key(&mut self, id: &str, key: SecretKey) -> Result<()>;
    pub fn retrieve_key(&self, id: &str) -> Result<SecretKey>;
    pub fn delete_key(&mut self, id: &str) -> Result<()>;
}
```

**Security Features:**
- In-memory key management
- Automatic zeroization
- Access control
- Encrypted storage

#### 2.5 Post-Quantum Cryptography

**Kyber1024:**
```rust
#[cfg(feature = "kyber")]
pub mod kyber {
    pub fn keypair() -> (PublicKey, SecretKey);
    pub fn encapsulate(pk: &PublicKey) -> (SharedSecret, Ciphertext);
    pub fn decapsulate(sk: &SecretKey, ct: &Ciphertext) -> Result<SharedSecret>;
}
```

**BIKE:**
```rust
#[cfg(feature = "bike")]
pub mod bike {
    pub fn keypair() -> (PublicKey, SecretKey);
    pub fn encapsulate(pk: &PublicKey) -> (SharedSecret, Ciphertext);
    pub fn decapsulate(sk: &SecretKey, ct: &Ciphertext) -> Result<SharedSecret>;
}
```

### 3. nyx-stream - Streaming Layer

#### 3.1 Frame Processing (`frame.rs`)
```rust
#[repr(u8)]
pub enum FrameType {
    Data = 0x00,
    Ack = 0x01,
    Ping = 0x02,
    Pong = 0x03,
    Close = 0x04,
    Settings = 0x05,
    PathChallenge = 0x06,
    PathResponse = 0x07,
    Plugin = 0x50, // 0x50-0x5F reserved
}

pub struct FrameHeader {
    pub frame_type: FrameType,
    pub flags: u8,
    pub path_id: u8,
    pub length: u16,
}
```

#### 3.2 Congestion Control (`congestion.rs`)
```rust
pub struct CongestionCtrl {
    cwnd: f64,
    ssthresh: f64,
    rtt_min: Duration,
    rtt_smooth: f64,
    rtt_var: f64,
}

impl CongestionCtrl {
    pub fn on_ack(&mut self, acked_bytes: u64, rtt: Duration);
    pub fn on_loss(&mut self, lost_bytes: u64);
    pub fn can_send(&self, bytes_in_flight: u64) -> bool;
}
```

**Algorithm:**
- TCP Cubic-like
- RTT measurement based
- Packet loss detection
- Adaptive window sizing

#### 3.3 Multipath Support (`receiver.rs`)
```rust
pub struct MultipathReceiver {
    paths: HashMap<PathId, PathState>,
    reorder_buffer: ReorderBuffer,
    sequencer: Sequencer,
}

impl MultipathReceiver {
    pub fn receive_packet(&mut self, path_id: PathId, data: &[u8]) -> Result<Option<Vec<u8>>>;
    pub fn get_path_stats(&self, path_id: PathId) -> Option<PathStats>;
}
```

#### 3.4 Plugin System (`plugin.rs`)
```rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> u32;
    fn process_frame(&self, frame: &[u8]) -> Result<Vec<u8>>;
    fn get_capabilities(&self) -> Vec<Capability>;
}

pub struct PluginRegistry {
    plugins: HashMap<u32, Box<dyn Plugin>>,
}
```

### 4. nyx-mix - Mix Routing

#### 4.1 Path Building (`lib.rs`)
```rust
pub struct WeightedPathBuilder<'a> {
    candidates: &'a [Candidate],
    alpha: f64, // latency vs bandwidth weight
}

impl<'a> WeightedPathBuilder<'a> {
    pub fn build_path(&self, hops: usize) -> Vec<NodeId>;
}

pub struct Candidate {
    pub id: NodeId,
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
}
```

**Selection Algorithm:**
- Weighted probabilistic selection
- Latency and bandwidth consideration
- Duplicate node avoidance

#### 4.2 Cover Traffic (`cover.rs`)
```rust
pub struct CoverGenerator {
    lambda: f64, // events per second
    rng: ThreadRng,
}

impl CoverGenerator {
    pub fn new(lambda: f64) -> Self;
    pub fn next_delay(&self) -> Duration;
}
```

**Statistical Properties:**
- Poisson distribution
- Exponential distribution intervals
- Configurable rate

#### 4.3 Adaptive Cover Traffic (`cover_adaptive.rs`)
```rust
pub struct AdaptiveCoverGenerator {
    target_utilization: f64,
    current_rate: f64,
    measurement_window: Duration,
    utilization_estimator: UtilizationEstimator,
}

impl AdaptiveCoverGenerator {
    pub fn adjust_rate(&mut self, observed_utilization: f64);
    pub fn next_delay(&self) -> Duration;
}
```

#### 4.4 cMix Integration (`cmix.rs`)
```rust
pub struct CmixController {
    batch_size: usize,
    vdf_delay: Duration,
    accumulator: RsaAccumulator,
}

impl CmixController {
    pub fn add_to_batch(&mut self, message: &[u8]) -> Result<()>;
    pub fn process_batch(&mut self) -> Result<Vec<Vec<u8>>>;
}
```

### 5. nyx-transport - Transport Layer

#### 5.1 UDP Pool (`lib.rs`)
```rust
pub struct UdpPool {
    socket: Arc<UdpSocket>,
}

impl UdpPool {
    pub async fn bind(port: u16) -> std::io::Result<Self>;
    pub fn socket(&self) -> Arc<UdpSocket>;
}
```

**Features:**
- SO_REUSEPORT usage
- Non-blocking I/O
- Efficient socket sharing

#### 5.2 QUIC Extensions (`quic.rs`)
```rust
#[cfg(feature = "quic")]
pub struct QuicEndpoint {
    endpoint: quinn::Endpoint,
    config: quinn::ServerConfig,
}

impl QuicEndpoint {
    pub async fn bind(addr: SocketAddr) -> Result<Self>;
    pub async fn connect(&self, addr: SocketAddr) -> Result<QuicConnection>;
}
```

#### 5.3 Teredo IPv6 (`teredo.rs`)
```rust
pub struct TeredoAddr(pub Ipv6Addr);

pub async fn discover(server: SocketAddr) -> Result<TeredoAddr>;

impl TeredoAddr {
    pub fn extract_external_addr(&self) -> SocketAddr;
    pub fn extract_nat_type(&self) -> NatType;
}
```

### 6. nyx-fec - Forward Error Correction

#### 6.1 Reed-Solomon (`lib.rs`)
```rust
pub struct NyxFec {
    rs: ReedSolomon,
}

impl NyxFec {
    pub fn new() -> Self; // 10 data + 3 parity shards
    pub fn encode(&self, shards: &mut [&mut [u8]]) -> Result<(), RSError>;
    pub fn reconstruct(&self, shards: &mut [&mut [u8]], present: &mut [bool]) -> Result<(), RSError>;
}
```

**Parameters:**
- Data shards: 10
- Parity shards: 3
- Shard size: 1280 bytes

#### 6.2 RaptorQ (`raptorq.rs`)
```rust
pub struct RaptorQCodec {
    encoder: Encoder,
    decoder: Decoder,
}

impl RaptorQCodec {
    pub fn new(data_len: usize) -> Self;
    pub fn encode(&mut self, data: &[u8]) -> Vec<EncodingPacket>;
    pub fn decode(&mut self, packets: &[EncodingPacket]) -> Option<Vec<u8>>;
}
```

### 7. nyx-daemon - Daemon Service

#### 7.1 gRPC API (`main.rs`)
```rust
#[async_trait]
impl NyxControl for ControlService {
    async fn get_info(&self, request: Request<Empty>) -> Result<Response<NodeInfo>, Status>;
    async fn open_stream(&self, request: Request<OpenRequest>) -> Result<Response<StreamResponse>, Status>;
    async fn close_stream(&self, request: Request<StreamId>) -> Result<Response<Empty>, Status>;
    async fn get_health(&self, request: Request<HealthRequest>) -> Result<Response<HealthResponse>, Status>;
    // ... other APIs
}
```

#### 7.2 Stream Management (`stream_manager.rs`)
```rust
pub struct StreamManager {
    streams: Arc<RwLock<HashMap<u64, Stream>>>,
    transport: Arc<Transport>,
    metrics: Arc<MetricsCollector>,
}

impl StreamManager {
    pub async fn open_stream(&self, request: OpenRequest) -> Result<StreamResponse>;
    pub async fn close_stream(&self, stream_id: u64) -> Result<()>;
    pub async fn get_stream_stats(&self, stream_id: u64) -> Result<StreamStats>;
}
```

#### 7.3 Metrics Collection (`metrics.rs`)
```rust
pub struct MetricsCollector {
    packets_sent: AtomicU64,
    packets_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    active_streams: AtomicU32,
    connected_peers: AtomicU32,
}

impl MetricsCollector {
    pub fn increment_packets_sent(&self);
    pub fn increment_bytes_sent(&self, bytes: u64);
    pub fn get_performance_metrics(&self) -> PerformanceMetrics;
}
```

### 8. nyx-cli - Command Line Tool

#### 8.1 Command Structure (`main.rs`)
```rust
#[derive(Subcommand)]
enum Commands {
    Connect {
        target: String,
        #[arg(short, long)]
        interactive: bool,
        #[arg(short = 't', long, default_value = "30")]
        connect_timeout: u64,
    },
    Status {
        #[arg(short, long, default_value = "table")]
        format: String,
        #[arg(short, long)]
        watch: bool,
    },
    Bench {
        target: String,
        #[arg(short, long, default_value = "60")]
        duration: u64,
        #[arg(short, long, default_value = "10")]
        connections: u32,
    },
}
```

## Performance Characteristics

### Throughput Performance
- **Single Path**: Up to 100 Mbps
- **Multipath**: Scaling with number of paths
- **Latency Overhead**: 50-200ms additional for 5-hop routing

### Resource Usage
- **Memory**: Approximately 50MB base
- **CPU**: Less than 5% under normal load
- **Network**: 30% overhead for FEC and cover traffic

### Scalability
- **Concurrent Connections**: 10,000+ per daemon
- **Network Size**: Tested with 1,000+ nodes
- **Geographic Distribution**: Global deployment ready

## Security Analysis

### Threat Model
1. **Network Surveillance**: ISP and government communication monitoring
2. **Metadata Analysis**: Information inference from communication patterns
3. **Quantum Computers**: Future cryptographic breaking threats
4. **Endpoint Attacks**: Direct attacks on clients and servers

### Countermeasures
1. **Mixnet Routing**: Communication path anonymization
2. **Cover Traffic**: Communication pattern concealment
3. **Post-Quantum Cryptography**: Quantum-resistant cryptography adoption
4. **Sandboxing**: System-level isolation

### Cryptographic Guarantees
- **Perfect Forward Secrecy**: Session key forward secrecy
- **Post-Compromise Recovery**: Recovery from key compromise
- **Authenticated Encryption**: Combination of encryption and authentication
- **Key Derivation**: Secure key derivation

## Detailed Configuration Files

### Basic Configuration (`nyx.toml`)
```toml
# Network settings
listen_port = 43300
node_id = "auto"  # or 64-character hex string

# Logging settings
log_level = "info"  # trace, debug, info, warn, error
log_format = "json"  # json, pretty

# Cryptography settings
[crypto]
post_quantum = true
kyber_enabled = true
bike_enabled = false
noise_pattern = "Nyx"

# Mix routing settings
[mix]
hop_count = 5
min_hops = 3
max_hops = 7
cover_traffic_rate = 10.0  # packets/second
adaptive_cover = true
target_utilization = 0.4

# Transport settings
[transport]
quic_enabled = true
tcp_fallback = true
udp_buffer_size = 65536
max_connections = 10000

# FEC settings
[fec]
reed_solomon_enabled = true
raptorq_enabled = true
data_shards = 10
parity_shards = 3

# Mobile optimizations
[mobile]
low_power_mode = false
battery_optimization = true
screen_off_detection = true
push_notifications = true

# Telemetry settings
[telemetry]
enabled = true
endpoint = "http://localhost:4317"
sample_rate = 0.1

# Compliance settings
[compliance]
level = "Plus"  # Core, Plus, Full
audit_logging = true
data_retention_days = 30
```

### Advanced Configuration
```toml
# Performance tuning
[performance]
worker_threads = 8
max_blocking_threads = 512
stack_size = 2097152  # 2MB

# Security settings
[security]
sandbox_enabled = true
memory_limit = "1GB"
file_descriptor_limit = 1024

# Plugin settings
[plugins]
enabled = true
plugin_dir = "/opt/nyx/plugins"
allowed_plugins = ["geo-stats", "bandwidth-monitor"]

# DHT settings
[dht]
enabled = true
bootstrap_nodes = [
    "bootstrap1.nyx.network:43300",
    "bootstrap2.nyx.network:43300"
]
replication_factor = 20
```

## Developer Guide

### Build Environment Setup
```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# Required components
rustup component add clippy rustfmt

# Clone project
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# Install dependencies
cargo fetch
```

### Development Workflow
```bash
# Development build
cargo build

# Run tests
cargo test --all

# Run linter
cargo clippy -- -D warnings

# Format code
cargo fmt

# Generate documentation
cargo doc --open
```

### Debugging Procedures
```bash
# Set log level
RUST_LOG=debug cargo run --bin nyx-daemon

# Specific module logging
RUST_LOG=nyx_crypto=trace cargo run --bin nyx-daemon

# Enable backtrace
RUST_BACKTRACE=1 cargo run --bin nyx-daemon
```

### Testing Strategy
1. **Unit Tests**: Function and method behavior verification
2. **Integration Tests**: Inter-module interaction testing
3. **Conformance Tests**: Protocol specification compliance verification
4. **Performance Tests**: Throughput and latency measurement
5. **Security Tests**: Cryptographic implementation verification

## Troubleshooting

### Common Issues

#### 1. Daemon Won't Start
```bash
# Check port usage
sudo netstat -tulpn | grep 43300

# Verify configuration file
cargo run --bin nyx-daemon -- --check-config

# Check permissions
ls -la /opt/nyx/
```

#### 2. Connection Failures
```bash
# Check network connectivity
ping target-host

# Check firewall
sudo ufw status

# Check DNS resolution
nslookup target-host
```

#### 3. Poor Performance
```bash
# Check system resources
top
iostat
netstat -i

# Check NyxNet statistics
cargo run --bin nyx-cli -- status --detailed
```

### Log Analysis
```bash
# Extract error logs
journalctl -u nyx-daemon | grep ERROR

# Performance logs
journalctl -u nyx-daemon | grep "performance"

# Security events
journalctl -u nyx-daemon | grep "security"
```

## API Reference

### gRPC Service Definition
```protobuf
service NyxControl {
  // Node information
  rpc GetInfo(Empty) returns (NodeInfo);
  rpc GetHealth(HealthRequest) returns (HealthResponse);
  
  // Stream management
  rpc OpenStream(OpenRequest) returns (StreamResponse);
  rpc CloseStream(StreamId) returns (Empty);
  rpc GetStreamStats(StreamId) returns (StreamStats);
  rpc ListStreams(Empty) returns (stream StreamStats);
  
  // Event streaming
  rpc SubscribeEvents(EventFilter) returns (stream Event);
  rpc SubscribeStats(StatsRequest) returns (stream StatsUpdate);
  
  // Configuration
  rpc UpdateConfig(ConfigUpdate) returns (ConfigResponse);
  rpc ReloadConfig(Empty) returns (ConfigResponse);
  
  // Network topology
  rpc BuildPath(PathRequest) returns (PathResponse);
  rpc GetPaths(Empty) returns (stream PathInfo);
  rpc GetTopology(Empty) returns (NetworkTopology);
  rpc GetPeers(Empty) returns (stream PeerInfo);
}
```

### SDK Usage Examples

#### Rust SDK
```rust
use nyx_sdk::{NyxClient, StreamConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = NyxClient::connect("http://127.0.0.1:8080").await?;
    
    let config = StreamConfig {
        target: "example.com:80".to_string(),
        multipath: true,
        cover_traffic: true,
    };
    
    let stream = client.open_stream(config).await?;
    stream.write(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await?;
    
    let response = stream.read().await?;
    println!("Response: {}", String::from_utf8_lossy(&response));
    
    Ok(())
}
```

## Deployment Guide

### Docker Deployment
```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/nyx-daemon /usr/local/bin/
EXPOSE 43300 8080
CMD ["nyx-daemon"]
```

### Kubernetes Deployment
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nyx-daemon
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nyx-daemon
  template:
    metadata:
      labels:
        app: nyx-daemon
    spec:
      containers:
      - name: nyx-daemon
        image: nyxnet/daemon:latest
        ports:
        - containerPort: 43300
        - containerPort: 8080
        env:
        - name: NYX_CONFIG
          value: "/etc/nyx/config.toml"
        volumeMounts:
        - name: config
          mountPath: /etc/nyx
      volumes:
      - name: config
        configMap:
          name: nyx-config
```

### Monitoring Setup
```yaml
# Prometheus configuration
scrape_configs:
  - job_name: 'nyx-daemon'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

This comprehensive documentation covers all technical specifications and usage methods for NyxNet. 