//! 管理ページ: registry 操作とバックアップの結果ハンドリング

use crate::config::Config;
use crate::registry;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct OperationResult {
    pub lines: Vec<String>,
    pub success: bool,
}

pub fn run_apply(config: &Config) -> OperationResult {
    match registry::apply(config) {
        Ok(()) => OperationResult { lines: vec!["✓ 適用に成功しました".into()], success: true },
        Err(e) => OperationResult { lines: vec![format!("✗ 適用に失敗: {e}")], success: false },
    }
}

pub fn run_unregister(config: &Config) -> OperationResult {
    match registry::unregister(config) {
        Ok(()) => OperationResult { lines: vec!["✓ 解除に成功しました".into()], success: true },
        Err(e) => OperationResult { lines: vec![format!("✗ 解除に失敗: {e}")], success: false },
    }
}

pub fn run_doctor(config: &Config, config_path: &Path) -> OperationResult {
    match registry::doctor(config, config_path) {
        Ok(()) => OperationResult {
            lines: vec!["✓ 診断が完了しました (問題なし)".into()],
            success: true,
        },
        Err(e) => OperationResult { lines: vec![format!("✗ 診断に失敗: {e}")], success: false },
    }
}

pub fn run_backup(config: &Config) -> OperationResult {
    match registry::backup(config) {
        Ok(path) => OperationResult {
            lines: vec![format!("✓ バックアップ保存: {}", path.display())],
            success: true,
        },
        Err(e) => OperationResult {
            lines: vec![format!("✗ バックアップ失敗: {e}")],
            success: false,
        },
    }
}

pub fn run_restore(file: Option<&Path>) -> OperationResult {
    match registry::restore(file) {
        Ok(()) => OperationResult { lines: vec!["✓ 復元に成功しました".into()], success: true },
        Err(e) => OperationResult { lines: vec![format!("✗ 復元に失敗: {e}")], success: false },
    }
}
