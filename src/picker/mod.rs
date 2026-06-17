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
pub fn show(target_label: String, candidates: Vec<Candidate>, _timeout_ms: u64) -> Result<Option<String>> {
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
    ui.set_candidates(candidate_model.into());
    ui.set_icons(icon_model.into());

    let (cx, cy) = get_cursor_pos();
    ui.window().set_position(slint::PhysicalPosition::new(cx, cy));

    let (tx, rx) = mpsc::channel::<Option<String>>();
    let candidate_names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();

    ui.on_cancel({
        let tx = tx.clone();
        move || {
            let _ = tx.send(None);
        }
    });

    ui.on_choose({
        let tx = tx.clone();
        move |index: i32| {
            let name = candidate_names.get(index as usize).cloned().unwrap_or_default();
            let _ = tx.send(Some(name));
        }
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
