use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::engine::{build_command_line, evaluate, Decision, Modifiers, Target};

/// シム本体。OS のハンドラとして呼ばれ、評価結果のアプリを起動する
pub fn open(config: &Config, raw_target: &str) -> Result<()> {
    let target = Target::parse(raw_target);
    let mods = current_modifiers();
    match evaluate(config, &target, &mods) {
        Decision::Launch { app, .. } => {
            let def = &config.apps[&app];
            let (program, args) = build_command_line(def, target.raw());
            std::process::Command::new(&program)
                .args(&args)
                .spawn()
                .with_context(|| format!("起動に失敗しました: {program}"))?;
            Ok(())
        }
        Decision::Pick { candidates, .. } => {
            // TODO(M3): ランチャー UI (SPEC 6.5)。実装までは候補を提示して終了する
            bail!(
                "ピッカー UI は未実装です。候補: {}",
                if candidates.is_empty() { "(なし)".to_string() } else { candidates.join(", ") }
            )
        }
        Decision::NoRoute { reason } => bail!("ルートがありません: {reason}"),
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
        Decision::Pick { candidates, rule_index } => {
            println!("判定: ルール #{} に一致 → ピッカー表示", rule_index + 1);
            println!(
                "候補: {}",
                if candidates.is_empty() { "(なし)".to_string() } else { candidates.join(", ") }
            );
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
                None => println!("    default → (なし)"),
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

/// 起動時点の修飾キー押下状態 (SPEC 6: GetAsyncKeyState)
#[cfg(windows)]
fn current_modifiers() -> Modifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    let down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
    Modifiers {
        shift: down(VK_SHIFT.0 as i32),
        ctrl: down(VK_CONTROL.0 as i32),
        alt: down(VK_MENU.0 as i32),
    }
}

#[cfg(not(windows))]
fn current_modifiers() -> Modifiers {
    Modifiers::default()
}
