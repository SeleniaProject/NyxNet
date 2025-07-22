---
# Nyx Project Outstanding Tasks / 未実装タスク一覧

## 1. デーモン・オーケストレーション層
- [x] `nyx-daemon` クレートを新規作成し、Tokio ランタイム上で Transport / Stream / Mix / Control / Telemetry 各タスクを起動・停止・監視する
- [x] gRPC Unix Domain Socket サーバを実装し CLI/SDK からの要求を受け付ける
- [x] Panic フック・自動再起動戦略(systemd) の統合

## 2. Stream Layer 完全実装
- [x] フロー制御 (BBRv2 派⽣) アルゴリズムの実装
- [x] ストリーム状態機械 (Idle/Open/HalfClosed/Closed) と再送・ACK 合算ロジック
- [x] ACK フレーム生成・遅延 ACK タイマ
- [x] Loss/RTT サンプリング、CongestionCtrl へのフィード

## 3. Multipath 拡張
- [x] PathID 付きヘッダの送受信実運用コード
- [x] Weighted Round-Robin スケジューラの実装とテレメトリ計測
- [x] 新経路 Path Validation (PATH_CHALLENGE/RESPONSE) の実装

## 4. FEC / オブフスケーション
- [x] RaptorQ エンジンの実装と適応冗長率制御
- [x] タイミング平滑化 (±σ ランダムディレイ) 送信キュー
- [x] 固定 1280B パディング I/O パイプラインへの統合

## 5. Mix Routing / cMix
- [x] Wesolowski VDF 高速化 (Montgomery + 並列化)
- [x] RSA アキュムレータ多者鍵儀式 (MPC) 実装
- [x] `CmixController` をデーモンに統合し、バッチ発射 → 下位 Transport への接続
- [x] LARMix++ プローブ結果を利用して経路長を動的決定

## 6. Transport Adapter
- [x] QUIC DATAGRAM サポート (quinn と統合)
- [x] TCP フォールバック実装
- [x] Teredo (IPv6 トンネル) 経路ハンドラ
- [x] ICE Lite / STUN サーバ実装を実ネットワークテストで検証

## 7. Control Plane
- [x] libp2p-kad DHT ノード起動とレコード管理
- [x] Rendezvous サーバ同期処理 (`probe.rs`, `push.rs`) のネットワーク実装
- [x] SETTINGS フレーム双方向同期・ホットリロード

## 8. プラグインフレーム & サンドボックス
- [ ] Cross-platform サンドボックス (macOS システム拡張、Windows Job Object)
- [ ] Plugin IPC (tokio mpsc → Unix Domain Socket / NamedPipe) 実装
- [ ] `dynamic_plugin` フィーチャのビルド・テストワークフロー追加

## 9. モバイル低電力モード
- [ ] iOS / Android Battery & Screen Off イベントブリッジ実装
- [ ] Cover Traffic λ スケーリングのリアルタイムテスト
- [ ] Push Gateway (FCM/APNS) 統合と E2E 起床シーケンス

## 10. Telemetry / Observability
- [ ] OpenTelemetry OTLP エクスポータ実装 (tracing-opentelemetry)
- [ ] Prometheus Collector を廃止し OTLP へ移行、Grafana ダッシュボード更新
- [ ] エラーコード / レイテンシ分布メトリクス追加

## 11. CLI / SDK
- [ ] `nyx-cli` サブコマンド: `connect`, `status`, `bench` のネットワーク実装
- [ ] Fluent i18n メッセージ拡充 (en, ja, zh 全 100%)
- [ ] SDK `NyxStream` にストリーム再接続・エラー伝播 API 追加

## 12. セキュリティ強化
- [ ] サンドボックス (seccomp / pledge / unveil) テストカバレッジ
- [ ] age‐encrypted Keystore のパスフレーズリトライ UI (CLI, SDK)
- [ ] Miri + AddressSanitizer CI ジョブ追加

## 13. テスト & CI/CD
- [ ] `nyx-conformance` 全 120 ケースを Green にする
- [ ] fuzz target 強化: Stream パーサ, PathBuilder, VDF プルーフ
- [ ] GitHub Actions: wasm32 / Windows / macOS / Linux matrix, `