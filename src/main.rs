mod commands;
mod config;
mod engine;

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use config::Config;

#[derive(Parser)]
#[command(name = "winassoc", version, about = "Windows のファイル関連付け/URL をルールベースでルーティングするシム")]
struct Cli {
    /// 設定ファイル (既定: %APPDATA%\winassoc\config.toml)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// シム本体: ルールを評価して実アプリを起動する (OS のハンドラとして呼ばれる)
    Open { target: String },
    /// dry-run: どのルールに一致し何が起動されるかを表示する (起動しない)
    Test {
        target: String,
        /// 修飾キー押下を擬似指定 (shift,ctrl,alt をカンマ区切り)
        #[arg(long, value_delimiter = ',')]
        modifier: Vec<String>,
    },
    /// 設定済みルートと登録状態の一覧
    List,
    /// 設定中の拡張子/プロトコルを HKCU に登録する
    Apply,
    /// 登録解除
    Unregister,
    /// 設定とレジストリの乖離を診断する
    Doctor,
    /// 起動ログの表示
    Log,
    /// 関連付け状態のバックアップ
    Backup,
    /// バックアップからの復元
    Restore,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = match cli.config {
        Some(path) => path,
        None => config::default_config_path()?,
    };
    let config = Config::load(&config_path)?;

    match cli.command {
        Command::Open { target } => commands::open(&config, &target),
        Command::Test { target, modifier } => commands::test(&config, &target, &modifier),
        Command::List => commands::list(&config),
        Command::Apply | Command::Unregister | Command::Doctor => {
            bail!("このコマンドは未実装です (M2: レジストリ登録で対応予定)")
        }
        Command::Log | Command::Backup | Command::Restore => {
            bail!("このコマンドは未実装です (M4: 運用機能で対応予定)")
        }
    }
}
