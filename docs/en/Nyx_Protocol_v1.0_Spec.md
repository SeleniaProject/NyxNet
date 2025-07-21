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

## 7. Plugin Framework
Nyx v1.0 reserves Frame Type **0x50–0x5F** for pluggable extension modules.
Each plugin frame begins with a CBOR header `{id:u32, flags:u8, data:bytes}`:

| Field | Size | Description |
|-------|------|-------------|
| id    | 32-bit | Capability identifier registered in the Nyx extension registry. |
| flags | 8-bit  | Bit-0 (**0x01**) = *required* – peer must support or abort with **0x07 UNSUPPORTED_CAP**. |
| data  | N-byte | Plugin-specific opaque payload. |

During the handshake the endpoint advertises its plugin requirements via the SETTINGS capability list.  A required plugin unknown to the peer triggers immediate session close with error **0x07**.

---

## 8. Multipath Data Plane
A Nyx connection may bind up to **8** concurrent network paths that share the same cryptographic context.

* **PathID (uint8)** is appended to the packet header when `Flags.MULTIPATH=1`.
* Sender uses *Weighted Round-Robin* scheduling: weight = `1/RTT`.
* Receiver holds a per-path reorder buffer sized to `RTT_diff + 2·jitter`.


---

## 9. Hybrid Post-Quantum Handshake & HPKE Export
The mandatory handshake pattern combines **X25519** and **Kyber1024**.

```
<- s
-> e, ee_x25519, ee_kyber, s, ss
<- se_x25519, se_kyber, es, ee_x25519, ee_kyber
```

Secret derivation:
`Secret = HKDF-Extract(SHA-512, concat(dh25519, kyber_shared))`

Derived traffic keys follow the HPKE **Export** interface with context string `"nyx-stream"` to guarantee algorithm agility.

---

## 10. cMix Integration
When the session negotiates `mode=cmix`, packets are delayed into **100-packet** batches.  Each batch is time-locked for **100 ms** using the Wesolowski VDF over an RSA-2048 modulus.  Mix nodes collectively publish RSA accumulator proofs to guarantee verifiability.

---

## 11. Adaptive Cover Traffic
Cover traffic rate λ is adjusted once per second to maintain the configured ratio `cover / (cover + real)` between **0.2 – 0.6**.  The sender estimates real throughput over a 5 s sliding window and updates λ accordingly:

```
λ_new = max(base_λ, util_pps · target_ratio / (1 − target_ratio))
```

---

## 12. Low Power Mode
Mobile devices may advertise *Low Power* preference via SETTINGS.  A node observing screen-off or battery discharging scales λ to **0.1×** and extends keep-alive intervals to **60 s**.  Push notifications (FCM / APNS) are tunneled through a Nyx Gateway.

---

## 13. Telemetry & Compliance Levels
OpenTelemetry spans:
* `nyx.stream.send` – attributes: `cid`, `path_id`.
* `nyx.handshake` – attributes: `pq_mode`.

Compliance tiers:
| Level | Mandatory Features |
|-------|--------------------|
| **Core** | v0.1 baseline |
| **Plus** | Multipath, Hybrid PQ |
| **Full** | cMix, Plugin, Low Power |

---

## 14. Security Considerations
* **Post-Compromise Recovery** – every 1 GiB or 10 minutes a key update is triggered.
* **Traffic Correlation Mitigation** – fixed-length 1280 B packets + path mixing + adaptive cover.
* **Replay Protection** – 64-bit sequence window of `2²⁰` entries.

---

> **Status:** Final – this document is now frozen for Nyx v1.0 release. 