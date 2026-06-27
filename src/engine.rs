use std::collections::HashMap;
use std::sync::LazyLock;

use crate::bail;
use crate::error::Result;

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
            match name.to_ascii_lowercase().as_str() {
                "shift" => m.shift = true,
                "ctrl" => m.ctrl = true,
                "alt" => m.alt = true,
                other => bail!("不明な修飾キー: {other} (shift / ctrl / alt)"),
            }
        }
        Ok(m)
    }

    fn pressed(&self, name: &str) -> bool {
        match name.to_ascii_lowercase().as_str() {
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

fn parse_url(input: &str) -> Option<(String, String, Option<String>)> {
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
    /// pick ルール一致 (rule_index = Some)、またはルート/default 未定義のフォールバック (None)
    Pick { candidates: Vec<String>, rule_index: Option<usize>, reason: String },
    /// [apps] が空で何も起動できない
    NoRoute { reason: String },
}

pub fn evaluate(config: &Config, target: &Target, mods: &Modifiers) -> Decision {
    if config.apps.is_empty() {
        return Decision::NoRoute { reason: "[apps] が 1 つも定義されていません".to_string() };
    }

    let Some((table_name, table)) = target.route_table(config) else {
        let reason = match target {
            Target::File { ext: Some(ext), .. } => format!("[ext.{ext}] が未定義です"),
            Target::File { ext: None, .. } => "拡張子のないファイルです".to_string(),
            Target::Url { scheme, .. } => format!("[protocol.{scheme}] が未定義です"),
        };
        return Decision::Pick { candidates: all_apps(config), rule_index: None, reason };
    };

    for (i, rule) in table.rules.iter().enumerate() {
        if rule_matches(rule, target, mods) {
            return match &rule.app {
                Some(app) => Decision::Launch { app: app.clone(), rule_index: Some(i) },
                None => Decision::Pick {
                    candidates: candidates_or_all(config, table),
                    rule_index: Some(i),
                    reason: format!("[{table_name}] rules[{i}] の pick 指定", i = i + 1),
                },
            };
        }
    }

    match &table.default {
        Some(app) => Decision::Launch { app: app.clone(), rule_index: None },
        None => Decision::Pick {
            candidates: candidates_or_all(config, table),
            rule_index: None,
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

fn all_apps(config: &Config) -> Vec<String> {
    config.apps.keys().cloned().collect()
}

/// テーブル内候補。空なら全アプリにフォールバック
fn candidates_or_all(config: &Config, table: &RouteTable) -> Vec<String> {
    if let Some(explicit_candidates) = &table.candidates {
        if !explicit_candidates.is_empty() {
            return explicit_candidates.clone();
        }
    }
    let candidates = pick_candidates(table);
    if candidates.is_empty() {
        all_apps(config)
    } else {
        candidates
    }
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
    fn pick_rule_falls_back_to_all_apps_when_table_has_no_refs() {
        let target = Target::parse(r"C:\site\index.html");
        let mods = Modifiers { ctrl: true, ..Default::default() };
        match evaluate(&config(), &target, &mods) {
            Decision::Pick { candidates, rule_index, .. } => {
                assert_eq!(rule_index, Some(0));
                // ext.html テーブルに app 参照がないため全アプリが候補になる
                assert_eq!(candidates.len(), 4);
            }
            other => panic!("Pick を期待: {other:?}"),
        }
    }

    #[test]
    fn unknown_extension_falls_back_to_pick_all_apps() {
        let target = Target::parse(r"C:\a.xyz");
        match evaluate(&config(), &target, &Modifiers::default()) {
            Decision::Pick { candidates, rule_index, .. } => {
                assert_eq!(rule_index, None);
                assert_eq!(candidates.len(), 4);
            }
            other => panic!("Pick を期待: {other:?}"),
        }
    }

    #[test]
    fn no_match_without_default_falls_back_to_pick() {
        let target = Target::parse(r"C:\site\index.html");
        assert!(matches!(
            evaluate(&config(), &target, &Modifiers::default()),
            Decision::Pick { rule_index: None, .. }
        ));
    }

    #[test]
    fn empty_apps_is_no_route() {
        let config: Config = toml::from_str("").unwrap();
        let target = Target::parse(r"C:\a.md");
        assert!(matches!(
            evaluate(&config, &target, &Modifiers::default()),
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

    #[test]
    fn pick_uses_explicit_candidates_when_configured() {
        let text = r#"
            [apps.vscode]
            cmd = 'C:\Tools\Code.exe'
            [apps.chrome]
            cmd = 'chrome.exe'
            [apps.firefox]
            cmd = 'firefox.exe'

            [ext.html]
            candidates = ["chrome", "firefox"]
            rules = [
              { modifier = "ctrl", pick = true },
            ]
        "#;
        let config: Config = toml::from_str(text).unwrap();
        let target = Target::parse(r"C:\site\index.html");
        let mods = Modifiers { ctrl: true, ..Default::default() };
        match evaluate(&config, &target, &mods) {
            Decision::Pick { candidates, rule_index, .. } => {
                assert_eq!(rule_index, Some(0));
                assert_eq!(candidates, vec!["chrome".to_string(), "firefox".to_string()]);
            }
            other => panic!("Pick を期待: {other:?}"),
        }
    }
}
