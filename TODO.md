# Nyx Project TODO / タスク一覧

> **更新日: 01-03** (随時更新)

---

## ✅ 完了済み / Done
- [x] 仕様書 (`Nyx_Protocol_v0.1_Spec.md`) 確定
- [x] `nyx-core` クレート作成 & 基本 Config / エラー実装
- [x] `nyx-crypto` クレート作成

---

## 🏗️ 進行中 / In Progress
- [ ] Noise_Nyx パターン実装 & テストベクタ合致確認
- [ ] UDP Transport Adapter + 基本ホールパンチング
- [ ] Fuzz テスト基盤セットアップ (`cargo-fuzz`)

---

## 🔜 v0.1 Alpha リリース目標タスク
### Core & Crypto
- [ ] `nyx-core` エラーハンドリング統一 (thiserror)
- [ ] `nyx-crypto` Kyber1024 fallback (feature `pq`)
- [ ] HKDF misuse-resistantラッパー

### Stream Layer
- [ ] Frame parser (nom) 完全カバレッジ (>95%)
- [ ] Flow control (BBRv2) 最低限実装
- [ ] フレーム生成・多重化

### Mix Routing
- [ ] PathBuilder (Kademlia lookup) PoC
- [ ] Cover traffic generator (Poisson) 実装

### Obfuscation & FEC
- [ ] Reed-Solomon codec ラッパ (benchmarks)
- [ ] Timing obfuscator (async queue)

### Transport
- [ ] ICE Lite ハンドシェイク
- [ ] STUN server モジュール (optional)

### Control Plane
- [ ] Kademlia DHT integration (`libp2p`) テストネット起動
- [ ] SETTINGS フォーマットの schema validation

### Telemetry
- [ ] Prometheus exporter initial metrics
- [ ] Bunyan log middleware integration

### CI / Tooling
- [ ] GitHub Actions miri ジョブ追加
- [ ] Dependabot config

---

## 🚀 v0.1 Beta 追加タスク
- [ ] Windows ビルド & CI
- [ ] WASM SDK (nyx-sdk-wasm)
- [ ] Docker multi-arch (amd64, arm64)
- [ ] Helm Chart リリース

---

## 🔐 セキュリティ / Security
- [ ] Secrets zeroize audit (cargo make-zeroize)
- [ ] Constant-time audit (cargo criterion-cmp)
- [ ] seccomp sandbox profile

---

## 📈 パフォーマンス / Performance
- [ ] UDP `send_mmsg` ベンチ
- [ ] SIMD RS-FEC speedup (`stdsimd`) 評価
- [ ] Profiling flamegraph baseline

---

## 📚 ドキュメント / Docs
- [ ] Rustdoc doc-cfg annotations
- [ ] MkDocs サイト自動公開 (GitHub Pages)
- [ ] チュートリアル「最小チャットアプリ」

---

## 🌐 I18N
- [ ] cli メッセージ fluent localization (en, zh)
- [ ] ドキュメント多言語切替

---

## 🧪 テスト / QA
- [ ] Conformance Suite 120 ケース実装
- [ ] E2E test matrix (KinD 5-node) in CI
- [ ] Chaos-mesh latency injection tests

---

## 🔮 将来計画 / Future Ideas
- [ ] Multipath (MPR) 実験実装
- [ ] Verifiable Delay Mix (cMix) 組み込み検討
- [ ] Post-Quantum only mode (PQ-only handshake)

---

## 🛣️ v1.0 Roadmap (フル機能)
### Core & PQ Cryptography
- [ ] Hybrid X25519+Kyber Handshake 実装
- [ ] PQ-Only モード (Kyber/Bike) CI  matrix
- [ ] HPKE API サポート (exporter)

### Multipath
- [ ] PathID 拡張パケットヘッダ実装
- [ ] Weighted Round Robin スケジューラ
- [ ] Reordering Buffer Adaptive サイズ
- [ ] Multipath Conformance Tests (loss, reorder)

### Latency-Aware Routing (LARMix++)
- [ ] RTT & Bandwidth Probing in Control Plane
- [ ] LARMix++ アルゴリズム実装
- [ ] Load-Balancing Evaluation via simulation

### cMix / VDF
- [ ] RSA Accumulator 設計 & key ceremony
- [ ] VDF delay 100ms ラスタ実装 (Rust VDF crate)
- [ ] cMix バッチコントローラ

### Plugin Framework
- [ ] Frame Type 0x50–0x5F パーサー/serializer
- [ ] CBOR Schema Validation (serde_cbor)
- [ ] Plugin Registration & Permission Model
- [ ] Sample Plugin: GeoStat collection

### Adaptive Cover Traffic & Low Power Mode
- [ ] Utilization Estimator (time window)
- [ ] λ 調整アルゴリズム実装
- [ ] Mobile battery state integration (Android/iOS bindings)

### Transport Extensions
- [ ] QUIC DATAGRAM path
- [ ] TCP Fallback encapsulation
- [ ] Teredo-like IPv6 NAT traversal module

### Observability
- [ ] OpenTelemetry Exporter (OTLP gRPC)
- [ ] Grafana Dashboard v1.0

### Security Hardening
- [ ] Post-Compromise Recovery Key Update
- [ ] Memory-sanitizer build in CI

### Documentation
- [ ] v1.0 完全仕様 English 版
- [ ] Plugin 開発ガイド作成

### Release Process
- [ ] Semantic Versioning: tag v1.0.0-rc1
- [ ] Release notes generator (cargo-release)

---

> v1.0 のタスクは `milestone/1.0` に関連付け、各PRに `Feature-v1` ラベルを付与してください。 