#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use clap::Parser;
mod ftp;
use http::handler::http_request;
use tracing::Level;
mod http;
use crate::ftp::handler::ftp_request;
mod cli;
use clap::CommandFactory;

use crate::cli::app_config::Cli;

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();
    if cli.help {
        let mut cmd = Cli::command();
        cmd.build();
        cmd.print_help();
        std::process::exit(0);
    } else if cli.help_all {
        let mut cmd = Cli::command();
        cmd.build();
        cmd.print_long_help();
        std::process::exit(0);
    }
    if let Err(e) = do_request(cli).await {
        println!("{}", e);
    }
}
async fn do_request(cli: Cli) -> Result<(), anyhow::Error> {
    let url = cli.url.clone();
    let uri: hyper::Uri = url.parse()?;
    if let Some(scheme) = uri.scheme() {
        let scheme_string = scheme.to_string();
        let scheme_str = scheme_string.as_str();
        match scheme_str {
            "http" | "https" => {
                http_request(cli, scheme_str).await?;
            }
            "ftp" | "ftps" | "sftp" => {
                ftp_request(cli, scheme_str).await?;
            }
            _ => Err(anyhow!("Can not find scheme in the uri:{}.", uri))?,
        }
    } else {
        Err(anyhow!("Can not find scheme in the uri:{}.", uri))?;
    };

    Ok(())
}
