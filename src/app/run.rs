use crate::cli::app_config::{Cli, QuickCommand};
use crate::count::handler::count_lines_command;
use crate::disk::handler::disk_size_command;
use crate::dns::handler::dns_command;
use crate::ftp::handler::ftp_request;
use crate::git::handler::git_statistic_command;
use crate::history::command::command_from_cli;
use crate::history::storage::load_history_entries;
use crate::history::storage::save_request;
use crate::http::handler::http_request_with_redirects;
use crate::ping::handler::ping_command;
use crate::port::handler::{port_find_command, port_kill_command, port_list_command};
use crate::response::res::RcurlResponse;
use crate::telnet::handler::telnet_command;
use crate::tui::history_selector::HistorySelector;
use crate::whois::handler::whois_command;
use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;
pub async fn main_with_error() -> Result<RcurlResponse, anyhow::Error> {
    let cli: Cli = Cli::parse();

    do_request(cli).await
}

async fn do_request(cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    // 如果没有 URL 且没有命令，进入交互模式
    if cli.url.is_none() && cli.quick_cmd.is_none() {
        return interactive_mode().await;
    }

    // 初始化日志
    init_logging(cli.verbosity);

    // 保存请求到历史
    let command = command_from_cli(&cli);
    if let Err(e) = save_request(&command, &cli) {
        eprintln!("Warning: Failed to save request history: {}", e);
    }

    // 执行请求
    execute_request(cli).await
}

fn init_logging(verbosity: u8) {
    let log_level = match verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy()
        .add_directive(
            "hyper_util=off"
                .parse()
                .unwrap_or_else(|_| "hyper_util=off".parse().unwrap_or_default()),
        );
    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_span_events(FmtSpan::NONE)
        .with_max_level(log_level)
        .with_env_filter(filter)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

async fn interactive_mode() -> Result<RcurlResponse, anyhow::Error> {
    let history = load_history_entries().unwrap_or_default();

    if history.is_empty() {
        println!("No request history found.");
        println!("Run a command first to create history.");
        return Ok(RcurlResponse::Ftp(()));
    }

    // 使用 TUI 界面选择历史命令
    let mut selector = HistorySelector::new(history);
    let selected = selector.run()?;

    let selected = match selected {
        Some(cmd) => cmd,
        None => return Ok(RcurlResponse::Ftp(())),
    };

    // 将选中的命令交给用户的 shell：Windows 下注入到控制台输入缓冲区，
    // PowerShell(cmd) 会在提示符处回显该命令，用户可编辑后回车执行；
    // 其他平台回退为直接打印。
    handoff_command(&selected);

    Ok(RcurlResponse::Ftp(()))
}

/// 将选中的命令交给用户的 shell 处理。
fn handoff_command(command: &str) {
    #[cfg(windows)]
    {
        if let Err(e) = console_input::inject(command) {
            eprintln!(
                "Warning: failed to inject command into console input ({}); printing instead:",
                e
            );
        } else {
            return;
        }
    }

    // 非 Windows，或注入失败时的回退：直接打印。
    println!("{}", command);
}

#[cfg(windows)]
mod console_input {
    use std::io;

    use windows_sys::Win32::Foundation::{FALSE, INVALID_HANDLE_VALUE, TRUE};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD,
        KEY_EVENT_RECORD_0, STD_INPUT_HANDLE, WriteConsoleInputW,
    };

    /// 将命令文本写入控制台输入缓冲区。调用方进程退出后，宿主 shell
    /// （PowerShell 的 PSReadLine 或 cmd）会把这些事件当作键盘输入读入，
    /// 用户即可在提示符处编辑后回车执行。
    pub fn inject(text: &str) -> io::Result<()> {
        let text = text.trim_end_matches(['\r', '\n']);

        unsafe {
            let handle = GetStdHandle(STD_INPUT_HANDLE);
            // GetStdHandle 失败时返回 NULL 或 INVALID_HANDLE_VALUE(-1)。
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }

            // 每个 UTF-16 码元生成一对按键事件（按下 + 抬起），全部一次性写入，
            // 以便宿主 shell 将其识别为粘贴并原样插入（避免触发自动配对等编辑行为）。
            let mut records: Vec<INPUT_RECORD> =
                Vec::with_capacity(text.encode_utf16().count() * 2);
            for unit in text.encode_utf16() {
                for down in [TRUE, FALSE] {
                    let key_event = KEY_EVENT_RECORD {
                        bKeyDown: down,
                        wRepeatCount: 1,
                        wVirtualKeyCode: 0,
                        wVirtualScanCode: 0,
                        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                        dwControlKeyState: 0,
                    };
                    records.push(INPUT_RECORD {
                        EventType: KEY_EVENT as u16,
                        Event: INPUT_RECORD_0 { KeyEvent: key_event },
                    });
                }
            }

            let mut written: u32 = 0;
            let status = WriteConsoleInputW(handle, records.as_ptr(), records.len() as u32, &mut written);
            if status == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

async fn execute_request(cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    // Handle quick commands
    if let Some(ref cmd) = cli.quick_cmd {
        return match cmd {
            QuickCommand::Ping { target } => ping_command(target.clone(), cli).await,
            QuickCommand::Disk { target } => disk_size_command(target.clone(), cli).await,
            QuickCommand::Count { target } => count_lines_command(target.clone(), cli).await,
            QuickCommand::Git { target } => git_statistic_command(target.clone(), cli).await,
            QuickCommand::Telnet { host, port } => telnet_command(host.clone(), *port, cli).await,
            QuickCommand::Ns { domain } => dns_command(domain.clone(), cli).await,
            QuickCommand::Whois { target } => whois_command(target.clone(), cli).await,
            QuickCommand::Port { port, kill } => {
                if let Some(p) = port {
                    if *kill {
                        port_kill_command(*p, cli).await
                    } else {
                        port_find_command(*p, cli).await
                    }
                } else {
                    port_list_command(cli).await
                }
            }
        };
    }

    // Default URL-based behavior
    let url = cli.url.clone().unwrap_or_default();
    let uri: hyper::Uri = url.parse()?;

    if let Some(scheme) = uri.scheme() {
        let scheme_string = scheme.to_string();
        let scheme_str = scheme_string.as_str();
        let s = match scheme_str {
            "http" | "https" => {
                let _http_parts = http_request_with_redirects(cli).await?;
                RcurlResponse::Http(())
            }
            "ftp" | "ftps" | "sftp" => {
                ftp_request(cli, scheme_str).await?;
                RcurlResponse::Ftp(())
            }
            _ => Err(anyhow!("Can not find scheme in the uri:{}.", uri))?,
        };
        Ok(s)
    } else {
        Err(anyhow!("Can not find scheme in the uri:{}.", uri))?
    }
}
