# Nyx Project Outstanding Tasks / 未実装タスク一覧

## v0.1 未実装・要修正タスク
- [x] Weighted Round Robin Path Scheduler の実装 (inverse RTT 重み付け)
- [x] LARMix++ 遅延・帯域プロービング & 経路最適化
- [x] QUIC DATAGRAM サポート (`nyx-transport`)
- [x] TCP Fallback Encapsulation 実装
- [x] PathID 拡張パケットヘッダ実装 (Multipath Data Plane)
- [x] Reordering Buffer Adaptive サイズ制御
- [x] Plugin Registration & Permission Model (Frame Type 0x50–0x5F)
- [x] Sample Plugin: GeoStat collection
- [x] Telemetry テスト用 dev-dependencies 追加 (`nyx-telemetry`, `prometheus`)
- [x] `pprof` / `nix` 依存を `cfg(unix)` ガードし Windows ビルド対応
- [x] Conformance Telemetry テストを Windows でも成功させる

## TODO.md で未チェックの項目
- [x] Multipath (MPR) 実験実装
- [x] Verifiable Delay Mix (cMix) 組み込み検討
- [x] Post-Quantum only mode (PQ-only handshake)
- [x] Hybrid X25519+Kyber Handshake 実装
- [x] PQ-Only モード (Kyber/Bike) CI matrix
- [x] HPKE API サポート (exporter)
- [x] PathID 拡張パケットヘッダ実装
- [x] Weighted Round Robin スケジューラ
- [x] Reordering Buffer Adaptive サイズ
- [x] Multipath Conformance Tests (loss, reorder)
- [x] RTT & Bandwidth Probing in Control Plane
- [x] LARMix++ アルゴリズム実装
- [x] Load-Balancing Evaluation via simulation
- [x] RSA Accumulator 設計 & key ceremony
- [x] VDF delay 100ms ラスタ実装 (Rust VDF crate)
- [x] cMix バッチコントローラ
- [x] Frame Type 0x50–0x5F パーサー/serializer
- [x] CBOR Schema Validation (serde_cbor)
- [x] Plugin Registration & Permission Model
- [x] Sample Plugin: GeoStat collection
- [x] Utilization Estimator (time window)
- [x] λ 調整アルゴリズム実装
- [x] Mobile battery state integration (Android/iOS bindings)
- [x] QUIC DATAGRAM path
- [x] TCP Fallback encapsulation
- [ ] Teredo-like IPv6 NAT traversal module
- [ ] v1.0 完全仕様 English 版
- [ ] Plugin 開発ガイド作成
- [ ] Semantic Versioning: tag v1.0.0-rc1
- [ ] Release notes generator (cargo-release)

## v1.0 追加機能タスク (Nyx_Protocol_v1.0_Spec.md)
- [ ] Hybrid Post-Quantum Handshake (X25519 + Kyber/Bike) 実装
- [ ] HPKE サポート (export secrets)
- [ ] Multipath 同時通信 (動的 PathID, 重み付けスケジューラ)
- [ ] Latency-aware routing (LARMix++) 実装
- [ ] 動的 Hop 数 (3–7) サポート
- [ ] RaptorQ FEC & adaptive redundancy
- [ ] VDF-based cMix batch (RSA accumulator & 100ms delay) 実装
- [ ] Capability Negotiation via CBOR (UNSUPPORTED_CAP 処理含む)
- [ ] Low Power Mode: Adaptive cover traffic rate + mobile bindings
- [ ] OTLP Telemetry span 拡張 (`nyx.stream.send` attrs)
- [ ] Compliance Level 判定ロジック (Core / Plus / Full)

## Windows ビルド互換タスク
- [ ] `pprof` 依存を optional feature 化し、Windows ではデフォルト無効
- [ ] `nix` 依存 API 使用箇所を `cfg(unix)` で分離 or `windows-sys` 代替
- [ ] `libp2p` の Windows UDP ソケット制限を検証しパッチ適用
- [ ] GitHub Actions Windows runner で `cargo test --all --all-features` 緑化
- [ ] Cross-compilation (x86_64-pc-windows-gnu/msvc) チェック
- [ ] PQCrypto (Kyber) の SIMD アセンブリを Windows 対応 or `no_simd` fallback 