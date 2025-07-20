# Nyx Plugin Development Guide (Draft)

> Applies to Nyx Protocol v1.0 — Frame Type 0x50–0x5F

---

## 1. Overview
Nyx allows out-of-tree extensions, *plugins*, to exchange custom frames over an existing secure session.  A plugin frame consists of a CBOR-encoded header followed by arbitrary opaque payload.

```
+--------------+----------------------+-----------------+
|  0x50–0x5F   | CBOR header (len L) |  Payload (len P)|
+--------------+----------------------+-----------------+
```

Header schema (CBOR map / canonical order):
| Field | Type | Description |
|-------|------|-------------|
| id    | uint | 32-bit plugin identifier allocated by Nyx registry. |
| flags | uint | Bitmask (permission, encryption level). |
| data  | bytes| Optional small control blob (≤256B recommended). |

The reference implementation exposes `nyx_stream::PluginHeader` which automatically serialises / validates the header against the JSON schema.

---

## 2. Project Layout
```
my_plugin/
 ├─ Cargo.toml       # crate type = `cdylib` or staticlib for FFI
 ├─ src/lib.rs       # exports `nyx_plugin_entry` symbol
 ├─ README.md
 └─ schema.json      # (optional) custom payload schema for receivers
```

### 2.1 Cargo.toml
```toml
[package]
name = "my_plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
```
Export a C ABI entry point:
```rust
#[no_mangle]
pub extern "C" fn nyx_plugin_entry() -> NyxPluginInfo {
    NyxPluginInfo {
        id: 0xfeed_cafe,
        name: "my_plugin\0".into(),
        permissions: Permission::SEND_STREAM.bits(),
    }
}
```

---

## 3. Permissions
Use `nyx_stream::Permission` bitflags:
* `SEND_STREAM` – may transmit Stream frames
* `SEND_DATAGRAM` – may transmit QUIC Datagram frames
* `ACCESS_GEO` – may read coarse geolocation sensor data

A plugin must request the minimal required set during registration.  Nodes MAY refuse a session if a plugin asks for unavailable permissions.

---

## 4. Testing & Conformance
Enable `--features conformance` when running Nyx test-suite.  Provide sample frames under `tests/` exercising:
1. Header round-trip (encode / decode / schema validate)
2. Permission enforcement path (positive & negative)

---

## 5. Deployment Checklist
- [ ] Unique 32-bit ID allocated (`registry/plugins.tsv` PR).
- [ ] Semantic version tag >=0.1.0.
- [ ] README lists required permissions.
- [ ] Fuzz test coverage ≥90 % on payload parser.

---
_Document is a living draft; contributions welcome._ 