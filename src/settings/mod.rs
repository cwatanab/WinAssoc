//! 設定画面ロジック — Slint バインディング

use crate::config::Config;
use crate::error::Result;
use crate::{platform, registry};

use slint::ComponentHandle;

pub fn run() -> Result<()> {
    platform::init_com();
    let config_path = crate::config::resolve_config_path()
        .unwrap_or_else(|_| std::path::PathBuf::from("winassoc.toml"));

    let config = Config::load(&config_path)
        .unwrap_or_else(|_| Config::default_config());

    let ui = crate::Settings::new()
        .map_err(|e| crate::error::Error::new(format!("設定画面の起動に失敗しました: {e}")))?;

    ui.set_config_path(config_path.display().to_string().into());

    let _config_clone = config.clone();
    let _config_path_clone = config_path.clone();
    ui.on_apply(move || {
        if let Err(e) = registry::apply(&_config_clone) {
            platform::show_error_dialog(&format!("適用に失敗しました: {e}"));
        }
    });

    let _config_clone2 = config.clone();
    let _config_path_clone2 = config_path.clone();
    ui.on_unregister(move || {
        if let Err(e) = registry::unregister(&_config_clone2) {
            platform::show_error_dialog(&format!("解除に失敗しました: {e}"));
        }
    });

    let _config_clone3 = config.clone();
    let _config_path_clone3 = config_path.clone();
    ui.on_doctor(move || {
        if let Err(e) = registry::doctor(&_config_clone3, &_config_path_clone3) {
            platform::show_error_dialog(&format!("診断に失敗しました: {e}"));
        }
    });

    let config_clone4 = config.clone();
    ui.on_backup(move || {
        match registry::backup(&config_clone4) {
            Ok(path) => { let _ = path; }
            Err(e) => platform::show_error_dialog(&format!("バックアップに失敗しました: {e}")),
        }
    });

    ui.on_restore(move || {
        if let Err(e) = registry::restore(None) {
            platform::show_error_dialog(&format!("復元に失敗しました: {e}"));
        }
    });

    let config_clone5 = config.clone();
    let config_path_clone5 = config_path.clone();
    ui.on_save_config(move || {
        let text = toml::to_string_pretty(&config_clone5).unwrap_or_default();
        if let Err(e) = std::fs::write(&config_path_clone5, text) {
            platform::show_error_dialog(&format!("保存に失敗しました: {e}"));
        } else {
            // TODO: reload config or update dirty flag
        }
    });

    ui.run()
        .map_err(|e| crate::error::Error::new(format!("設定画面の実行に失敗しました: {e}")))?;
    Ok(())
}
