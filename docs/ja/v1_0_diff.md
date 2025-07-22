# Nyx プロトコル v1.0 変更点まとめ

このドキュメントでは v0.1 実装との差分を俯瞰します。正式仕様は `spec/Nyx_Protocol_v1.0_Spec.md` を参照してください。

## 1. 暗号

| 項目 | v0.1 | v1.0 |
|------|------|------|
| DH | X25519 | ハイブリッド (X25519 + Kyber) / PQ-Only |
| HPKE | – | 必須サポート |
| ハンドシェイク | Noise_Nyx | Post-Quantum 拡張パターン |

## 2. ルーティング

* ホップ数を 3–7 に動的交渉。
* Multipath 同時送信、RTT 逆数 WRR スケジューラ。

## 3. トランスポート

* QUIC DATAGRAM 連携、パス ID 対応。
* TCP フォールバック + Teredo トンネル。

## 4. FEC

RS(255,223) → RaptorQ（冗長率アダプティブ）。

## 5. プラグインフレームワーク

* Frame 0x50–0x5F 予約。
* CBOR Capability ネゴシエーション。
* ライフサイクル API と Permission モデル。

## 6. テレメトリ

* OpenTelemetry スパン & Prometheus 指標拡充。

## 7. コンプライアンスレベル

| レベル | 必須機能 |
|--------|---------|
| Core | v0.1 機能 |
| Plus | Multipath, PQ ハイブリッド |
| Full | cMix, Plugin, LowPower |

---
*main ブランチの Rust 実装は上記すべてに対応済み。v0.1 互換性は Capability で確保しています。* 