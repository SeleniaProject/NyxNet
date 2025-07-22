---
# Nyx Project Outstanding Tasks / 未実装タスク一覧

最終更新: 2025-07-21

---
- [x] `nyx-mix/src/cmix.rs` で VDF 証明をネットワーク I/O と統合し、現状の疑似ディレイ／プレースホルダを置き換える
- [x] プラグイン実行環境を seccomp + PID/UTS 名前空間で完全分離 (`dynamic_plugin` feature) し、強隔離を実現する
- [x] `nyx-transport/src/teredo.rs` の IPv6 Teredo fallback をフル実装し、E2E テストを追加する
- [x] `nyx-transport/src/tcp_fallback.rs` の TCP フォールバック実装を完成させ、接続維持テストを整備する
- [x] ハイブリッド X25519 + Kyber ハンドシェイクの秘密共有統合ロジックを実装（`nyx-crypto/src/noise.rs` L126 付近の TODO）
- [x] `nyx-stream/src/congestion.rs` の BBRv2 アルゴリズムを最小実装から完全実装へ拡充する
- [x] `nyx-transport/src/ice.rs` および `nyx-transport/src/stun_server.rs` の STUN/BINDING メッセージ構築プレースホルダを正式仕様準拠に置換する
- [x] `nyx-control/src/push.rs` の APNS Push 実装を HTTP/2 + JWT 認証含めて完成させる
- [x] `nyx-core/src/sandbox.rs` の seccomp フィルタを全システムコール網羅に更新し、最小セットコメントを除去する
- [x] `nyx-transport/src/teredo.rs` の interface identifier プレースホルダを正式アルゴリズムで算出する
- [ ] `src/empty.rs` プレースホルダクレートを削除するか実用途のユーティリティクレートへ置換する
- [ ] `nyx-stream/src/management.rs` CLOSE コード 0x07 (UNSUPPORTED_CAP) ハンドリングを追加し、Capability Negotiation エラーを適切に伝播する
- [ ] v1.0 仕様書最終稿とコードコメントの差分を確認し、ドキュメントを同期する
- [ ] `nyx-mix/src/lib.rs` の `PathBuilder` (uniform random) を `WeightedPathBuilder` ベースへ置き換え、レイテンシ／帯域メトリクスを活用する
- [ ] `nyx-crypto/src/keystore.rs` に永続ストレージ I/O 実装（age-encrypt ファイル保存）とキー定期ローテーション機能を追加する


