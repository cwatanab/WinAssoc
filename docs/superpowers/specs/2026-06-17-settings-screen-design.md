# winassoc 設定画面 設計

**作成日:** 2026-06-17
**スコープ:** settings GUI 新設 + picker を egui から Slint へ移行

---

## 背景

winassoc.exe は現在 CLI 専用で、設定は TOML の手動編集が必要。またピッカー GUI は egui/eframe で実装されている。Slint + Fluent Design による設定 GUI を新設し、ピッカーも Slint に一本化する。

---

## 変更対象概要

### 追加

| パス | 内容 |
|------|------|
| `src/settings/mod.rs` | 設定画面ロジック、Slintバインディング |
| `src/settings/settings.slint` | 設定画面UI定義 (サイドバー+4ページ) |
| `src/picker/picker.slint` | ピッカーUI定義 (egui版と同等機能) |

### 変更

| パス | 内容 |
|------|------|
| `Cargo.toml` | eframe/egui/raw-window-handle 削除、slint/slint-build 追加 |
| `src/main.rs` | 引数なし→settings::run()、引数あり→既存CLI |
| `src/picker/mod.rs` | egui→Slint に書き換え、show() のインターフェース維持 |
| `src/picker/window.rs` | 削除 (Slint がウィンドウ管理) |
| `src/commands.rs` | picker 呼び出し部分の調整 |

### 削除

| パス | 内容 |
|------|------|
| `src/picker/window.rs` | Slint がウィンドウ管理を担当 |

---

## アーキテクチャ

```
winassoc.exe (引数なし) → settings::run() → settings.slint
winassoc.exe (引数あり) → 既存 CLI 処理
winassoc-open.exe       → commands::open() → (Pick時) picker::show() → picker.slint
```

### 依存関係

```
settings/mod.rs ──→ config::Config (読込・保存・バリデーション)
                ──→ registry::apply / unregister / doctor / backup
                ──→ settings.slint

picker/mod.rs   ──→ engine, config, platform, icon
                ──→ picker.slint
```

---

## UI レイアウト

### 設定画面 (settings.slint) — サイドバー方式

```
┌────────────┬──────────────────────────────────────────────────┐
│  アプリ     │                                                  │
│  拡張子     │   メインコンテンツエリア                           │
│  プロトコル  │                                                  │
│  管理        │                                                  │
│            │                                                  │
│            │                                                  │
├────────────┴──────────────────────────────────────────────────┤
│                       ステータスバー                           │
│    設定ファイルパス  |  保存状態                                 │
└───────────────────────────────────────────────────────────────┘
```

### 各ページ内容

**アプリ ページ:**
- 左側: アプリ一覧リスト (名前)
- 右側: 選択中アプリの編集フォーム
  - 名前 (name)
  - 実行ファイルパス (cmd) + 参照ボタン
  - 引数 (args)
  - 表示ラベル (label)
  - アイコンパス (icon)
- 追加 / 削除 / 複製 ボタン

**拡張子 ページ:**
- 左側: 拡張子一覧 (.md, .svg, ...) + 追加/削除
- 右側:
  - デフォルトアプリ選択 (default)
  - 候補一覧 (candidates) 編集
  - ルール一覧 (rules) テーブル編集
    - 各ルール: glob, host, url, modifier, app, pick

**プロトコル ページ:**
- 拡張子ページと同構成 (https, ftp, ...)

**管理 ページ:**
- 設定ファイルパス表示
- [保存] ボタン
- [適用 (apply)] ボタン + 結果表示
- [解除 (unregister)] ボタン + 結果表示
- [診断 (doctor)] ボタン + 結果テキストエリア
- [バックアップ] ボタン
- [復元 (restore)] ボタン (バックアップファイル選択)

### ピッカー (picker.slint)

- egui 版と同等の機能:
  - アイコングリッド表示
  - キーボード操作 (←→ Enter Esc 数字キー)
  - ホバー/選択アニメーション
  - Mica/Acrylic 背景 (Slint Fluent テーマ)
  - カーソル位置ポップアップ

---

## データフロー

### 設定画面のデータフロー

```
Config (TOML) ──読込──→ settings/mod.rs ──表示──→ settings.slint
                                      │
                         ユーザー編集 (Slint callback)
                                      │
                                      ↓
                          Config (メモリ上更新)
                                      │
                          [保存] ボタン
                                      │
                                      ↓
                          Config::validate()
                                      │
                          Config (TOML 書出)
```

- Slint の `global` / `property` / `callback` で双方向バインディング
- リストは `Vec<Model>` で Slint に伝達
- 編集は即時メモリ反映、ファイル書出は明示的保存操作時

### 管理操作フロー

```
[適用] → registry::apply() → 結果表示
[診断] → registry::doctor() → 結果表示
[解除] → registry::unregister() → 結果表示
[bkup] → registry::backup::backup() → 結果表示
[復元] → registry::backup::restore() → 結果表示
```

### ピッカーフロー

```
commands::open()
  → engine::evaluate() → Decision::Pick
  → picker::show(candidates, target)
  → picker.slint 表示
  → ユーザー選択 → アプリ起動 (std::process::Command)
```

---

## エラーハンドリング

| 状況 | 処理 |
|------|------|
| 設定ファイル読込失敗 | エラーダイアログ、デフォルト設定で起動 |
| 設定ファイル書出失敗 | ステータスバーにエラー表示 |
| apply/unregister 失敗 | 管理ページの結果エリアにエラー詳細 |
| バリデーションエラー | 保存前に検証、問題箇所をハイライト |
| アイコン抽出失敗 | 頭文字ジェネリックタイル (既存 icon.rs に委譲) |
| アプリ exe 不在 | doctor 結果に警告、設定編集時にもインジケータ表示 |

---

## Cargo.toml 変更

```toml
# 削除
# eframe = "0.32"
# egui = "0.32"
# raw-window-handle = "0.6"

# 追加
[dependencies]
slint = "1"

[build-dependencies]
slint-build = "1"
```

---

## 変更対象外

- `src/config/` — 設定構造体・バリデーションは変更なし
- `src/engine.rs` — 変更なし
- `src/registry/` — 変更なし
- `src/icon.rs` — 変更なし
- `src/logging.rs` — 変更なし
- `src/error.rs` — 変更なし
- `src/platform.rs` — 既存関数維持、Slint テーマ判別に追加使用の可能性あり
- `build.rs` — 変更なし (アイコン埋め込みは継続)
- `config.example.toml` — 変更なし

---

## 検証計画

1. `cargo check` — コンパイル確認
2. `cargo test` — 既存テスト全通過
3. `cargo clippy` — lint 確認
4. `cargo build --release` — リリースビルド確認
5. 手動テスト:
   - 設定画面: アプリ/拡張子/プロトコルの CRUD 操作 → 保存 → 再読込
   - 管理操作: apply / unregister / doctor / backup / restore
   - ピッカー: 修飾キー押下 → ピッカー表示 → 選択 → 正しいアプリ起動
   - バイナリサイズ: リリースビルドの winassoc.exe / winassoc-open.exe サイズ比較
