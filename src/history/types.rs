use serde::{Deserialize, Serialize};
use crate::cli::app_config::Cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    #[serde(skip)]
    pub cli: Option<Cli>,
}

impl HistoryEntry {
    /// 获取用于显示的命令，如果超过指定长度（按字符计）则截取
    ///
    /// 截取时保留首尾两段并在中间用省略号连接，便于在 TUI 中同时看到
    /// 命令开头（rcurl 子命令）和结尾（URL）。这里按字符而非字节切片，
    /// 避免把多字节字符（中文、emoji 等）拆开导致 panic。
    pub fn display_command(&self, max_length: usize) -> String {
        if max_length <= 3 {
            return "...".to_string();
        }

        let chars: Vec<char> = self.command.chars().collect();
        if chars.len() <= max_length {
            return self.command.clone();
        }

        // 截取前半部分和后半部分，中间用省略号连接
        let half_length = (max_length - 3) / 2;
        let first_part: String = chars[..half_length].iter().collect();
        let second_start = chars.len() - half_length;
        let second_part: String = chars[second_start..].iter().collect();

        format!("{}...{}", first_part, second_part)
    }

    /// 获取用于终端显示的命令，如果超过指定长度则截取
    /// 这个方法专门用于 TUI 界面
    pub fn display_terminal_command(&self, max_length: usize) -> String {
        self.display_command(max_length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(command: &str) -> HistoryEntry {
        HistoryEntry {
            command: command.to_string(),
            cli: None,
        }
    }

    #[test]
    fn short_commands_are_not_truncated() {
        let display = entry("rcurl https://example.com").display_command(100);
        assert_eq!(display, "rcurl https://example.com");
    }

    #[test]
    fn long_commands_keep_both_ends() {
        let command = format!("rcurl https://example.com/{}", "a".repeat(200));
        let display = entry(&command).display_command(60);

        assert!(display.starts_with("rcurl https://example.com/"));
        assert!(display.ends_with("aaaaaaaaaa"));
        assert!(display.contains("..."));
        assert!(display.chars().count() <= 60);
    }

    #[test]
    fn multibyte_commands_are_truncated_on_char_boundaries() {
        // 150 个中文字符远超 100 的限制，按字节切片会切开字符并 panic。
        let command = format!("rcurl https://例子.中国/{}", "中文路径".repeat(40));
        let display = entry(&command).display_command(100);

        let command_chars: Vec<char> = command.chars().collect();
        assert!(display.chars().count() <= 100);
        assert!(display.contains("..."));
        assert_ne!(display, command);

        // 首尾都保留自原命令，说明截取没有拆坏多字节字符。
        let head: String = command_chars[..20].iter().collect();
        let tail: String = command_chars[command_chars.len() - 20..].iter().collect();
        assert!(display.starts_with(&head));
        assert!(display.ends_with(&tail));
    }

    #[test]
    fn tiny_max_length_does_not_underflow() {
        let command = entry("rcurl https://example.com");

        assert_eq!(command.display_command(0), "...");
        assert_eq!(command.display_command(3), "...");
        assert_eq!(command.display_command(4), "...");
        assert_eq!(entry("rcurl").display_command(9), "rcurl");
    }
}
