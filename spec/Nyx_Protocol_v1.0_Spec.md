# Nyx Protocol v1.0 — 完全仕様書 / Full Feature Specification

> **Status: Draft-Complete (予定機能含む)**

---

## 追加機能差分 (v0.1 → v1.0)
| カテゴリ | v0.1 | v1.0 拡張 |
|-----------|------|-----------|
| 暗号 | X25519, Kyber optional | PQ-Only モード (Kyber/Bike), Hybrid DH, HPKE support |
| ルーティング | 固定5ホップ Mix | 動的 Hop 数 (3–7), Multipath 同時通信, Latency-aware routing (LARMix++) |
| トランスポート | 単一UDP | UDP + QUIC DATAGRAM, TCP fallback, IPv6 Teredo内蔵 |
| FEC | RS(255,223) | RaptorQ, adaptive redundancy |
| セキュリティ | Basic replay防御 | VDF-based cMix batch, post-compromise recovery |
| 拡張 | SETTINGSのみ | Capability Negotiation via CBOR, Plugin Frames |
| モニタリング | Prometheus | OpenTelemetry tracing, push/pull both |
| モバイル | - | Low Power Mode (Adaptive cover traffic rate) |

---

## 1. プロトコルコンビネータ (Plugin Framework)
* 新Frame Type 0x50–0x5F を「Plugin」予約。
* CBOR ヘッダ `{id:u32, flags:u8, data:bytes}`。
* Plugin 向け handshake: SETTINGS `PLUGIN_REQUIRED` advertising.

## 2. Multipath Data Plane
* CID ごとに `path_id` (uint8)。パケットヘッダに追加。
* 送信スケジューラ: Weighted Round Robin of paths, weight = inverse RTT.
* Reordering buffer size dynamic (RTT diff + jitter *2).

## 3. Hybrid Post-Quantum Handshake
```
<- s
-> e, ee_dh25519, ee_kyber, s, ss
<- se_dh25519, se_kyber, es, ee_dh25519, ee_kyber
```
* Secret = HKDF-Extract( SHA-512, concat(dh25519, kyber) ).

## 4. cMix Integration
* オプション `mode=cmix` で batch = 100, VDF delay 100ms。
* Mixノードは RSA accumulator で証明公開。

## 5. Adaptive Cover Traffic
* Target util U∈[0.2,0.6]. 1s 窓で測定→λ調整。

## 6. Low Power Mode (モバイル)
* Screen-Off 検知で `cover_ratio`=0.1, keepalive 60s。
* Push通知経路: FCM / APNS WebPush over Nyx Gateway.

## 7. Extended Packet Format
| Byte | 名称 | 説明 |
|------|------|------|
| 0–11 | CID | Connection ID |
| 12 | Type(2) + Flags(6) |
| 13 | PathID (8) |
| 14–15 | Length |
| 16–... | Payload |

## 8. Capability Negotiation
* Extension list CBOR array in first CRYPTO frame.
* Unsupported Required → CLOSE 0x07 (UNSUPPORTED_CAP).

## 9. Telemetry Schema (OTLP)
* span name = "nyx.stream.send" attr: path_id, cid.

## 10. Compliance Levels
| Level | 必須機能 | 説明 |
|-------|----------|------|
| Core | v0.1 set | 最低互換性 |
| Plus | Multipath, Hybrid PQ | デフォルト推奨 |
| Full | cMix, Plugin, LowPower | 全機能 |

---

詳細章は v0.1 仕様を継承し、差分のみ記載。 