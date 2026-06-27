use std::rc::Rc;
use std::cell::{RefCell, Cell};
use slint::{ComponentHandle, Model, VecModel, SharedString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::error::Result;
use crate::platform;
use crate::config::Config;

pub mod apps;
pub mod data;
pub mod ext;
pub mod management;
pub mod protocol;
pub mod theme;
pub mod validation;

use apps::AppsState;
use ext::ExtState;
use protocol::ProtocolState;

fn get_app_icon(cmd: &str) -> slint::Image {
    if let Some(rgba) = crate::icon::extract_icon_rgba(cmd, 24) {
        let mut pixels = rgba.pixels;
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3] as u32;
            px[0] = ((px[0] as u32 * a) / 255) as u8;
            px[1] = ((px[1] as u32 * a) / 255) as u8;
            px[2] = ((px[2] as u32 * a) / 255) as u8;
        }
        let buffer = slint::SharedPixelBuffer::clone_from_slice(&pixels, rgba.width as u32, rgba.height as u32);
        slint::Image::from_rgba8_premultiplied(buffer)
    } else {
        slint::Image::default()
    }
}

// 構造体変換ユーティリティ
fn to_slint_rule(r: &data::RuleEntry) -> crate::RuleEntry {
    crate::RuleEntry {
        glob: r.glob.as_str().into(),
        host: r.host.as_str().into(),
        url: r.url.as_str().into(),
        modifier: r.modifier.to_uppercase().as_str().into(),
        app: r.app.as_str().into(),
        pick: r.pick,
        error: slint::SharedString::new(),
    }
}

fn from_slint_rule(r: &crate::RuleEntry) -> data::RuleEntry {
    data::RuleEntry {
        glob: r.glob.to_string(),
        host: r.host.to_string(),
        url: r.url.to_string(),
        modifier: r.modifier.to_lowercase(),
        app: r.app.to_string(),
        pick: r.pick,
    }
}

fn to_slint_app(a: &data::AppEntry) -> crate::AppEntry {
    let args: Vec<slint::SharedString> = a.args.iter().map(|s| s.as_str().into()).collect();
    crate::AppEntry {
        name: a.name.as_str().into(),
        cmd: a.cmd.as_str().into(),
        args: Rc::new(slint::VecModel::from(args)).into(),
        label: a.label.as_str().into(),
        icon: a.icon.clone(),
        name_error: a.name_error.as_str().into(),
        cmd_error: a.cmd_error.as_str().into(),
    }
}

pub fn run() -> Result<()> {
    platform::init_com();
    
    let config_path = crate::config::resolve_config_path()
        .unwrap_or_else(|_| {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            p.join("config.toml")
        });
        
    let config = Config::load(&config_path).unwrap_or_default();
    
    let ui = crate::Settings::new()
        .map_err(|e| crate::error::Error::new(format!("設定画面の起動に失敗しました: {e}")))?;
        
    // ウィンドウハンドルの取得とMicaの適用
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        let handle = ui.window().window_handle();
        if let Ok(raw_window_handle) = handle.window_handle() {
            if let RawWindowHandle::Win32(win32_handle) = raw_window_handle.as_raw() {
                let hwnd = HWND(win32_handle.hwnd.get() as _);
                theme::apply_mica(hwnd);
            }
        }
    }
    
    // システムアクセントカラーの適用
    #[cfg(windows)]
    {
        let (r, g, b) = theme::system_accent_color();
        let accent_color = slint::Color::from_rgb_u8(r, g, b);
        let theme_global = ui.global::<crate::Theme>();
        
        let mut light_colors = theme_global.get_light();
        light_colors.accent_default = slint::Brush::SolidColor(accent_color);
        light_colors.accent_secondary = slint::Brush::SolidColor(slint::Color::from_rgb_u8(
            r.saturating_add(20), g.saturating_add(20), b.saturating_add(20)
        ));
        theme_global.set_light(light_colors);
        
        let mut dark_colors = theme_global.get_dark();
        dark_colors.accent_default = slint::Brush::SolidColor(accent_color);
        dark_colors.accent_secondary = slint::Brush::SolidColor(slint::Color::from_rgb_u8(
            r.saturating_add(20), g.saturating_add(20), b.saturating_add(20)
        ));
        theme_global.set_dark(dark_colors);
    }
    
    // 初期状態のロード
    let apps_state = Rc::new(RefCell::new(AppsState::from_apps(&config.apps)));
    let ext_state = Rc::new(RefCell::new(ExtState::from_ext(&config.ext)));
    let proto_state = Rc::new(RefCell::new(ProtocolState::from_protocol(&config.protocol)));
    
    let config_path_shared = Rc::new(config_path);
    let dirty = Rc::new(Cell::new(false));
    let result_lines = Rc::new(RefCell::new(Vec::<String>::new()));
    let result_success = Rc::new(Cell::new(true));
    
    // UI初期値の設定
    ui.set_config_path(SharedString::from(config_path_shared.to_string_lossy().to_string()));
    ui.set_picker_timeout_ms(config.settings.picker_timeout_ms as i32);
    ui.set_theme_mode(SharedString::from(config.settings.theme_mode.clone()));
    
    // テーマ初期設定
    update_theme_ui(&ui);
    
    // コールバックとイベントのバインド
    let ui_weak = ui.as_weak();
    
    // UIを同期するヘルパーマクロ/クロージャの定義
    let sync_ui = {
        let ui_weak = ui_weak.clone();
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        
        Rc::new(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let apps_state_borrowed = apps_state.borrow();
                let ext_state_borrowed = ext_state.borrow();
                let proto_state_borrowed = proto_state.borrow();

                // (1) アプリ定義の同期
                let slint_apps: Vec<crate::AppEntry> = apps_state_borrowed.model.iter().map(|a| to_slint_app(&a)).collect();
                ui.set_apps(Rc::new(VecModel::from(slint_apps)).into());
                
                let app_icons: Vec<slint::Image> = apps_state_borrowed.model.iter().map(|e| get_app_icon(&e.cmd)).collect();
                ui.set_app_icons(Rc::new(VecModel::from(app_icons)).into());
                let app_names: Vec<SharedString> = apps_state_borrowed.model.iter().map(|e| SharedString::from(e.name.clone())).collect();
                ui.set_app_names(Rc::new(VecModel::from(app_names)).into());
                
                let get_app_icon_by_name = |app_name: &str| -> slint::Image {
                    for i in 0..apps_state_borrowed.model.row_count() {
                        if let Some(app) = apps_state_borrowed.model.row_data(i) {
                            if app.name == app_name {
                                return get_app_icon(&app.cmd);
                            }
                        }
                    }
                    slint::Image::default()
                };

                // (2) 拡張子の同期
                let route_keys: Vec<SharedString> = ext_state_borrowed.route_model.iter().map(|e| SharedString::from(e.key.clone())).collect();
                ui.set_route_keys(Rc::new(VecModel::from(route_keys)).into());
                
                let route_defaults: Vec<SharedString> = ext_state_borrowed.route_model.iter().map(|e| SharedString::from(e.default.clone())).collect();
                ui.set_route_defaults(Rc::new(VecModel::from(route_defaults)).into());
                
                let route_icons: Vec<slint::Image> = ext_state_borrowed.route_model.iter().map(|e| {
                    get_app_icon_by_name(&e.default)
                }).collect();
                ui.set_route_icons(Rc::new(VecModel::from(route_icons)).into());
                
                let sel_ext = ui.get_selected_route();
                if sel_ext >= 0 && (sel_ext as usize) < ext_state_borrowed.route_model.row_count() {
                    let route = ext_state_borrowed.route_model.row_data(sel_ext as usize).unwrap();
                    let candidates: Vec<SharedString> = route.candidates.iter().map(|s| SharedString::from(s.clone())).collect();
                    ui.set_candidates(Rc::new(VecModel::from(candidates)).into());
                    
                    let cand_icons: Vec<slint::Image> = route.candidates.iter().map(|s| {
                        get_app_icon_by_name(s)
                    }).collect();
                    ui.set_candidates_icons(Rc::new(VecModel::from(cand_icons)).into());
                    
                    let rules_model = ext_state_borrowed.rules_models.borrow().get(&route.key).cloned()
                        .unwrap_or_else(|| Rc::new(VecModel::from(vec![])));
                    let rules: Vec<crate::RuleEntry> = rules_model.iter().map(|r| to_slint_rule(&r)).collect();
                    ui.set_rules(Rc::new(VecModel::from(rules)).into());
                    ui.set_current_default(SharedString::from(route.default.clone()));
                    ui.set_current_default_icon(get_app_icon_by_name(&route.default));
                } else {
                    ui.set_candidates(Rc::new(VecModel::from(vec![])).into());
                    ui.set_candidates_icons(Rc::new(VecModel::from(vec![])).into());
                    ui.set_rules(Rc::new(VecModel::from(vec![])).into());
                    ui.set_current_default(SharedString::from(""));
                    ui.set_current_default_icon(slint::Image::default());
                }
                
                // (3) プロトコルの同期
                let proto_keys: Vec<SharedString> = proto_state_borrowed.route_model.iter().map(|e| SharedString::from(e.key.clone())).collect();
                ui.set_proto_route_keys(Rc::new(VecModel::from(proto_keys)).into());
                
                let proto_defaults: Vec<SharedString> = proto_state_borrowed.route_model.iter().map(|e| SharedString::from(e.default.clone())).collect();
                ui.set_proto_defaults(Rc::new(VecModel::from(proto_defaults)).into());
                
                let proto_icons: Vec<slint::Image> = proto_state_borrowed.route_model.iter().map(|e| {
                    get_app_icon_by_name(&e.default)
                }).collect();
                ui.set_proto_icons(Rc::new(VecModel::from(proto_icons)).into());
                
                let sel_proto = ui.get_selected_proto();
                if sel_proto >= 0 && (sel_proto as usize) < proto_state_borrowed.route_model.row_count() {
                    let route = proto_state_borrowed.route_model.row_data(sel_proto as usize).unwrap();
                    let candidates: Vec<SharedString> = route.candidates.iter().map(|s| SharedString::from(s.clone())).collect();
                    ui.set_proto_candidates(Rc::new(VecModel::from(candidates)).into());
                    
                    let cand_icons: Vec<slint::Image> = route.candidates.iter().map(|s| {
                        get_app_icon_by_name(s)
                    }).collect();
                    ui.set_proto_candidates_icons(Rc::new(VecModel::from(cand_icons)).into());
                    
                    let rules_model = proto_state_borrowed.rules_models.borrow().get(&route.key).cloned()
                        .unwrap_or_else(|| Rc::new(VecModel::from(vec![])));
                    let rules: Vec<crate::RuleEntry> = rules_model.iter().map(|r| to_slint_rule(&r)).collect();
                    ui.set_proto_rules(Rc::new(VecModel::from(rules)).into());
                    ui.set_proto_current_default(SharedString::from(route.default.clone()));
                    ui.set_proto_current_default_icon(get_app_icon_by_name(&route.default));
                } else {
                    ui.set_proto_candidates(Rc::new(VecModel::from(vec![])).into());
                    ui.set_proto_candidates_icons(Rc::new(VecModel::from(vec![])).into());
                    ui.set_proto_rules(Rc::new(VecModel::from(vec![])).into());
                    ui.set_proto_current_default(SharedString::from(""));
                    ui.set_proto_current_default_icon(slint::Image::default());
                }
                
                // (4) 管理の同期
                let r_lines: Vec<SharedString> = result_lines.borrow().iter().map(|s| SharedString::from(s.clone())).collect();
                ui.set_result_lines(Rc::new(VecModel::from(r_lines)).into());
                
                let r_text = result_lines.borrow().join("\n");
                ui.set_result_text(SharedString::from(r_text));
                
                ui.set_result_success(result_success.get());
                
                // 全体バリデーションの実行とエラー表示の同期
                let current_config = Config {
                    apps: apps_state_borrowed.to_config(),
                    ext: ext_state_borrowed.to_config(),
                    protocol: proto_state_borrowed.to_config(),
                    settings: crate::config::Settings {
                        picker_timeout_ms: ui.get_picker_timeout_ms() as u64,
                        theme_mode: ui.get_theme_mode().to_string(),
                    },
                };
                let errors = validation::validate(&current_config);
                
                // アプリ定義のエラー設定
                for i in 0..apps_state_borrowed.model.row_count() {
                    let mut app = apps_state_borrowed.model.row_data(i).unwrap();
                    let old_name_err = app.name_error.clone();
                    let old_cmd_err = app.cmd_error.clone();
                    app.name_error = String::new();
                    app.cmd_error = String::new();
                    for err in &errors {
                        match err {
                            validation::ValidationError::InvalidAppName(name) if name == &app.name => {
                                app.name_error = "英数字と_のみ使用できます".into();
                            }
                            validation::ValidationError::DuplicateAppName(name) if name == &app.name => {
                                app.name_error = "重複するアプリ名です".into();
                            }
                            validation::ValidationError::EmptyCmd(name) if name == &app.name => {
                                app.cmd_error = "実行ファイルを入力してください".into();
                            }
                            _ => {}
                        }
                    }
                    if app.name_error != old_name_err || app.cmd_error != old_cmd_err {
                        apps_state_borrowed.model.set_row_data(i, app);
                    }
                }
                
                // 拡張子のエラー設定
                for i in 0..ext_state_borrowed.route_model.row_count() {
                    let mut route = ext_state_borrowed.route_model.row_data(i).unwrap();
                    let old_def_err = route.default_error.clone();
                    let old_cand_err = route.candidates_error.clone();
                    route.default_error = String::new();
                    route.candidates_error = String::new();
                    for err in &errors {
                        match err {
                            validation::ValidationError::UnknownDefaultApp { section, key, app }
                                if section == "ext" && key == &route.key =>
                            {
                                route.default_error = format!("未定義のアプリ: {app}");
                            }
                            validation::ValidationError::UnknownCandidate { section, key, app }
                                if section == "ext" && key == &route.key =>
                            {
                                route.candidates_error = format!("候補に未定義のアプリ: {app}");
                            }
                            validation::ValidationError::EmptyCandidates(key) if key == &route.key => {
                                route.candidates_error = "候補リストが空です".into();
                            }
                            _ => {}
                        }
                    }
                    if route.default_error != old_def_err || route.candidates_error != old_cand_err {
                        ext_state_borrowed.route_model.set_row_data(i, route);
                    }
                }
                
                // プロトコルのエラー設定
                for i in 0..proto_state_borrowed.route_model.row_count() {
                    let mut route = proto_state_borrowed.route_model.row_data(i).unwrap();
                    let old_def_err = route.default_error.clone();
                    let old_cand_err = route.candidates_error.clone();
                    route.default_error = String::new();
                    route.candidates_error = String::new();
                    for err in &errors {
                        match err {
                            validation::ValidationError::UnknownDefaultApp { section, key, app }
                                if section == "protocol" && key == &route.key =>
                            {
                                route.default_error = format!("未定義のアプリ: {app}");
                            }
                            validation::ValidationError::UnknownCandidate { section, key, app }
                                if section == "protocol" && key == &route.key =>
                            {
                                route.candidates_error = format!("候補に未定義のアプリ: {app}");
                            }
                            validation::ValidationError::EmptyCandidates(key) if key == &route.key => {
                                route.candidates_error = "候補リストが空です".into();
                            }
                            _ => {}
                        }
                    }
                    if route.default_error != old_def_err || route.candidates_error != old_cand_err {
                        proto_state_borrowed.route_model.set_row_data(i, route);
                    }
                }
                
                // 現在選択されているルートのエラー表示を更新
                if sel_ext >= 0 && (sel_ext as usize) < ext_state_borrowed.route_model.row_count() {
                    let route = ext_state_borrowed.route_model.row_data(sel_ext as usize).unwrap();
                    ui.set_default_error(SharedString::from(route.default_error));
                    ui.set_candidates_error(SharedString::from(route.candidates_error));
                } else {
                    ui.set_default_error(SharedString::from(""));
                    ui.set_candidates_error(SharedString::from(""));
                }
                
                if sel_proto >= 0 && (sel_proto as usize) < proto_state_borrowed.route_model.row_count() {
                    let route = proto_state_borrowed.route_model.row_data(sel_proto as usize).unwrap();
                    ui.set_proto_default_error(SharedString::from(route.default_error));
                    ui.set_proto_candidates_error(SharedString::from(route.candidates_error));
                } else {
                    ui.set_proto_default_error(SharedString::from(""));
                    ui.set_proto_candidates_error(SharedString::from(""));
                }
                
                ui.set_dirty(dirty.get());
            }
        })
    };
    
    // (1) 全体の操作コールバック
    {
        let ui_weak = ui_weak.clone();
        let dirty = dirty.clone();
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let config_path = config_path_shared.clone();
        let sync_ui = sync_ui.clone();
        
        ui.on_refresh_clicked(move || {
            let config = Config::load(&config_path).unwrap_or_default();
            *apps_state.borrow_mut() = AppsState::from_apps(&config.apps);
            *ext_state.borrow_mut() = ExtState::from_ext(&config.ext);
            *proto_state.borrow_mut() = ProtocolState::from_protocol(&config.protocol);
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_picker_timeout_ms(config.settings.picker_timeout_ms as i32);
                ui.set_theme_mode(SharedString::from(config.settings.theme_mode.clone()));
                update_theme_ui(&ui);
            }
            dirty.set(false);
            sync_ui();
        });
    }
    
    {
        let ui_weak = ui_weak.clone();
        let dirty = dirty.clone();
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let config_path = config_path_shared.clone();
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        
        ui.on_save_clicked(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let current_config = Config {
                    apps: apps_state.borrow().to_config(),
                    ext: ext_state.borrow().to_config(),
                    protocol: proto_state.borrow().to_config(),
                    settings: crate::config::Settings {
                        picker_timeout_ms: ui.get_picker_timeout_ms() as u64,
                        theme_mode: ui.get_theme_mode().to_string(),
                    },
                };
                
                // バリデーションチェック
                let errors = validation::validate(&current_config);
                if !errors.is_empty() {
                    let mut lines = vec!["✗ 保存できません。入力エラーがあります。".to_string()];
                    for err in &errors {
                        lines.push(format!("・{err:?}"));
                    }
                    *result_lines.borrow_mut() = lines;
                    result_success.set(false);
                    sync_ui();
                    return;
                }
                
                match current_config.save(&config_path) {
                    Ok(()) => {
                        *result_lines.borrow_mut() = vec!["✓ 設定ファイルを保存しました".into()];
                        result_success.set(true);
                        dirty.set(false);
                    }
                    Err(e) => {
                        *result_lines.borrow_mut() = vec![format!("✗ 保存失敗: {e}")];
                        result_success.set(false);
                    }
                }
                sync_ui();
            }
        });
    }
    
    {
        let ui_weak = ui_weak.clone();
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let config_path = config_path_shared.clone();
        let dirty = dirty.clone();
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        
        let apply_handler = move || {
            if let Some(ui) = ui_weak.upgrade() {
                let current_config = Config {
                    apps: apps_state.borrow().to_config(),
                    ext: ext_state.borrow().to_config(),
                    protocol: proto_state.borrow().to_config(),
                    settings: crate::config::Settings {
                        picker_timeout_ms: ui.get_picker_timeout_ms() as u64,
                        theme_mode: ui.get_theme_mode().to_string(),
                    },
                };
                
                // バリデーションチェック
                let errors = validation::validate(&current_config);
                if !errors.is_empty() {
                    let mut lines = vec!["✗ 適用できません。入力エラーがあります。".to_string()];
                    for err in &errors {
                        lines.push(format!("・{err:?}"));
                    }
                    *result_lines.borrow_mut() = lines;
                    result_success.set(false);
                    sync_ui();
                    return;
                }
                
                // 保存を実行
                match current_config.save(&config_path) {
                    Ok(()) => {
                        // 保存成功したら、システム適用を実行
                        let res = management::run_apply(&current_config);
                        let mut lines = vec!["✓ 設定ファイルを保存しました".to_string()];
                        lines.extend(res.lines);
                        *result_lines.borrow_mut() = lines;
                        result_success.set(res.success);
                        if res.success {
                            dirty.set(false);
                        }
                    }
                    Err(e) => {
                        *result_lines.borrow_mut() = vec![format!("✗ 保存失敗: {e}")];
                        result_success.set(false);
                    }
                }
                sync_ui();
            }
        };
        ui.on_apply_clicked(apply_handler.clone());
        ui.on_management_apply_clicked(apply_handler);
    }
    
    {
        let ui_weak = ui_weak.clone();
        let sync_ui = sync_ui.clone();
        ui.on_theme_clicked(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mode = ui.get_theme_mode().to_string();
                let next = match mode.as_str() {
                    "system" => "light",
                    "light" => "dark",
                    _ => "system",
                };
                ui.set_theme_mode(SharedString::from(next));
                update_theme_ui(&ui);
                sync_ui();
            }
        });
    }
    
    // (2) アプリ操作コールバック
    {
        let apps_state = apps_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_app(move || {
            apps_state.borrow().add("new_app".into());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_delete_app(move |idx| {
            apps_state.borrow().remove(idx as usize);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_duplicate_app(move |idx| {
            let apps_state_borrowed = apps_state.borrow();
            if let Some(original) = apps_state_borrowed.model.row_data(idx as usize) {
                let dup_name = format!("{}_copy", original.name);
                apps_state_borrowed.add(dup_name.clone());
                let count = apps_state_borrowed.model.row_count();
                apps_state_borrowed.update(count - 1, data::AppEntry {
                    name: dup_name,
                    cmd: original.cmd.clone(),
                    args: original.args.clone(),
                    label: original.label.clone(),
                    icon: original.icon.clone(),
                    name_error: String::new(),
                    cmd_error: String::new(),
                });
                dirty.set(true);
                drop(apps_state_borrowed);
                sync_ui();
            }
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_app(move |idx, name, cmd, args, label| {
            let entry = data::AppEntry {
                name: name.to_string(),
                cmd: cmd.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
                label: label.to_string(),
                icon: get_app_icon(&cmd),
                name_error: String::new(),
                cmd_error: String::new(),
            };
            apps_state.borrow().update(idx as usize, entry);
            dirty.set(true);
            sync_ui();
        });
    }
    
    // (3) 拡張子操作コールバック
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_route(move |key| {
            ext_state.borrow().add_route(key.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_route(move |idx| {
            ext_state.borrow().remove_route(idx as usize);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let sync_ui = sync_ui.clone();
        ui.on_select_route(move |_| {
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_rule(move |key| {
            ext_state.borrow().add_rule(&key);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_rule(move |key, idx| {
            ext_state.borrow().remove_rule(&key, idx as usize);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_move_rule(move |key, idx, delta| {
            ext_state.borrow().move_rule(&key, idx as usize, delta);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_candidate(move |key, cand| {
            ext_state.borrow().add_candidate(&key, cand.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_candidate(move |key, cand| {
            ext_state.borrow().remove_candidate(&key, cand.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_default(move |key, def| {
            ext_state.borrow().update_default(&key, def.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let ext_state = ext_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_rule(move |key, idx, entry| {
            ext_state.borrow().update_rule(&key, idx as usize, from_slint_rule(&entry));
            dirty.set(true);
            sync_ui();
        });
    }
    
    // (4) プロトコル操作コールバック
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_protocol(move |key| {
            proto_state.borrow().add_route(key.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_protocol(move |idx| {
            proto_state.borrow().remove_route(idx as usize);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let sync_ui = sync_ui.clone();
        ui.on_select_protocol(move |_| {
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_proto_rule(move |key| {
            proto_state.borrow().add_rule(&key);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_proto_rule(move |key, idx| {
            proto_state.borrow().remove_rule(&key, idx as usize);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_move_proto_rule(move |key, idx, delta| {
            proto_state.borrow().move_rule(&key, idx as usize, delta);
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_add_proto_candidate(move |key, cand| {
            proto_state.borrow().add_candidate(&key, cand.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_remove_proto_candidate(move |key, cand| {
            proto_state.borrow().remove_candidate(&key, cand.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_proto_default(move |key, def| {
            proto_state.borrow().update_default(&key, def.to_string());
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let proto_state = proto_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_proto_rule(move |key, idx, entry| {
            proto_state.borrow().update_rule(&key, idx as usize, from_slint_rule(&entry));
            dirty.set(true);
            sync_ui();
        });
    }
    
    // (5) 管理操作コールバック
    {
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_picker_timeout(move |_timeout| {
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_update_theme_mode(move |_mode| {
            dirty.set(true);
            sync_ui();
        });
    }
    
    {
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        ui.on_management_unregister_clicked(move || {
            let config = Config::default_config();
            let res = management::run_unregister(&config);
            *result_lines.borrow_mut() = res.lines;
            result_success.set(res.success);
            sync_ui();
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let config_path = config_path_shared.clone();
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        ui.on_management_doctor_clicked(move || {
            let current_config = Config {
                apps: apps_state.borrow().to_config(),
                ext: ext_state.borrow().to_config(),
                protocol: proto_state.borrow().to_config(),
                settings: crate::config::Settings::default(),
            };
            let res = management::run_doctor(&current_config, &config_path);
            *result_lines.borrow_mut() = res.lines;
            result_success.set(res.success);
            sync_ui();
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let ext_state = ext_state.clone();
        let proto_state = proto_state.clone();
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        ui.on_management_backup_clicked(move || {
            let current_config = Config {
                apps: apps_state.borrow().to_config(),
                ext: ext_state.borrow().to_config(),
                protocol: proto_state.borrow().to_config(),
                settings: crate::config::Settings::default(),
            };
            let res = management::run_backup(&current_config);
            *result_lines.borrow_mut() = res.lines;
            result_success.set(res.success);
            sync_ui();
        });
    }
    
    {
        let result_lines = result_lines.clone();
        let result_success = result_success.clone();
        let sync_ui = sync_ui.clone();
        ui.on_management_restore_clicked(move || {
            let res = management::run_restore(None);
            *result_lines.borrow_mut() = res.lines;
            result_success.set(res.success);
            sync_ui();
        });
    }
    
    {
        let apps_state = apps_state.clone();
        let dirty = dirty.clone();
        let sync_ui = sync_ui.clone();
        ui.on_browse_app_cmd(move |idx| {
            if let Some(path) = select_executable_dialog() {
                let file_stem = std::path::Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("app");
                let app_name = sanitize_app_name(file_stem);
                
                let apps_state_borrowed = apps_state.borrow();
                if let Some(mut entry) = apps_state_borrowed.model.row_data(idx as usize) {
                    entry.cmd = path.clone();
                    entry.name = app_name;
                    entry.icon = get_app_icon(&path);
                    apps_state_borrowed.update(idx as usize, entry);
                    dirty.set(true);
                    drop(apps_state_borrowed);
                    sync_ui();
                }
            }
        });
    }
    
    // UI同期の初回実行
    sync_ui();
    
    ui.run().map_err(|e| crate::error::Error::new(format!("設定画面の実行に失敗しました: {e}")))?;
    Ok(())
}

fn sanitize_app_name(file_name: &str) -> String {
    let mut name = String::new();
    for c in file_name.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if c == '_' || c == '-' || c == ' ' {
            name.push('_');
        }
    }
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name.trim_matches('_').to_string()
}

fn select_executable_dialog() -> Option<String> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        dialog.Show(None).ok()?;
        let result = dialog.GetResult().ok()?;
        let path = result.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path_str = path.to_string().ok()?;
        Some(path_str)
    }
}

fn update_theme_ui(ui: &crate::Settings) {
    let mode_str = ui.get_theme_mode().to_string();
    let os_dark = theme::os_prefers_dark_theme();
    let theme_global = ui.global::<crate::Theme>();
    
    match mode_str.as_str() {
        "light" => {
            theme_global.set_mode(crate::ThemeMode::Light);
        }
        "dark" => {
            theme_global.set_mode(crate::ThemeMode::Dark);
        }
        _ => {
            theme_global.set_mode(crate::ThemeMode::System);
        }
    }
    
    theme_global.invoke_refresh(os_dark);
    
    // アイコンの同期
    let icon_global = ui.global::<crate::FluentIcon>();
    let resolved_mode = if mode_str == "system" {
        if os_dark { "dark" } else { "light" }
    } else {
        mode_str.as_str()
    };
    
    if resolved_mode == "dark" {
        ui.set_theme_icon(icon_global.get_dark());
    } else {
        ui.set_theme_icon(icon_global.get_light());
    }
}
