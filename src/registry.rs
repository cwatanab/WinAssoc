//! M2: HKCU への登録・診断・バックアップ (SPEC 5)
//!
//! UserChoice ハッシュは偽造しない。既定アプリの最終確定が必要なものは
//! ms-settings:defaultapps での 1 クリックをユーザーに案内する。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};
use winreg::RegKey;

use crate::config::{expand_env, Config};

const APP_NAME: &str = "WinAssoc";
const URL_PROGID: &str = "WinAssoc.Url";
const PROGID_PREFIX: &str = "WinAssoc.";
const CLIENT_PATH: &str = r"Software\Clients\StartMenuInternet\WinAssoc";
const FILE_EXTS_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts";
const URL_ASSOC_PATH: &str = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations";

fn hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

/// ハンドラとして登録するシム exe。コンソールを出さない winassoc-open.exe を
/// 優先し、見つからなければ自分自身 (開発時の cargo run 等) にフォールバック
fn shim_exe() -> Result<(PathBuf, bool)> {
    let exe = std::env::current_exe()?;
    let open_exe = exe.with_file_name("winassoc-open.exe");
    if open_exe.exists() {
        Ok((open_exe, true))
    } else {
        Ok((exe, false))
    }
}

fn shim_command_for(target_placeholder: &str) -> Result<String> {
    let (exe, is_open_exe) = shim_exe()?;
    if is_open_exe {
        Ok(format!("\"{}\" \"{target_placeholder}\"", exe.display()))
    } else {
        Ok(format!("\"{}\" open \"{target_placeholder}\"", exe.display()))
    }
}

fn shim_command() -> Result<String> {
    shim_command_for("%1")
}

fn progid_for_ext(ext: &str) -> String {
    format!("{PROGID_PREFIX}{ext}")
}

// ─────────────────────────── apply ───────────────────────────

pub fn apply(config: &Config) -> Result<()> {
    let backup_path = backup(config)?;
    println!("既存の関連付けをバックアップしました: {}", backup_path.display());

    let (classes, _) = hkcu().create_subkey(r"Software\Classes")?;
    let command = shim_command()?;

    // 拡張子: ProgID + .ext default + OpenWithProgids
    for (ext, table) in &config.ext {
        let progid = progid_for_ext(ext);
        let (progid_key, _) = classes.create_subkey(&progid)?;
        progid_key.set_value("", &format!(".{ext} (winassoc ルーター)"))?;
        if let Some(default_app) = table.default.as_ref().and_then(|name| config.apps.get(name)) {
            let (icon, _) = progid_key.create_subkey("DefaultIcon")?;
            icon.set_value("", &format!("{},0", expand_env(&default_app.cmd)))?;
        }
        let (cmd_key, _) = progid_key.create_subkey(r"shell\open\command")?;
        cmd_key.set_value("", &command)?;

        let (ext_key, _) = classes.create_subkey(format!(".{ext}"))?;
        ext_key.set_value("", &progid)?;
        let (owp, _) = ext_key.create_subkey("OpenWithProgids")?;
        owp.set_value(&progid, &"")?;
        println!("登録: .{ext} → {progid}");
    }

    // プロトコル: 共通 URL ProgID + ブラウザ Capabilities
    if !config.protocol.is_empty() {
        let (url_key, _) = classes.create_subkey(URL_PROGID)?;
        url_key.set_value("", &"WinAssoc URL ルーター")?;
        let (cmd_key, _) = url_key.create_subkey(r"shell\open\command")?;
        cmd_key.set_value("", &command)?;

        let (client, _) = hkcu().create_subkey(CLIENT_PATH)?;
        client.set_value("", &APP_NAME)?;
        let (client_cmd, _) = client.create_subkey(r"shell\open\command")?;
        client_cmd.set_value("", &shim_command_for("about:blank")?)?;
        let (caps, _) = client.create_subkey("Capabilities")?;
        caps.set_value("ApplicationName", &APP_NAME)?;
        caps.set_value("ApplicationDescription", &"URL をルールベースで振り分けるルーター")?;

        let (url_assoc, _) = caps.create_subkey("URLAssociations")?;
        for scheme in config.protocol.keys() {
            url_assoc.set_value(scheme, &URL_PROGID)?;
            register_custom_scheme(&classes, scheme, &command)?;
            println!("登録: {scheme}: → {URL_PROGID}");
        }
        let (file_assoc, _) = caps.create_subkey("FileAssociations")?;
        for ext in config.ext.keys() {
            file_assoc.set_value(format!(".{ext}"), &progid_for_ext(ext))?;
        }

        let (registered, _) = hkcu().create_subkey(r"Software\RegisteredApplications")?;
        registered.set_value(APP_NAME, &format!(r"{CLIENT_PATH}\Capabilities"))?;
    }

    notify_assoc_changed();
    println!();
    println!("登録完了。http/https など UserChoice 保護された既定の最終確定は");
    println!("  start ms-settings:defaultapps");
    println!("を開いて {APP_NAME} を選択してください (winassoc doctor で未確定項目を確認できます)");
    Ok(())
}

/// http/https 以外の未登録カスタムスキームは Classes に直接登録する。
/// 既に他アプリが所有しているスキームには触らない (doctor が報告する)
fn register_custom_scheme(classes: &RegKey, scheme: &str, command: &str) -> Result<()> {
    if matches!(scheme, "http" | "https") {
        return Ok(());
    }
    if let Ok(existing) = classes.open_subkey(scheme) {
        let cur: String = existing
            .open_subkey(r"shell\open\command")
            .and_then(|k| k.get_value(""))
            .unwrap_or_default();
        if !cur.is_empty() && !cur.contains("winassoc") {
            println!("  注意: {scheme}: は既存ハンドラがあるため Classes は変更しません ({cur})");
            return Ok(());
        }
    }
    let (key, _) = classes.create_subkey(scheme)?;
    key.set_value("", &format!("URL:{scheme} (winassoc ルーター)"))?;
    key.set_value("URL Protocol", &"")?;
    let (cmd_key, _) = key.create_subkey(r"shell\open\command")?;
    cmd_key.set_value("", &command)?;
    Ok(())
}

// ───────────────────────── unregister ─────────────────────────

pub fn unregister(config: &Config) -> Result<()> {
    let classes = hkcu().open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;
    let saved = load_backup(None).ok();

    // WinAssoc.* ProgID を全削除
    let progids: Vec<String> = classes
        .enum_keys()
        .filter_map(|k| k.ok())
        .filter(|k| k.starts_with(PROGID_PREFIX))
        .collect();
    for progid in &progids {
        classes.delete_subkey_all(progid)?;
        println!("削除: ProgID {progid}");
    }

    // .ext の default と OpenWithProgids を復元/掃除
    for ext in config.ext.keys() {
        let progid = progid_for_ext(ext);
        if let Ok(ext_key) = classes.open_subkey_with_flags(format!(".{ext}"), KEY_ALL_ACCESS) {
            let cur: String = ext_key.get_value("").unwrap_or_default();
            if cur == progid {
                let restored = saved
                    .as_ref()
                    .and_then(|b| b.ext.get(ext))
                    .and_then(|e| e.class_default.clone());
                match restored {
                    Some(value) => ext_key.set_value("", &value)?,
                    None => {
                        let _ = ext_key.delete_value("");
                    }
                }
                println!("復元: .{ext} の既定 ProgID");
            }
            if let Ok(owp) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_ALL_ACCESS) {
                let _ = owp.delete_value(&progid);
            }
        }
        cleanup_empty_ext_key(&classes, ext)?;
    }

    // カスタムスキームのうち自分が登録したものを削除
    for scheme in config.protocol.keys() {
        if matches!(scheme.as_str(), "http" | "https") {
            continue;
        }
        if let Ok(key) = classes.open_subkey(scheme) {
            let cur: String = key
                .open_subkey(r"shell\open\command")
                .and_then(|k| k.get_value(""))
                .unwrap_or_default();
            if cur.contains("winassoc") {
                classes.delete_subkey_all(scheme)?;
                println!("削除: {scheme}:");
            }
        }
    }

    // ブラウザ登録の削除
    let software = hkcu().open_subkey_with_flags("Software", KEY_ALL_ACCESS)?;
    if software.open_subkey(CLIENT_PATH.trim_start_matches(r"Software\")).is_ok() {
        software.delete_subkey_all(CLIENT_PATH.trim_start_matches(r"Software\"))?;
        println!("削除: ブラウザ登録 (StartMenuInternet)");
    }
    if let Ok(registered) =
        hkcu().open_subkey_with_flags(r"Software\RegisteredApplications", KEY_ALL_ACCESS)
    {
        let _ = registered.delete_value(APP_NAME);
    }

    notify_assoc_changed();
    println!("登録解除が完了しました");
    Ok(())
}

/// apply が新規作成した .ext キーは、復元後に空なら丸ごと削除する。
/// 既存の関連付けがあった拡張子は値が残るため消えない
fn cleanup_empty_ext_key(classes: &RegKey, ext: &str) -> Result<()> {
    let name = format!(".{ext}");
    let Ok(key) = classes.open_subkey(&name) else {
        return Ok(());
    };
    let info = key.query_info()?;
    let only_empty_owp = info.sub_keys == 1
        && key
            .open_subkey("OpenWithProgids")
            .and_then(|owp| owp.query_info())
            .map(|i| i.values == 0 && i.sub_keys == 0)
            .unwrap_or(false);
    if info.values == 0 && (info.sub_keys == 0 || only_empty_owp) {
        drop(key);
        classes.delete_subkey_all(&name)?;
        println!("削除: .{ext} (空キー)");
    }
    Ok(())
}

// ─────────────────────────── doctor ───────────────────────────

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

fn read_user_choice_ext(ext: &str) -> Option<String> {
    hkcu()
        .open_subkey(format!(r"{FILE_EXTS_PATH}\.{ext}\UserChoice"))
        .and_then(|k| k.get_value("ProgId"))
        .ok()
}

fn read_user_choice_protocol(scheme: &str) -> Option<String> {
    hkcu()
        .open_subkey(format!(r"{URL_ASSOC_PATH}\{scheme}\UserChoice"))
        .and_then(|k| k.get_value("ProgId"))
        .ok()
}

// ────────────────────── backup / restore ──────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Backup {
    pub created: String,
    #[serde(default)]
    pub ext: BTreeMap<String, ExtBackup>,
    #[serde(default)]
    pub protocol: BTreeMap<String, ProtocolBackup>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExtBackup {
    /// HKCU\Software\Classes\.ext の既定値
    pub class_default: Option<String>,
    /// Explorer FileExts UserChoice の ProgId (参考情報: プログラムからは復元不可)
    pub user_choice: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProtocolBackup {
    pub user_choice: Option<String>,
}

fn backup_dir() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .context("%LOCALAPPDATA% を特定できません")?
        .join("winassoc")
        .join("backup");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 設定対象の現在の関連付け状態をファイルに保存し、保存先を返す
pub fn backup(config: &Config) -> Result<PathBuf> {
    let classes = hkcu().open_subkey(r"Software\Classes")?;
    let mut data = Backup {
        created: chrono::Local::now().to_rfc3339(),
        ..Default::default()
    };

    for ext in config.ext.keys() {
        let class_default: Option<String> = classes
            .open_subkey(format!(".{ext}"))
            .and_then(|k| k.get_value(""))
            .ok()
            .filter(|v: &String| !v.is_empty() && !v.starts_with(PROGID_PREFIX));
        data.ext.insert(
            ext.clone(),
            ExtBackup { class_default, user_choice: read_user_choice_ext(ext) },
        );
    }
    for scheme in config.protocol.keys() {
        data.protocol.insert(
            scheme.clone(),
            ProtocolBackup { user_choice: read_user_choice_protocol(scheme) },
        );
    }

    let dir = backup_dir()?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("backup-{stamp}.toml"));
    let text = toml::to_string_pretty(&data)?;
    std::fs::write(&path, &text)?;
    std::fs::write(dir.join("latest.toml"), &text)?;
    Ok(path)
}

fn load_backup(file: Option<&Path>) -> Result<Backup> {
    let path = match file {
        Some(p) => p.to_path_buf(),
        None => backup_dir()?.join("latest.toml"),
    };
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("バックアップを読めません: {}", path.display()))?;
    Ok(toml::from_str(&text)?)
}

pub fn restore(file: Option<&Path>) -> Result<()> {
    let data = load_backup(file)?;
    let classes = hkcu().open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;

    if data.ext.is_empty() && data.protocol.is_empty() {
        bail!("バックアップに復元対象がありません");
    }
    println!("バックアップ ({}) から復元します", data.created);

    for (ext, saved) in &data.ext {
        if let Ok(ext_key) = classes.open_subkey_with_flags(format!(".{ext}"), KEY_ALL_ACCESS) {
            match &saved.class_default {
                Some(value) => {
                    ext_key.set_value("", value)?;
                    println!("復元: .{ext} → {value}");
                }
                None => {
                    let cur: String = ext_key.get_value("").unwrap_or_default();
                    if cur.starts_with(PROGID_PREFIX) {
                        let _ = ext_key.delete_value("");
                        println!("復元: .{ext} → (既定なし)");
                    }
                }
            }
        }
        if let Some(choice) = &saved.user_choice {
            if !choice.starts_with(PROGID_PREFIX) {
                println!("  参考: .{ext} の UserChoice は {choice} でした (必要なら設定アプリで再選択)");
            }
        }
    }
    for (scheme, saved) in &data.protocol {
        if let Some(choice) = &saved.user_choice {
            if !choice.starts_with(PROGID_PREFIX) {
                println!("  参考: {scheme}: の UserChoice は {choice} でした (必要なら設定アプリで再選択)");
            }
        }
    }

    notify_assoc_changed();
    println!("復元完了。UserChoice はプログラムからは書き換えられないため、上記の参考情報をもとに");
    println!("ms-settings:defaultapps で再選択してください");
    Ok(())
}

// ─────────────────────────── 共通 ───────────────────────────

fn notify_assoc_changed() {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
