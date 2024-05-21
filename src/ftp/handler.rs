use crate::cli::app_config::Cli;
use crate::ftp;
use async_std::fs::File;
use async_std::path::Path;
use env_logger::Builder;
use futures::io::BufReader;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use log::LevelFilter;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use suppaftp::async_native_tls::TlsConnector;
use suppaftp::types::FileType;
use suppaftp::{AsyncNativeTlsConnector, AsyncNativeTlsFtpStream};
use tokio::fs::OpenOptions;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::{self, AsyncBufReadExt};
#[derive(Debug)]
pub struct ProgressBarIter<T> {
    pub it: T,
    pub progress: ProgressBar,
}
impl<W: futures::io::AsyncRead + Unpin> futures::io::AsyncRead for ProgressBarIter<W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let prev_len = buf.len() as u64;
        if let Poll::Ready(e) = Pin::new(&mut self.it).poll_read(cx, buf) {
            self.progress.inc(buf.len() as u64);
            Poll::Ready(e)
        } else {
            Poll::Pending
        }
    }
}
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

    if let Some(authority) = cli.authority_option.clone() {
        let split: Vec<&str> = authority.splitn(2, ':').collect();
        ensure!(split.len() == 2, "User data error");
        ftp_stream
            .login(split[0], split[1])
            .await
            .map_err(|e| anyhow!("login error:{}", e))?;
    }
    ftp_stream.cwd(uri.path()).await?;

    assert!(ftp_stream.transfer_type(FileType::Binary).await.is_ok());
    if let Some(upload_file) = cli.uploadfile_option {
        let path = Path::new(&upload_file);
        let f = File::open(path).await?;

        let reader = BufReader::new(f.clone());
        let metadata = f.metadata().await?;
        let file_name = path
            .file_name()
            .ok_or(anyhow!("Can not find file name!"))?
            .to_string_lossy();
        let pb = ProgressBar::new(metadata.len());
        pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        ?
        .progress_chars("#>-"));
        let mut pro = ProgressBarIter {
            it: reader,
            progress: pb.clone(),
        };
        let _ = ftp_stream.put_file(String::from(file_name), &mut pro).await;
        pb.finish_with_message("upload success");
    } else if let Some(quote) = cli.quote_option.clone() {
        let response = ftp_stream.site(quote).await?;
    } else {
        let file_list = ftp_stream
            .list(None)
            .await
            .map_err(|e| anyhow!("Command failed, error:{}", e))?;
        let str = format!("{:?}", file_list);
        output(cli, str.as_bytes().to_vec()).await?;
    }
    Ok(())
}
async fn output(cli: Cli, mut item: Vec<u8>) -> Result<(), anyhow::Error> {
    if let Some(range) = cli.range_option {
        let parsed_range = http_range_header::parse_range_header(&range)?;
        let vec_ranges = parsed_range.validate(item.len() as u64)?;
        let mut concatenated_bytes = Vec::new();
        for range in vec_ranges {
            let start = *range.start() as usize;
            let end = *range.end() as usize;
            let bytes = item[start..end].to_vec();
            concatenated_bytes.extend(bytes);
        }
        item = concatenated_bytes.clone();
    }
    if let Some(file_path) = cli.file_path_option {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_path)
            .await?;
        let _ = file.write_all(item.as_slice()).await;
    } else {
        println!("{}", String::from_utf8_lossy(item.as_slice()));
    }
    Ok(())
}
