use crate::cli::app_config::Cli;
use env_logger::Builder;
use log::LevelFilter;
use suppaftp::async_native_tls::TlsConnector;
use suppaftp::types::FileType;

use suppaftp::{AsyncNativeTlsConnector, AsyncNativeTlsFtpStream};
pub async fn ftp_request(cli: Cli, scheme: &str) -> Result<(), anyhow::Error> {
    let log_level_hyper = if cli.debug {
        LevelFilter::Trace
    } else {
        LevelFilter::Info
    };

    // init logger
    Builder::new().filter_level(log_level_hyper).init();
    let uri: hyper::Uri = cli.url.parse()?;
    let host = uri.host().ok_or(anyhow!(""))?;
    let port = uri.port_u16().unwrap_or(21);
    let mut ftp_stream = AsyncNativeTlsFtpStream::connect(format!("{}:{}", host, port)).await?;
    if scheme == "ftps" {
        let ctx = TlsConnector::new()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true);
        ftp_stream = ftp_stream
            .into_secure(AsyncNativeTlsConnector::from(ctx), host)
            .await?;
    };

    if let Some(authority) = cli.authority_option {
        let split: Vec<&str> = authority.splitn(2, ':').collect();
        ensure!(split.len() == 2, "User data error");
        ftp_stream
            .login(split[0], split[1])
            .await
            .map_err(|e| anyhow!("login error:{}", e))?;
    }
    assert!(ftp_stream.transfer_type(FileType::Binary).await.is_ok());
    let file_list = ftp_stream
        .list(None)
        .await
        .map_err(|e| anyhow!("Command failed, error:{}", e))?;
    file_list.iter().for_each(|f| println!("{f}"));
    Ok(())
}
