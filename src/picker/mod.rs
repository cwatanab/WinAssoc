//! ランチャー画面 — Slint 版
//!
//! アイコン横並びグリッド。キーボード操作対応。
//! マウスカーソル付近にポップアップする。

use std::sync::mpsc;

use crate::error::{Error, Result};
use crate::{icon, platform};

use slint::ComponentHandle;
use crate::CandidateData as SlintCandidateData;

const ICON_SIZE: i32 = 64;

pub struct Candidate {
    pub name: String,
    pub label: Option<String>,
    pub program: String,
}

/// ピッカーを表示し、選ばれたアプリ名を返す (None = キャンセル)
pub fn show(target_label: String, candidates: Vec<Candidate>, timeout_ms: u64) -> Result<Option<String>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    platform::init_com();

    let mut icons: Vec<slint::Image> = Vec::new();
    for c in &candidates {
        match icon::extract_icon_rgba(&c.program, ICON_SIZE) {
            Some(img) => {
                let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                    &img.pixels, img.width as u32, img.height as u32,
                );
                icons.push(slint::Image::from_rgba8(buffer));
            }
            None => {
                icons.push(slint::Image::default());
            }
        }
    }

    let candidate_data: Vec<SlintCandidateData> = candidates.iter().map(|c| {
        SlintCandidateData {
            name: c.name.clone().into(),
            label: c.label.clone().unwrap_or_default().into(),
        }
    }).collect();

    let ui = crate::Picker::new()
        .map_err(|e| Error::new(format!("ピッカーの起動に失敗しました: {e}")))?;

    let candidate_model = std::rc::Rc::new(slint::VecModel::from(candidate_data));
    let icon_model = std::rc::Rc::new(slint::VecModel::from(icons));

    ui.set_target_label(target_label.into());
    ui.set_candidates(candidate_model.clone().into());
    ui.set_icons(icon_model.clone().into());

    if platform::prefers_dark_theme() {
        ui.set_panel_color(slint::Brush::from(slint::Color::from_argb_encoded(0xd7181a20)));
        ui.set_text_color(slint::Brush::from(slint::Color::from_argb_encoded(0xffebebeb)));
        ui.set_subtle_color(slint::Brush::from(slint::Color::from_argb_encoded(0xffa0a0a0)));
        ui.set_accent_color(slint::Brush::from(slint::Color::from_argb_encoded(0xff60a5fa)));
    } else {
        ui.set_panel_color(slint::Brush::from(slint::Color::from_argb_encoded(0xf0ffffff)));
        ui.set_text_color(slint::Brush::from(slint::Color::from_argb_encoded(0xff1a1a1a)));
        ui.set_subtle_color(slint::Brush::from(slint::Color::from_argb_encoded(0xff666666)));
        ui.set_accent_color(slint::Brush::from(slint::Color::from_argb_encoded(0xff2563eb)));
    }

    let (cx, cy) = get_cursor_pos();
    let monitor_scale = get_monitor_scale(cx, cy);
    ui.window().set_position(slint::PhysicalPosition::new(
        (cx as f64 / monitor_scale) as i32 - 150,
        (cy as f64 / monitor_scale) as i32 + 20,
    ));

    let (tx, rx) = mpsc::channel::<Option<String>>();
    let candidate_names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();

    ui.on_cancel({
        let tx = tx.clone();
        move || {
            let _ = tx.send(None);
            let _ = slint::quit_event_loop();
        }
    });

    ui.on_choose({
        let tx = tx.clone();
        move |index: i32| {
            let name = candidate_names.get(index as usize).cloned().unwrap_or_default();
            let _ = tx.send(Some(name));
            let _ = slint::quit_event_loop();
        }
    });

    let tx_timeout = tx.clone();
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(timeout_ms), move || {
        let _ = tx_timeout.send(None);
        let _ = slint::quit_event_loop();
    });

    ui.run()
        .map_err(|e| Error::new(format!("ピッカーの実行に失敗しました: {e}")))?;

    Ok(rx.recv().ok().flatten())
}

fn get_cursor_pos() -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_ok() {
            (cursor.x, cursor.y)
        } else {
            (200, 200)
        }
    }
}

fn get_monitor_scale(x: i32, y: i32) -> f64 {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        (dpi_x as f64 / 96.0).max(0.5)
    }
}
