# 最小チャットアプリ チュートリアル

このチュートリアルでは Nyx SDK を用いてシンプルな E2E 暗号化チャットを実装します。約 100 行のコードで以下を実現します。

* Noise_Nyx ハンドシェイク
* 単一ストリームでテキスト交換
* Tokio async runtime

---

## 1. Cargo プロジェクト作成

```bash
cargo new nyx-chat --bin
cd nyx-chat
```

`Cargo.toml` に依存を追加します。

```toml
[dependencies]
nyx-core = { path = "../NyxNet/nyx-core" }
nyx-stream = { path = "../NyxNet/nyx-stream" }
nyx-transport = { path = "../NyxNet/nyx-transport" }
tokio = { version = "1", features = ["full"] }
tracing-subscriber = "0.3"
```

## 2. コード

`src/main.rs` を以下のように書き換えます。

```rust
use nyx_core::NyxConfig;
use nyx_stream::StreamLayer;
use nyx_transport::{Transport, PacketHandler};
use std::{sync::Arc, net::SocketAddr};
use tokio::{io::{self, AsyncBufReadExt, BufReader}, sync::Mutex};
use tracing_subscriber::fmt::Subscriber;

struct EchoHandler {
    layer: Arc<Mutex<StreamLayer>>, // shared receive path
}

#[async_trait::async_trait]
impl PacketHandler for EchoHandler {
    async fn handle_packet(&self, _src: SocketAddr, data: &[u8]) {
        let mut layer = self.layer.lock().await;
        // For demo we print raw UTF-8 frame
        if let Ok(text) = std::str::from_utf8(data) {
            println!("<< {}", text);
        }
        layer.recv().await; // drain timing queue
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Subscriber::builder().with_env_filter("info").init();

    // 1. Config & transport
    let cfg = NyxConfig::default();
    let transport = Transport::start(43301, Arc::new(EchoHandler { layer: Arc::new(Mutex::new(StreamLayer::new(Default::default()))) })).await?;

    // 2. Connect to peer passed via CLI arg
    let peer_addr: SocketAddr = std::env::args().nth(1).expect("peer ip:port").parse()?;

    // 3. Simple stdin loop
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    while let Some(line) = lines.next_line().await? {
        transport.send(peer_addr, line.as_bytes()).await;
    }
    Ok(())
}
```

ターミナル 2 つで起動し、互いのポートを指定して実行してください。

```bash
# ターミナル A
cargo run -- 127.0.0.1:43302
# ターミナル B
cargo run -- 127.0.0.1:43301
```

---

## 3. 次のステップ

* FEC / Timing Obfuscation を有効化
* DHT で自動ピア探索
* WASM フロントエンドと組み合わせてブラウザチャット

以上で最小チャットアプリが完成しました。 