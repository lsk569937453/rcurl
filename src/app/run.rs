use crate::cli::app_config::Cli;
use crate::ftp::handler::ftp_request;
use crate::http::handler::http_request_with_redirects;
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
    let log_level = match cli.verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };
    let filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy()
        .add_directive("hyper_util=off".parse()?);
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

    let url = cli.url.clone();
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
