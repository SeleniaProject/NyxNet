# Nyx Protocol v1.0 – What’s New?

This document highlights the delta between the reference implementation that targets v0.1 and the upcoming v1.0 **draft-complete** specification. For the full normative text see `spec/Nyx_Protocol_v1.0_Spec.md`.

## 1. Cryptography

| Area | v0.1 | v1.0 |
|------|------|------|
| DH | X25519 | Hybrid DH (X25519 + Kyber) \| PQ-Only (Kyber/Bike) |
| HPKE | – | Mandatory support (`hpke_export` frame) |
| Handshake | Noise_Nyx | Hybrid Post-Quantum pattern |

## 2. Routing

* Dynamic hop length (3-7) negotiated at handshake.
* Multipath simultaneous dispatch with latency-aware WRR scheduler.

## 3. Transport

* QUIC DATAGRAM integration (path-id aware).
* TCP fallback and Teredo IPv6 tunnelling for restricted networks.

## 4. FEC

Switched from RS(255,223) to RaptorQ with adaptive redundancy controller.

## 5. Plugin Framework

* New frame range 0x50–0x5F allocated.
* CBOR Capability Negotiation handshake.
* Runtime life-cycle API (`load`/`unload`) + permission model.

## 6. Telemetry

* OpenTelemetry tracing and Prometheus metrics aligned with spec §9.

## 7. Compliance Levels

| Level | Mandatory features |
|-------|--------------------|
| Core | v0.1 baseline |
| Plus | Multipath, Hybrid PQ |
| Full | cMix, Plugin, Low-Power Mode |

---
_Note: the Rust codebase on `main` already implements all bullet-points above. Backwards-compatibility with v0.1 peers is preserved via capability negotiation._ 