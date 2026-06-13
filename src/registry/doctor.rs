use std::path::Path;

use anyhow::Result;

use super::{hkcu, progid_for_ext, read_user_choice_ext, read_user_choice_protocol, shim_command,
           shim_exe, APP_NAME, CLIENT_PATH, URL_PROGID};
use crate::config::{expand_env, Config};

pub fn doctor(config: &Config, config_path: &Path) -> Result<()> {
    let exe = shim_exe()?.0.display().to_string();
    let command = shim_command()?;
    let classes = hkcu().open_subkey(r"Software\Classes")?;
    let mut warnings = 0usize;

    println!("設定ファイル: {}", config_path.display());
    println!("シム exe:     {exe}");
    println!();

    println!("[アプリ]");
    for (name, app) in &config.apps {
        let cmd = expand_env(&app.cmd);
        let path = Path::new(&cmd);
        // パス区切りのない裸のコマンド名は PATH 解決に委ねる
        let ok = !cmd.contains(['\\', '/']) || path.exists();
        if ok {
            println!("  ok  {name}: {cmd}");
        } else {
            warnings += 1;
            println!("  !!  {name}: 実行ファイルが見つかりません: {cmd}");
        }
    }

    println!();
    println!("[拡張子]");
    for ext in config.ext.keys() {
        let progid = progid_for_ext(ext);
        let registered_cmd: String = classes
            .open_subkey(format!(r"{progid}\shell\open\command"))
            .and_then(|k| k.get_value(""))
            .unwrap_or_default();
        if registered_cmd.is_empty() {
            warnings += 1;
            println!("  !!  .{ext}: 未登録です (winassoc apply を実行してください)");
            continue;
        }
        if registered_cmd != command {
            warnings += 1;
            println!("  !!  .{ext}: 登録コマンドが現在の exe と異なります: {registered_cmd}");
            continue;
        }
        match read_user_choice_ext(ext) {
            Some(choice) if choice == progid => println!("  ok  .{ext}: UserChoice = {progid}"),
            Some(other) => {
                warnings += 1;
                println!("  !!  .{ext}: UserChoice が {other} を指しています (設定アプリで {APP_NAME} を選択してください)");
            }
            None => println!("  ok  .{ext}: 登録済み (UserChoice 未設定 → ProgID 既定で動作)"),
        }
    }

    println!();
    println!("[プロトコル]");
    let browser_registered = hkcu().open_subkey(CLIENT_PATH).is_ok();
    if !config.protocol.is_empty() && !browser_registered {
        warnings += 1;
        println!("  !!  ブラウザ登録 (StartMenuInternet) がありません (winassoc apply を実行してください)");
    }
    for scheme in config.protocol.keys() {
        match read_user_choice_protocol(scheme) {
            Some(choice) if choice == URL_PROGID => println!("  ok  {scheme}: UserChoice = {URL_PROGID}"),
            Some(other) => {
                warnings += 1;
                println!("  !!  {scheme}: UserChoice が {other} を指しています (設定アプリで {APP_NAME} を選択してください)");
            }
            None => {
                let direct: String = classes
                    .open_subkey(format!(r"{scheme}\shell\open\command"))
                    .and_then(|k| k.get_value(""))
                    .unwrap_or_default();
                if direct.contains("winassoc") {
                    println!("  ok  {scheme}: Classes 直接登録で動作");
                } else {
                    warnings += 1;
                    println!("  !!  {scheme}: 未確定です (設定アプリで {APP_NAME} を選択してください)");
                }
            }
        }
    }

    println!();
    if warnings == 0 {
        println!("問題は見つかりませんでした");
    } else {
        println!("{warnings} 件の問題があります。UserChoice 関連は以下で確定できます:");
        println!("  start ms-settings:defaultapps");
    }
    Ok(())
}
