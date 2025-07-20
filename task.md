# Nyx Project Outstanding Tasks / 未実装タスク一覧

## v1.0 未実装タスク
- [x] Noise_Nyx フレーム暗号化フル実装（ChaCha20-Poly1305 / BLAKE3、0-RTT 再生防止）
- [x] Capability Negotiation (CBOR SETTINGS) の送受信実装および `UNSUPPORTED_CAP` (0x07) クローズ処理
- [x] Verifiable Delay Mix (cMix) 本番統合（RSA Accumulator + Wesolowski VDF 実装）
- [x] Plugin Registry の永続化と実行時権限取り消し API
- [ ] Low Power Mode 時の Cover Traffic λ 動的調整（Android / iOS 連携）
- [ ] Hybrid KEM の HPKE exporter API 最終仕様への統合
- [ ] Formal Verification: TLA+ モデルを Multipath / Plugin 拡張点含め更新
- [ ] Windows 向け `raptorq` SIMD アクセラレーション最適化 & fallback 実装
- [ ] Conformance Test Suite 拡充：Capability Negotiation / cMix / Low Power Mode ケース追加
- [ ] ドキュメント更新：v1.0 完全仕様 (英語版) の最終化と公開 