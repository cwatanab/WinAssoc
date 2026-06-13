use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use winreg::enums::KEY_ALL_ACCESS;

use super::{hkcu, notify_assoc_changed, read_user_choice_ext, read_user_choice_protocol, PROGID_PREFIX};

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
pub fn backup(config: &crate::config::Config) -> Result<PathBuf> {
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

pub(crate) fn load_backup(file: Option<&Path>) -> Result<Backup> {
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
