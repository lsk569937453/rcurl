use bytes::buf;
use bytes::Bytes;
use futures::StreamExt;
use futures_util::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::http::header::CONTENT_LENGTH;
use hyper::Response;

use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper::{body::Buf, Request};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::convert::Infallible;
use std::fs::OpenOptions;
use std::io::Write as WriteStd;
use std::{cmp::min, fmt::Write};
use tokio::io::AsyncReadExt;
use tokio::io::BufReader;
use tokio::io::{self, AsyncWriteExt};
pub async fn handle_response(
    file_path_option: Option<String>,
    res: Response<Incoming>,
) -> Result<(), anyhow::Error> {
    let content_length = res
        .headers()
        .get(CONTENT_LENGTH)
        .ok_or(anyhow!("Can not parse content_length!"))?
        .to_str()?
        .parse::<u64>()?;
    let incoming = res.into_body();
    let mut body_streaming = BodyStream::new(incoming);
    if let Some(file_path) = file_path_option {
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
            return Err(anyhow!("The content_length is large than 100MB!"));
        }

        // let dst = body.copy_to_bytes(content_length as usize);
        // let response_string = String::from_utf8_lossy(&dst);
        // println!("{}", response_string);
    }

    Ok(())
}
