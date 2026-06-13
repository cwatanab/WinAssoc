use crate::bail;
use crate::error::Result;

use crate::config::{expand_env, Config};
use crate::engine::{build_command_line, evaluate, Decision, Modifiers, Target};
use crate::{logging, picker, platform};

/// シム本体。OS のハンドラとして呼ばれ、評価結果のアプリを起動する
pub fn open(config: &Config, raw_target: &str) -> Result<()> {
    let target = Target::parse(raw_target);
    let mods = platform::current_modifiers();
    match evaluate(config, &target, &mods) {
        Decision::Launch { app, rule_index } => {
            let matched = match rule_index {
                Some(i) => format!("rule#{}", i + 1),
                None => "default".to_string(),
            };
            launch(config, &app, target.raw(), &matched)
        }
        Decision::Pick { candidates, .. } => {
            let entries = candidates
                .iter()
                .map(|name| {
                    let def = &config.apps[name];
                    picker::Candidate {
                        name: name.clone(),
                        label: def.label.clone(),
                        program: expand_env(&def.cmd),
                    }
                })
                .collect();
            match picker::show(target_label(&target), entries, config.settings.picker_timeout_ms)? {
                Some(app) => launch(config, &app, target.raw(), "picker"),
                None => {
                    logging::log_launch(target.raw(), "picker", "-", "cancelled");
                    Ok(())
                }
            }
        }
        Decision::NoRoute { reason } => {
            logging::log_launch(target.raw(), "-", "-", "no-route");
            bail!("ルートがありません: {reason}")
        }
    }
}

fn launch(config: &Config, app: &str, target: &str, matched: &str) -> Result<()> {
    let (program, args) = build_command_line(&config.apps[app], target);
    let result = std::process::Command::new(&program).args(&args).spawn();
    match result {
        Ok(_) => {
            logging::log_launch(target, matched, app, "ok");
            Ok(())
        }
        Err(e) => {
            logging::log_launch(target, matched, app, &format!("error: {e}"));
            Err(crate::error::Error::new(format!("起動に失敗しました: {program}: {e}")))
        }
    }
}

/// ピッカーのタイトルに出す短い表示名 (ファイル名 or 短縮 URL)
fn target_label(target: &Target) -> String {
    let label = match target {
        Target::File { path, .. } => std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone()),
        Target::Url { url, .. } => url.clone(),
    };
    if label.chars().count() > 48 {
        let head: String = label.chars().take(45).collect();
        format!("{head}…")
    } else {
        label
    }
}

/// dry-run。何にどう一致して何が起動されるかを表示するだけで起動しない
pub fn test(config: &Config, raw_target: &str, modifier_names: &[String]) -> Result<()> {
    let target = Target::parse(raw_target);
    let mods = Modifiers::from_names(modifier_names.iter().map(String::as_str))?;

    match &target {
        Target::File { path, ext } => println!(
            "対象: {path} (ファイル, 拡張子: {})",
            ext.as_deref().map(|e| format!(".{e}")).unwrap_or_else(|| "なし".to_string())
        ),
        Target::Url { url, scheme, host } => println!(
            "対象: {url} (URL, プロトコル: {scheme}, ホスト: {})",
            host.as_deref().unwrap_or("なし")
        ),
    }
    if !modifier_names.is_empty() {
        println!("修飾キー: {}", modifier_names.join(" + "));
    }

    match evaluate(config, &target, &mods) {
        Decision::Launch { app, rule_index } => {
            match rule_index {
                Some(i) => println!("判定: ルール #{} に一致 → {app}", i + 1),
                None => println!("判定: ルール不一致 → default の {app}"),
            }
            let (program, args) = build_command_line(&config.apps[&app], target.raw());
            println!("起動: \"{program}\" {}", quote_args(&args));
        }
        Decision::Pick { candidates, rule_index, reason } => {
            match rule_index {
                Some(i) => println!("判定: ルール #{} に一致 → ピッカー表示", i + 1),
                None => println!("判定: ピッカー表示 ({reason})"),
            }
            println!("候補: {}", candidates.join(", "));
        }
        Decision::NoRoute { reason } => println!("判定: ルートなし ({reason})"),
    }
    Ok(())
}

/// 設定済みルートの一覧
pub fn list(config: &Config) -> Result<()> {
    println!("アプリ定義 ({}):", config.apps.len());
    for (name, def) in &config.apps {
        let label = def.label.as_deref().map(|l| format!(" ({l})")).unwrap_or_default();
        println!("  {name}{label} → {}", def.cmd);
    }

    for (kind, tables) in [("拡張子", &config.ext), ("プロトコル", &config.protocol)] {
        if tables.is_empty() {
            continue;
        }
        println!("\n{kind}ルート ({}):", tables.len());
        for (key, table) in tables {
            let prefix = if kind == "拡張子" { "." } else { "" };
            println!("  {prefix}{key}");
            for (i, rule) in table.rules.iter().enumerate() {
                let mut conds = Vec::new();
                if let Some(g) = &rule.glob {
                    conds.push(format!("glob = '{g}'"));
                }
                if let Some(h) = &rule.host {
                    conds.push(format!("host = '{h}'"));
                }
                if let Some(u) = &rule.url {
                    conds.push(format!("url = '{u}'"));
                }
                if let Some(m) = &rule.modifier {
                    conds.push(format!("modifier = {m}"));
                }
                let cond = if conds.is_empty() { "(常に)".to_string() } else { conds.join(" AND ") };
                let action = rule.app.clone().unwrap_or_else(|| "ピッカー表示".to_string());
                println!("    #{} {cond} → {action}", i + 1);
            }
            match &table.default {
                Some(app) => println!("    default → {app}"),
                None => println!("    default → (なし → ピッカー)"),
            }
        }
    }
    Ok(())
}

fn quote_args(args: &[String]) -> String {
    args.iter()
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ")
}


