use serde::{Deserialize, Serialize};
use crate::cli::app_config::Cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    #[serde(skip)]
    pub cli: Option<Cli>,
}

impl HistoryEntry {
    /// 获取用于显示的命令，如果超过指定长度则截取
    pub fn display_command(&self, max_length: usize) -> String {
        if self.command.len() <= max_length {
            return self.command.clone();
        }

        // 截取前半部分和后半部分，中间用省略号连接
        let half_length = (max_length - 3) / 2;
        let first_part = &self.command[..half_length];
        let second_start = self.command.len() - half_length;
        let second_part = &self.command[second_start..];

        format!("{}...{}", first_part, second_part)
    }

    /// 获取用于终端显示的命令，如果超过指定长度则截取
    /// 这个方法专门用于 TUI 界面
    pub fn display_terminal_command(&self, max_length: usize) -> String {
        self.display_command(max_length)
    }
}
