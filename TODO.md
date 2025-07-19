# Nyx Project TODO / タスク一覧

> **更新日: 01-07** (随時更新)

---

## ✅ 完了済み / Done
- [x] 仕様書 (`Nyx_Protocol_v0.1_Spec.md`) 確定
- [x] `nyx-core` クレート作成 & 基本 Config / エラー実装
- [x] `nyx-crypto` クレート作成
- [x] HKDF misuse-resistantラッパー

---

## 🏗️ 進行中
- [x] Noise_Nyx パターン実装 & テストベクタ合致確認 (RFC7748 X25519 unit test)
- [x] UDP Transport Adapter + basic hole punching
- [x] Timing obfuscator integration in stream layer (StreamLayer, E2E test)
- [x] Fuzz harness setup (`cargo-fuzz`) - initial noise_handshake target

---

## 🔜 v0.1 Alpha リリース目標タスク
### Core & Crypto
- [x] `nyx-core` エラーハンドリング統一 (thiserror)
- [x] `nyx-crypto` Kyber1024 fallback (feature `pq`)

### Stream Layer
- [x] Frame parser skeleton (nom) with header parsing
- [x] Flow control skeleton implemented (BBRv2 style)
- [x] Frame multiplexing (StreamFrame encoder/decoder)

### Mix Routing
- [x] PathBuilder (uniform random) PoC
- [x] Cover traffic generator (Poisson) 実装

### Obfuscation & FEC
- [x] Reed-Solomon codec wrapper (benchmarks pending)
- [x] Timing obfuscator (async queue)

### Transport
- [x] ICE Lite ハンドシェイク
- [x] STUN server モジュール

### Control Plane
- [x] Kademlia DHT integration (`libp2p`) テストネット起動
- [x] SETTINGS フォーマットの schema validation

### Telemetry
- [x] Prometheus exporter initial metrics
- [x] Bunyan log middleware integration

### CI / Tooling
- [x] GitHub Actions miri ジョブ追加
- [x] Dependabot config

---

## 🚀 v0.1 Beta 追加タスク
- [x] Windows ビルド & CI
- [x] WASM SDK (nyx-sdk-wasm)
- [x] Docker multi-arch (amd64, arm64)
- [x] Helm Chart リリース

---

## 🔐 セキュリティ / Security
- [x] Secrets zeroize audit (cargo make-zeroize)
- [x] Constant-time audit (cargo criterion-cmp)
- [x] seccomp sandbox profile

---

## 📈 パフォーマンス / Performance
- [x] UDP `send_mmsg` ベンチ
- [x] SIMD RS-FEC speedup (`stdsimd`) 評価
- [x] Profiling flamegraph baseline

---

## 📚 ドキュメント / Docs
- [x] Rustdoc doc-cfg annotations
- [x] MkDocs サイト自動公開 (GitHub Pages)
- [x] チュートリアル「最小チャットアプリ」

---

## 🌐 I18N
- [x] cli メッセージ fluent localization (en, zh)
- [x] ドキュメント多言語切替

---

## 🧪 テスト / QA
- [ ] Conformance Suite 120 ケース実装 (34/120) <!-- after warnings cleanup tests -->
- [x] E2E test matrix (KinD 5-node) in CI
- [x] Chaos-mesh latency injection tests

### Newly Completed
- Implemented LOCALIZED_STRING frame parser/builder and associated conformance tests.

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
- [x] OpenTelemetry Exporter (OTLP gRPC)
- [x] Grafana Dashboard v1.0

### Security Hardening
- [x] Post-Compromise Recovery Key Update
- [x] Memory-sanitizer build in CI

### Documentation
- [ ] v1.0 完全仕様 English 版
- [ ] Plugin 開発ガイド作成

### Release Process
- [ ] Semantic Versioning: tag v1.0.0-rc1
- [ ] Release notes generator (cargo-release)

---

> v1.0 のタスクは `milestone/1.0` に関連付け、各PRに `Feature-v1` ラベルを付与してください。 