use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

use super::report::print_results;
use super::runner::run_load_test;
use super::types::LtConfig;

#[allow(clippy::too_many_arguments)]
pub async fn load_test_command(
    url: String,
    concurrency: u16,
    duration: Option<Duration>,
    requests: u64,
    headers: Vec<(String, String)>,
    body: Option<String>,
    timeout: Duration,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    // Read body from file if needed
    let body_bytes = if let Some(b) = body {
        if let Some(path) = b.strip_prefix('@') {
            Some(tokio::fs::read(path).await?)
        } else {
            Some(b.into_bytes())
        }
    } else {
        None
    };

    let config = LtConfig {
        url: url.clone(),
        concurrency,
        duration,
        total_requests: if duration.is_none() {
            Some(requests)
        } else {
            None
        },
        headers,
        body: body_bytes,
        timeout,
    };

    // Create progress bar
    let progress = ProgressBar::new(if config.duration.is_some() {
        config.duration.map_or(1, |d| d.as_secs().max(1))
    } else {
        config.total_requests.unwrap_or(500000)
    });
    progress.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}",
            )?
            .progress_chars("##-"),
    );
    progress.set_message("Running load test...");

    // Spawn the test in the background
    let (progress_tx, progress_rx) = mpsc::channel(100);
    let config_clone = config.clone();
    let test_handle = tokio::spawn(async move {
        // In duration mode there is no request counter to report.
        let progress_tx = if config_clone.duration.is_some() {
            None
        } else {
            Some(progress_tx)
        };
        run_load_test(config_clone, progress_tx).await
    });

    // Update progress
    if let Some(test_duration) = config.duration {
        // Duration mode: advance the bar by elapsed seconds until the test ends.
        let start = Instant::now();
        loop {
            progress.set_position(start.elapsed().as_secs());
            if start.elapsed() >= test_duration {
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
    } else {
        // Request mode: track real-time progress from the channel.
        let total_requests = config.total_requests.unwrap_or(500000);
        let mut rx = progress_rx;
        while let Some(completed) = rx.recv().await {
            progress.set_position(completed);
            if completed >= total_requests {
                break;
            }
        }
    }

    let result = test_handle.await??;

    progress.finish_with_message("Test completed!");

    // Print results
    print_results(&result, &url);

    Ok(RcurlResponse::LoadTest(()))
}
