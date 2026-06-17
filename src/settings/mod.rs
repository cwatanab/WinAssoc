//! 設定画面ロジック — Slint バインディング
//!
//! Task 16 でフル実装に置換される最小スタブ。
//! 現状は Settings Window を開いて表示するだけ。

use slint::ComponentHandle;

use crate::error::Result;
use crate::platform;

pub mod apps;
pub mod data;
pub mod ext;
pub mod protocol;
pub mod theme;
pub mod validation;

pub fn run() -> Result<()> {
    platform::init_com();
    let _ = crate::config::resolve_config_path();
    let ui = crate::Settings::new()
        .map_err(|e| crate::error::Error::new(format!("設定画面の起動に失敗しました: {e}")))?;
    ui.run().map_err(|e| crate::error::Error::new(format!("設定画面の実行に失敗しました: {e}")))?;
    Ok(())
}
