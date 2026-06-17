//! 設定画面ロジック — Slint バインディング

use std::cell::RefCell;
use std::rc::Rc;

use crate::config::Config;
use crate::error::Result;
use crate::{platform, registry};

use slint::{ComponentHandle, Model, VecModel};

pub fn run() -> Result<()> {
    platform::init_com();
    let config_path = crate::config::resolve_config_path()
        .unwrap_or_else(|_| std::path::PathBuf::from("winassoc.toml"));

    let config = Config::load(&config_path)
        .unwrap_or_else(|_| Config::default_config());

    let ui = crate::Settings::new()
        .map_err(|e| crate::error::Error::new(format!("設定画面の起動に失敗しました: {e}")))?;

    ui.set_config_path(config_path.display().to_string().into());

    let config_rc = Rc::new(RefCell::new(config));
    let config_path_rc = Rc::new(config_path.clone());

    let build_apps_model = |cfg: &Config| -> Vec<crate::AppEntry> {
        cfg.apps.iter().map(|(name, def)| {
            crate::AppEntry {
                name: name.clone().into(),
                cmd: def.cmd.clone().into(),
                args: def.args.join(" ").into(),
                label: def.label.clone().unwrap_or_default().into(),
            }
        }).collect()
    };

    let apps_model: Rc<VecModel<crate::AppEntry>> =
        Rc::new(VecModel::from(build_apps_model(&config_rc.borrow())));
    ui.set_apps(apps_model.clone().into());

    let ui_weak = ui.as_weak();

    {
        let cc = config_rc.clone();
        let cm = apps_model.clone();
        let uw = ui_weak.clone();
        ui.on_add_app(move || {
            let name = format!("app{}", cm.row_count() + 1);
            cm.push(crate::AppEntry {
                name: name.clone().into(),
                cmd: "".into(),
                args: "".into(),
                label: "".into(),
            });
            cc.borrow_mut().apps.insert(name, Default::default());
            if let Some(ui) = uw.upgrade() { ui.set_dirty(true); }
        });
    }
    {
        let cc = config_rc.clone();
        let cm = apps_model.clone();
        let uw = ui_weak.clone();
        ui.on_delete_app(move |idx: i32| {
            if idx >= 0 && (idx as usize) < cm.row_count() {
                if let Some(entry) = cm.row_data(idx as usize) {
                    let key = entry.name.to_string();
                    cm.remove(idx as usize);
                    cc.borrow_mut().apps.remove(&key);
                    if let Some(ui) = uw.upgrade() { ui.set_dirty(true); }
                }
            }
        });
    }
    {
        let cc = config_rc.clone();
        ui.on_apply(move || {
            if let Err(e) = registry::apply(&cc.borrow()) {
                platform::show_error_dialog(&format!("適用に失敗しました: {e}"));
            }
        });
    }
    {
        let cc = config_rc.clone();
        ui.on_unregister(move || {
            if let Err(e) = registry::unregister(&cc.borrow()) {
                platform::show_error_dialog(&format!("解除に失敗しました: {e}"));
            }
        });
    }
    {
        let cc = config_rc.clone();
        let cp = config_path_rc.clone();
        ui.on_doctor(move || {
            if let Err(e) = registry::doctor(&cc.borrow(), &cp) {
                platform::show_error_dialog(&format!("診断に失敗しました: {e}"));
            }
        });
    }
    {
        let cc = config_rc.clone();
        ui.on_backup(move || {
            if let Err(e) = registry::backup(&cc.borrow()) {
                platform::show_error_dialog(&format!("バックアップに失敗しました: {e}"));
            }
        });
    }
    ui.on_restore(move || {
        if let Err(e) = registry::restore(None) {
            platform::show_error_dialog(&format!("復元に失敗しました: {e}"));
        }
    });
    {
        let cc = config_rc.clone();
        let cp = config_path_rc.clone();
        let uw = ui_weak.clone();
        ui.on_save_config(move || {
            let text = toml::to_string_pretty(&*cc.borrow()).unwrap_or_default();
            if let Err(e) = std::fs::write(&*cp, text) {
                platform::show_error_dialog(&format!("保存に失敗しました: {e}"));
            } else if let Some(ui) = uw.upgrade() {
                ui.set_dirty(false);
            }
        });
    }

    ui.run()
        .map_err(|e| crate::error::Error::new(format!("設定画面の実行に失敗しました: {e}")))?;
    Ok(())
}
