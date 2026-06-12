use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub apps: BTreeMap<String, AppDef>,
    #[serde(default)]
    pub ext: BTreeMap<String, RouteTable>,
    #[serde(default)]
    pub protocol: BTreeMap<String, RouteTable>,
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
    Ok(base.join("winassoc").join("config.toml"))
}

/// 設定ファイルの探索順 (--config 未指定時):
/// 1. exe と同じディレクトリの config.toml (ポータブル運用)
/// 2. %APPDATA%\winassoc\config.toml
pub fn resolve_config_path() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let portable = exe.with_file_name("config.toml");
        if portable.exists() {
            return Ok(portable);
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
        let tables = self
            .ext
            .iter()
            .map(|(k, v)| (format!("ext.{k}"), v))
            .chain(self.protocol.iter().map(|(k, v)| (format!("protocol.{k}"), v)));

        for (name, table) in tables {
            if let Some(app) = &table.default {
                if !self.apps.contains_key(app) {
                    bail!("[{name}] default の \"{app}\" が [apps] に定義されていません");
                }
            }
            for (i, rule) in table.rules.iter().enumerate() {
                let at = format!("[{name}] rules[{i}]");
                match (&rule.app, rule.pick) {
                    (Some(app), false) => {
                        if !self.apps.contains_key(app) {
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
