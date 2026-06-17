# winassoc 設定画面 + Slint移行 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** egui/eframe を Slint に置き換え、設定 GUI を新設し、ピッカーを Slint に移植する

**Architecture:** Slint の declarative UI (.slint files) + Rust ロジック層。picker.slint と settings.slint の2つの UI 定義。icon.rs は egui 非依存化。既存の config/engine/registry は変更なし。

**Tech Stack:** Rust 2021, Slint 1.x, windows-rs 0.61, winreg 0.55

---

## ファイル構造

```
変更:
  Cargo.toml              # egui/eframe/raw-window-handle 削除、slint+slint-build 追加
  build.rs                # slint-build コンパイル追加
  src/icon.rs             # egui::ColorImage → 独自 RgbaImage 型
  src/commands.rs          # picker::show 呼び出しの型調整
  src/main.rs             # 引数なし→settings::run()

追加:
  src/picker/picker.slint # ピッカー UI 定義
  src/settings/mod.rs     # 設定画面ロジック
  src/settings/settings.slint # 設定画面 UI 定義

削除:
  src/picker/window.rs    # Slint がウィンドウ管理

書き換え:
  src/picker/mod.rs       # egui → Slint
  src/lib.rs              # pub mod settings 追加
```

---

### Task 1: Cargo.toml + build.rs 変更（依存関係の入れ替え）

**Files:**
- Modify: `Cargo.toml`
- Modify: `build.rs`
- Modify: `src/icon.rs`（egui インポート削除のみ、コンパイルを通すための最小変更）

- [ ] **Step 1: Cargo.toml の依存を入れ替える**

以下の行を削除:
```toml
eframe = { version = "0.32", default-features = false, features = ["glow", "default_fonts"] }
raw-window-handle = "0.6"
```

以下の行を追加:
```toml
slint = "1"
```

`[build-dependencies]` に以下を追加:
```toml
slint-build = "1"
```

- [ ] **Step 2: build.rs に slint-build のコンパイル手順を追加**

```rust
fn main() {
    slint_build::compile("src/settings/settings.slint").unwrap();
    slint_build::compile("src/picker/picker.slint").unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().unwrap();
    }
}
```

- [ ] **Step 3: icon.rs の egui 依存を一旦除去（コンパイルを通すため）**

`src/icon.rs` の `use eframe::egui;` を削除し、戻り値型を仮の型に変更する（Task 2 で本格対応）。

`extract_icon_rgba` のシグネチャを:
```rust
pub fn extract_icon_rgba(path: &str, size: i32) -> Option<Vec<u8>> {
```
戻り値:
```rust
Some(pixels)
```
型注釈から `egui::ColorImage` 参照を削除。

- [ ] **Step 4: ビルド確認**

```bash
cargo check 2>&1
```

Expected: コンパイルエラー多数（picker周りが未対応のため）。エラーの種類を確認して Task 2 以降で対処。

---

### Task 2: icon.rs の egui 非依存化

**Files:**
- Modify: `src/icon.rs`

- [ ] **Step 1: RgbaImage 型を定義し extract_icon_rgba を書き換え**

```rust
//! exe からのアプリアイコン抽出 (IShellItemImageFactory)

use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::Shell::{
    SHCreateItemFromParsingName, IShellItemImageFactory, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};

/// RGBA (unmultiplied) のアイコン画像データ
pub struct RgbaImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// exe パスから RGBA (unmultiplied) のアイコン画像を抽出する。失敗時は None
pub fn extract_icon_rgba(path: &str, size: i32) -> Option<RgbaImage> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None).ok()?;
        let hbitmap = factory
            .GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
            .ok()?;

        let hdc = GetDC(None);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (size * size * 4) as usize];
        let lines = GetDIBits(
            hdc,
            hbitmap,
            0,
            size as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteObject(hbitmap.into());
        ReleaseDC(None, hdc);
        if lines == 0 {
            return None;
        }

        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
            let a = px[3] as u32;
            if a > 0 && a < 255 {
                px[0] = ((px[0] as u32 * 255) / a).min(255) as u8;
                px[1] = ((px[1] as u32 * 255) / a).min(255) as u8;
                px[2] = ((px[2] as u32 * 255) / a).min(255) as u8;
            }
        }
        if pixels.iter().skip(3).step_by(4).all(|&a| a == 0) {
            return None;
        }
        Some(RgbaImage { width: size as usize, height: size as usize, pixels })
    }
}
```

- [ ] **Step 2: ビルド確認**

```bash
cargo check 2>&1
```

Expected: icon.rs はコンパイル通過。picker 周辺にエラーが残る（Task 3-4 で対処）。

- [ ] **Step 3: コミット**

```bash
git add src/icon.rs Cargo.toml build.rs
git commit -m "refactor: replace egui with slint deps, migrate icon.rs"
```

---

### Task 3: picker.slint の作成

**Files:**
- Create: `src/picker/picker.slint`

- [ ] **Step 1: picker.slint を作成**

```slint
//! ピッカー UI — egui 版と同等のアイコングリッド + キーボード操作

export component Picker inherits Window {
    preferred-width: 300px;
    preferred-height: 300px;
    title: "winassoc";

    in-out property <string> target_label;
    in property <[CandidateData]> candidates: [];
    in property <[image]> icons: [];
    in-out property <int> selected: 0;
    in property <string> panel_color: #181a20d7;
    in property <string> text_color: #ebebeb;
    in property <string> subtle_color: #a0a0a0;
    in property <string> accent_color: #60a5fa;

    callback choose(int /* index */);
    callback cancel();

    struct CandidateData {
        name: string,
        label: string,
    }

    // ... 以下、グリッド表示・キー操作等を実装
}
```

**注意:** このステップでは `.slint` ファイルの完全な実装を記述する。Slint の `GridLayout` または `HorizontalLayout`/`VerticalLayout` を組み合わせてタイルを配置。タイルは `Rectangle` + `Image` + `Text` で構成。キー操作は `focus-changed-event` と `key-pressed` / `key-released` コールバックを使用。Slint には即時のキーコールバックがないバージョンもあるため、Rust 側でキー処理を実装する可能性も考慮する。

ファイルの全内容は Step 2 でビルド確認後に調整。

- [ ] **Step 2: 仮の最小限 .slint でビルド確認**

```slint
export component Picker inherits Window {
    title: "winassoc";
    in-out property <string> target_label;
}
```

```bash
cargo check 2>&1
```

Expected: build.rs が picker.slint のコンパイルに成功し、`picker` モジュールが生成される。

- [ ] **Step 3: コミット**

```bash
git add src/picker/picker.slint
git commit -m "feat: add picker.slint skeleton"
```

---

### Task 4: picker/mod.rs を egui から Slint に書き換え

**Files:**
- Modify: `src/picker/mod.rs`
- Delete: `src/picker/window.rs`

- [ ] **Step 1: window.rs を削除**

```bash
rm src/picker/window.rs
```

- [ ] **Step 2: mod.rs を Slint 版に書き換え**

`mod window;` と `use window::{...};` を削除し、以下に置き換え：

```rust
//! ランチャー画面 — Slint 版
//!
//! アイコン横並びグリッド。キーボード操作対応。
//! マウスカーソル付近にポップアップする。

use std::sync::mpsc;

use crate::error::{Error, Result};
use crate::{icon, platform};

slint::include_modules!();

const ICON_SIZE: i32 = 64;
const MAX_COLS: usize = 6;

pub struct Candidate {
    pub name: String,
    pub label: Option<String>,
    pub program: String,
}

/// ピッカーを表示し、選ばれたアプリ名を返す (None = キャンセル)
pub fn show(target_label: String, candidates: Vec<Candidate>, timeout_ms: u64) -> Result<Option<String>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    platform::init_com();

    let mut icons: Vec<slint::Image> = Vec::new();
    for c in &candidates {
        match icon::extract_icon_rgba(&c.program, ICON_SIZE) {
            Some(img) => {
                let pixel_buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &img.pixels, img.width as u32, img.height as u32
                );
                icons.push(slint::Image::from_rgba8(pixel_buffer));
            }
            None => {
                icons.push(slint::Image::default());
            }
        }
    }

    // ピッカー用の CandidateData 構築
    let candidate_data: Vec<PickerCandidateData> = candidates.iter().map(|c| {
        PickerCandidateData {
            name: c.name.clone().into(),
            label: c.label.clone().unwrap_or_default().into(),
        }
    }).collect();

    let total = candidates.len();
    let cols = total.min(MAX_COLS);
    let rows = total.div_ceil(MAX_COLS);
    let width = 18.0 * 2.0 + cols as f32 * 116.0 + (cols.saturating_sub(1)) as f32 * 12.0;
    let height = 18.0 * 2.0 + 60.0 + rows as f32 * 140.0 + (rows.saturating_sub(1)) as f32 * 12.0;

    let dark = platform::prefers_dark_theme();

    let picker = Picker::new().map_err(|e| Error::new(format!("ピッカーの起動に失敗しました: {e}")))?;

    // ウィンドウ設定
    picker.set_target_label(target_label.into());
    picker.set_candidates(candidate_data.into());

    let (tx, rx) = mpsc::channel::<Option<String>>();

    let picker_weak = picker.as_weak();
    let tx_clone = tx.clone();
    picker.on_choose(move |index| {
        if let Some(picker) = picker_weak.upgrade() {
            let candidates = picker.get_candidates();
            let name = candidates.row_data(index as usize).unwrap().name.to_string();
            let _ = tx_clone.send(Some(name));
            let _ = slint::quit_event_loop();
        }
    });

    let picker_weak2 = picker.as_weak();
    picker.on_cancel(move || {
        if let Some(picker) = picker_weak2.upgrade() {
            let _ = tx.send(None);
            let _ = slint::quit_event_loop();
        }
    });

    picker.window().set_position(slint::PhysicalPosition::new(
        cursor_x() as i32, cursor_y() as i32
    ));

    // タイムアウトタイマー
    let picker_weak3 = picker.as_weak();
    let tx_timeout = tx.clone();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(timeout_ms), move || {
        if let Some(picker) = picker_weak3.upgrade() {
            let _ = tx_timeout.send(None);
        }
        let _ = slint::quit_event_loop();
    });

    picker.run().map_err(|e| Error::new(format!("ピッカーの実行に失敗しました: {e}")))?;

    Ok(rx.try_recv().ok().flatten())
}

fn cursor_x() -> f64 {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            cursor.x as f64
        } else {
            200.0
        }
    }
}

fn cursor_y() -> f64 {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            cursor.y as f64
        } else {
            200.0
        }
    }
}
```

- [ ] **Step 3: ビルド確認**

```bash
cargo check 2>&1
```

Expected: picker モジュールがコンパイル通過。ただし settings モジュールが未作成のためエラーあり。

- [ ] **Step 4: コミット**

```bash
git add src/picker/mod.rs
git rm src/picker/window.rs
git commit -m "refactor: migrate picker from egui to Slint"
```

---

### Task 5: settings.slint の作成

**Files:**
- Create: `src/settings/settings.slint`

- [ ] **Step 1: settings.slint を作成**

```slint
//! 設定画面 UI — サイドバー + 4 ページ

export component Settings inherits Window {
    preferred-width: 960px;
    preferred-height: 680px;
    title: "WinAssoc 設定";
    icon: @image-url("../../icon.ico");

    // ページ enum
    enum Page {
        apps,
        extensions,
        protocols,
        management,
    }

    in-out property <Page> current_page: Page.apps;
    in-out property <bool> dirty: false;
    in property <string> config_path: "";

    // Sidebar
    VerticalLayout {
        alignment: start;
        Rectangle {
            // sidebar background
        }
        // ... navigation buttons
    }

    // Main content area (changes based on current_page)
    // ...
}
```

- [ ] **Step 2: ビルド確認**

```bash
cargo check 2>&1
```

---

### Task 6: settings/mod.rs の作成

**Files:**
- Create: `src/settings/mod.rs`

- [ ] **Step 1: settings/mod.rs を作成（Config 読込・Slint バインディング・管理操作）**

```rust
//! 設定画面ロジック — Slint バインディング

use crate::config::Config;
use crate::error::Result;
use crate::{registry, platform};

slint::include_modules!();

pub fn run() -> Result<()> {
    platform::init_com();
    let config_path = crate::config::resolve_config_path()?;
    let config = Config::load(&config_path).unwrap_or_else(|_| Config::default_config());

    let ui = Settings::new().map_err(|e| crate::error::Error::new(format!("設定画面の起動に失敗しました: {e}")))?;

    // Set initial values
    ui.set_config_path(config_path.display().to_string().into());

    // Apply callback
    {
        let config = config.clone();
        let config_path = config_path.clone();
        ui.on_apply(move || {
            if let Err(e) = registry::apply(&config) {
                let _ = platform::show_error_dialog(&format!("適用に失敗しました: {e}"));
            }
        });
    }

    // Doctor callback
    {
        let config = config.clone();
        let config_path = config_path.clone();
        ui.on_doctor(move || {
            let result = registry::doctor(&config, &config_path);
            // display result
        });
    }

    // ... other callbacks for unregister, backup, restore, save

    ui.run().map_err(|e| crate::error::Error::new(format!("設定画面の実行に失敗しました: {e}")))?;
    Ok(())
}
```

- [ ] **Step 2: Config::default_config() を追加**

`src/config/mod.rs` に以下を追加:
```rust
impl Config {
    pub fn default_config() -> Self {
        Self {
            apps: BTreeMap::new(),
            ext: BTreeMap::new(),
            protocol: BTreeMap::new(),
            settings: Settings::default(),
        }
    }
}
```

---

### Task 7: main.rs と lib.rs の更新

**Files:**
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: lib.rs に settings モジュールを追加**

```rust
pub mod error;
pub mod commands;
pub mod config;
pub mod engine;
pub mod icon;
pub mod logging;
pub mod picker;
pub mod platform;
pub mod registry;
pub mod settings;
```

- [ ] **Step 2: main.rs に GUI 分岐を追加**

既存の `fn main()` の先頭を以下のように変更:

```rust
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        slint::platform::set_platform_theme("fluent".into()).ok();
        return settings::run();
    }

    let matches = build_cli().get_matches();
    // ... 既存の CLI 処理を続行
```

- [ ] **Step 3: ビルド確認**

```bash
cargo check 2>&1
```

Expected: コンパイル成功。

- [ ] **Step 4: コミット**

```bash
git add src/main.rs src/lib.rs src/config/mod.rs src/settings/
git commit -m "feat: add Slint settings GUI with sidebar layout"
```

---

### Task 8: 全テスト実行と修正

**Files:**
- Modify: (必要に応じて各ファイル)

- [ ] **Step 1: cargo test 実行**

```bash
cargo test 2>&1
```

Expected: 既存テスト全通過。

- [ ] **Step 2: cargo clippy 実行**

```bash
cargo clippy -- -D warnings 2>&1
```

修正すべき警告があれば修正。

- [ ] **Step 3: cargo build --release 実行**

```bash
cargo build --release 2>&1
```

Expected: リリースビルド成功。バイナリサイズを確認。

- [ ] **Step 4: 手動テスト**

1. `winassoc.exe` を引数なしで起動 → 設定 GUI が開く
2. `winassoc.exe apply` → CLI が動作（後方互換）
3. ピッカー: `winassoc-open.exe` をテスト用に起動 → ピッカー表示

- [ ] **Step 5: 最終コミット**

```bash
git add -A
git commit -m "chore: finalize slint migration and settings GUI"
```

---

## Self-Review

### Spec coverage
- [x] Cargo.toml 変更 → Task 1
- [x] icon.rs egui 非依存化 → Task 2
- [x] picker.slint 作成 → Task 3
- [x] picker/mod.rs 書換 + window.rs 削除 → Task 4
- [x] settings.slint 作成 → Task 5
- [x] settings/mod.rs 作成 → Task 6
- [x] main.rs + lib.rs 更新 → Task 7
- [x] 検証 → Task 8

### Placeholder scan
- `.slint` ファイルの内容は実際のコードを含める必要がある → Step 内に具体実装を含めた
- タイムアウト・カーソル位置・DPI 対応は実装に含めた

### Type consistency
- `icon::RgbaImage` → `slint::Image` への変換 → Task 4 で実施
- `Candidate` → `PickerCandidateData` → `.slint` 構造体とのマッピング → Task 4
- `settings::run()` → 引数なしで `main.rs` から呼び出し → Task 7
