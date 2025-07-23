# NyxNet - 高度な匿名通信プロトコル実装

## 概要

NyxNetは、最新の暗号技術とミックスネット技術を組み合わせた次世代匿名通信プロトコルの参照実装です。量子コンピュータ耐性暗号、マルチパス通信、適応的カバートラフィックなど、最先端の技術を統合して高度なプライバシー保護を実現します。

## 詳細アーキテクチャ

### システム全体構成

```
┌─────────────────────────────────────────────────────────────┐
│                    アプリケーション層                          │
├─────────────────┬─────────────────┬─────────────────────────┤
│   nyx-cli       │   nyx-sdk       │   カスタムアプリ           │
│   (CLI Tool)    │   (SDK)         │   (Third-party)        │
└─────────┬───────┴─────────┬───────┴─────────────────────────┘
          │ gRPC             │ SDK API
          └─────────────┬────┘
                        │
          ┌─────────────▼─────────────┐
          │      nyx-daemon           │
          │   (制御・管理サービス)        │
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

## コンポーネント詳細仕様

### 1. nyx-core - コアライブラリ

#### 1.1 設定管理 (`config.rs`)
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

**主要機能:**
- TOML形式の設定ファイル読み込み
- 環境変数からの設定オーバーライド
- ホットリロード対応（ファイル監視）
- 設定検証とデフォルト値適用

#### 1.2 エラーハンドリング (`error.rs`)
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

**特徴:**
- `thiserror`を使用した統一エラー型
- エラーチェーン対応
- ログ出力との連携
- デバッグ情報の保持

#### 1.3 型定義 (`types.rs`)
```rust
pub type NodeId = [u8; 32];
pub type SessionKey = [u8; 32];
pub type ConnectionId = [u8; 12];

pub struct StreamId(pub u64);
pub struct PathId(pub u8);
```

#### 1.4 サンドボックス機能

**Linux (seccomp-bpf):**
- システムコール制限
- ファイルアクセス制御
- ネットワークアクセス制限

**OpenBSD (pledge/unveil):**
- プロセス権限制限
- ファイルシステムアクセス制御
- ネットワーク権限管理

#### 1.5 国際化対応 (`i18n.rs`)
```rust
pub fn localize(lang: &str, key: &str, args: Option<&HashMap<String, String>>) -> Result<String, NyxError>
```

**対応言語:**
- 日本語 (ja)
- 英語 (en)
- 中国語 (zh)

### 2. nyx-crypto - 暗号化エンジン

#### 2.1 Noise Protocol実装 (`noise.rs`)
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

**実装パターン:**
- Noise_Nyx: `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`
- ハンドシェイク: 3ラウンド認証
- 前方秘匿性: エフェメラルキー使用

#### 2.2 HKDF実装 (`kdf.rs`)
```rust
pub enum KdfLabel {
    Handshake,
    Application,
    Rekey,
    Export,
}

pub fn hkdf_expand(prk: &[u8], label: KdfLabel, length: usize) -> Vec<u8>
```

**セキュリティ特性:**
- ラベル付きセマンティクス
- 誤用耐性設計
- RFC 5869準拠

#### 2.3 AEAD暗号化 (`aead.rs`)
```rust
pub fn encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
pub fn decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8], ad: &[u8]) -> Result<Vec<u8>>
```

**使用アルゴリズム:**
- ChaCha20-Poly1305
- 96ビットnonce
- 128ビット認証タグ

#### 2.4 キーストア (`keystore.rs`)
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

**セキュリティ機能:**
- メモリ内キー管理
- 自動ゼロ化
- アクセス制御
- 暗号化ストレージ

#### 2.5 Post-Quantum暗号

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

### 3. nyx-stream - ストリーミングレイヤー

#### 3.1 フレーム処理 (`frame.rs`)
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

#### 3.2 輻輳制御 (`congestion.rs`)
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

**アルゴリズム:**
- TCP Cubic類似
- RTT測定ベース
- パケットロス検出
- 適応的ウィンドウサイズ

#### 3.3 マルチパス対応 (`receiver.rs`)
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

#### 3.4 プラグインシステム (`plugin.rs`)
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

### 4. nyx-mix - ミックスルーティング

#### 4.1 パス構築 (`lib.rs`)
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

**選択アルゴリズム:**
- 重み付き確率選択
- レイテンシ・帯域幅考慮
- 重複ノード回避

#### 4.2 カバートラフィック (`cover.rs`)
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

**統計特性:**
- ポアソン分布
- 指数分布間隔
- 設定可能レート

#### 4.3 適応型カバートラフィック (`cover_adaptive.rs`)
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

#### 4.4 cMix統合 (`cmix.rs`)
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

### 5. nyx-transport - トランスポート層

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

**機能:**
- SO_REUSEPORT使用
- 非ブロッキングI/O
- 効率的なソケット共有

#### 5.2 QUIC拡張 (`quic.rs`)
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

### 6. nyx-fec - 前方誤り訂正

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

**パラメータ:**
- データシャード: 10
- パリティシャード: 3
- シャードサイズ: 1280バイト

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

### 7. nyx-daemon - デーモンサービス

#### 7.1 gRPC API (`main.rs`)
```rust
#[async_trait]
impl NyxControl for ControlService {
    async fn get_info(&self, request: Request<Empty>) -> Result<Response<NodeInfo>, Status>;
    async fn open_stream(&self, request: Request<OpenRequest>) -> Result<Response<StreamResponse>, Status>;
    async fn close_stream(&self, request: Request<StreamId>) -> Result<Response<Empty>, Status>;
    async fn get_health(&self, request: Request<HealthRequest>) -> Result<Response<HealthResponse>, Status>;
    // ... 他のAPI
}
```

#### 7.2 ストリーム管理 (`stream_manager.rs`)
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

#### 7.3 メトリクス収集 (`metrics.rs`)
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

### 8. nyx-cli - コマンドラインツール

#### 8.1 コマンド構造 (`main.rs`)
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

## パフォーマンス特性

### スループット性能
- **単一パス**: 最大100 Mbps
- **マルチパス**: パス数に応じたスケーリング
- **レイテンシオーバーヘッド**: 5ホップで50-200ms追加

### リソース使用量
- **メモリ**: ベースで約50MB
- **CPU**: 通常負荷で5%未満
- **ネットワーク**: FECとカバートラフィックで30%オーバーヘッド

### スケーラビリティ
- **同時接続**: デーモンあたり10,000+
- **ネットワークサイズ**: 1,000+ノードでテスト済み
- **地理的分散**: グローバル展開対応

## セキュリティ分析

### 脅威モデル
1. **ネットワーク監視**: ISP、政府機関による通信監視
2. **メタデータ分析**: 通信パターンからの情報推定
3. **量子コンピュータ**: 将来の暗号解読脅威
4. **エンドポイント攻撃**: クライアント・サーバーへの直接攻撃

### 対策
1. **ミックスネットルーティング**: 通信経路の匿名化
2. **カバートラフィック**: 通信パターンの隠蔽
3. **Post-Quantum暗号**: 量子耐性暗号の採用
4. **サンドボックス**: システムレベルの隔離

### 暗号学的保証
- **Perfect Forward Secrecy**: セッション鍵の前方秘匿性
- **Post-Compromise Recovery**: 鍵漏洩からの回復
- **Authenticated Encryption**: 暗号化と認証の組み合わせ
- **Key Derivation**: 安全な鍵導出

## 設定ファイル詳細

### 基本設定 (`nyx.toml`)
```toml
# ネットワーク設定
listen_port = 43300
node_id = "auto"  # または64文字のhex文字列

# ログ設定
log_level = "info"  # trace, debug, info, warn, error
log_format = "json"  # json, pretty

# 暗号化設定
[crypto]
post_quantum = true
kyber_enabled = true
bike_enabled = false
noise_pattern = "Nyx"

# ミックスルーティング設定
[mix]
hop_count = 5
min_hops = 3
max_hops = 7
cover_traffic_rate = 10.0  # packets/second
adaptive_cover = true
target_utilization = 0.4

# トランスポート設定
[transport]
quic_enabled = true
tcp_fallback = true
udp_buffer_size = 65536
max_connections = 10000

# FEC設定
[fec]
reed_solomon_enabled = true
raptorq_enabled = true
data_shards = 10
parity_shards = 3

# モバイル最適化
[mobile]
low_power_mode = false
battery_optimization = true
screen_off_detection = true
push_notifications = true

# テレメトリ設定
[telemetry]
enabled = true
endpoint = "http://localhost:4317"
sample_rate = 0.1

# コンプライアンス設定
[compliance]
level = "Plus"  # Core, Plus, Full
audit_logging = true
data_retention_days = 30
```

### 高度な設定
```toml
# パフォーマンスチューニング
[performance]
worker_threads = 8
max_blocking_threads = 512
stack_size = 2097152  # 2MB

# セキュリティ設定
[security]
sandbox_enabled = true
memory_limit = "1GB"
file_descriptor_limit = 1024

# プラグイン設定
[plugins]
enabled = true
plugin_dir = "/opt/nyx/plugins"
allowed_plugins = ["geo-stats", "bandwidth-monitor"]

# DHT設定
[dht]
enabled = true
bootstrap_nodes = [
    "bootstrap1.nyx.network:43300",
    "bootstrap2.nyx.network:43300"
]
replication_factor = 20
```

## 開発者ガイド

### ビルド環境構築
```bash
# Rust toolchainインストール
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# 必要なコンポーネント
rustup component add clippy rustfmt

# プロジェクトクローン
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# 依存関係インストール
cargo fetch
```

### 開発ワークフロー
```bash
# 開発ビルド
cargo build

# テスト実行
cargo test --all

# リント実行
cargo clippy -- -D warnings

# フォーマット
cargo fmt

# ドキュメント生成
cargo doc --open
```

### デバッグ手順
```bash
# ログレベル設定
RUST_LOG=debug cargo run --bin nyx-daemon

# 特定モジュールのログ
RUST_LOG=nyx_crypto=trace cargo run --bin nyx-daemon

# Backtrace有効化
RUST_BACKTRACE=1 cargo run --bin nyx-daemon
```

### テスト戦略
1. **単体テスト**: 各関数・メソッドの動作確認
2. **統合テスト**: モジュール間の連携テスト
3. **適合性テスト**: プロトコル仕様準拠確認
4. **性能テスト**: スループット・レイテンシ測定
5. **セキュリティテスト**: 暗号実装の検証

## トラブルシューティング

### よくある問題

#### 1. デーモンが起動しない
```bash
# ポート使用状況確認
sudo netstat -tulpn | grep 43300

# 設定ファイル確認
cargo run --bin nyx-daemon -- --check-config

# 権限確認
ls -la /opt/nyx/
```

#### 2. 接続が失敗する
```bash
# ネットワーク接続確認
ping target-host

# ファイアウォール確認
sudo ufw status

# DNS解決確認
nslookup target-host
```

#### 3. パフォーマンスが低い
```bash
# システムリソース確認
top
iostat
netstat -i

# NyxNet統計確認
cargo run --bin nyx-cli -- status --detailed
```

### ログ分析
```bash
# エラーログ抽出
journalctl -u nyx-daemon | grep ERROR

# パフォーマンスログ
journalctl -u nyx-daemon | grep "performance"

# セキュリティイベント
journalctl -u nyx-daemon | grep "security"
```

この詳細ドキュメントは、NyxNetの完全な技術仕様と使用方法を網羅しています。 