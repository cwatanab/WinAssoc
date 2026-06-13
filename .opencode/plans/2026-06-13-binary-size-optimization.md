# winassoc バイナリサイズ削減 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `winassoc.exe` / `winassoc-open.exe` を 3MB 未満にする（現在 6.0〜6.7MB）

**Architecture:** Phase 1 で Cargo.toml のサイズ最適化設定を適用し、Phase 2 で外部クレートを自前実装・軽量クレートに置き換えて依存を削減する。UI フレームワークは変更せず、機能を維持したままサイズを削減する。

**Tech Stack:** Rust 2021, cargo

---

## 変更対象ファイル構成

```
Cargo.toml              # プロファイル設定、依存関係変更
src/engine.rs           # Target::parse を url クレートから自前実装へ
src/registry/backup.rs  # chrono を自前 RFC3339 実装へ
src/main.rs             # clap derive を builder API へ
src/error.rs            # anyhow の代替（新設、オプション）
```

---

### Task 1: Phase 1 — コンパイル設定の最適化

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: ベースライン計測**

```bash
cargo build --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: 現在のサイズを記録（winassoc.exe ~6.7MB, winassoc-open.exe ~6.0MB）

- [ ] **Step 2: `panic = "abort"` を追加**

`Cargo.toml` の `[profile.release]` を以下に変更:

```toml
[profile.release]
opt-level = 3
lto = true
strip = true
panic = "abort"
```

```bash
cargo build --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: サイズ削減（数百 KB 程度）

- [ ] **Step 3: `codegen-units = 1` を追加**

```toml
[profile.release]
opt-level = 3
lto = true
strip = true
panic = "abort"
codegen-units = 1
```

```bash
cargo build --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: さらに数%削減

- [ ] **Step 4: `opt-level = "z"` を試す**

```toml
[profile.release]
opt-level = "z"
lto = true
strip = true
panic = "abort"
codegen-units = 1
```

```bash
cargo build --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
cargo test --release
```

Expected: サイズ大幅削減。ただしテスト時間/起動レイテンシを計測し、許容範囲なら採用。

- [ ] **Step 5: 最適な設定を確定してコミット**

`opt-level = "z"` がレイテンシに悪影響する場合は `opt-level = 3` に戻す。

```bash
git add Cargo.toml
git commit -m "build: optimize release profile for binary size"
```

---

### Task 2: `url` クレートを自前軽量パーサーに置き換え

**Files:**
- Modify: `src/engine.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: 現在の `Target::parse` テストを確認**

```bash
cargo test --release engine::tests
```

Expected: 全テスト PASS

- [ ] **Step 2: 自前 URL パーサー関数を追加**

`src/engine.rs` の `use` 文から `url` を削除し、以下を追加:

```rust
fn parse_url(input: &str) -> Option<(String, String, Option<String>)> {
    // scheme://host... の形式を手軽に解析
    // scheme = [a-zA-Z][a-zA-Z0-9+.-]*
    let scheme_end = input.find("://")?;
    let scheme = &input[..scheme_end];
    if scheme.is_empty() || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    let rest = &input[scheme_end + 3..];
    let host = rest
        .split(&['/', '?', '#'])
        .next()
        .filter(|h| !h.is_empty())
        .map(str::to_string);
    Some((scheme.to_ascii_lowercase(), input.to_string(), host))
}
```

- [ ] **Step 3: `Target::parse` を自前パーサーに置き換え**

```rust
impl Target {
    pub fn parse(input: &str) -> Target {
        let looks_like_drive = input.len() >= 2
            && input.as_bytes()[1] == b':'
            && input.as_bytes()[0].is_ascii_alphabetic()
            && matches!(input.as_bytes().get(2), None | Some(b'\\') | Some(b'/'));
        if !looks_like_drive {
            if let Some((scheme, url, host)) = parse_url(input) {
                return Target::Url { url, scheme, host };
            }
        }
        let ext = std::path::Path::new(input)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        Target::File { path: input.to_string(), ext }
    }
}
```

- [ ] **Step 4: `Cargo.toml` から `url` を削除**

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
globset = "0.4"
dirs = "5"
chrono = "0.4"
winreg = "0.55"
eframe = "0.32"
raw-window-handle = "0.6"
```

`url = "2"` の行を削除。

- [ ] **Step 5: ビルドとテスト**

```bash
cargo check
cargo test --release engine::tests
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: テスト全 PASS、サイズ削減

- [ ] **Step 6: コミット**

```bash
git add src/engine.rs Cargo.toml Cargo.lock
git commit -m "refactor: replace url crate with lightweight parser"
```

---

### Task 3: `chrono` を自前 RFC3339 実装に置き換え

**Files:**
- Modify: `src/registry/backup.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: 現在の chrono 使用箇所を確認**

`src/registry/backup.rs` 内の `chrono::Local::now()` 呼び出しを確認。

- [ ] **Step 2: 自前 RFC3339 タイムスタンプ関数を追加**

`src/registry/backup.rs` の先頭に追加:

```rust
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    const MONTHS: [&str; 12] = [
        "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
    ];
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    // 1970-01-01 からの日数計算（簡易版、閏年対応）
    let mut days = secs / 86400;
    let mut year = 1970u64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let ydays = if leap { 366 } else { 365 };
        if days < ydays {
            break;
        }
        days -= ydays;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 0;
    while month < 12 && days >= month_days[month] as u64 {
        days -= month_days[month] as u64;
        month += 1;
    }
    let day = days + 1;
    let rem = secs % 86400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        year,
        month + 1,
        day,
        hour,
        min,
        sec,
    )
}
```

- [ ] **Step 3: chrono 呼び出しを置き換え**

`backup()` 内:

```rust
let mut data = Backup {
    created: now_rfc3339(),
    ..Default::default()
};
```

から `chrono::Local::now().to_rfc3339()` を `now_rfc3339()` に置き換え。

`backup-{stamp}` ファイル名のタイムスタンプも自前で生成:

```rust
fn backup_timestamp() -> String {
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    // 簡易 YYYYmmdd-HHMMSS
    let (year, month, day, hour, min, sec) = date_time_from_secs(secs);
    format!("{year:04}{month:02}{day:02}-{hour:02}{min:02}{sec:02}")
}
```

- [ ] **Step 4: `Cargo.toml` から `chrono` を削除**

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
globset = "0.4"
dirs = "5"
winreg = "0.55"
eframe = "0.32"
raw-window-handle = "0.6"
```

- [ ] **Step 5: ビルドとテスト**

```bash
cargo check
cargo test --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: テスト全 PASS、サイズ削減

- [ ] **Step 6: コミット**

```bash
git add src/registry/backup.rs Cargo.toml Cargo.lock
git commit -m "refactor: replace chrono with lightweight timestamp implementation"
```

---

### Task 4: `clap` derive を builder API に置き換え

**Files:**
- Modify: `src/main.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: 現在の clap derive 実装を確認**

`src/main.rs` の `#[derive(Parser)]` / `#[derive(Subcommand)]` を確認。

- [ ] **Step 2: builder API で CLI を再定義**

`src/main.rs` を以下のように書き換え:

```rust
use std::path::PathBuf;

use anyhow::Result;
use clap::{Arg, ArgAction, Command};

use winassoc::config::{self, Config};
use winassoc::{commands, logging, registry};

#[derive(Debug)]
struct Cli {
    config: Option<PathBuf>,
    command: CommandKind,
}

#[derive(Debug)]
enum CommandKind {
    Open { target: String },
    Test { target: String, modifier: Vec<String> },
    List,
    Apply,
    Unregister,
    Doctor,
    Log { tail: usize },
    Backup,
    Restore { file: Option<PathBuf> },
}

fn build_cli() -> Command {
    Command::new("winassoc")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Windows のファイル関連付け/URL をルールベースでルーティングするシム")
        .arg(
            Arg::new("config")
                .long("config")
                .global(true)
                .value_name("PATH")
                .help("設定ファイル (既定: winassoc.toml または config.toml)"),
        )
        .subcommand(Command::new("open").about("シム本体: ルールを評価して実アプリを起動する").arg(Arg::new("target").required(true)))
        .subcommand(
            Command::new("test")
                .about("dry-run: どのルールに一致し何が起動されるかを表示する")
                .arg(Arg::new("target").required(true))
                .arg(
                    Arg::new("modifier")
                        .long("modifier")
                        .value_delimiter(',')
                        .value_name("MODIFIERS")
                        .help("修飾キー押下を擬似指定 (shift,ctrl,alt をカンマ区切り)"),
                ),
        )
        .subcommand(Command::new("list").about("設定済みルートの一覧"))
        .subcommand(Command::new("apply").about("設定中の拡張子/プロトコルを HKCU に登録する"))
        .subcommand(Command::new("unregister").about("登録解除"))
        .subcommand(Command::new("doctor").about("設定とレジストリの乖離を診断する"))
        .subcommand(
            Command::new("log")
                .about("起動ログの表示")
                .arg(
                    Arg::new("tail")
                        .long("tail")
                        .default_value("20")
                        .value_parser(clap::value_parser!(usize)),
                ),
        )
        .subcommand(Command::new("backup").about("現在の関連付け状態をバックアップする"))
        .subcommand(
            Command::new("restore")
                .about("バックアップから関連付けを復元する")
                .arg(Arg::new("file").value_name("FILE")),
        )
}

fn parse_cli() -> Result<Cli> {
    let matches = build_cli().get_matches();
    let config = matches.get_one::<PathBuf>("config").cloned();
    let command = match matches.subcommand() {
        Some(("open", m)) => CommandKind::Open {
            target: m.get_one::<String>("target").cloned().unwrap_or_default(),
        },
        Some(("test", m)) => CommandKind::Test {
            target: m.get_one::<String>("target").cloned().unwrap_or_default(),
            modifier: m
                .get_many::<String>("modifier")
                .map(|v| v.cloned().collect())
                .unwrap_or_default(),
        },
        Some(("list", _)) => CommandKind::List,
        Some(("apply", _)) => CommandKind::Apply,
        Some(("unregister", _)) => CommandKind::Unregister,
        Some(("doctor", _)) => CommandKind::Doctor,
        Some(("log", m)) => CommandKind::Log {
            tail: *m.get_one::<usize>("tail").unwrap_or(&20),
        },
        Some(("backup", _)) => CommandKind::Backup,
        Some(("restore", m)) => CommandKind::Restore {
            file: m.get_one::<PathBuf>("file").cloned(),
        },
        _ => anyhow::bail!("サブコマンドを指定してください"),
    };
    Ok(Cli { config, command })
}

fn main() -> Result<()> {
    let cli = parse_cli()?;
    let config_path = match cli.config {
        Some(path) => path,
        None => config::resolve_config_path()?,
    };

    match cli.command {
        CommandKind::Log { tail } => logging::tail(tail),
        CommandKind::Restore { file } => registry::restore(file.as_deref()),
        CommandKind::Open { target } => {
            let config = Config::load(&config_path)?;
            commands::open(&config, &target)
        }
        CommandKind::Test { target, modifier } => {
            let config = Config::load(&config_path)?;
            commands::test(&config, &target, &modifier)
        }
        CommandKind::List => {
            let config = Config::load(&config_path)?;
            commands::list(&config)
        }
        CommandKind::Apply => {
            let config = Config::load(&config_path)?;
            registry::apply(&config)
        }
        CommandKind::Unregister => {
            let config = Config::load(&config_path)?;
            registry::unregister(&config)
        }
        CommandKind::Doctor => {
            let config = Config::load(&config_path)?;
            registry::doctor(&config, &config_path)
        }
        CommandKind::Backup => {
            let config = Config::load(&config_path)?;
            let path = registry::backup(&config)?;
            println!("バックアップを保存しました: {}", path.display());
            Ok(())
        }
    }
}
```

- [ ] **Step 3: `Cargo.toml` から clap の derive feature を削除**

```toml
[dependencies]
anyhow = "1"
clap = { version = "4" }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
globset = "0.4"
dirs = "5"
winreg = "0.55"
eframe = "0.32"
raw-window-handle = "0.6"
```

- [ ] **Step 4: ビルドとテスト**

```bash
cargo check
cargo test --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: テスト全 PASS、サイズ削減

- [ ] **Step 5: コミット**

```bash
git add src/main.rs Cargo.toml Cargo.lock
git commit -m "refactor: replace clap derive with builder API"
```

---

### Task 5: 最終検証とサイズ計測

- [ ] **Step 1: 全テスト実行**

```bash
cargo test --release
```

Expected: 全テスト PASS

- [ ] **Step 2: リリースビルドサイズ計測**

```bash
cargo build --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: 3MB 未満を目指す。達成できない場合は Task 6（anyhow 削減）へ。

- [ ] **Step 3: 簡易動作確認**

```bash
./target/release/winassoc.exe --help
./target/release/winassoc.exe list --config config.example.toml
```

Expected: ヘルプと list コマンドが正常動作

- [ ] **Step 4: 最終コミット（必要に応じて）**

```bash
git add -A
git commit -m "build: final size optimization verification" || echo "No changes to commit"
```

---

### Task 6（フォールバック）: `anyhow` を自前エラー型に置き換え

**Files:**
- Create: `src/error.rs`
- Modify: 全 `src/*.rs` / `src/**/*.rs`

> このタスクは Task 5 で 3MB 未満に達成できなかった場合のみ実施。

- [ ] **Step 1: `src/error.rs` を作成**

```rust
use std::fmt;

#[derive(Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(e.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::new(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::new(format!($($arg)*)))
    };
}
```

- [ ] **Step 2: 既存の `anyhow::{bail, Context, Result}` を自前型に置き換え**

全ファイルを走査し、`anyhow` の使用を `crate::error` に置き換え。
`.with_context()` は適切なエラーメッセージに置き換える。

- [ ] **Step 3: `Cargo.toml` から `anyhow` を削除**

```toml
[dependencies]
clap = { version = "4" }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
globset = "0.4"
dirs = "5"
winreg = "0.55"
eframe = "0.32"
raw-window-handle = "0.6"
```

- [ ] **Step 4: ビルドとテスト**

```bash
cargo test --release
ls -la target/release/winassoc.exe target/release/winassoc-open.exe
```

Expected: テスト全 PASS、さらなるサイズ削減

- [ ] **Step 5: コミット**

```bash
git add -A
git commit -m "refactor: replace anyhow with lightweight error type"
```

---

## Self-Review

### Spec coverage
- Phase 1 コンパイル設定最適化 → Task 1 ✓
- url クレート置き換え → Task 2 ✓
- chrono 置き換え → Task 3 ✓
- clap derive → builder → Task 4 ✓
- anyhow フォールバック → Task 6 ✓
- 検証 → Task 5 ✓

### Placeholder scan
- 各ステップに具体的なコードブロックを含む
- TODO/TBD なし

### Type consistency
- `Target::parse` の戻り値は既存と同じ `Target` ✓
- `parse_url` は `(scheme, url, host)` のタプルを返す ✓
- `now_rfc3339` は `String` を返す ✓
- `build_cli` / `parse_cli` は既存の `Cli`/`Command` 相当 ✓
