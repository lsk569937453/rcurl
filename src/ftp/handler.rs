use std::path::Path;
use url::Url;

use crate::cli::app_config::Cli;
use anyhow::Context as AnyhowContext;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::sync::Arc;
use suppaftp::RustlsConnector;
use suppaftp::RustlsFtpStream;
use suppaftp::types::FileType;

use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct ProgressBarIter<T> {
    pub it: T,
    pub progress: ProgressBar,
}

impl<T: Read> Read for ProgressBarIter<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_read = self.it.read(buf)?;

        if bytes_read > 0 {
            self.progress.inc(bytes_read as u64);
        }

        Ok(bytes_read)
    }
}

pub async fn ftp_request(cli: Cli, scheme: &str) -> Result<(), anyhow::Error> {
    let url = cli.url.as_ref().ok_or(anyhow!("URL is required"))?;
    let uri: hyper::Uri = url.parse()?;
    let host = uri.host().ok_or(anyhow!(""))?;
    let port = uri.port_u16().unwrap_or(21);
    let mut ftp_stream = RustlsFtpStream::connect(format!("{host}:{port}"))?;
    ftp_stream.set_mode(suppaftp::Mode::ExtendedPassive);

    if scheme == "ftps" {
        let root_store = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        let ctx = RustlsConnector::from(Arc::new(config));
        ftp_stream = ftp_stream.into_secure(ctx, host)?;
    };

    if let Some(authority) = cli.authority_option.clone() {
        let split: Vec<&str> = authority.splitn(2, ':').collect();
        ensure!(split.len() == 2, "User data error");
        ftp_stream
            .login(split[0], split[1])
            .context("Login error")?;
    } else if uri.authority().is_some() {
        let url_str = cli.url.as_ref().ok_or(anyhow!("URL is required"))?;
        let url = Url::parse(url_str)?;
        let user = url.username();
        let pass = url.password().ok_or(anyhow!("Password is empty!"))?;

        ftp_stream
            .login(user, pass)
            .context("Login error with credentials from URL")?;
    }
    ftp_stream.cwd(uri.path())?;

    assert!(ftp_stream.transfer_type(FileType::Binary).is_ok());
    if let Some(upload_file) = cli.uploadfile_option {
        let path = Path::new(&upload_file);
        let f = File::open(path)?;

        let reader = BufReader::new(f);
        let metadata = reader.get_ref().metadata()?;
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
        let _ = ftp_stream.put_file(String::from(file_name), &mut pro);
        pb.finish_with_message("upload success");
    } else if let Some(quote) = cli.quote_option.clone() {
        let response = ftp_stream.site(quote)?;
    } else {
        let file_list = ftp_stream
            .list(None)
            .map_err(|e| anyhow!("Command failed, error:{}", e))?;
        let joined = file_list.join("\n");

        output(cli, joined.as_bytes().to_vec()).await?;
    }
    Ok(())
}

async fn output(cli: Cli, mut item: Vec<u8>) -> Result<(), anyhow::Error> {
    if let Some(mut range) = cli.range_option {
        range = format!("bytes={range}");
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
            .truncate(true)
            .open(file_path)
            .await?;
        let _ = file.write_all(item.as_slice()).await;
    } else {
        println!("{}", String::from_utf8_lossy(item.as_slice()));
    }
    Ok(())
}
