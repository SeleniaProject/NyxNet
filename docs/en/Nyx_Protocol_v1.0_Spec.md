# Nyx Protocol v1.0 — Full Feature Specification (Draft)

> Status: Work-in-Progress — incremental drafting in progress. Sections marked with `🔒` are frozen; others may evolve.

---

## 0. Document Conventions
* All sizes are in **bytes** unless otherwise noted.
* Integers are **big-endian** network order.
* `CID` denotes Connection Identifier (96-bit).

---

## 1. Introduction 🔒
Nyx is a high-anonymity, low-latency overlay protocol that combines mix routing, FEC, and QUIC-style streams.  Version **1.0** extends the reference v0.1 feature set with mandatory post-quantum cryptography, multipath data-plane, cMix batching and mobile optimisations.

---

## 2. Terminology 🔒
| Term | Description |
|------|-------------|
| Node | An endpoint participating in Nyx routing (client, relay or rendezvous). |
| Stream | Reliable byte-oriented sub-channel carried over Nyx Secure Stream. |
| PathID | 8-bit identifier for a specific network path in a multipath session. |
| Plugin | Extension module transported via Frame Type **0x50–0x5F**. |

---

## 3. Delta Overview (v0.1 → v1.0) 🔒
| Category | v0.1 | v1.0 New / Changed |
|----------|-------|---------------------|
| Cryptography | X25519, Kyber optional | **PQ-Only** mode (Kyber/Bike), Hybrid DH, HPKE exporter |
| Routing | Fixed 5-hop mix | Variable 3-7 hops, Multipath, LARMix++ latency-aware |
| Transport | UDP only | UDP + QUIC Datagram, TCP Fallback, Teredo6 |
| FEC | Reed-Solomon (255,223) | RaptorQ, adaptive redundancy |
| Obfuscation | Fixed pad/timing | cMix with 100 ms Verifiable Delay Function |
| Monitoring | Prometheus | OpenTelemetry spans with path attributes |

---

## 4. Packet Format
### 4.1 Base Header 🔒
```
0               1               2               3
+---------------+---------------+---------------+---------------+
|       CID (96 bits)                                        |
+---------------+---------------+---------------+---------------+
|T|Flags|Len|Reserved| PathID |             ↘
+---------------+---------------+---------------+---------------+
```
* `T`  (2-bit)  0=Data 1=Control 2=Crypto 3=Reserved.
* `PathID` present only when `Flags & 0x40 != 0`.

### 4.2 Multipath Extension
When `Flags.MULTIPATH=1`, byte 13 encodes `PathID`.  Up to 8 active paths may co-exist.

---

## 5. Handshake & Cryptography
### 5.1 Hybrid Post-Quantum Pattern 🔒
```
<- s
-> e, ee_x25519, ee_kyber, s, ss
<- se_x25519, se_kyber, es, ee_x25519, ee_kyber
```
`Secret = HKDF-Extract(SHA-512, concat(dh25519, kyber1024))`

### 5.2 HPKE Export
Stream encryption keys derive from HPKE Exporter using context `"nyx-stream"`.

---

## 6. Mix Routing Layer
* **Batch size**: 100 packets (cMix mode).
* **VDF delay**: 100 ms Wesolowski over RSA-2048 group.

---

*The remaining sections (7–14) will be drafted in subsequent commits.* 