# winassoc 設定画面 — Fluent Design 忠実実装

**作成日:** 2026-06-17
**スコープ:** 設定画面のビジュアル刷新 + 各ページの本実装 (フル CRUD)

---

## 背景

前段 (`2026-06-17-settings-screen-design.md`) で Slint への移行と設定 GUI の新設は完了したが、装飾は最小限のベタ色 (`#181a20d7` 等) で Win11 Fluent Design には遠く及ばない。本スペックは、

1. トークン / バックドロップ / タイポグラフィ / コンポーネントの **Fluent Design 2 忠実化**
2. 「アプリ」「拡張子」「プロトコル」各ページの **フル CRUD 実装** (現状プレースホルダのまま)
3. 「管理」ページに **picker_timeout_ms / theme_mode** を統合

を一体的に行う。前回の specs/plans は本スペックの前提として参照のみ。

---

## スコープ

### In scope

- `src/ui/` 配下の全面再構成 (テーマ / コンポーネント / ページ分割)
- 4 ページの本実装 (Apps / Extensions / Protocols / Management)
- Win11 Mica バックドロップ + Reveal ハイライト + 拡張タイトルバー
- 3 状態テーマ トグル (System / Light / Dark)
- 設定ファイル保存時の検証 (参照整合性 / 一意性)
- ルール AND ビルダー UI

### Out of scope

- `src/config/`, `src/engine.rs`, `src/registry/`, `src/icon.rs`, `src/error.rs` のロジック変更
- ピッカーの UI 刷新 (Fluent トークン共有のみ、ロジックは変更なし)
- 多言語対応 (UI 文字列は現状の日本語を維持)
- ドラッグ&ドロップによるファイル取り込み (将来検討)
- インポート / エクスポート (将来検討)
- 自動バックアップ ローテーション (将来検討)

---

## 設計決定 (前段で確認済み)

| # | 決定事項 | 値 |
|---|----------|----|
| 1 | スコープ | ビジュアル刷新 + 各ページ本実装 |
| 2 | Fluent 美学 | Windows 11 (Fluent 2 / WinUI 3 スタイル) |
| 3 | CRUD 範囲 | フル CRUD + ルール並び替え |
| 4 | [settings] 配置 | 管理ページに統合 |
| 5 | ナビゲーション | NavigationView レール (アイコン+テキスト、240px) |
| 6 | ルール編集 UX | 条件 AND ビルダー |
| 7 | テーマ動作 | システム追従 + オーバーライド (3 状態) |
| 8 | バックドロップ | Mica + Acrylic 使い分け |
| 9 | ファイル構造 | テーマトークン + コンポーネント分割 |

---

## アーキテクチャ

### ファイル構成

```
src/
├── ui/
│   ├── theme.slint       … global Theme { ThemeColors × 2 + Typography + spacing + radii }
│   ├── icons.slint       … Segoe Fluent Icons テキスト定数 (AppFolder, Page, Globe, Settings, Add, Delete, ...)
│   ├── components/
│   │   ├── nav_item.slint          … NavigationView アイテム (アイコン + ラベル + 選択インジケータ + Reveal)
│   │   ├── command_bar.slint       … タイトルバー右上のテーマトグル / 保存 / 適用 / 更新
│   │   ├── accent_button.slint     … PrimaryButton / StandardButton / HyperlinkButton
│   │   ├── text_field.slint        … Fluent TextBox (境界線 + フォーカス時 accent ライン)
│   │   ├── number_box.slint        … スピナー付き数値入力
│   │   ├── combo_box.slint         … Fluent ComboBox (Chevron + ポップオーバー)
│   │   ├── checkbox.slint          … Fluent CheckBox
│   │   ├── toggle_switch.slint     … Fluent ToggleSwitch
│   │   ├── card.slint              … 角丸 8px カード (Acrylic / Mica 選択可)
│   │   ├── breadcrumb_bar.slint    … パンくずリスト (将来用、現状未使用)
│   │   ├── master_detail.slint     … 左リスト / 右詳細 の 2 ペイン レイアウト
│   │   ├── rule_row.slint          … ルール 1 件分の AND ビルダー
│   │   ├── condition_chip.slint    … 条件 (glob=..., host=..., url=..., modifier=...) チップ
│   │   └── info_badge.slint        … 状態インジケータ (success / caution / critical / attention)
│   ├── pages/
│   │   ├── apps_page.slint         … アプリ一覧 + 編集フォーム
│   │   ├── extensions_page.slint   … 拡張子 + デフォルト + 候補 + ルール
│   │   ├── protocols_page.slint    … プロトコル (host / url 条件チップ表示)
│   │   └── management_page.slint   … Config 状態 + picker_timeout + テーマ + レジストリ + バックアップ
│   ├── settings.slint     … Settings Window (NavigationView rail + 拡張タイトルバー + pages + ステータスバー)
│   └── picker.slint       … ピッカー (テーマトークン共有のみ)
└── settings/
    ├── mod.rs            … run()、UI 起動、テーマ反映、ページ間配線
    ├── theme.rs          … Mica 適用 (DWM)、アクセント取得、OS テーマ追従
    ├── apps.rs           … AppDef の CRUD (追加/複製/削除/編集 → Config 更新 + VecModel 反映)
    ├── ext.rs            … 拡張子テーブルの CRUD
    ├── protocol.rs       … プロトコルテーブルの CRUD
    ├── management.rs     … apply/unregister/doctor/backup/restore の結果ハンドリング
    ├── validation.rs     … 保存時バリデーション (一意性、参照整合性)
    └── data.rs           … Slint struct ↔ Config struct 変換、入れ子 VecModel 構築
```

### モジュール依存

```
settings/mod.rs ─┬─→ config::Config (既存)
                 ├─→ registry::{apply, unregister, doctor, backup, restore} (既存)
                 ├─→ platform::{init_com, show_error_dialog} (既存)
                 ├─→ settings::theme (新規、DWM)
                 ├─→ settings::apps / ext / protocol / management / validation / data
                 └─→ slint::Settings (ui/settings.slint)

settings/data.rs ─┬─→ config::{Config, AppDef, RouteTable, Rule}
                   └─→ Slint 構造体 (settings.slint で定義)
```

---

## UI レイアウト

### ウィンドウ全体構造

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ 🟦 WinAssoc 設定                       [🌐] [保存] [適用] [⟳] [─][□][×]     │ ← 拡張タイトルバー (32px)
├──────────┬───────────────────────────────────────────────────────────────────┤
│          │  拡張子                                              + 追加 [⋯]   │ ← ページ ヘッダー (60px)
│  📋 Apps │ ┌─────────────────┬────────────────────────────────────────────┐│
│  📄 Ext  │ │  .html          │  デフォルト: [vivaldi ▼]                  ││
│  🌐 Proto│ │  .md            │  候補:     [vivaldi] [brave] [+]           ││
│  ⚙ 管理  │ │  .pdf           │  ルール:                                    ││
│          │ │  .svg           │  ┌──────────────────────────────────────┐  ││
│          │ │  .txt           │  │ 条件 AND            │ 動作 │ ▲▼ ✕ │  ││
│          │ │  + 追加         │  ├─────────────────────┼──────┼─────┤  ││
│          │ │                 │  │ [glob][mod=shift]   │ pick │ ▲▼ ✕ │  ││
│          │ │                 │  └─────────────────────┴──────┴─────┘  ││
│          │ │                 │  + ルールの追加                            ││
│          │ └─────────────────┴────────────────────────────────────────────┘│
│          │  C:\...\config.toml · 未保存                                    │ ← ステータス バー (24px)
└──────────┴───────────────────────────────────────────────────────────────────┘
        ↑ NavigationView Rail (240px)
```

ウィンドウ サイズ: `preferred-width: 1100px`, `preferred-height: 760px`
最小サイズ: `min-width: 880px`, `min-height: 600px`

### 拡張タイトルバー (32px)

Slint 1.x に extended title bar API が無いため、クライアント領域最上部に自前実装:
- 左 16px パディング + アイコン (16x16) + 8px + タイトル テキスト
- 右: テーマ トグル (☀/🌙/🌐 サイクル)、保存、適用、更新、最小化、最大化、閉じる
- 最大化 / 最小化 / 閉じるは WM_NCLBUTTONDOWN フック (Rust 側で `WindowProc` サブクラス化、または `WM_SYSCOMMAND` を SendMessage で送信)

### NavigationView Rail (240px 幅)

- 4 アイテム: Apps / Extensions / Protocols / Management
- 各アイテム:
  - 高さ 40px
  - 左 3px の選択インジケータ (accent-default、選択時のみ表示)
  - アイコン 16x16 + 8px gap + ラベル (Body typography)
  - 背景: 通常 transparent、ホバー時 `nav-hover-reveal` (150ms トランジション)、選択時 `nav-selected`
  - アイコン → ラベル マップ:
    - Apps → `AppFolder` (\uE7BF)
    - Extensions → `Page` (\uE7C3)
    - Protocols → `Globe` (\uE774)
    - Management → `Settings` (\uE713)

### コマンド バー (タイトルバー右側)

- `[🌐/☀/🌙]` テーマ トグル: System → Light → Dark → System のサイクル。アイコン + ツールチップ
- `[保存]`: dirty=true のときのみ enabled、accent-default 塗りつぶし
- `[適用]`: アクセントアウトライン (StandardButton)
- `[⟳]`: 再読み込み (破棄確認ダイアログ: dirty=true の場合)

### ステータス バー (24px)

- 左: 設定ファイル パス (Caption, tertiary 色)
- 中央: dirty 状態インジケータ (InfoBadge: ●保存済 / ●未保存)
- 右: 最終保存日時 (任意)

### ページ ヘッダー (各ページ共通)

- 左: ページ タイトル (Title typography, 28px)
- 右: ページ レベルのコマンド (+ 追加、[⋯] オーバーフロー)

---

## テーマ トークン (`ui/theme.slint`)

```
global Theme {
    in-out property <ThemeMode> mode: ThemeMode.system;

    in property <ThemeColors> light;
    in property <ThemeColors> dark;
    in-out property <ThemeColors> current;

    in property <Typography> typo;

    // mode と OS の AppsUseLightTheme から current を解決
    public function refresh(os-dark: bool) { ... }
}

enum ThemeMode { system, light, dark }

struct ThemeColors {
    // Background
    bg-mica: brush,
    bg-acrylic: brush,
    bg-card: brush,
    bg-sidebar: brush,

    // Layer
    layer-on-acrylic: brush,
    layer-on-mica-base: brush,

    // Text
    text-primary: brush,
    text-secondary: brush,
    text-tertiary: brush,
    text-accent: brush,
    text-on-accent: brush,

    // Control
    control-default: brush,
    control-secondary: brush,
    control-tertiary: brush,
    control-stroke: brush,
    control-focus-stroke: brush,

    // Accent (システム設定から動的)
    accent-default: brush,
    accent-secondary: brush,
    accent-tertiary: brush,

    // System
    system-success: brush,
    system-caution: brush,
    system-critical: brush,
    system-attention: brush,

    // NavigationView
    nav-selected: brush,
    nav-hover-reveal: brush,
    nav-indicator: brush,
}

struct Typography {
    font-family: string,
    caption: { size: 12, line: 16 },
    body: { size: 14, line: 20 },
    body-strong: { size: 14, line: 20, weight: 600 },
    body-large: { size: 18, line: 24 },
    subtitle: { size: 20, line: 28 },
    title: { size: 28, line: 36 },
    display: { size: 40, line: 52 },
}
```

### トークン値 (Light)

| トークン | 値 |
|----------|-----|
| `bg-mica` | `#F3F3F3` |
| `bg-acrylic` | `#FFFFFF` @ 70% |
| `bg-card` | `#FFFFFF` |
| `bg-sidebar` | `#F3F3F3` @ 80% |
| `text-primary` | `#1C1C1C` |
| `text-secondary` | `#5D5D5D` |
| `text-tertiary` | `#8A8A8A` |
| `text-accent` | `#005FB8` |
| `control-default` | `#FFFFFF` |
| `control-stroke` | `#C5C5C5` |
| `accent-default` | `#0078D4` (システム未設定時) |
| `system-success` | `#0F7B0F` |
| `system-critical` | `#C42B1C` |
| `nav-selected` | `#E5E5E5` @ 80% |
| `nav-hover-reveal` | `#000000` @ 5% |

### トークン値 (Dark)

| トークン | 値 |
|----------|-----|
| `bg-mica` | `#202020` |
| `bg-acrylic` | `#2C2C2C` @ 70% |
| `bg-card` | `#2C2C2C` |
| `bg-sidebar` | `#1C1C1C` @ 80% |
| `text-primary` | `#FFFFFF` |
| `text-secondary` | `#C5C5C5` |
| `text-tertiary` | `#8A8A8A` |
| `text-accent` | `#5CC2FF` |
| `control-default` | `#2C2C2C` |
| `control-stroke` | `#4D4D4D` |
| `accent-default` | `#4CC2FF` (システム未設定時) |
| `system-success` | `#54B054` |
| `system-critical` | `#FC8585` |
| `nav-selected` | `#2C2C2C` @ 80% |
| `nav-hover-reveal` | `#FFFFFF` @ 5% |

### スペーシング / 角丸

```
spacing-xxs: 2, xs: 4, s: 8, m: 12, l: 16, xl: 20, xxl: 24, xxxl: 32 px
radius-control: 4px
radius-card: 8px
radius-flyout: 8px
radius-window: 8px
```

### タイポグラフィ フォント

- Primary: `"Segoe UI Variable Text", "Segoe UI", sans-serif`
- Text Segoe UI Variable は Win11 22H2+ で標準搭載、それ未満は `Segoe UI` にフォールバック
- Monospace (結果表示): `"Cascadia Mono", "Consolas", monospace`

---

## バックドロップ / タイトルバー 実装

### Mica 適用 (`src/settings/theme.rs`)

```rust
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};

const DWMWA_SYSTEMBACKDROP_TYPE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(38);
const DWMWA_WINDOW_CORNER_PREFERENCE: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(33);
const DWMWA_CAPTION_COLOR: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(35);

#[repr(i32)]
enum DwmSystemBackdrop {
    Auto = 0, Mica = 2, Acrylic = 3, MicaAlt = 4,
}

#[repr(i32)]
enum DwmWindowCorner {
    Default = 0, DoNotRound = 1, Round = 2, RoundSmall = 3,
}

pub fn apply_fluent_backdrop(hwnd: HWND, mode: ThemeMode) -> Result<()> {
    let backdrop = match mode {
        ThemeMode::Light | ThemeMode::Dark => DwmSystemBackdrop::Mica as i32,
        _ => DwmSystemBackdrop::Auto as i32,
    };
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as _, std::mem::size_of_val(&backdrop) as u32)?;
        let corner = DwmWindowCorner::Default as i32;
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as _, std::mem::size_of_val(&corner) as u32)?;
        // Caption color を Mica の上に透けて見えるよう調整 (アルファ 0 で透過)
        let caption = 0x00FFFFFFu32;
        DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR,
            &caption as *const _ as _, std::mem::size_of_val(&caption) as u32)?;
    }
    Ok(())
}
```

Win10 フォールバック: `apply_fluent_backdrop` は失敗しても握りつぶし、Slint 側で純色 `bg-mica` を背景に描画。

### システム アクセント取得

```rust
pub fn system_accent_color() -> Option<(u8, u8, u8)> {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\DWM")
        .and_then(|k| k.get_value::<u32, _>("AccentColor"))
        .ok()
        .map(|v| {
            // DWORD は AABBGGRR 順
            let r = (v & 0xFF) as u8;
            let g = ((v >> 8) & 0xFF) as u8;
            let b = ((v >> 16) & 0xFF) as u8;
            (r, g, b)
        })
}
```

Slint の `Brush` に変換して `Theme.current.accent-default` を上書き。

### タイトルバー カスタマイズ (WM_NCLBUTTONDOWN フック)

Slint のウィンドウ ハンドル取得 (1.x: `window().window_handle()` 等で HWND にアクセス可能、要 platform-specific API) → `SetWindowSubclass` で `WindowProc` を差し替え、`WM_NCHITTEST` でクライアント領域最上部の特定矩形を `HTCAPTION` として返す → ユーザーがそこをドラッグすると OS がウィンドウを移動。

最大化 / 最小化 / 閉じるボタン押下時は `WM_SYSCOMMAND` を `SendMessage` で送信。

実装簡略化のため、当初は **Slint 1.x の標準タイトルバーを残し、コマンドボタンをインライン化** する選択肢も検討したが、Mica がタイトルバー下に隠れるため不採用。最終的にクライアント領域に自前タイトルバーを置く。

---

## ページ別詳細

### 1. Apps ページ (`pages/apps_page.slint`)

**左ペイン (320px):** アプリ一覧 ListView
- 各行: アイコン (exe から 24x24 で抽出、失敗時は頭文字タイル) + 名前 (Body Strong) + ラベル (Caption, secondary 色)
- 下部: `+ アプリを追加` 行
- 選択中の行は `nav-selected` 背景

**右ペイン (`Card`):** 編集フォーム
- 名前 (TextField) — 検証: 英数字と `_` のみ、ユニーク、エラー時 `info_badge(critical)` を右に
- 実行ファイル (TextField + `[参照]` ボタン → ネイティブ ファイル ダイアログ `IFileOpenDialog`)
- 引数 (ListView + `[+ 追加]`/`[− 削除]`) — 各行 TextField、`{target}` プレースホルダ ヒント
- 表示ラベル (TextField, 任意)

**コマンド (ページ ヘッダー右):**
- `+ 追加` (新規行を追加し選択)
- `複製` (選択中のアプリを複製)
- `削除` (選択中を削除、未参照チェック → あれば警告ダイアログ)

### 2. Extensions ページ (`pages/extensions_page.slint`)

**左ペイン (320px):** 拡張子一覧
- `.html`, `.md`, `.svg`, ... (アルファベット順)
- 下部: `+ 拡張子を追加` 行 (TextField インライン)
- 選択中の行はハイライト

**右ペイン (`Card`):**
- **デフォルト:** `ComboBox` (登録済み Apps から選択、空欄 = ピッカー使用)
- **候補 (candidates):** チップ入力
  - 既存のチップ: `[vivaldi ×]` `[brave ×]`
  - TextField でアプリ名入力 → Enter で確定 → chip 追加
  - 空文字列は追加不可、重複は警告
- **ルール:** `RuleRow` リスト
  - 各行レイアウト (左から右へ):
    - 条件 AND ビルダー (`ConditionChip` を並べる):
      - `glob`: TextField (glob パターン、`*.md` 等)
      - `modifier`: ComboBox (None / Shift / Ctrl / Alt)
      - `[+ 条件]` ボタンで行内追加、未設定の条件は薄字で表示
    - セパレータ (`→`)
    - 動作: `ComboBox` (アプリ選択) または `ToggleSwitch` (pick=true、選択時アプリはグレーアウト)
    - 操作: `▲` `▼` (並び替え) `✕` (削除)
- フッタ: `+ ルールの追加`

**コマンド:**
- `+ 追加` (拡張子追加)
- `削除` (確認ダイアログ → rules / candidates 込み削除)

### 3. Protocols ページ (`pages/protocols_page.slint`)

Extensions と同構成。条件チップに差異:
- `glob` を **非表示**
- `host` / `url` を **表示** (TextField)
- `modifier` は Extensions と同じ

### 4. Management ページ (`pages/management_page.slint`)

**セクション 1: 設定** (Card)
- 設定ファイル パス (TextField, 読み取り専用)
- picker_timeout_ms (NumberBox, 1000〜60000, ステップ 500)
- テーマ モード (ComboBox: System / Light / Dark)
- → 変更すると即時 `Theme.mode` に反映、dirty=true

**セクション 2: レジストリ操作** (Card)
- `適用` / `解除` / `診断` のボタン行
- 結果テキストエリア (スクロール可、`Cascadia Mono`, 成功=success 色、エラー行=critical 色、InfoBadge で件数表示)

**セクション 3: バックアップ** (Card)
- `バックアップ` / `復元` (`[ファイル選択]` ダイアログ)
- 最後のバックアップ ファイル名と日時

**セクション 4: 危険ゾーン** (Card, 注意書き付き)
- `関連付けを全て解除` (赤系ボタン、確認ダイアログ必須)

dirty でない限り `[保存]` は enabled。`[保存]` はウィンドウ全体共有 (タイトルバー側)。

---

## データフロー

```
Config (Rc<RefCell<Config>>)
  ├─ apps: BTreeMap<String, AppDef>
  ├─ ext: BTreeMap<String, RouteTable>
  ├─ protocol: BTreeMap<String, RouteTable>
  └─ settings: Settings { picker_timeout_ms, theme_mode (新規) }

                    ↓ 起動時 (mod.rs)

Vec<AppEntry>          → apps_page.apps_model (Rc<VecModel>)
Vec<ExtEntry>          → ext_page.exts_model (Rc<VecModel<ExtEntry>>)
                          各 ExtEntry が candidates: Rc<VecModel<String>>,
                          rules: Rc<VecModel<RuleEntry>>
Vec<ProtoEntry>        → protocols_page.protocols_model (同構造)

                    ↓ UI 操作 (callback)

apps.rs / ext.rs / protocol.rs が:
  1. Config (Rc<RefCell>) を更新
  2. 対応する VecModel を操作 (push/remove/set_row_data)
  3. dirty=true を通知

                    ↓ [保存] 押下

mod.rs:
  1. validation::validate(&Config) → エラーあればフィールド単位 InfoBadge で表示、保存中止
  2. toml::to_string_pretty(&Config) → fs::write(config_path)
  3. dirty=false
```

### Slint 構造体 (settings.slint 冒頭)

```slint
export struct AppEntry {
    name: string,
    cmd: string,
    args: [string],
    label: string,
    icon: image,           // 24x24 アイコン (キャッシュ)
    error: string,         // 検証エラー メッセージ
}

// 表示と編集を分離: 表示は不変 [T] プロパティ、編集はコールバック経由で VecModel を直接操作
export struct ExtEntry {
    key: string,           // ".html"
    default: string,
    default-error: string,
    candidates: [string],  // 表示用 (VecModel と同期)
    candidates-error: string,
    rules: [RuleEntry],    // 表示用 (VecModel と同期)
}

export struct ProtoEntry {
    scheme: string,        // "https"
    default: string,
    default-error: string,
    candidates: [string],
    candidates-error: string,
    rules: [RuleEntry],
}

export struct RuleEntry {
    glob: string,
    host: string,
    url: string,
    modifier: string,      // "" | "shift" | "ctrl" | "alt"
    app: string,           // "" = pick
    pick: bool,
    error: string,
}
```

入れ子リスト (candidates / rules) は **表示用 [T] プロパティ** と **編集用 VecModel** を分離:

```rust
// ページ コンポーネントのプロパティとして VecModel を保持 (in プロパティ経由)
pub struct PagesState {
    pub ext_model: Rc<VecModel<ExtEntry>>,
    pub ext_candidates_models: HashMap<String, Rc<VecModel<String>>>,
    pub ext_rules_models: HashMap<String, Rc<VecModel<RuleEntry>>>,
    pub proto_model: Rc<VecModel<ProtoEntry>>,
    pub proto_candidates_models: HashMap<String, Rc<VecModel<String>>>,
    pub proto_rules_models: HashMap<String, Rc<VecModel<RuleEntry>>>,
}
```

各コールバック (ルール追加 / 候補追加等) は `state.ext_rules_models.get(&key)` で `Rc<VecModel<RuleEntry>>` を取得し、`push` / `remove` / `set_row_data` で直接操作。同時に外側の `VecModel<ExtEntry>` の該当行を `set_row_data(idx, ...)` で更新して `candidates` / `rules` フィールドを最新化。

---

## 検証 (`src/settings/validation.rs`)

| ルール | 検証内容 | エラー表示 |
|--------|----------|-----------|
| apps 名前 | `[a-zA-Z0-9_]+`、ユニーク | name フィールド横 InfoBadge(critical) |
| apps cmd | 空でない、ファイル存在 (ベストエフォート) | cmd フィールド横 |
| apps args | 各要素に `{target}` または static 引数 | args リスト各行 |
| ext / protocol key | 先頭 `.` (ext) または英字+英数 (protocol) | key フィールド横 |
| ext / protocol default | apps に存在 | default ComboBox 横 |
| ext / protocol candidates | 各要素が apps に存在 | chips の下にリスト |
| rule.app | apps に存在 | rule 行 |
| rule.modifier | `""` / `"shift"` / `"ctrl"` / `"alt"` | modifier ComboBox 横 |
| rule 動作 | `app != ""` または `pick == true` のいずれか | rule 行 (pick トグルと app ComboBox 同時無効化は UI で防止) |

検証は `[保存]` 押下時に全件走査。エラー箇所を `InfoBadge` で赤表示し、保存は実行しない。修正後に dirty=false に戻すには保存成功が必須。

---

## Cargo.toml 変更

**なし (新規 crate 追加なし、既存依存の追加もなし)**

要件確認:
- `slint = "1"` (既存) で UI 実装は完結
- `windows = "0.61"` の features は現状の `Win32_Foundation`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_Shell`, `Win32_UI_WindowsAndMessaging`, `Win32_UI_HiDpi`, `Win32_Graphics_Gdi`, `Win32_Graphics_Dwm`, `Win32_System_Com` で Mica (DWMWA_SYSTEMBACKDROP_TYPE / DWMWA_WINDOW_CORNER_PREFERENCE) と SetWindowSubclass に必要十分
- `serde`, `toml`, `winreg`, `dirs` も既存で充足
- `slint-build = "1"` (既存) で `.slint` 分割コンパイル対応

ビルド スクリプト (`build.rs`) は `slint_build::compile` の対象を `src/ui/settings.slint` に変更 (`src/ui.slint` を削除して新規ファイル群をコンパイル)。

---

## エラーハンドリング

| 状況 | 処理 |
|------|------|
| 設定ファイル読込失敗 | 既存通りデフォルト設定で起動、doctor 推奨をステータスバーに InfoBadge(caution) |
| 設定ファイル書出失敗 | InfoBadge(critical) をステータスバーに、トースト表示 |
| apply/unregister 失敗 | Management の結果エリアに詳細、InfoBadge(critical) |
| doctor 警告 | Management の結果エリアに InfoBadge(caution) 行 |
| 検証エラー | 保存中止、フィールド横 InfoBadge(critical)、右ペイン タイトル下に集計バッジ |
| アイコン抽出失敗 | 頭文字タイル (既存 `icon.rs` のフォールバック) |
| アプリ exe 不在 | apps ページの cmd フィールド横に InfoBadge(caution) |

---

## 変更対象外

- `src/config/` (Config 構造体は維持、`theme_mode` 追加のみ)
- `src/engine.rs` (ルール評価ロジック変更なし)
- `src/registry/` (apply / unregister / doctor / backup / restore の API 維持)
- `src/icon.rs` (既存 API 維持)
- `src/error.rs`、`src/logging.rs`、`src/commands.rs`
- ピッカーの UI 刷新 (テーマ トークン共有のみ、ロジック変更なし)
- `config.example.toml` (theme_mode の追加に合わせて 1 行追加のみ)

---

## 検証計画

1. `cargo check` 通過
2. `cargo test` 既存テスト全通過
3. `cargo clippy -- -D warnings` 警告ゼロ
4. `cargo build --release` 成功
5. 手動テスト:
   - 設定画面: アプリ / 拡張子 / プロトコル の CRUD → 保存 → 再ロード → `apply`
   - テーマ切替: System / Light / Dark が即時反映、OS 設定変更で再起動後反映
   - Mica: Win11 22H2+ でメイン ウィンドウに Mica、Win10 でフォールバック (純色)
   - 拡張タイトルバー: ドラッグで移動、最小化 / 最大化 / 閉じる動作
   - 検証エラー: 重複名 / 不在アプリ参照 / 不正 modifier で保存ブロック
   - ルール並び替え: ▲▼ で `config.toml` の順序が更新される
   - 参照整合性: アプリ削除時、参照している ext/protocol で警告
6. バイナリ サイズ: リリース ビルドのサイズ変化を記録
7. スクリーンショット: ライト / ダークそれぞれを `docs/` に保存

---

## 実装順序 (writing-plans で詳細化)

1. テーマ トークン (theme.slint) + Rust 側 theme.rs (DWM, アクセント, OS テーマ)
2. ベース コンポーネント (text_field, accent_button, card, info_badge, nav_item)
3. 拡張タイトルバー + コマンド バー + NavigationView rail (空 Settings ウィンドウ)
4. Apps ページ (master/detail + CRUD)
5. Extensions / Protocols ページ (RuleRow, ConditionChip, AND ビルダー)
6. Management ページ (picker_timeout, theme_mode, registry ops, backup)
7. validation.rs + 保存フロー + dirty 状態管理
8. ステータス バー + 拡張タイトルバーの WM_NCHITTEST フック
9. cargo test / clippy / リリース ビルド + 手動テスト

---

## リスクと対策

| リスク | 影響 | 対策 |
|--------|------|------|
| Slint 1.x の HWND アクセス API が限定的 | Mica 適用 / WM_NCHITTEST フック不可 | 1.x の `window_handle()` 系 API を確認しアクセス、なければ `slint::platform::set_platform` で独自プラットフォーム抽象を提供。Rust 側は `windows-rs` で HWND を取得して DwmSet を直接呼ぶ |
| 拡張タイトルバーのドラッグ実装が複雑 | UX 低下 (最大化 / ドラッグ不可) | **第一選択**: `SetWindowSubclass` + `WM_NCHITTEST` でクライアント領域最上部を `HTCAPTION` 化、最大化 / 最小化 / 閉じるは `WM_SYSCOMMAND` 経由。**フォールバック**: 標準タイトルバーを残し、コマンドバーはクライアント領域内 (タイトルバー直下) にインライン配置 |
| Mica が古い Win11 で表示崩れ | 美観 | `DWMWA_SYSTEMBACKDROP_TYPE` 未対応時は DwmSet がエラー → 握りつぶして純色背景 |
| 入れ子 VecModel のメモリ管理 | 性能/リーク | Rc 循環参照に注意、VecModel はページごとに 1 個、再構築時は明示的に drop |
| Segoe UI Variable 非搭載環境 (Win10) | タイポ | `Segoe UI` フォールバック、font-family 文字列で吸収 |
| アイコン抽出性能 | UX | 起動時にバックグラウンドで先読み、ListView スクロール時のジッタ回避 |

---

## 補足: 既存 specs/plans との関係

- `2026-06-17-settings-screen-design.md`: Slint 移行と設定 GUI の新設 (完了済み)。本スペックはその **後継** として位置付け、内容は破棄する。
- `2026-06-13-refactoring-design.md` / `2026-06-13-refactoring.md`: エンジン分離のリファクタリング (完了済み)。本スペックとは独立。
- 完了済みの Slint 移行 (`ui.slint`, `settings/mod.rs` 等) は本スペックで全面書き換え。コミット ログ上は "refactor: redo settings UI with Fluent 2 fidelity" 1 コミット (もしくは段階的に複数コミット)。
