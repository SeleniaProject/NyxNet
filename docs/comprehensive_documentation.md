# NyxNet - 包括的プロジェクトドキュメント

このドキュメントは、NyxNetプロジェクトの全体的な理解を目的とした包括的なドキュメントセットです。

## 目次

1. [プロジェクト概要](#プロジェクト概要)
2. [システムアーキテクチャ設計](#システムアーキテクチャ設計)
3. [主要機能詳細](#主要機能詳細)
4. [API / 外部インターフェースリファレンス](#api--外部インターフェースリファレンス)
5. [開発環境セットアップガイド](#開発環境セットアップガイド)

---

## プロジェクト概要

**NyxNet**は、次世代の匿名通信プロトコルの実装で、最先端の暗号技術とミックスネットワーク技術を組み合わせて、高性能でプライバシーを保護するネットワーク通信を提供します。Rustで実装され、メモリ安全性、量子耐性、クロスプラットフォーム対応を実現しています。

### プロジェクトの核心

NyxNetは以下の課題を解決します：

- **ネットワーク監視とトラフィック分析**からの保護
- **メタデータ相関攻撃**に対する堅牢な防御
- **ポスト量子暗号時代**に向けた準備
- **高性能**と**匿名性**の両立

### 技術スタックの概要

- **主要言語**: Rust (memory-safe, `#![forbid(unsafe_code)]`)
- **暗号ライブラリ**: 純Rust実装（Kyber1024、X25519、ChaCha20Poly1305）
- **ネットワーク**: libp2p、QUIC、UDP、TCP with fallback
- **プロトコル**: Noise Protocol Framework、gRPC
- **メッセージフォーマット**: Protocol Buffers
- **設定**: TOML
- **テレメトリ**: Prometheus、OpenTelemetry
- **プラットフォーム**: Windows、Linux、macOS、WebAssembly

### 高レベルのディレクトリ構造

- **`nyx-core/`** - 設定管理、エラー処理、型システム、サンドボックス
- **`nyx-crypto/`** - Noise Protocol、AEAD、ポスト量子暗号
- **`nyx-mix/`** - ミックスルーティング、カバートラフィック、匿名化
- **`nyx-stream/`** - ストリーム多重化、フロー制御、フレーム処理
- **`nyx-transport/`** - ネットワークI/O、パケット処理、NAT traversal
- **`nyx-fec/`** - Reed-Solomon、RaptorQ前方誤り訂正
- **`nyx-daemon/`** - メインサービス、gRPC API、レイヤー統合
- **`nyx-cli/`** - コマンドライン管理ツール
- **`nyx-sdk/`** - 開発者向けSDK
- **`formal/`** - TLA+形式検証モデル
- **`docs/`** - 多言語対応ドキュメント

---

## システムアーキテクチャ設計

### 主要なコンポーネント

#### 1. アプリケーションレイヤー

- **nyx-cli**: CLIツール（接続、ステータス監視、ベンチマーク）
- **nyx-sdk**: 開発者向けSDK（Rust、WASM、モバイルFFI）
- **サードパーティアプリ**: カスタムアプリケーション統合

#### 2. 制御・管理レイヤー

- **nyx-daemon**: 
  - セッション管理とストリーム管理
  - gRPC APIサーバー（127.0.0.1:50051）
  - メトリクス収集（Prometheus）
  - レイヤー間の調整

#### 3. プロトコルスタック

```
┌─────────────────────────────────────┐
│           Application               │
├─────────────────────────────────────┤
│              Mix Layer              │ ← 匿名化、カバートラフィック
├─────────────────────────────────────┤
│             Stream Layer            │ ← 多重化、フロー制御
├─────────────────────────────────────┤
│             Crypto Layer            │ ← 暗号化、鍵管理
├─────────────────────────────────────┤
│              FEC Layer              │ ← 前方誤り訂正
├─────────────────────────────────────┤
│           Transport Layer           │ ← UDP/QUIC/TCP
└─────────────────────────────────────┘
```

### 相互作用とデータフロー

#### データ送信フロー

1. **アプリケーション** → アプリデータを`nyx-daemon`に送信
2. **Stream Layer** → フレーム化、多重化、フロー制御
3. **Crypto Layer** → Noise Protocolによる暗号化
4. **FEC Layer** → Reed-Solomon/RaptorQエラー訂正
5. **Mix Layer** → ミックスルーティング、カバートラフィック
6. **Transport Layer** → UDP/QUIC/TCPでの送信

#### 鍵管理フロー

- **Noise Handshake**: X25519 + Kyber1024 hybrid
- **Perfect Forward Secrecy**: 定期的な鍵更新
- **Post-Compromise Recovery**: HKDF-based key rekeying

### アーキテクチャパターン

#### レイヤードアーキテクチャ

各層は明確に分離され、上位層は下位層のサービスのみを利用：

- 依存関係の明確化（`LayerManager`による管理）
- テスタビリティの向上
- 保守性の確保

#### イベント駆動型

- **非同期メッセージング**: Tokio channels
- **リアクティブな処理**: イベントベースの状態遷移
- **メトリクス収集**: リアルタイムテレメトリ

#### プラグインアーキテクチャ

- **動的プラグイン**: sandboxed plugin system
- **セキュリティ**: seccomp/pledge/unveil制限
- **拡張性**: カスタム機能の追加

### 外部システム連携

- **Bootstrap Nodes**: DHT peer discovery
- **Prometheus**: メトリクス収集
- **gRPC**: 制御API
- **libp2p**: P2Pネットワーキング

---

## 主要機能詳細

### 1. ミックスネットワークルーティングシステム

#### 機能の目的とユーザーのユースケース

通信パスを匿名化し、トラフィック分析攻撃から保護。ユーザーの通信が複数のノードを経由することで、送信者と受信者の関連性を隠します。

#### 関連する主要なファイル/モジュール

- **`nyx-mix/src/lib.rs`**: ミックスルーティングの核心
- **`nyx-mix/src/cmix.rs`**: cMixアルゴリズム実装
- **`nyx-mix/src/cover.rs`**: カバートラフィック生成
- **`nyx-mix/src/larmix.rs`**: レイテンシ認識ルーティング
- **`nyx-daemon/src/path_builder.rs`**: パス構築ロジック

#### コアロジック

1. **パス選択アルゴリズム**:

```rust
pub struct WeightedPathBuilder {
    candidates: &[Candidate],
    alpha: f64,  // レイテンシ vs 帯域幅のバランス
}
```

重み計算: `w = (1/latency_ms) * α + (bandwidth_mbps/MAX_BW) * (1-α)`

2. **カバートラフィック**:
   - Poisson分布による送信間隔
   - 適応的な生成率（target utilization: 0.2-0.6）
   - バッテリー効率を考慮したモバイル最適化

3. **cMix統合**:
   - バッチサイズ100での集合暗号化
   - VDF（Verifiable Delay Function）によるタイミング制御
   - RSA accumulatorによる証明

#### データモデルと永続化

```rust
pub struct Candidate {
    pub id: NodeId,
    pub latency_ms: f64,
    pub bandwidth_mbps: f64,
}
```

ピア情報はDHTで分散保存、ローカルキャッシュで高速アクセス。

#### エラーハンドリング

- **パス構築失敗**: 代替ノード選択、グレースフルデグラデーション
- **ノード障害**: 自動迂回、冗長パス
- **ネットワーク分断**: DHT bootstrap nodeによる復旧

#### バリデーション

- ノードID形式検証（32バイトハッシュ）
- レイテンシ閾値チェック（最大10秒）
- 帯域幅制限確認

#### テスト

- **単体テスト**: パス選択アルゴリズムの正確性
- **統合テスト**: 実ネットワークでのルーティング検証
- **プロパティテスト**: 匿名性保証の数学的検証

### 2. ポスト量子暗号システム

#### 機能の目的とユーザーのユースケース

量子コンピュータ攻撃に対する耐性を提供。将来の暗号学的脅威からユーザーデータを保護します。

#### 関連する主要なファイル/モジュール

- **`nyx-crypto/src/noise.rs`**: Noise Protocol実装
- **`nyx-crypto/src/kyber.rs`**: Kyber1024実装
- **`nyx-crypto/src/kdf.rs`**: 鍵導出関数
- **`nyx-crypto/src/aead.rs`**: 認証付き暗号化

#### コアロジック

1. **ハイブリッド鍵交換**:

```
Handshake Pattern:
<- s
-> e, ee_dh25519, ee_kyber, s, ss
<- se_dh25519, se_kyber, es, ee_dh25519, ee_kyber
```

2. **鍵導出**:
   - HKDF-Extract(SHA-512, concat(dh25519_secret, kyber_secret))
   - Perfect Forward Secrecy
   - Post-Compromise Recovery

3. **暗号化**:
   - ChaCha20Poly1305 AEAD
   - 32バイトセッション鍵
   - Nonce管理とリプレイ攻撃防止

#### データモデルと永続化

```rust
pub struct SessionKey([u8; 32]);
pub struct NoiseSession {
    send_cipher: ChaCha20Poly1305,
    recv_cipher: ChaCha20Poly1305,
    send_nonce: u64,
    recv_nonce: u64,
}
```

鍵は揮発性メモリのみに保存、zeroize on drop。

#### エラーハンドリング

- **鍵交換失敗**: 古典暗号へのフォールバック不可（セキュリティ優先）
- **復号化エラー**: セッション再構築
- **Nonce枯渇**: 自動リキー

#### バリデーション

- 公開鍵フォーマット検証
- 暗号文完全性チェック
- タイミング攻撃対策

#### テスト

- RFC test vectorsによる適合性
- プロパティベーステスト
- 暗号学的強度検証

### 3. 高性能ストリーミングシステム

#### 機能の目的とユーザーのユースケース

複数のデータストリームを効率的に多重化し、ネットワーク変動に適応的に対応。リアルタイム通信とファイル転送の両方をサポート。

#### 関連する主要なファイル/モジュール

- **`nyx-stream/src/lib.rs`**: ストリームレイヤー中核
- **`nyx-stream/src/congestion.rs`**: 輻輳制御
- **`nyx-stream/src/flow_controller.rs`**: フロー制御
- **`nyx-stream/src/frame.rs`**: フレーム処理
- **`nyx-stream/src/tx.rs`**: 送信キュー管理

#### コアロジック

1. **適応的輻輳制御**:
   - BBR-like algorithm
   - RTT/bandwidth estimation
   - Multipath load balancing

2. **フロー制御**:
   - Stream-level back-pressure
   - Connection-level window management
   - Prioritization support

3. **マルチパス通信**:
   - Path ID routing (0-255)
   - Weighted Round Robin scheduling
   - Reordering buffer管理

#### データモデルと永続化

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

フレームヘッダー（14バイト）+ ペイロード、最大MTU 1280バイト対応。

#### エラーハンドリング

- **パケット損失**: 自動再送、FECによる復旧
- **順序不整合**: リオーダリングバッファ
- **接続断**: マルチパス冗長性
- **バックプレッシャー**: フロー制御による調整

#### バリデーション

- ストリームID範囲チェック
- フレームサイズ制限
- シーケンス番号検証

#### テスト

- **性能テスト**: スループット、レイテンシ測定
- **ストレステスト**: 大量接続、高負荷環境
- **ネットワークシミュレーション**: パケット損失、遅延変動

---

## API / 外部インターフェースリファレンス

### gRPCサービス定義

#### NyxControl Service

**エンドポイント**: `127.0.0.1:50051` (TCP)

#### 主要APIエンドポイント

##### 1. ノード情報取得

```protobuf
rpc GetInfo(Empty) returns (NodeInfo);
```

**リクエストパラメータ**: なし

**レスポンス構造**:
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

**HTTPステータスコード**:
- `200 OK`: 正常取得
- `500 Internal Server Error`: ノード情報取得失敗

##### 2. ストリーム管理

```protobuf
rpc OpenStream(OpenRequest) returns (StreamResponse);
rpc CloseStream(StreamId) returns (Empty);
rpc GetStreamStats(StreamId) returns (StreamStats);
rpc ListStreams(Empty) returns (stream StreamStats);
```

**OpenRequest構造**:
- `target` (string): 接続先（"example.com:80"）
- `multipath` (bool): マルチパス有効化
- `cover_traffic` (bool): カバートラフィック有効化
- `hop_count` (uint32): ホップ数（3-7、省略可）
- `priority` (uint32): ストリーム優先度（0-255）

**StreamResponse構造**:
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

##### 3. リアルタイム統計

```protobuf
rpc SubscribeStats(StatsRequest) returns (stream StatsUpdate);
```

**StatsRequest構造**:
- `interval_ms` (uint32): 更新間隔（ミリ秒）
- `metrics` (string[]): 監視対象メトリクス配列

**対応メトリクス**:
- `"stream_count"`: アクティブストリーム数
- `"bandwidth_usage"`: 帯域使用量
- `"latency_avg"`: 平均レイテンシ
- `"error_rate"`: エラー率

##### 4. ヘルスチェック

```protobuf
rpc GetHealth(HealthRequest) returns (HealthResponse);
```

**HealthRequest構造**:
- `include_details` (bool): 詳細情報含有フラグ

**HealthResponse構造**:
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

##### 5. パス管理

```protobuf
rpc BuildPath(PathRequest) returns (PathResponse);
rpc GetPaths(Empty) returns (stream PathInfo);
```

**PathRequest構造**:
- `target` (string): 宛先ホスト
- `hops` (uint32): 希望ホップ数
- `strategy` (string): "latency_optimized" | "bandwidth_optimized" | "reliability_optimized"

#### 認証・認可

**認証メカニズム**:
- **gRPC Metadata**: `authorization: Bearer <api_key>`
- **Unix Socket**: ローカル接続での権限制御
- **mTLS**: プロダクション環境での相互認証

**認可レベル**:
- `READ`: 情報取得のみ
- `STREAM`: ストリーム操作
- `ADMIN`: 設定変更、システム制御

#### 共通エラー応答

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

**Nyx固有エラーコード**:
- `NYX_CRYPTO_ERROR` (100): 暗号化エラー
- `NYX_MIX_ROUTING_FAILED` (101): ルーティング失敗
- `NYX_INSUFFICIENT_PEERS` (102): ピア不足
- `NYX_STREAM_LIMIT_EXCEEDED` (103): ストリーム数上限
- `NYX_INVALID_HOP_COUNT` (104): ホップ数範囲外

### SDK APIリファレンス

#### Rust SDK使用例

```rust
use nyx_sdk::{NyxClient, StreamConfig, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // クライアント接続
    let client = NyxClient::connect("http://127.0.0.1:50051").await?;
    
    // ストリーム設定
    let config = StreamConfig {
        target: "example.com:80".to_string(),
        multipath: true,
        cover_traffic: true,
        hop_count: Some(5),
        priority: 128,
    };
    
    // ストリーム開始
    let mut stream = client.open_stream(config).await?;
    
    // データ送信
    stream.write(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await?;
    
    // レスポンス受信
    let response = stream.read().await?;
    println!("Response: {}", String::from_utf8_lossy(&response));
    
    // ストリーム終了
    stream.close().await?;
    
    Ok(())
}
```

#### JavaScript SDK使用例

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

#### Python SDK使用例

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

## 開発環境セットアップガイド

### 前提条件

#### 必須ソフトウェア

**Rust Toolchain**: 1.70以上
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable
rustup component add clippy rustfmt
```

**Protocol Buffers Compiler**: 3.20以上
- Linux: `sudo apt-get install protobuf-compiler`
- Windows: `choco install protoc`
- macOS: `brew install protobuf`

**Java**: 17以上（TLA+形式検証用）
```bash
# Ubuntu
sudo apt-get install openjdk-17-jdk

# Windows
choco install openjdk17

# macOS
brew install openjdk@17
```

**Python**: 3.9以上（検証スクリプト用）
```bash
python3 --version  # 3.9以上であることを確認
pip3 install --upgrade pip
```

#### プラットフォーム固有要件

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
# Chocolatey経由でインストール
choco install protoc cmake visualstudio2022buildtools

# または手動でProtocをダウンロード・配置
# https://github.com/protocolbuffers/protobuf/releases
```

**macOS**:
```bash
brew install protobuf pkg-config cmake
xcode-select --install  # Xcode Command Line Tools
```

### プロジェクトのクローン

```bash
git clone https://github.com/SeleniaProject/NyxNet.git
cd NyxNet

# サブモジュールがある場合
git submodule update --init --recursive
```

### 依存関係のインストール

#### Rust依存関係

```bash
# 依存関係の事前取得（オプション）
cargo fetch

# 必要なRustコンポーネントの追加
rustup component add clippy rustfmt rust-docs

# プラットフォーム固有のターゲット追加（必要に応じて）
rustup target add wasm32-unknown-unknown  # WASM用
rustup target add aarch64-apple-darwin   # Apple Silicon用
```

#### 検証ツール

```bash
# TLA+ツールのインストール
./scripts/install-tla-tools.sh

# Python検証依存関係
pip3 install -r scripts/requirements.txt
```

### 環境設定

#### 設定ファイル作成

```bash
# 基本設定ファイルをコピー
cp nyx.toml.example nyx.toml

# 開発用の設定を適用
cp docs/config/development.toml nyx.toml
```

**基本設定** (`nyx.toml`):
```toml
# 基本ネットワーク設定
listen_port = 43300
node_id = "auto"
log_level = "info"

# 開発モード設定
[network]
bind_addr = "127.0.0.1:43300"
development = true

# 暗号設定
[crypto]
post_quantum = true
kyber_enabled = true
bike_enabled = false

# ミックスルーティング設定
[mix]
hop_count = 5
min_hops = 3
max_hops = 7
cover_traffic_rate = 10.0
adaptive_cover = true

# トランスポート設定
[transport]
quic_enabled = true
tcp_fallback = true
udp_buffer_size = 65536

# DHT設定
[dht]
enabled = true
port = 43301
bootstrap_peers = [
    "/dns4/testnet-validator1.nymtech.net/tcp/1789/p2p/12D3KooWNyxTestnet1"
]
```

#### 環境変数設定

**Linux/macOS** (`.bashrc` または `.zshrc`):
```bash
export RUST_LOG=info
export NYX_CONFIG_PATH=./nyx.toml
export NYX_DAEMON_ENDPOINT="127.0.0.1:50051"
export RUST_BACKTRACE=1

# 開発用の追加設定
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

### 開発サーバーの起動

#### 初回ビルド

```bash
# デバッグビルド（開発用）
cargo build

# リリースビルド（性能測定用）
cargo build --release

# 全機能有効ビルド
cargo build --all-features

# 特定コンポーネントのみ
cargo build -p nyx-daemon
```

#### デーモン起動

**ターミナル1（デーモン）**:
```bash
# 開発モードでの起動
RUST_LOG=debug cargo run --bin nyx-daemon

# または事前ビルド済みバイナリの実行
./target/debug/nyx-daemon

# バックグラウンド実行
nohup ./target/release/nyx-daemon > daemon.log 2>&1 &
```

#### CLI動作確認

**ターミナル2（CLI確認）**:
```bash
# デーモン状態確認
cargo run --bin nyx-cli -- status

# 接続テスト
cargo run --bin nyx-cli -- connect example.com:80

# ベンチマークテスト
cargo run --bin nyx-cli -- bench example.com:80 --duration 30
```

### テストの実行

#### 基本テスト

```bash
# 全体テスト実行
cargo test --workspace --all-features

# 特定クレートのテスト
cargo test -p nyx-crypto
cargo test -p nyx-mix --test integration_tests

# 詳細ログ付きテスト
RUST_LOG=debug cargo test test_name -- --nocapture
```

#### 統合テスト

```bash
# 適合性テスト
cargo test --package nyx-conformance --all-features

# プロパティベーステスト（拡張）
PROPTEST_CASES=10000 cargo test -p nyx-conformance

# ネットワークシミュレーションテスト
cargo test --test network_simulation --features network-testing
```

#### 性能テスト

```bash
# ベンチマーク実行
cargo bench

# 特定コンポーネントのベンチマーク
cargo bench -p nyx-crypto
cargo bench -p nyx-stream

# プロファイリング付きベンチマーク
cargo bench -- --profile-time=5
```

#### 形式検証

```bash
# TLA+モデルチェック
./scripts/verify.py --timeout 600

# 包括的検証
./scripts/build-verify.sh

# 特定設定での検証
./scripts/verify.py --config formal/basic.cfg
```

### 開発ワークフロー

#### 日常的な開発

```bash
# 1. コード変更前の確認
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# 2. 変更後のテスト
cargo test --workspace
cargo build --release

# 3. コミット前の最終確認
./scripts/pre-commit-check.sh
```

#### デバッグプロシージャ

**詳細ログ取得**:
```bash
# モジュール別ログレベル設定
RUST_LOG=nyx_crypto=trace,nyx_mix=debug,nyx_daemon=info cargo run --bin nyx-daemon

# 特定機能のデバッグ
RUST_LOG=nyx_stream::flow_controller=trace cargo test flow_control_test
```

**プロファイリング**:
```bash
# CPU使用量分析
cargo install cargo-profdata
cargo profdata -- --bench crypto_bench

# メモリ使用量分析
cargo install cargo-bloat
cargo bloat --release --crates
```

**パフォーマンス監視**:
```bash
# Prometheusメトリクス確認
curl http://127.0.0.1:9090/metrics

# リアルタイム統計
cargo run --bin nyx-cli -- status --watch --format json
```

### 一般的なトラブルシューティング

#### ビルドエラー

**1. protoc not found**:
```bash
# インストール確認
which protoc
protoc --version

# パス設定
export PATH=$PATH:/usr/local/bin
# Windowsの場合: $env:PATH += ";C:\tools\protoc\bin"
```

**2. libcap not found (Linux)**:
```bash
sudo apt-get install libcap-dev
pkg-config --cflags --libs libcap
```

**3. リンクエラー**:
```bash
# 依存関係の再インストール
cargo clean
cargo build

# システムライブラリの確認
ldconfig -p | grep ssl  # Linux
otool -L binary_name   # macOS
```

#### 実行時エラー

**1. ポート衝突**:
```bash
# ポート使用状況確認
netstat -tlnp | grep :43300  # Linux
netstat -an | findstr :43300  # Windows

# 設定ファイルでポート変更
sed -i 's/listen_port = 43300/listen_port = 43301/' nyx.toml
```

**2. 権限エラー (Linux)**:
```bash
# capabilities確認
getcap ./target/debug/nyx-daemon

# 必要に応じてcapabilityを設定
sudo setcap cap_net_bind_service=+ep ./target/debug/nyx-daemon

# または非特権ポートを使用
echo "listen_port = 8080" >> nyx.toml
```

**3. DHT接続問題**:
```bash
# ファイアウォール確認
sudo ufw status                    # Linux
netsh advfirewall show allprofiles # Windows

# bootstrap_peers設定確認
grep -A 5 "bootstrap_peers" nyx.toml

# テストネット用bootstrap設定
cp docs/config/testnet-bootstrap.toml nyx.toml
```

#### デバッグのヒント

**メモリリーク検出**:
```bash
# Valgrind (Linux)
cargo build
valgrind --tool=memcheck ./target/debug/nyx-daemon

# AddressSanitizer
RUSTFLAGS="-Z sanitizer=address" cargo run --bin nyx-daemon
```

**ネットワークトレース**:
```bash
# パケットキャプチャ
sudo tcpdump -i lo port 43300

# Wiresharkでの解析
tshark -i lo -f "port 43300" -w nyx-traffic.pcap
```

**ログ分析**:
```bash
# 構造化ログ出力
RUST_LOG=debug cargo run --bin nyx-daemon 2>&1 | jq '.'

# 特定エラーの検索
grep -E "ERROR|WARN" daemon.log | tail -20
```

### IDE設定

#### Visual Studio Code

**推奨拡張機能**:
- rust-analyzer
- CodeLLDB (デバッグ用)
- Better TOML
- Protocol Buffer support

**設定** (`.vscode/settings.json`):
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

**Rust Plugin設定**:
- Toolchain: `$HOME/.cargo/bin/rustc`
- Standard library: 自動検出
- Cargo project: workspace root選択

### 継続的統合

#### 本格的なCI環境構築

```bash
# GitHub Actions workflow確認
cat .github/workflows/comprehensive_ci.yml

# ローカルでCIテスト
act -j test-linux  # act toolが必要
```

このセットアップガイドにより、開発者は迅速にNyxNetの開発環境を構築し、効率的にプロジェクトに貢献できます。

---

**最終更新**: 2025年8月1日  
**ドキュメントバージョン**: v1.0  
**対応NyxNetバージョン**: v0.1以降
