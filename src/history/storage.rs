use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::types::HistoryEntry;
use super::dirs;

/// 获取历史记录文件路径
pub fn get_history_file_path() -> PathBuf {
    dirs::get_history_dir().join("rcurl.req")
}

/// 加载历史记录（返回命令字符串列表用于显示）
pub fn load_history() -> Result<Vec<String>> {
    let path = get_history_file_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let history: Vec<HistoryEntry> = serde_json::from_str(&content).unwrap_or_default();
    Ok(history.iter().map(|h| h.command.clone()).collect())
}

/// 加载历史记录（返回完整 HistoryEntry 列表）
pub fn load_history_entries() -> Result<Vec<HistoryEntry>> {
    let path = get_history_file_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let history: Vec<HistoryEntry> = serde_json::from_str(&content).unwrap_or_default();
    Ok(history)
}

/// 保存请求到历史记录（去重，最新放最前）
pub fn save_request(command: &str, cli: &crate::cli::app_config::Cli) -> Result<()> {
    let dir = dirs::get_history_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }

    let path = get_history_file_path();
    let mut history = load_history_entries().unwrap_or_default();

    // 去重：如果已存在相同命令，先删除
    history.retain(|h| h.command != command);

    // 将新命令插入到最前面
    let entry = HistoryEntry {
        command: command.to_string(),
        cli: Some(cli.clone()),
    };
    history.insert(0, entry);

    // 写入文件（只保存 command，cli 不保存到文件以避免序列化问题）
    let save_history: Vec<HistoryEntry> = history
        .iter()
        .map(|h| HistoryEntry {
            command: h.command.clone(),
            cli: None,
        })
        .collect();
    let json = serde_json::to_string_pretty(&save_history)?;
    fs::write(&path, json)?;

    Ok(())
}
