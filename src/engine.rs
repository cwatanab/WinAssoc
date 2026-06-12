use anyhow::{bail, Result};
use globset::GlobBuilder;

use crate::config::{expand_env, AppDef, Config, Rule, RouteTable};

#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<Modifiers> {
        let mut m = Modifiers::default();
        for name in names {
            match name {
                "shift" => m.shift = true,
                "ctrl" => m.ctrl = true,
                "alt" => m.alt = true,
                other => bail!("不明な修飾キー: {other} (shift / ctrl / alt)"),
            }
        }
        Ok(m)
    }

    fn pressed(&self, name: &str) -> bool {
        match name {
            "shift" => self.shift,
            "ctrl" => self.ctrl,
            "alt" => self.alt,
            _ => false,
        }
    }
}

/// シムに渡された起動対象 (ファイルパス or URL)
#[derive(Debug)]
pub enum Target {
    File { path: String, ext: Option<String> },
    Url { url: String, scheme: String, host: Option<String> },
}

impl Target {
    pub fn parse(input: &str) -> Target {
        // "C:\..." / "C:/..." はドライブレターであって URL スキームではない
        let looks_like_drive = input.len() >= 2
            && input.as_bytes()[1] == b':'
            && input.as_bytes()[0].is_ascii_alphabetic()
            && matches!(input.as_bytes().get(2), None | Some(b'\\') | Some(b'/'));
        if !looks_like_drive {
            if let Ok(parsed) = url::Url::parse(input) {
                return Target::Url {
                    url: input.to_string(),
                    scheme: parsed.scheme().to_ascii_lowercase(),
                    host: parsed.host_str().map(str::to_string),
                };
            }
        }
        let ext = std::path::Path::new(input)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase());
        Target::File { path: input.to_string(), ext }
    }

    pub fn raw(&self) -> &str {
        match self {
            Target::File { path, .. } => path,
            Target::Url { url, .. } => url,
        }
    }

    /// この対象に対応するルートテーブル (ext.* / protocol.*) を引く
    pub fn route_table<'c>(&self, config: &'c Config) -> Option<(String, &'c RouteTable)> {
        match self {
            Target::File { ext, .. } => {
                let ext = ext.as_ref()?;
                config.ext.get(ext).map(|t| (format!("ext.{ext}"), t))
            }
            Target::Url { scheme, .. } => config
                .protocol
                .get(scheme)
                .map(|t| (format!("protocol.{scheme}"), t)),
        }
    }
}

#[derive(Debug)]
pub enum Decision {
    /// ルール一致 (rule_index = Some) またはフォールバック default (None)
    Launch { app: String, rule_index: Option<usize> },
    /// pick = true ルールに一致。候補はテーブル内に登場するアプリ
    Pick { candidates: Vec<String>, rule_index: usize },
    NoRoute { reason: String },
}

pub fn evaluate(config: &Config, target: &Target, mods: &Modifiers) -> Decision {
    let Some((table_name, table)) = target.route_table(config) else {
        return Decision::NoRoute {
            reason: match target {
                Target::File { ext: Some(ext), .. } => format!("[ext.{ext}] が未定義です"),
                Target::File { ext: None, .. } => "拡張子のないファイルです".to_string(),
                Target::Url { scheme, .. } => format!("[protocol.{scheme}] が未定義です"),
            },
        };
    };

    for (i, rule) in table.rules.iter().enumerate() {
        if rule_matches(rule, target, mods) {
            return match &rule.app {
                Some(app) => Decision::Launch { app: app.clone(), rule_index: Some(i) },
                None => Decision::Pick { candidates: pick_candidates(table), rule_index: i },
            };
        }
    }

    match &table.default {
        Some(app) => Decision::Launch { app: app.clone(), rule_index: None },
        None => Decision::NoRoute {
            reason: format!("[{table_name}] にルール一致がなく default も未指定です"),
        },
    }
}

fn rule_matches(rule: &Rule, target: &Target, mods: &Modifiers) -> bool {
    if let Some(pattern) = &rule.glob {
        let Target::File { path, .. } = target else { return false };
        if !glob_match(pattern, &path.replace('\\', "/")) {
            return false;
        }
    }
    if let Some(pattern) = &rule.host {
        let Target::Url { host: Some(host), .. } = target else { return false };
        if !glob_match(pattern, host) {
            return false;
        }
    }
    if let Some(pattern) = &rule.url {
        let Target::Url { url, .. } = target else { return false };
        if !glob_match(pattern, url) {
            return false;
        }
    }
    if let Some(modifier) = &rule.modifier {
        if !mods.pressed(modifier) {
            return false;
        }
    }
    true
}

/// パス区切りを跨いで `**`/`*` を一様に扱う、大文字小文字無視の glob 一致
fn glob_match(pattern: &str, text: &str) -> bool {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(false)
        .build()
        .map(|g| g.compile_matcher().is_match(text))
        .unwrap_or(false)
}

/// pick 時の候補: テーブル内に登場する順で default + 各ルールの app (重複除去)
fn pick_candidates(table: &RouteTable) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        if !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    };
    if let Some(app) = &table.default {
        push(app);
    }
    for rule in &table.rules {
        if let Some(app) = &rule.app {
            push(app);
        }
    }
    out
}

/// 実際に spawn するコマンドラインを組み立てる ({target} 展開 + %VAR% 展開)
pub fn build_command_line(app: &AppDef, target: &str) -> (String, Vec<String>) {
    let program = expand_env(&app.cmd);
    let args = if app.args.is_empty() {
        vec![target.to_string()]
    } else {
        app.args
            .iter()
            .map(|a| expand_env(a).replace("{target}", target))
            .collect()
    };
    (program, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        let text = r#"
            [apps.vscode]
            cmd = 'C:\Tools\Code.exe'
            args = ['{target}']

            [apps.obsidian]
            cmd = '%LOCALAPPDATA%\Obsidian\Obsidian.exe'

            [apps.chrome-work]
            cmd = 'chrome.exe'
            args = ['--profile-directory=Profile 1', '{target}']

            [apps.firefox]
            cmd = 'firefox.exe'

            [ext.md]
            default = "obsidian"
            rules = [
              { glob = 'D:/Develop/**', app = "vscode" },
              { modifier = "shift", app = "vscode" },
            ]

            [ext.html]
            rules = [
              { modifier = "ctrl", pick = true },
            ]

            [protocol.https]
            default = "firefox"
            rules = [
              { host = '*.corp.example.com', app = "chrome-work" },
              { url = 'https://github.com/myorg/**', app = "chrome-work" },
            ]
        "#;
        let config: Config = toml::from_str(text).unwrap();
        config.validate().unwrap();
        config
    }

    fn launched(decision: Decision) -> (String, Option<usize>) {
        match decision {
            Decision::Launch { app, rule_index } => (app, rule_index),
            other => panic!("Launch を期待: {other:?}"),
        }
    }

    #[test]
    fn glob_rule_matches_backslash_path() {
        let target = Target::parse(r"D:\Develop\notes\a.md");
        let decision = evaluate(&config(), &target, &Modifiers::default());
        assert_eq!(launched(decision), ("vscode".to_string(), Some(0)));
    }

    #[test]
    fn falls_back_to_default() {
        let target = Target::parse(r"C:\Users\x\memo.md");
        let decision = evaluate(&config(), &target, &Modifiers::default());
        assert_eq!(launched(decision), ("obsidian".to_string(), None));
    }

    #[test]
    fn modifier_rule_requires_key() {
        let target = Target::parse(r"C:\Users\x\memo.md");
        let mods = Modifiers { shift: true, ..Default::default() };
        let decision = evaluate(&config(), &target, &mods);
        assert_eq!(launched(decision), ("vscode".to_string(), Some(1)));
    }

    #[test]
    fn url_routes_by_host_glob() {
        let target = Target::parse("https://wiki.corp.example.com/page");
        let decision = evaluate(&config(), &target, &Modifiers::default());
        assert_eq!(launched(decision), ("chrome-work".to_string(), Some(0)));
    }

    #[test]
    fn url_routes_by_url_glob() {
        let target = Target::parse("https://github.com/myorg/repo/pull/1");
        let decision = evaluate(&config(), &target, &Modifiers::default());
        assert_eq!(launched(decision), ("chrome-work".to_string(), Some(1)));
    }

    #[test]
    fn url_falls_back_to_default() {
        let target = Target::parse("https://example.org/");
        let decision = evaluate(&config(), &target, &Modifiers::default());
        assert_eq!(launched(decision), ("firefox".to_string(), None));
    }

    #[test]
    fn drive_letter_is_not_a_url_scheme() {
        assert!(matches!(Target::parse(r"C:\a.md"), Target::File { .. }));
        assert!(matches!(Target::parse("C:/a.md"), Target::File { .. }));
        assert!(matches!(Target::parse("https://a.example/x"), Target::Url { .. }));
    }

    #[test]
    fn pick_rule_collects_candidates() {
        let target = Target::parse(r"C:\site\index.html");
        let mods = Modifiers { ctrl: true, ..Default::default() };
        match evaluate(&config(), &target, &mods) {
            Decision::Pick { candidates, rule_index } => {
                assert_eq!(rule_index, 0);
                assert!(candidates.is_empty()); // テーブルに app 参照がないため
            }
            other => panic!("Pick を期待: {other:?}"),
        }
    }

    #[test]
    fn no_route_for_unknown_extension() {
        let target = Target::parse(r"C:\a.xyz");
        assert!(matches!(
            evaluate(&config(), &target, &Modifiers::default()),
            Decision::NoRoute { .. }
        ));
    }

    #[test]
    fn no_modifier_falls_through_pick_to_no_route() {
        let target = Target::parse(r"C:\site\index.html");
        assert!(matches!(
            evaluate(&config(), &target, &Modifiers::default()),
            Decision::NoRoute { .. }
        ));
    }

    #[test]
    fn command_line_substitutes_target() {
        let config = config();
        let (program, args) = build_command_line(&config.apps["chrome-work"], "https://x.example/");
        assert_eq!(program, "chrome.exe");
        assert_eq!(args, vec!["--profile-directory=Profile 1", "https://x.example/"]);
    }
}
