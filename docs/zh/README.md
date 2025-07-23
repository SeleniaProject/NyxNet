# NyxNet - 高级匿名通信协议实现

## 概述

NyxNet是结合最新密码技术和混合网络技术的下一代匿名通信协议参考实现。它集成了后量子密码学、多路径通信、自适应掩护流量等尖端技术，实现高级隐私保护。

## 详细架构

### 系统整体架构

```
┌─────────────────────────────────────────────────────────────┐
│                     应用程序层                               │
├─────────────────┬─────────────────┬─────────────────────────┤
│   nyx-cli       │   nyx-sdk       │   自定义应用              │
│   (CLI工具)      │   (SDK)         │   (第三方)               │
└─────────┬───────┴─────────┬───────┴─────────────────────────┘
          │ gRPC             │ SDK API
          └─────────────┬────┘
                        │
          ┌─────────────▼─────────────┐
          │      nyx-daemon           │
          │   (控制和管理服务)          │
          │                           │
          │  ┌─────────────────────┐  │
          │  │  会话管理器          │  │
          │  │  流管理器            │  │
          │  │  路径构建器          │  │
          │  │  指标收集器          │  │
          │  │  健康监视器          │  │
          │  │  事件系统            │  │
          │  └─────────────────────┘  │
          └─────────────┬─────────────┘
                        │
    ┌───────────────────┼───────────────────┐
    │                   │                   │
┌───▼────┐    ┌────▼─────┐    ┌─────▼────┐
│nyx-mix │    │nyx-stream│    │nyx-crypto│
│        │    │          │    │          │
│ ┌────┐ │    │ ┌──────┐ │    │ ┌──────┐ │
│ │路径│ │    │ │帧处  │ │    │ │Noise │ │
│ │构建│ │    │ │理    │ │    │ │协议  │ │
│ └────┘ │    │ └──────┘ │    │ └──────┘ │
│ ┌────┐ │    │ ┌──────┐ │    │ ┌──────┐ │
│ │掩护│ │    │ │拥塞  │ │    │ │AEAD  │ │
│ │流量│ │    │ │控制  │ │    │ │      │ │
│ └────┘ │    │ └──────┘ │    │ └──────┘ │
└────────┘    └──────────┘    └──────────┘
    │               │               │
    └───────────────┼───────────────┘
                    │
    ┌───────────────▼───────────────┐
    │        nyx-transport          │
    │                               │
    │  ┌─────┐ ┌─────┐ ┌─────────┐  │
    │  │ UDP │ │QUIC │ │TCP后备  │  │
    │  │池   │ │扩展 │ │        │  │
    │  └─────┘ └─────┘ └─────────┘  │
    │  ┌─────┐ ┌─────┐ ┌─────────┐  │
    │  │ICE  │ │Trd  │ │路径验证 │  │
    │  │Lite │ │IPv6 │ │        │  │
    │  └─────┘ └─────┘ └─────────┘  │
    └───────────────────────────────┘
                    │
    ┌───────────────▼───────────────┐
    │         nyx-fec               │
    │                               │
    │  ┌─────────┐ ┌─────────────┐  │
    │  │Reed-Sol │ │RaptorQ      │  │
    │  │omon     │ │自适应       │  │
    │  └─────────┘ └─────────────┘  │
    │  ┌─────────┐ ┌─────────────┐  │
    │  │时序混淆 │ │填充         │  │
    │  │        │ │1280B固定    │  │
    │  └─────────┘ └─────────────┘  │
    └───────────────────────────────┘
```

## 组件详细规范

### 1. nyx-core - 核心库

#### 1.1 配置管理 (`config.rs`)
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

**主要功能:**
- TOML格式配置文件加载
- 环境变量配置覆盖
- 热重载支持（文件监视）
- 配置验证和默认值应用

#### 1.2 错误处理 (`error.rs`)
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

**特点:**
- 使用`thiserror`的统一错误类型
- 错误链支持
- 与日志输出集成
- 调试信息保留

#### 1.3 类型定义 (`types.rs`)
```rust
pub type NodeId = [u8; 32];
pub type SessionKey = [u8; 32];
pub type ConnectionId = [u8; 12];

pub struct StreamId(pub u64);
pub struct PathId(pub u8);
```

#### 1.4 沙箱功能

**Linux (seccomp-bpf):**
- 系统调用限制
- 文件访问控制
- 网络访问限制

**OpenBSD (pledge/unveil):**
- 进程权限限制
- 文件系统访问控制
- 网络权限管理

#### 1.5 国际化支持 (`i18n.rs`)
```rust
pub fn localize(lang: &str, key: &str, args: Option<&HashMap<String, String>>) -> Result<String, NyxError>
```

**支持语言:**
- 日语 (ja)
- 英语 (en)
- 中文 (zh)

### 2. nyx-crypto - 密码引擎

#### 2.1 Noise协议实现 (`noise.rs`)
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

**实现模式:**
- Noise_Nyx: `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`
- 握手: 3轮认证
- 前向保密: 临时密钥使用

#### 2.2 HKDF实现 (`kdf.rs`)
```rust
pub enum KdfLabel {
    Handshake,
    Application,
    Rekey,
    Export,
}

pub fn hkdf_expand(prk: &[u8], label: KdfLabel, length: usize) -> Vec<u8>
```

**安全特性:**
- 标签语义
- 抗误用设计
- RFC 5869兼容

#### 2.3 AEAD加密 (`aead.rs`)
```rust
pub fn encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
pub fn decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
```

**使用算法:**
- ChaCha20-Poly1305
- 96位nonce
- 128位认证标签

#### 2.4 密钥存储 (`keystore.rs`)
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

**安全功能:**
- 内存密钥管理
- 自动清零
- 访问控制
- 加密存储

#### 2.5 后量子密码学

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

### 3. nyx-stream - 流媒体层

#### 3.1 帧处理 (`frame.rs`)
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

#### 3.2 拥塞控制 (`congestion.rs`)
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

**算法:**
- 类似TCP Cubic
- 基于RTT测量
- 数据包丢失检测
- 自适应窗口大小

#### 3.3 多路径支持 (`receiver.rs`)
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

#### 3.4 插件系统 (`plugin.rs`)
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

### 4. nyx-mix - 混合路由

#### 4.1 路径构建 (`lib.rs`)
```rust
pub struct WeightedPathBuilder<'a> {
    candidates: &'a [Candidate],
    alpha: f64, // 延迟与带宽权重
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

**选择算法:**
- 加权概率选择
- 延迟和带宽考虑
- 避免重复节点

#### 4.2 掩护流量 (`cover.rs`)
```rust
pub struct CoverGenerator {
    lambda: f64, // 每秒事件数
    rng: ThreadRng,
}

impl CoverGenerator {
    pub fn new(lambda: f64) -> Self;
    pub fn next_delay(&self) -> Duration;
}
```

**统计特性:**
- 泊松分布
- 指数分布间隔
- 可配置速率

#### 4.3 自适应掩护流量 (`cover_adaptive.rs`)
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

#### 4.4 cMix集成 (`cmix.rs`)
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

### 5. nyx-transport - 传输层

#### 5.1 UDP池 (`lib.rs`)
```rust
pub struct UdpPool {
    socket: Arc<UdpSocket>,
}

impl UdpPool {
    pub async fn bind(port: u16) -> std::io::Result<Self>;
    pub fn socket(&self) -> Arc<UdpSocket>;
}
```

**功能:**
- SO_REUSEPORT使用
- 非阻塞I/O
- 高效套接字共享

#### 5.2 QUIC扩展 (`quic.rs`)
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

### 6. nyx-fec - 前向纠错

#### 6.1 Reed-Solomon (`lib.rs`)
```rust
pub struct NyxFec {
    rs: ReedSolomon,
}

impl NyxFec {
    pub fn new() -> Self; // 10数据 + 3奇偶校验片
    pub fn encode(&self, shards: &mut [&mut [u8]]) -> Result<(), RSError>;
    pub fn reconstruct(&self, shards: &mut [&mut [u8]], present: &mut [bool]) -> Result<(), RSError>;
}
```

**参数:**
- 数据片: 10
- 奇偶校验片: 3
- 片大小: 1280字节

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

### 7. nyx-daemon - 守护进程服务

#### 7.1 gRPC API (`main.rs`)
```rust
#[async_trait]
impl NyxControl for ControlService {
    async fn get_info(&self, request: Request<Empty>) -> Result<Response<NodeInfo>, Status>;
    async fn open_stream(&self, request: Request<OpenRequest>) -> Result<Response<StreamResponse>, Status>;
    async fn close_stream(&self, request: Request<StreamId>) -> Result<Response<Empty>, Status>;
    async fn get_health(&self, request: Request<HealthRequest>) -> Result<Response<HealthResponse>, Status>;
    // ... 其他API
}
```

#### 7.2 流管理 (`stream_manager.rs`)
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

#### 7.3 指标收集 (`metrics.rs`)
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

### 8. nyx-cli - 命令行工具

#### 8.1 命令结构 (`main.rs`)
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

## 性能特征

### 吞吐量性能
- **单路径**: 最高100 Mbps
- **多路径**: 随路径数量扩展
- **延迟开销**: 5跳路由增加50-200ms

### 资源使用
- **内存**: 基础约50MB
- **CPU**: 正常负载下小于5%
- **网络**: FEC和掩护流量30%开销

### 可扩展性
- **并发连接**: 每个守护进程10,000+
- **网络规模**: 已在1,000+节点测试
- **地理分布**: 全球部署就绪

## 安全分析

### 威胁模型
1. **网络监控**: ISP和政府通信监控
2. **元数据分析**: 从通信模式推断信息
3. **量子计算机**: 未来密码破解威胁
4. **端点攻击**: 对客户端和服务器的直接攻击

### 对策
1. **混合网络路由**: 通信路径匿名化
2. **掩护流量**: 通信模式隐藏
3. **后量子密码学**: 采用量子抗性密码
4. **沙箱**: 系统级隔离

### 密码学保证
- **完美前向保密**: 会话密钥前向保密
- **后妥协恢复**: 从密钥泄露中恢复
- **认证加密**: 加密和认证的结合
- **密钥派生**: 安全密钥派生

## 详细配置文件

### 基本配置 (`nyx.toml`)
```toml
# 网络设置
listen_port = 43300
node_id = "auto"  # 或64字符十六进制字符串

# 日志设置
log_level = "info"  # trace, debug, info, warn, error
log_format = "json"  # json, pretty

# 密码设置
[crypto]
post_quantum = true
kyber_enabled = true
bike_enabled = false
noise_pattern = "Nyx"

# 混合路由设置
[mix]
hop_count = 5
min_hops = 3
max_hops = 7
cover_traffic_rate = 10.0  # 数据包/秒
adaptive_cover = true
target_utilization = 0.4

# 传输设置
[transport]
quic_enabled = true
tcp_fallback = true
udp_buffer_size = 65536
max_connections = 10000

# FEC设置
[fec]
reed_solomon_enabled = true
raptorq_enabled = true
data_shards = 10
parity_shards = 3

# 移动优化
[mobile]
low_power_mode = false
battery_optimization = true
screen_off_detection = true
push_notifications = true

# 遥测设置
[telemetry]
enabled = true
endpoint = "http://localhost:4317"
sample_rate = 0.1

# 合规设置
[compliance]
level = "Plus"  # Core, Plus, Full
audit_logging = true
data_retention_days = 30
```

### 高级配置
```toml
# 性能调优
[performance]
worker_threads = 8
max_blocking_threads = 512
stack_size = 2097152  # 2MB

# 安全设置
[security]
sandbox_enabled = true
memory_limit = "1GB"
file_descriptor_limit = 1024

# 插件设置
[plugins]
enabled = true
plugin_dir = "/opt/nyx/plugins"
allowed_plugins = ["geo-stats", "bandwidth-monitor"]

# DHT设置
[dht]
enabled = true
bootstrap_nodes = [
    "bootstrap1.nyx.network:43300",
    "bootstrap2.nyx.network:43300"
]
replication_factor = 20
```

## 开发者指南

### 构建环境设置
```bash
# 安装Rust工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# 必需组件
rustup component add clippy rustfmt

# 克隆项目
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# 安装依赖
cargo fetch
```

### 开发工作流程
```bash
# 开发构建
cargo build

# 运行测试
cargo test --all

# 运行linter
cargo clippy -- -D warnings

# 格式化代码
cargo fmt

# 生成文档
cargo doc --open
```

### 调试程序
```bash
# 设置日志级别
RUST_LOG=debug cargo run --bin nyx-daemon

# 特定模块日志
RUST_LOG=nyx_crypto=trace cargo run --bin nyx-daemon

# 启用回溯
RUST_BACKTRACE=1 cargo run --bin nyx-daemon
```

### 测试策略
1. **单元测试**: 函数和方法行为验证
2. **集成测试**: 模块间交互测试
3. **一致性测试**: 协议规范符合性验证
4. **性能测试**: 吞吐量和延迟测量
5. **安全测试**: 密码实现验证

## 故障排除

### 常见问题

#### 1. 守护进程无法启动
```bash
# 检查端口使用情况
sudo netstat -tulpn | grep 43300

# 验证配置文件
cargo run --bin nyx-daemon -- --check-config

# 检查权限
ls -la /opt/nyx/
```

#### 2. 连接失败
```bash
# 检查网络连接
ping target-host

# 检查防火墙
sudo ufw status

# 检查DNS解析
nslookup target-host
```

#### 3. 性能不佳
```bash
# 检查系统资源
top
iostat
netstat -i

# 检查NyxNet统计
cargo run --bin nyx-cli -- status --detailed
```

### 日志分析
```bash
# 提取错误日志
journalctl -u nyx-daemon | grep ERROR

# 性能日志
journalctl -u nyx-daemon | grep "performance"

# 安全事件
journalctl -u nyx-daemon | grep "security"
```

## API参考

### gRPC服务定义
```protobuf
service NyxControl {
  // 节点信息
  rpc GetInfo(Empty) returns (NodeInfo);
  rpc GetHealth(HealthRequest) returns (HealthResponse);
  
  // 流管理
  rpc OpenStream(OpenRequest) returns (StreamResponse);
  rpc CloseStream(StreamId) returns (Empty);
  rpc GetStreamStats(StreamId) returns (StreamStats);
  rpc ListStreams(Empty) returns (stream StreamStats);
  
  // 事件流
  rpc SubscribeEvents(EventFilter) returns (stream Event);
  rpc SubscribeStats(StatsRequest) returns (stream StatsUpdate);
  
  // 配置
  rpc UpdateConfig(ConfigUpdate) returns (ConfigResponse);
  rpc ReloadConfig(Empty) returns (ConfigResponse);
  
  // 网络拓扑
  rpc BuildPath(PathRequest) returns (PathResponse);
  rpc GetPaths(Empty) returns (stream PathInfo);
  rpc GetTopology(Empty) returns (NetworkTopology);
  rpc GetPeers(Empty) returns (stream PeerInfo);
}
```

### SDK使用示例

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

## 部署指南

### Docker部署
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

### Kubernetes部署
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

### 监控设置
```yaml
# Prometheus配置
scrape_configs:
  - job_name: 'nyx-daemon'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

这份综合文档涵盖了NyxNet的所有技术规范和使用方法。 