# Nyx プラグイン開発ガイド（ドラフト）

> 対象: Nyx Protocol v1.0 / Frame Type 0x50–0x5F

---

## 1. 概要
Nyx では外部拡張モジュール（プラグイン）が CBOR ヘッダ + 任意ペイロード形式のフレームを送受信できます。

```
+--------------+----------------------+-----------------+
| 0x50–0x5F    | CBOR Header (L)     | Payload (P)     |
+--------------+----------------------+-----------------+
```

ヘッダスキーマ:
| フィールド | 型   | 説明 |
|------------|------|------|
| id         | uint | 32bit プラグイン ID（予約表で一意） |
| flags      | uint | 権限フラグ / 暗号レベル |
| data       | bytes| 小規模制御データ（≤256B 推奨） |

`nyx_stream::PluginHeader` で自動シリアライズ / JSON Schema 検証が可能です。

---

## 2. ディレクトリ構成例
```
my_plugin/
 ├─ Cargo.toml  # crate-type = cdylib
 ├─ src/lib.rs  # `nyx_plugin_entry` を公開
 └─ schema.json # 任意ペイロードスキーマ
```

エントリ例:
```rust
#[no_mangle]
pub extern "C" fn nyx_plugin_entry() -> NyxPluginInfo {
    NyxPluginInfo {
        id: 0xfeed_cafe,
        name: "geo_stat\0".into(),
        permissions: Permission::ACCESS_GEO.bits(),
    }
}
```

---

## 3. パーミッション
* `SEND_STREAM`
* `SEND_DATAGRAM`
* `ACCESS_GEO`

不要な権限は申請しないこと。ノード側は超過権限要求で接続を拒否できます。

---

## 4. テスト
* `cargo test --features conformance` で CI に統合。
* `tests/` に round-trip & 権限制御ケースを追加。

---

## 5. リリースチェックリスト
- [ ] `plugins.tsv` に ID 登録 PR
- [ ] SemVer ≥0.1.0 タグ
- [ ] README に権限記載
- [ ] Fuzz カバレッジ 90% 以上

---
_当ガイドは随時更新予定。_ 