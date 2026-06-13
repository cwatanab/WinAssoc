# winassoc リファクタリング Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** picker.rs / registry.rs の責務分割、platform 抽象層の導入、glob キャッシュによるパフォーマンス改善、config validate の分離

**Architecture:** 既存の公開 API を維持したまま、肥大化したファイルを責務単位で分割する。プラットフォーム依存（Windows API）は platform.rs に一元化する。

**Tech Stack:** Rust 2021, windows-rs 0.61, eframe 0.32, globset 0.4, winreg 0.55

---

## 依存関係図

```
platform.rs ──┐
              ├──→ commands.rs
              ├──→ picker/mod.rs
              ├──→ picker/window.rs
              └──→ icon.rs

picker/mod.rs ── picker/window.rs, platform.rs, icon.rs
registry/mod.rs ── registry/doctor.rs, registry/backup.rs (re-export)
engine.rs ── globset (cache追加)
config.rs ── config/validate.rs (内部呼出)
```

---

### Task 1: platform.rs を作成し、OS 依存関数を抽出

**Files:**
- Create: `src/platform.rs`
- Modify: `src/lib.rs`
- Modify: `src/icon.rs`
- Modify: `src/commands.rs`
- Modify: `src/picker.rs`

- [ ] **Step 1: `src/platform.rs` を新規作成**

```rust
//! OS 依存処理の集約レイヤー
//!
//! COM 初期化・修飾キー取得・エラーダイアログ・OS テーマ判定

use crate::engine::Modifiers;

/// プロセスごとに一度だけ COM を初期化する
pub fn init_com() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
}

/// 起動時点の修飾キー押下状態 (GetAsyncKeyState)
#[cfg(windows)]
pub fn current_modifiers() -> Modifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    let down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
    Modifiers {
        shift: down(VK_SHIFT.0 as i32),
        ctrl: down(VK_CONTROL.0 as i32),
        alt: down(VK_MENU.0 as i32),
    }
}

#[cfg(not(windows))]
pub fn current_modifiers() -> Modifiers {
    Modifiers::default()
}

/// シェル起動時 (コンソールなし) のエラー通知ダイアログ
#[cfg(windows)]
pub fn show_error_dialog(message: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from("winassoc"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
pub fn show_error_dialog(message: &str) {
    eprintln!("{message}");
}

/// OS のアプリテーマ設定 (AppsUseLightTheme) を読む
pub fn prefers_dark_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|light| light == 0)
        .unwrap_or(false)
}
```

- [ ] **Step 2: `src/lib.rs` に `pub mod platform;` を追加**

```rust
pub mod commands;
pub mod config;
pub mod engine;
pub mod icon;
pub mod logging;
pub mod picker;
pub mod platform;
pub mod registry;
```

- [ ] **Step 3: `src/icon.rs` の `init_com()` を削除し、呼び出しを `platform::init_com` に変更**

`src/icon.rs` から以下を削除:
```rust
/// プロセスごとに一度だけ COM を初期化する
pub fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}
```

`use crate::icon;` → `use crate::platform;` に変更し、`icon::init_com()` → `platform::init_com()` に変更。これは picker/mod.rs での呼び出し。

- [ ] **Step 4: `src/commands.rs` の `current_modifiers()` と `show_error_dialog()` を削除し、`use crate::platform;` に切り替え**

削除対象:
- `current_modifiers()` 関数全体 (187-203行)
- `show_error_dialog()` 関数全体 (169-186行)
- `#[cfg(not(windows))]` の `show_error_dialog` スタブ (200-203行の下)

`current_modifiers()` の呼び出しを `platform::current_modifiers()` に変更。

- [ ] **Step 5: `src/picker.rs` の `prefers_dark_theme()` を削除し、`use crate::platform;` に切り替え**

`picker.rs` の `prefers_dark_theme()` 関数全体 (435-442行) を削除。

`let dark = prefers_dark_theme();` (47行目) → `let dark = platform::prefers_dark_theme();` に変更。

- [ ] **Step 6: ビルドとテスト確認**

```bash
cargo check 2>&1
```

Expected: コンパイル成功、warning なし

```bash
cargo test 2>&1
```

Expected: 全テスト PASS

- [ ] **Step 7: コミット**

```bash
git add src/platform.rs src/lib.rs src/icon.rs src/commands.rs src/picker.rs
git commit -m "refactor: extract platform layer (init_com, modifiers, dialog, theme)"
```

---

### Task 2: picker.rs を picker/mod.rs + picker/window.rs に分割

**Files:**
- Create: `src/picker/mod.rs`
- Create: `src/picker/window.rs`
- Delete: `src/picker.rs`

- [ ] **Step 1: `src/picker/window.rs` を作成**

```rust
//! ウィンドウ装飾 (DWM) とポップアップ位置計算

use eframe::egui::Pos2;
use raw_window_handle::RawWindowHandle;

/// Mica/Acrylic 背景 + 角丸 + ダークモードを DWM で適用
pub fn apply_window_effects(cc: &eframe::CreationContext<'_>, dark: bool) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_SYSTEMBACKDROP_TYPE,
    };

    let Ok(handle) = cc.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
    let hwnd = HWND(win32.hwnd.get() as *mut std::ffi::c_void);

    unsafe {
        let backdrop = DWM_SYSTEMBACKDROP_TYPE(3);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(),
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const _ as *const std::ffi::c_void).cast(),
            std::mem::size_of_val(&corner) as u32,
        );
        let dark_mode = BOOL(dark as i32);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_mode as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// マウスカーソル付近に表示し、モニタのワークエリア内にクランプ。
/// 戻り値は egui の論理ポイント
pub fn popup_position(width_pt: f32, height_pt: f32) -> Pos2 {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() {
            return Pos2::new(200.0, 200.0);
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);

        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let scale = (dpi_x as f32 / 96.0).max(0.5);

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let work = if GetMonitorInfoW(monitor, &mut info).as_bool() {
            info.rcWork
        } else {
            windows::Win32::Foundation::RECT { left: 0, top: 0, right: 1920, bottom: 1080 }
        };

        let w = width_pt * scale;
        let h = height_pt * scale;
        let x = (cursor.x as f32 - w / 2.0)
            .clamp(work.left as f32 + 8.0, (work.right as f32 - w - 8.0).max(work.left as f32));
        let y = (cursor.y as f32 + 16.0)
            .clamp(work.top as f32 + 8.0, (work.bottom as f32 - h - 8.0).max(work.top as f32));
        Pos2::new(x / scale, y / scale)
    }
}
```

- [ ] **Step 2: `src/picker/mod.rs` を作成（picker.rs から window/font/theme を除いた内容）**

既存の `src/picker.rs` の内容から以下を **削除**:
- `apply_window_effects()` 関数全体 (396-432行)
- `popup_position()` 関数全体 (446-485行)
- `prefers_dark_theme()` 関数全体 (435-442行) ※Task 1 で削除済みのはず
- `install_japanese_fonts()` 関数全体 (490-506行) → フォントは mod.rs に残す
- `use` 文の `raw_window_handle` 参照

**追加する use 文:**
```rust
use crate::platform;
mod window;
use window::{apply_window_effects, popup_position};
```

**変更箇所:**
- `show()` 内の `let dark = prefers_dark_theme();` → `let dark = platform::prefers_dark_theme();`
- `show()` 内の `icon::init_com();` → `platform::init_com();`
- `icon::extract_icon_rgba` は `crate::icon::extract_icon_rgba` に変更

- [ ] **Step 3: 古い `src/picker.rs` を削除**

```bash
rm src/picker.rs
```

- [ ] **Step 4: `src/lib.rs` を更新**

`pub mod picker;` → そのまま（Rust は `picker/mod.rs` を自動認識）

- [ ] **Step 5: ビルドとテスト確認**

```bash
cargo check 2>&1
```

Expected: コンパイル成功

```bash
cargo test 2>&1
```

Expected: 全テスト PASS

- [ ] **Step 6: コミット**

```bash
git add src/picker/ src/lib.rs
git rm src/picker.rs
git commit -m "refactor: split picker into mod.rs + window.rs"
```

---

### Task 3: registry.rs を registry/mod.rs + doctor.rs + backup.rs に分割

**Files:**
- Create: `src/registry/mod.rs`
- Create: `src/registry/doctor.rs`
- Create: `src/registry/backup.rs`
- Delete: `src/registry.rs`

- [ ] **Step 1: `src/registry/backup.rs` を作成**

`src/registry.rs` の以下を抽出:
- `Backup`, `ExtBackup`, `ProtocolBackup` 構造体 (353-373行)
- `backup_dir()` (375-382行)
- `backup()` (385-417行)
- `load_backup()` (419-427行)
- `restore()` (429-472行)

ファイル先頭に必要な use を記述:
```rust
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use winreg::enums::KEY_ALL_ACCESS;

use super::{hkcu, notify_assoc_changed, PROGID_PREFIX};

// ... Backup types ...
// ... backup_dir, backup, load_backup, restore ...
```

- [ ] **Step 2: `src/registry/doctor.rs` を作成**

`src/registry.rs` の以下を抽出:
- `doctor()` (246-335行)
- `read_user_choice_ext()` (337-342行)
- `read_user_choice_protocol()` (344-349行)

ファイル先頭:
```rust
use std::path::Path;

use anyhow::Result;

use super::{hkcu, progid_for_ext, shim_command, shim_exe, APP_NAME, CLIENT_PATH, 
           FILE_EXTS_PATH, URL_ASSOC_PATH, URL_PROGID};
use super::backup::Backup;
use crate::config::{expand_env, Config};
```

- [ ] **Step 3: `src/registry/mod.rs` を作成（残りの内容）**

`src/registry.rs` の以下を残す:
- 定数群 (16-21行)
- `hkcu()` (23-25行)
- `shim_exe()` / `shim_command()` / `shim_command_for()` (29-49行)
- `progid_for_ext()` (52-54行)
- `apply()` / `register_custom_scheme()` (58-144行)
- `unregister()` / `cleanup_empty_ext_key()` (148-242行)
- `notify_assoc_changed()` (476-481行)

先頭に追加:
```rust
pub mod backup;
pub mod doctor;
```

`doctor()` の呼び出し元は `registry::doctor(&config, &config_path)` → そのまま（re-export 不要）

- [ ] **Step 4: 古い `src/registry.rs` を削除**

```bash
rm src/registry.rs
```

- [ ] **Step 5: ビルドとテスト確認**

```bash
cargo check 2>&1
cargo test 2>&1
```

Expected: コンパイル成功、全テスト PASS

- [ ] **Step 6: コミット**

```bash
git add src/registry/ src/lib.rs
git rm src/registry.rs
git commit -m "refactor: split registry into mod + doctor + backup"
```

---

### Task 4: engine.rs の glob_match にキャッシュを導入

**Files:**
- Modify: `src/engine.rs`

- [ ] **Step 1: `glob_match()` に LazyLock キャッシュを導入**

`src/engine.rs` の `glob_match()` 関数 (163-169行) を以下で置換:

```rust
use std::collections::HashMap;
use std::sync::LazyLock;

static GLOB_CACHE: LazyLock<std::sync::Mutex<HashMap<String, globset::GlobMatcher>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// パス区切りを跨いで `**`/`*` を一様に扱う、大文字小文字無視の glob 一致
fn glob_match(pattern: &str, text: &str) -> bool {
    use globset::GlobBuilder;
    let mut cache = GLOB_CACHE.lock().unwrap();
    let matcher = cache.entry(pattern.to_string()).or_insert_with(|| {
        GlobBuilder::new(pattern)
            .case_insensitive(true)
            .literal_separator(false)
            .build()
            .map(|g| g.compile_matcher())
            .unwrap_or_else(|_| globset::Glob::new("*").unwrap().compile_matcher())
    });
    matcher.is_match(text)
}
```

- [ ] **Step 2: `use globset::GlobBuilder;` の行を削除（または共通 use 内に移動）**

ファイル先頭の `use globset::GlobBuilder;` (2行目) を削除し、`use std::collections::HashMap;` と `use std::sync::LazyLock;` を追加。

- [ ] **Step 3: ビルドとテスト確認**

```bash
cargo test 2>&1
```

Expected: 全テスト PASS（既存の engine のテストでキャッシュの動作も確認される）

- [ ] **Step 4: コミット**

```bash
git add src/engine.rs
git commit -m "perf: add glob pattern cache to engine"
```

---

### Task 5: config validate を config/validate.rs に分離

**Files:**
- Create: `src/config/validate.rs`
- Modify: `src/config.rs`

- [ ] **Step 1: `src/config/` ディレクトリを作成し、`validate.rs` を作成**

```rust
use std::collections::BTreeMap;

use anyhow::bail;

use super::{AppDef, Config, RouteTable, Rule};

pub fn validate_config(config: &Config) -> anyhow::Result<()> {
    let tables = config
        .ext
        .iter()
        .map(|(k, v)| (format!("ext.{k}"), v))
        .chain(config.protocol.iter().map(|(k, v)| (format!("protocol.{k}"), v)));

    for (name, table) in tables {
        if let Some(app) = &table.default {
            if !config.apps.contains_key(app) {
                bail!("[{name}] default の \"{app}\" が [apps] に定義されていません");
            }
        }
        if let Some(candidates) = &table.candidates {
            for app in candidates {
                if !config.apps.contains_key(app) {
                    bail!("[{name}] candidates の \"{app}\" が [apps] に定義されていません");
                }
            }
        }
        for (i, rule) in table.rules.iter().enumerate() {
            let at = format!("[{name}] rules[{i}]");
            match (&rule.app, rule.pick) {
                (Some(app), false) => {
                    if !config.apps.contains_key(app) {
                        bail!("{at}: app = \"{app}\" が [apps] に定義されていません");
                    }
                }
                (None, true) => {}
                (Some(_), true) => bail!("{at}: app と pick は同時指定できません"),
                (None, false) => bail!("{at}: app か pick = true のどちらかが必要です"),
            }
            if let Some(m) = &rule.modifier {
                if !matches!(m.as_str(), "shift" | "ctrl" | "alt") {
                    bail!("{at}: modifier は shift / ctrl / alt のいずれかです (指定値: {m})");
                }
            }
            if rule.glob.is_none() && rule.host.is_none() && rule.url.is_none() && rule.modifier.is_none() && i + 1 < table.rules.len() {
                bail!("{at}: 条件なしルール (catch-all) は最後にのみ置けます");
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 2: `src/config.rs` の `validate()` を呼び出しに変更**

`src/config.rs` を `src/config/mod.rs` にリネームし、`validate()` メソッドを:

```rust
mod validate;

// ...

impl Config {
    // ...
    pub fn validate(&self) -> Result<()> {
        validate::validate_config(self)
    }
}
```

`src/config.rs` の `validate()` の本体 (119-162行) を削除し、上記の委譲呼び出しに置き換え。

`src/lib.rs` は `pub mod config;` のまま（Rust が `config/mod.rs` を自動認識）。

- [ ] **Step 3: `src/config.rs` → `src/config/mod.rs` にリネーム**

```bash
mv src/config.rs src/config/mod.rs
```

- [ ] **Step 4: ビルドとテスト確認**

```bash
cargo check 2>&1
cargo test 2>&1
```

Expected: コンパイル成功、全テスト PASS

- [ ] **Step 5: コミット**

```bash
git add src/config/
git rm src/config.rs
git commit -m "refactor: extract config validation to validate.rs"
```

---

### Task 6: 最終確認とクリーンアップ

- [ ] **Step 1: 全体ビルド確認**

```bash
cargo check 2>&1
cargo clippy -- -D warnings 2>&1 || true
cargo test 2>&1
cargo build --release 2>&1
```

Expected: すべて成功

- [ ] **Step 2: `Cargo.lock` に変化がないことを確認**

```bash
git diff Cargo.lock
```

Expected: 差分なし（依存関係変更なしのため）

- [ ] **Step 3: コミット（必要な場合のみ）**

```bash
git add -A
git commit -m "refactor: final cleanup and verification" || echo "No changes to commit"
```

---

## Self-Review

### Spec coverage
- [x] platform.rs 新設 → Task 1
- [x] picker 分割 → Task 2
- [x] registry 分割 → Task 3
- [x] engine glob キャッシュ → Task 4
- [x] config validate 分離 → Task 5
- [x] 検証 → Task 6

### Placeholder scan
- No TBD, TODO, 未定義項目なし

### Type consistency
- `platform::current_modifiers()` → `Modifiers` → `engine::Modifiers` ✓
- `platform::prefers_dark_theme()` → `bool` → picker 内で一貫 ✓
- `registry::doctor()` → `Result<()>` → main.rs の呼び出しと一致 ✓
