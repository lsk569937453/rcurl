use crate::cli::app_config::{Cli, QuickCommand};
use crate::disk::handler::disk_size_command;
use crate::ftp::handler::ftp_request;
use crate::history::{command_from_cli, load_history, save_request};
use crate::http::handler::http_request_with_redirects;
use crate::ping::handler::ping_command;
use crate::response::res::RcurlResponse;
use clap::Parser;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

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
    let history = load_history().unwrap_or_default();

    if history.is_empty() {
        println!("No request history found.");
        println!("Run a command first to create history.");
        return Ok(RcurlResponse::Ftp(()));
    }

    let options = &history[..];

    let selected = inquire::Select::new("Select a request from history:", options.to_vec())
        .with_page_size(10)
        .prompt()?;

    // 解析选中的命令并执行
    let args = shell_words::split(&selected)
        .map_err(|e| anyhow::anyhow!("Failed to parse command: {}", e))?;

    // 跳过第一个参数（程序名）
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    // 使用选中的参数创建新的 Cli 并执行
    let cli = Cli::try_parse_from(args)?;
    // 初始化日志
    init_logging(cli.verbosity);

    // 保存请求到历史（将选中的命令移到顶部）
    let command = command_from_cli(&cli);
    if let Err(e) = save_request(&command, &cli) {
        eprintln!("Warning: Failed to save request history: {}", e);
    }

    // 执行请求
    execute_request(cli).await
}

async fn execute_request(cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    // Handle quick commands
    if let Some(ref cmd) = cli.quick_cmd {
        return match cmd {
            QuickCommand::Ping { target } => {
                ping_command(target.clone(), cli).await
            }
            QuickCommand::Disk { target } => {
                disk_size_command(target.clone(), cli).await
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
                let http_parts = http_request_with_redirects(cli).await?;
                RcurlResponse::Http(http_parts)
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
