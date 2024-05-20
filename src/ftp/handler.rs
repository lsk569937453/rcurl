use crate::cli::app_config::Cli;
use suppaftp::async_native_tls::TlsConnector;
use suppaftp::types::FileType;
use suppaftp::AsyncFtpStream;
use suppaftp::ImplAsyncFtpStream;
use suppaftp::{AsyncNativeTlsConnector, AsyncNativeTlsFtpStream};
use tokio::net::TcpStream;
pub async fn ftp_request(cli: Cli, scheme: &str) -> Result<(), anyhow::Error> {
    let uri: hyper::Uri = cli.url.parse()?;

    let host = uri.host().ok_or(anyhow!(""))?;
    let port = uri.port_u16().unwrap_or(21);
    let mut ftp_stream = AsyncNativeTlsFtpStream::connect(format!("{}:{}", host, port)).await?;
    if scheme == "ftps" {
        ftp_stream = ftp_stream
            .into_secure(AsyncNativeTlsConnector::from(TlsConnector::new()), host)
            .await?;
    };
    println!("dnoe1");

    if let Some(authority) = cli.authority_option {
        let split: Vec<&str> = authority.splitn(2, ':').collect();
        ensure!(split.len() == 2, "User data error");
        ftp_stream
            .login(split[0], split[1])
            .await
            .map_err(|e| anyhow!("aaa{}", e))?;
    }
    assert!(ftp_stream.transfer_type(FileType::Binary).await.is_ok());

    println!("dnoe2");
    let dir = ftp_stream.pwd().await?;
    println!("{dir}");
    let dir = ftp_stream.cwd("remote").await?;

    let file_list = ftp_stream
        .mlsd(None)
        .await
        .map_err(|e| anyhow!("aaa{}", e))?;
    file_list.iter().for_each(|f| println!("{f}"));
    // println!("dnoe{file_list}");
    Ok(())
}
