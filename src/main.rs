#![cfg_attr(not(test), windows_subsystem = "windows")]
use std::path::PathBuf;

use winassoc::bail;
use winassoc::error::Result;
use clap::{Arg, Command};

use winassoc::config::{self, Config};
use winassoc::{commands, logging, registry, settings};

fn build_cli() -> Command {
    Command::new("winassoc")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Windows のファイル関連付け/URL をルールベースでルーティングするシム")
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("設定ファイル (既定: winassoc.toml または config.toml)")
                .global(true),
        )
        .subcommand(
            Command::new("open")
                .about("シム本体: ルールを評価して実アプリを起動する (OS のハンドラとして呼ばれる)")
                .arg(Arg::new("target").required(true).help("ファイルパスまたはURL")),
        )
        .subcommand(
            Command::new("test")
                .about("dry-run: どのルールに一致し何が起動されるかを表示する (起動しない)")
                .arg(Arg::new("target").required(true).help("ファイルパスまたはURL"))
                .arg(
                    Arg::new("modifier")
                        .long("modifier")
                        .value_delimiter(',')
                        .help("修飾キー押下を擬似指定 (shift,ctrl,alt をカンマ区切り)"),
                ),
        )
        .subcommand(Command::new("list").about("設定済みルートの一覧"))
        .subcommand(Command::new("apply").about("設定中の拡張子/プロトコルを HKCU に登録する (実行前に自動バックアップ)"))
        .subcommand(Command::new("unregister").about("登録解除 (バックアップがあれば既定 ProgID を復元)"))
        .subcommand(Command::new("doctor").about("設定とレジストリの乖離を診断する"))
        .subcommand(
            Command::new("log")
                .about("起動ログの表示")
                .arg(
                    Arg::new("tail")
                        .long("tail")
                        .default_value("20")
                        .value_parser(clap::value_parser!(usize))
                        .help("末尾から表示する行数"),
                ),
        )
        .subcommand(Command::new("backup").about("現在の関連付け状態をバックアップする"))
        .subcommand(
            Command::new("restore")
                .about("バックアップから関連付けを復元する (省略時: 最新)")
                .arg(Arg::new("file").value_name("FILE")),
        )
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        return settings::run();
    } else {
        #[cfg(windows)]
        unsafe {
            winapi_attach_console();
        }
    }

    let matches = build_cli().get_matches();
    let config_path = match matches.get_one::<String>("config") {
        Some(path) => PathBuf::from(path),
        None => config::resolve_config_path()?,
    };

    if let Some((cmd, sub_m)) = matches.subcommand() {
        match cmd {
            "log" => {
                let tail: usize = *sub_m.get_one::<usize>("tail").unwrap_or(&20);
                return logging::tail(tail);
            }
            "restore" => {
                let file = sub_m.get_one::<String>("file").map(|s| PathBuf::from(s));
                return registry::restore(file.as_deref());
            }
            _ => {}
        }
    }

    let config = Config::load(&config_path)?;

    if let Some((cmd, sub_m)) = matches.subcommand() {
        match cmd {
            "open" => {
                let target = sub_m.get_one::<String>("target").unwrap().as_str();
                commands::open(&config, target)
            }
            "test" => {
                let target = sub_m.get_one::<String>("target").unwrap().as_str();
                let modifier: Vec<String> = sub_m
                    .get_many::<String>("modifier")
                    .map(|v| v.cloned().collect())
                    .unwrap_or_default();
                commands::test(&config, target, &modifier)
            }
            "list" => commands::list(&config),
            "apply" => registry::apply(&config),
            "unregister" => registry::unregister(&config),
            "doctor" => registry::doctor(&config, &config_path),
            "backup" => {
                let path = registry::backup(&config)?;
                println!("バックアップを保存しました: {}", path.display());
                Ok(())
            }
            "log" | "restore" => unreachable!(),
            _ => bail!("不明なサブコマンド: {cmd}"),
        }
    } else {
        bail!("サブコマンドを指定してください (open, test, list, apply, unregister, doctor, log, backup, restore)");
    }
}
 
#[cfg(windows)]
unsafe fn winapi_attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(dwProcessId: u32) -> i32;
        fn GetStdHandle(nStdHandle: u32) -> isize;
    }
    #[link(name = "msvcrt")]
    extern "C" {
        fn _open_osfhandle(osfhandle: isize, flags: i32) -> i32;
        fn _dup2(src: i32, dst: i32) -> i32;
    }
    if AttachConsole(0xFFFFFFFF) != 0 {
        let stdout_handle = GetStdHandle(0xFFFFFFF5);
        if stdout_handle != -1 && stdout_handle != 0 {
            let fd = _open_osfhandle(stdout_handle, 0);
            if fd >= 0 {
                let _ = _dup2(fd, 1);
            }
        }
        let stderr_handle = GetStdHandle(0xFFFFFFF4);
        if stderr_handle != -1 && stderr_handle != 0 {
            let fd = _open_osfhandle(stderr_handle, 0);
            if fd >= 0 {
                let _ = _dup2(fd, 2);
            }
        }
    }
}
