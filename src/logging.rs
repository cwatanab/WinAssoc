use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};

const MAX_LOG_BYTES: u64 = 1024 * 1024; // 1MB でローテーション

fn log_file() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("winassoc").join("logs").join("winassoc.log"))
}

/// 起動記録を 1 行追記する。ログ失敗でシム本来の動作を止めない (SPEC 6)
pub fn log_launch(target: &str, matched: &str, app: &str, result: &str) {
    let _ = try_log(target, matched, app, result);
}

fn try_log(target: &str, matched: &str, app: &str, result: &str) -> Result<()> {
    let path = log_file().context("ログディレクトリを特定できません")?;
    fs::create_dir_all(path.parent().unwrap())?;

    // サイズローテーション: winassoc.log → winassoc.log.1 (1 世代)
    if fs::metadata(&path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = fs::rename(&path, path.with_extension("log.1"));
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut file = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(file, "{now}\t{target}\t{matched}\t{app}\t{result}")?;
    Ok(())
}

/// `winassoc log --tail N`
pub fn tail(n: usize) -> Result<()> {
    let path = log_file().context("ログディレクトリを特定できません")?;
    if !path.exists() {
        println!("ログはまだありません ({})", path.display());
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    for line in &lines[start..] {
        println!("{line}");
    }
    Ok(())
}
