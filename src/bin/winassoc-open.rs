//! シム本体 (GUI subsystem)。OS のハンドラとして登録される側の exe。
//! コンソールを出さずに起動し、エラーはダイアログとログで通知する (SPEC 6)。
#![windows_subsystem = "windows"]

use winassoc::config::{self, Config};
use winassoc::{commands, logging};

fn main() {
    // 登録コマンドは `"winassoc-open.exe" "%1"` — 第 1 引数が対象
    let Some(target) = std::env::args().nth(1) else {
        commands::show_error_dialog("起動対象が指定されていません");
        std::process::exit(2);
    };

    let result = config::resolve_config_path()
        .and_then(|path| Config::load(&path))
        .and_then(|config| commands::open(&config, &target));

    if let Err(e) = result {
        logging::log_launch(&target, "-", "-", &format!("error: {e:#}"));
        commands::show_error_dialog(&format!("{e:#}"));
        std::process::exit(1);
    }
}
