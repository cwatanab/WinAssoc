//! ウィンドウ装飾 (DWM) とポップアップ位置計算

use eframe::egui::Pos2;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Mica/Acrylic 背景 + 角丸 + ダークモードを DWM で適用
pub fn apply_window_effects(cc: &eframe::CreationContext<'_>, dark: bool) {
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_SYSTEMBACKDROP_TYPE,
    };

    let Ok(handle) = cc.window_handle() else { return };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else { return };
    let hwnd = HWND(win32.hwnd.get() as *mut std::ffi::c_void);

    unsafe {
        let backdrop = DWM_SYSTEMBACKDROP_TYPE(3);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(),
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&corner as *const _ as *const std::ffi::c_void).cast(),
            std::mem::size_of_val(&corner) as u32,
        );
        let dark_mode = BOOL(dark as i32);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_mode as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// マウスカーソル付近に表示し、モニタのワークエリア内にクランプ。
/// 戻り値は egui の論理ポイント
pub fn popup_position(width_pt: f32, height_pt: f32) -> Pos2 {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut cursor = POINT::default();
        if GetCursorPos(&mut cursor).is_err() {
            return Pos2::new(200.0, 200.0);
        }
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);

        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let scale = (dpi_x as f32 / 96.0).max(0.5);

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let work = if GetMonitorInfoW(monitor, &mut info).as_bool() {
            info.rcWork
        } else {
            windows::Win32::Foundation::RECT { left: 0, top: 0, right: 1920, bottom: 1080 }
        };

        let w = width_pt * scale;
        let h = height_pt * scale;
        let x = (cursor.x as f32 - w / 2.0)
            .clamp(work.left as f32 + 8.0, (work.right as f32 - w - 8.0).max(work.left as f32));
        let y = (cursor.y as f32 + 16.0)
            .clamp(work.top as f32 + 8.0, (work.bottom as f32 - h - 8.0).max(work.top as f32));
        Pos2::new(x / scale, y / scale)
    }
}
