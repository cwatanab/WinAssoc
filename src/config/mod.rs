use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

mod validate;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub apps: BTreeMap<String, AppDef>,
    #[serde(default)]
    pub ext: BTreeMap<String, RouteTable>,
    #[serde(default)]
    pub protocol: BTreeMap<String, RouteTable>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// 未フォーカス起動時の自動終了タイムアウト時間 (ミリ秒)
    #[serde(default = "default_picker_timeout_ms")]
    pub picker_timeout_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            picker_timeout_ms: 5000,
        }
    }
}

fn default_picker_timeout_ms() -> u64 {
    5000
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppDef {
    pub cmd: String,
    /// `{target}` がファイルパス/URL に展開される。空なら `[{target}]` 扱い
    #[serde(default)]
    pub args: Vec<String>,
    /// ピッカー等に表示する補足ラベル (例: "Profile 1")
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteTable {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub candidates: Option<Vec<String>>,
}

/// 条件キーは複数指定で AND。アクションは `app` か `pick` のどちらか一方
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub glob: Option<String>,
    pub host: Option<String>,
    pub url: Option<String>,
    pub modifier: Option<String>,
    pub app: Option<String>,
    #[serde(default)]
    pub pick: bool,
}

pub fn default_config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("%APPDATA% を特定できません")?;
    let p1 = base.join("winassoc").join("winassoc.toml");
    if p1.exists() {
        return Ok(p1);
    }
    let p2 = base.join("winassoc").join("config.toml");
    if p2.exists() {
        return Ok(p2);
    }
    // どちらも存在しない場合のデフォルトは winassoc.toml とする
    Ok(p1)
}

/// 設定ファイルの探索順 (--config 未指定時):
/// 1. exe と同じディレクトリの winassoc.toml (ポータブル運用)
/// 2. exe と同じディレクトリの config.toml (ポータブル運用)
/// 3. %APPDATA%\winassoc\winassoc.toml
/// 4. %APPDATA%\winassoc\config.toml
pub fn resolve_config_path() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let portable1 = exe.with_file_name("winassoc.toml");
        if portable1.exists() {
            return Ok(portable1);
        }
        let portable2 = exe.with_file_name("config.toml");
        if portable2.exists() {
            return Ok(portable2);
        }
    }
    default_config_path()
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("設定ファイルを読めません: {}", path.display()))?;
        let config: Config = toml::from_str(&text)
            .with_context(|| format!("設定ファイルの形式が不正です: {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        validate::validate_config(self)
    }
}

/// `%VAR%` 形式の環境変数を展開する (未定義はそのまま残す)
pub fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        match rest[start + 1..].find('%') {
            Some(len) => {
                let name = &rest[start + 1..start + 1 + len];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => out.push_str(&rest[start..start + len + 2]),
                }
                rest = &rest[start + len + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}
