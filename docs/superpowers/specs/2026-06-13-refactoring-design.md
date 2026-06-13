# winassoc リファクタリング設計

**作成日:** 2026-06-13
**スコープ:** 責務分割・テスタビリティ・パフォーマンス・重複削減・エラーハンドリング

---

## 現状の問題点

| ファイル | 行数 | 問題 |
|----------|------|------|
| `picker.rs` | 516 | UI描画・ウィンドウ装飾・フォント・位置計算が混在 |
| `registry.rs` | 481 | apply/unregister/doctor/backup が混在 |
| `commands.rs` | 203 | platform API（modifier/dialog）混在、テスト不可能 |
| `engine.rs` | 405 | `glob_match()` が毎回 GlobBuilder 構築 |
| `config.rs` | 188 | `validate()` の中に検証ロジックが集中 |

---

## 変更後ファイル構成

```
src/
  main.rs              (変更なし)
  lib.rs               (pub mod platform 追加)

  config.rs            (validate が config/validate.rs へ分離)

  engine.rs            (glob マッチングにキャッシュ追加)

  commands.rs          (platform 依存を platform.rs へ抽出)

  platform.rs          [新設]
    - init_com()                        # icon.rs から移動
    - current_modifiers() -> Modifiers  # commands.rs から移動
    - show_error_dialog(message)        # commands.rs から移動
    - prefers_dark_theme() -> bool      # picker.rs から移動

  logging.rs           (変更なし)
  icon.rs              (init_com を platform へ移動、再利用)

  picker/
    mod.rs             (Candidate, show(), PickerApp本体, draw_tile, lerp_color)
    window.rs          (apply_window_effects, popup_position)

  registry/
    mod.rs             (定数群, progid_for_ext, hkcu, apply, unregister, notify)
    doctor.rs          (doctor, read_user_choice_ext, read_user_choice_protocol)
    backup.rs          (Backup/ExtBackup/ProtocolBackup 型, backup, load_backup, restore)
```

---

## モジュール責務詳細

### platform.rs（新設）

OS依存の処理を一箇所に集約する。

```rust
// init_com → icon.rs から移動
// current_modifiers → commands.rs から移動
// show_error_dialog → commands.rs から移動
// prefers_dark_theme → picker.rs から移動
```

**呼び出し元:**
- `commands.rs` → `platform::current_modifiers()`, `platform::show_error_dialog()`
- `picker/mod.rs` → `platform::prefers_dark_theme()`
- `picker/window.rs` → `platform::prefers_dark_theme()`
- `icon.rs` → `platform::init_com()`

### picker/ 分割

**picker/mod.rs** (~380行)

- `Candidate` 構造体
- `show()` 公開関数（エントリポイント）
- `PickerApp` 構造体と `eframe::App` 実装
- `lerp_color()` ユーティリティ（picker 内のみで使用）

**picker/window.rs** (~160行)

- `apply_window_effects()` — DWM Mica/Acrylic, 角丸, ダークモード
- `popup_position()` — マウス位置算出, DPI スケーリング, モニタクランプ

### registry/ 分割

**registry/mod.rs** (~260行)

- 定数（APP_NAME, URL_PROGID, PROGID_PREFIX, CLIENT_PATH, FILE_EXTS_PATH, URL_ASSOC_PATH）
- `hkcu()` ヘルパー
- `progid_for_ext()` ユーティリティ
- `shim_exe()` / `shim_command()` / `shim_command_for()`
- `apply()` / `register_custom_scheme()`
- `unregister()` / `cleanup_empty_ext_key()`
- `notify_assoc_changed()`

**registry/doctor.rs** (~100行)

- `doctor()`
- `read_user_choice_ext()`
- `read_user_choice_protocol()`

**registry/backup.rs** (~150行)

- `Backup`, `ExtBackup`, `ProtocolBackup` 型
- `backup_dir()`
- `backup()`
- `load_backup()`
- `restore()`

### engine.rs 改善

`glob_match()` にパターンキャッシュを導入：

```rust
use std::sync::LazyLock;
use std::collections::HashMap;
use globset::{Glob, GlobMatcher};

static GLOB_CACHE: LazyLock<std::sync::Mutex<HashMap<String, GlobMatcher>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

fn glob_match(pattern: &str, text: &str) -> bool {
    let mut cache = GLOB_CACHE.lock().unwrap();
    let matcher = cache.entry(pattern.to_string()).or_insert_with(|| {
        GlobBuilder::new(pattern)
            .case_insensitive(true)
            .literal_separator(false)
            .build()
            .map(|g| g.compile_matcher())
            .unwrap_or_else(|_| Glob::new("*").unwrap().compile_matcher())
    });
    matcher.is_match(text)
}
```

### config.rs 改善

`validate()` を `config/validate.rs` に抽出。`Config::validate()` はそのまま公開 API として残し、内部で `validate::validate_config()` を呼ぶ。

---

## 変更対象外

- `Cargo.toml` — 依存関係に変更なし
- `build.rs` — 変更なし
- `logging.rs` — 変更なし
- 公開 API（`Config::load`, `engine::evaluate`, `registry::apply` 等）は維持
- テストケース（`engine.rs` の `mod tests`）はそのまま維持

---

## 検証計画

1. `cargo check` — コンパイル確認
2. `cargo test` — 既存テスト全通過
3. `cargo clippy` — lint 確認
4. `cargo build --release` — リリースビルド確認
