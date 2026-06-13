//! OS 依存処理の集約レイヤー
//!
//! COM 初期化・修飾キー取得・エラーダイアログ・OS テーマ判定

use crate::engine::Modifiers;

/// プロセスごとに一度だけ COM を初期化する
pub fn init_com() {
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }
}

/// 起動時点の修飾キー押下状態 (GetAsyncKeyState)
#[cfg(windows)]
pub fn current_modifiers() -> Modifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
    let down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };
    Modifiers {
        shift: down(VK_SHIFT.0 as i32),
        ctrl: down(VK_CONTROL.0 as i32),
        alt: down(VK_MENU.0 as i32),
    }
}

#[cfg(not(windows))]
pub fn current_modifiers() -> Modifiers {
    Modifiers::default()
}

/// シェル起動時 (コンソールなし) のエラー通知ダイアログ
#[cfg(windows)]
pub fn show_error_dialog(message: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from("winassoc"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
pub fn show_error_dialog(message: &str) {
    eprintln!("{message}");
}

/// OS のアプリテーマ設定 (AppsUseLightTheme) を読む
pub fn prefers_dark_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|light| light == 0)
        .unwrap_or(false)
}
