use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use winassoc::config::{self, Config};
use winassoc::{commands, logging, registry};

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
    /// 設定済みルートの一覧
    List,
    /// 設定中の拡張子/プロトコルを HKCU に登録する (実行前に自動バックアップ)
    Apply,
    /// 登録解除 (バックアップがあれば既定 ProgID を復元)
    Unregister,
    /// 設定とレジストリの乖離を診断する
    Doctor,
    /// 起動ログの表示
    Log {
        /// 末尾から表示する行数
        #[arg(long, default_value_t = 20)]
        tail: usize,
    },
    /// 現在の関連付け状態をバックアップする
    Backup,
    /// バックアップから関連付けを復元する (省略時: 最新)
    Restore { file: Option<PathBuf> },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = match cli.config {
        Some(path) => path,
        None => config::default_config_path()?,
    };

    // log だけは設定ファイルなしでも動かす
    if let Command::Log { tail } = &cli.command {
        return logging::tail(*tail);
    }
    if let Command::Restore { file } = &cli.command {
        return registry::restore(file.as_deref());
    }

    let config = Config::load(&config_path)?;
    match cli.command {
        Command::Open { target } => commands::open(&config, &target),
        Command::Test { target, modifier } => commands::test(&config, &target, &modifier),
        Command::List => commands::list(&config),
        Command::Apply => registry::apply(&config),
        Command::Unregister => registry::unregister(&config),
        Command::Doctor => registry::doctor(&config, &config_path),
        Command::Backup => {
            let path = registry::backup(&config)?;
            println!("バックアップを保存しました: {}", path.display());
            Ok(())
        }
        Command::Log { .. } | Command::Restore { .. } => unreachable!(),
    }
}
