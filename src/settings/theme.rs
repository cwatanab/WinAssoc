//! OS テーマ統合: DWM Mica / アクセント / OS ライト・ダーク判定

#![cfg(windows)]

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};

/// アクセント未設定時のフォールバック (Win11 Fluent Blue)
pub const DEFAULT_ACCENT: (u8, u8, u8) = (0x00, 0x78, 0xD4);

/// DWM の AccentColor DWORD (AABBGGRR) を RGB タプルに展開
pub fn parse_accent_color(v: u32) -> (u8, u8, u8) {
    let r = (v & 0xFF) as u8;
    let g = ((v >> 8) & 0xFF) as u8;
    let b = ((v >> 16) & 0xFF) as u8;
    (r, g, b)
}

/// システム アクセント カラーを取得。失敗時は DEFAULT_ACCENT
pub fn system_accent_color() -> (u8, u8, u8) {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\DWM")
        .and_then(|k| k.get_value::<u32, _>("AccentColor"))
        .map(parse_accent_color)
        .unwrap_or(DEFAULT_ACCENT)
}

/// OS の AppsUseLightTheme を読む。取得失敗時は false (ダーク扱い)
pub fn os_prefers_dark_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|light| light == 0)
        .unwrap_or(false)
}

/// Mica バックドロップと角丸をウィンドウに適用。失敗時は静かに無視 (Win10 等)
pub fn apply_mica(hwnd: HWND) {
    let backdrop: i32 = 2; // DWMSBT_MAINMICA
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(38), // DWMWA_SYSTEMBACKDROP_TYPE
            &backdrop as *const _ as _,
            std::mem::size_of_val(&backdrop) as u32,
        );
        let corner: i32 = 0; // DWMWCP_DEFAULT
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33), // DWMWA_WINDOW_CORNER_PREFERENCE
            &corner as *const _ as _,
            4,
        );
        let caption: u32 = 0x00FFFFFF; // 透過
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(35), // DWMWA_CAPTION_COLOR
            &caption as *const _ as _,
            std::mem::size_of_val(&caption) as u32,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accent_color_bgr_layout() {
        let (r, g, b) = parse_accent_color(0x00FF8040);
        assert_eq!((r, g, b), (0x40, 0x80, 0xFF));
    }

    #[test]
    fn parse_accent_color_with_alpha() {
        let (r, g, b) = parse_accent_color(0xFF000000);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn default_accent_is_fluent_blue() {
        assert_eq!(DEFAULT_ACCENT, (0x00, 0x78, 0xD4));
    }
}