---
# Nyx Project Outstanding Tasks / 未実装タスク一覧

最終更新: 2025-07-21

## Core Cryptography
- [x] Hybrid Post-Quantum Handshake (X25519 + Kyber) 実装 (`nyx-crypto/hybrid.rs` 完全化)
- [x] HPKE レイヤ導入と Stream 層統合
- [x] age 形式でのキーストア暗号化 & zeroization 徹底監査

## cMix / VDF
- [x] Wesolowski VDF 実装 (最適化 Montgomery 乗算)
- [x] マルチノード RSA Accumulator キーセレモニー & 証明生成
- [x] cMix コントローラのミキサーパイプライン統合
- [ ] cMix コンフォーマンステスト & Fuzz コーパス拡充

## Multipath / Routing
- [x] RTT 逆数重み付け WRR スケジューラ本番統合
- [x] MPR 冗長率の動的調整アルゴリズム
- [ ] LARMix++ 遅延感応ルートビルダ (動的 Hop 3–7)
- [x] 経路差分に応じたリオーダバッファサイズ自動化
- [ ] PATH_VALIDATION_FAILED (0x05) エラー処理全層対応

## Plugin Framework
- [ ] Frame Type 0x50–0x5F パーサ & ディスパッチ
- [ ] CBOR Capability Negotiation ハンドシェイク実装
- [ ] Sandbox 分離 & Permission Enforcement 強化
- [ ] Plugin ライフサイクル API (load/unload, version check)
- [ ] 必須/任意 Plugin Capability テストケース

## FEC Layer
- [ ] RaptorQ エンコーダ/デコーダ統合 (`nyx-fec`)
- [ ] 冗長率アダプティブ制御ロジック
- [ ] SIMD 強化 Reed-Solomon 最適化

## Transport Adapter
- [ ] QUIC DATAGRAM サポート (quinn datagram 拡張)
- [ ] TCP フォールバック実装 & ポリシー
- [ ] IPv6 Teredo トンネリング最終化 (Windows 対応)

## Low Power Mode / Mobile
- [ ] 電源状態フック実装 (Android/iOS) と cover_ratio スケーリング
- [ ] Push Gateway (FCM/APNS) 経由ウェイクアップロジック
- [ ] 低電力モード E2E シナリオテスト

## Telemetry & Observability
- [ ] OpenTelemetry Span 追加 (全 async task)
- [ ] Prometheus メトリクス拡充 (新モジュール対応)
- [ ] Grafana ダッシュボード v1.0 メトリクス反映

## Deployment / Kubernetes
- [ ] Helm Chart: HPA (nyx_bytes_sent_total & CPU) 定義
- [ ] PodSecurityContext/ seccomp-bpf プロファイル強化

## CLI / SDK
- [ ] `nyx-cli key rotate` コマンド実装
- [ ] `nyx-cli quarantine <node>` 機能実装
- [ ] CLI/SDK 新規メッセージ i18n (ja, en, zh) 完全対応

## Documentation
- [ ] v1.0 仕様差分の Rustdoc & MkDocs 反映
- [ ] gRPC API 自動生成ドキュメントパイプライン

## Testing & CI
- [ ] Multipath スケジューラ & RaptorQ リカバリのプロパティテスト
- [ ] Plugin パーサ, VDF 証明検証用 Fuzz ターゲット追加
- [ ] `nyx-sdk-wasm` 用 WASM ビルド CI マトリクス

## Security Hardening
- [ ] Linux seccomp プロファイル生成 & 適用
- [ ] OpenBSD pledge/unveil バインディング実装
- [ ] systemd panic=abort リカバリ & コアダンプ連携
---
