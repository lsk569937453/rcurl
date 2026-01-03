use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::cli::app_config::{Cli, QuickCommand};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    #[serde(skip)]
    pub cli: Option<Cli>,
}

/// 获取用户目录下的 .rcurl 目录路径
pub fn get_history_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".rcurl")
}

/// 获取历史记录文件路径
pub fn get_history_file_path() -> PathBuf {
    get_history_dir().join("rcurl.req")
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
pub fn save_request(command: &str, cli: &Cli) -> Result<()> {
    let dir = get_history_dir();
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

/// 将 Cli 结构转换为命令字符串
pub fn command_from_cli(cli: &Cli) -> String {
    let mut cmd = String::from("rcurl");

    // Handle quick commands (ping, disk, telnet, etc.)
    if let Some(ref quick_cmd) = cli.quick_cmd {
        match quick_cmd {
            QuickCommand::Ping { target } => {
                cmd.push_str(&format!(" p {}", target));
                return cmd;
            }
            QuickCommand::Disk { target } => {
                cmd.push_str(&format!(" d {}", target));
                return cmd;
            }
            QuickCommand::Telnet { host, port } => {
                cmd.push_str(&format!(" t {} {}", host, port));
                return cmd;
            }
            QuickCommand::Ns { domain } => {
                cmd.push_str(&format!(" ns {}", domain));
                return cmd;
            }
        }
    }

    if let Some(ref method) = cli.method_option {
        cmd.push_str(&format!(" -X {}", method));
    }

    if let Some(ref data) = cli.body_option {
        cmd.push_str(&format!(" -d '{}'", data.replace('\'', "'\\''")));
    }

    for form in &cli.form_option {
        cmd.push_str(&format!(" -F '{}'", form.replace('\'', "'\\''")));
    }

    for header in &cli.headers {
        cmd.push_str(&format!(" -H '{}'", header.replace('\'', "'\\''")));
    }

    if let Some(ref cert) = cli.certificate_path_option {
        cmd.push_str(&format!(" -c '{}'", cert.replace('\'', "'\\''")));
    }

    if let Some(ref user) = cli.authority_option {
        cmd.push_str(&format!(" -u '{}'", user.replace('\'', "'\\''")));
    }

    if let Some(ref ua) = cli.user_agent_option {
        cmd.push_str(&format!(" -A '{}'", ua.replace('\'', "'\\''")));
    }

    if let Some(ref cookie) = cli.cookie_option {
        cmd.push_str(&format!(" -b '{}'", cookie.replace('\'', "'\\''")));
    }

    if let Some(ref referer) = cli.refer_option {
        cmd.push_str(&format!(" -e '{}'", referer.replace('\'', "'\\''")));
    }

    if let Some(ref output) = cli.file_path_option {
        cmd.push_str(&format!(" -o '{}'", output.replace('\'', "'\\''")));
    }

    if let Some(ref upload) = cli.uploadfile_option {
        cmd.push_str(&format!(" -T '{}'", upload.replace('\'', "'\\''")));
    }

    if let Some(ref quote) = cli.quote_option {
        cmd.push_str(&format!(" -Q '{}'", quote.replace('\'', "'\\''")));
    }

    if cli.skip_certificate_validate {
        cmd.push_str(" -k");
    }

    if cli.header_option {
        cmd.push_str(" -I");
    }

    if let Some(ref range) = cli.range_option {
        cmd.push_str(&format!(" -r '{}'", range.replace('\'', "'\\''")));
    }

    for _ in 0..cli.verbosity {
        cmd.push_str(" -v");
    }

    if cli.http2 {
        cmd.push_str(" --http2");
    }

    if cli.http2_prior_knowledge {
        cmd.push_str(" --http2-prior-knowledge");
    }

    if let Some(ref url) = cli.url {
        cmd.push(' ');
        cmd.push_str(url);
    }

    cmd
}

// 添加 dirs crate 的小模块
mod dirs {
    use std::env;
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        if cfg!(windows) {
            env::var("USERPROFILE").ok().map(PathBuf::from).or_else(|| {
                env::var("HOMEDRIVE").ok().and_then(|drive| {
                    env::var("HOMEPATH").ok().map(|path| {
                        let mut p = PathBuf::from(drive);
                        p.push(path);
                        p
                    })
                })
            })
        } else {
            env::var("HOME").ok().map(PathBuf::from)
        }
    }
}
