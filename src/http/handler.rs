use crate::cli::app_config::Cli;
use futures::StreamExt;
use http_body_util::BodyExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::http::header::CONTENT_LENGTH;
use hyper::Response;

use hyper::body::Buf;
use indicatif::{ProgressBar, ProgressStyle};
use std::cmp::min;
use std::fs::OpenOptions;
use std::io::Write as WriteStd;

pub async fn handle_response(cli: &Cli, res: Response<Incoming>) -> Result<(), anyhow::Error> {
    if cli.header_option {
        println!("{:?} {}", res.version(), res.status());
        for (key, value) in res.headers().iter() {
            println!("{}: {}", key, value.to_str()?);
        }
        return Ok(());
    }
    let content_length_option = res.headers().get(CONTENT_LENGTH);
    if let Some(content_lenth_str) = content_length_option {
        let content_length = content_lenth_str.to_str()?.parse::<u64>()?;

        if let Some(file_path) = cli.file_path_option.clone() {
            let incoming = res.into_body();
            let mut body_streaming = BodyStream::new(incoming);
            let mut downloaded = 0;
            let total_size = content_length;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(file_path)?;
            let pb = ProgressBar::new(total_size);
            pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        ?
        .progress_chars("#>-"));

            while let Some(Ok(t)) = body_streaming.next().await {
                if let Ok(bytes) = t.into_data() {
                    let new = min(downloaded + bytes.len() as u64, total_size);
                    downloaded = new;
                    pb.set_position(new);
                    file.write_all(&bytes)?;
                }
            }
            pb.finish_with_message("downloaded");
        } else {
            if content_length > 1024 * 1024 * 100 {
                return Err(anyhow!(
                    "Binary output can mess up your terminal. Use '--output -' to tell
rcurl to output it to your terminal anyway, or consider '--output
<FILE>' to save to a file."
                ));
            }
            let mut body = res.collect().await?.aggregate();
            let dst = body.copy_to_bytes(content_length as usize);
            let response_string = String::from_utf8_lossy(&dst);
            println!("{}", response_string);
        }
    }

    Ok(())
}
