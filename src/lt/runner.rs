use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::Incoming;
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderName, HeaderValue};
use hyper::{HeaderMap, Request, Response};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{Duration, Instant, sleep, timeout};
use tokio_util::sync::CancellationToken;

use super::client::create_http_client;
use super::types::{ErrorStat, LtConfig, LtResult, ResponseStat};

type HttpClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

/// Result sent from worker tasks to the collector.
enum WorkerResult {
    Response(ResponseStat),
    Error(String),
}

/// Progress tracker for real-time updates.
#[derive(Clone)]
struct ProgressTracker {
    completed: Arc<AtomicU64>,
}

impl ProgressTracker {
    fn new() -> Self {
        Self {
            completed: Arc::new(AtomicU64::new(0)),
        }
    }

    fn get(&self) -> u64 {
        self.completed.load(Ordering::Relaxed)
    }

    fn increment(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
    }
}

/// Run the load test and return results.
pub async fn run_load_test(
    config: LtConfig,
    progress_tx: Option<mpsc::Sender<u64>>,
) -> Result<LtResult, anyhow::Error> {
    let client = create_http_client()?;

    // Build the request template once; every send clones it.
    // Method is implicit: POST with a form Content-Type when a body is given,
    // GET otherwise (custom -H headers can still override Content-Type).
    let mut method = String::from("GET");
    let mut content_type_option = None;
    if config.body.is_some() {
        method = String::from("POST");
        content_type_option = Some(String::from("application/x-www-form-urlencoded"));
    }

    let mut req_builder = Request::builder()
        .method(method.as_str())
        .uri(config.url.clone());

    let mut header_map = HeaderMap::new();
    if let Some(content_type) = content_type_option {
        header_map.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
    }
    for (key, value) in &config.headers {
        header_map.insert(
            HeaderName::from_str(key.as_str())?,
            HeaderValue::from_str(value)?,
        );
    }
    for (key, val) in header_map {
        if let Some(key) = key {
            req_builder = req_builder.header(key, val);
        }
    }

    let body_bytes = config.body.clone().unwrap_or_default();
    let req = req_builder.body(Full::new(Bytes::from(body_bytes)))?;

    let start_time = Instant::now();

    let progress_tracker = ProgressTracker::new();
    let progress_tracker_for_ui = progress_tracker.clone();
    let total_requests = config.total_requests;

    // Spawn progress reporter if a channel is provided.
    if let Some(tx) = progress_tx {
        tokio::spawn(async move {
            let mut last_reported = 0u64;
            loop {
                let current = progress_tracker_for_ui.get();
                if current > last_reported {
                    if tx.send(current).await.is_err() {
                        break;
                    }
                    last_reported = current;
                }
                if total_requests.is_some_and(|total| current >= total) {
                    break;
                }
                sleep(Duration::from_millis(50)).await;
            }
        });
    }

    // Buffered channel for collecting results.
    let (result_tx, mut result_rx) = mpsc::channel(config.concurrency as usize * 16);

    // Dedicated collector task accumulating stats while workers run.
    let collector_handle = tokio::spawn(async move {
        let mut responses = Vec::new();
        let mut errors = Vec::new();
        let mut total_bytes = 0u64;

        while let Some(result) = result_rx.recv().await {
            match result {
                WorkerResult::Response(stat) => {
                    total_bytes += stat.content_length;
                    responses.push(stat);
                }
                WorkerResult::Error(msg) => add_error(&mut errors, &msg),
            }
        }

        (responses, errors, total_bytes)
    });

    let mut task_list = JoinSet::new();

    if let Some(duration) = config.duration {
        // Duration-based test: workers loop until the token is cancelled.
        let cancel_token = CancellationToken::new();
        let token_for_tasks = cancel_token.clone();

        for _ in 0..config.concurrency {
            let tx = result_tx.clone();
            let cloned_req = req.clone();
            let clone_client = client.clone();
            let token = token_for_tasks.clone();
            let req_timeout = config.timeout;

            task_list.spawn(async move {
                submit_task_duration(tx, clone_client, cloned_req, token, req_timeout).await;
            });
        }

        sleep(duration).await;

        cancel_token.cancel();
    } else {
        // Request-count based test: each worker gets ceil(total / concurrency)
        // requests, so the grand total may slightly overshoot.
        let total_requests = config
            .total_requests
            .ok_or_else(|| anyhow::anyhow!("total_requests run error"))?;

        for _ in 0..config.concurrency {
            let tx = result_tx.clone();
            let cloned_req = req.clone();
            let clone_client = client.clone();
            let tracker = progress_tracker.clone();
            let req_timeout = config.timeout;
            let requests_per_worker = (total_requests / config.concurrency as u64)
                + if total_requests % config.concurrency as u64 > 0 {
                    1
                } else {
                    0
                };

            task_list.spawn(async move {
                submit_task_requests(tx, clone_client, cloned_req, requests_per_worker, tracker, req_timeout)
                    .await;
            });
        }
    }

    // Drop our sender so the collector task knows we're done.
    drop(result_tx);

    while task_list.join_next().await.is_some() {}

    let duration_ns = start_time.elapsed().as_nanos();

    let (responses, errors, total_bytes) = collector_handle.await?;

    Ok(LtResult {
        duration_ns,
        responses,
        errors,
        total_bytes,
    })
}

async fn submit_task_duration(
    tx: mpsc::Sender<WorkerResult>,
    client: HttpClient,
    request: Request<Full<Bytes>>,
    cancel_token: CancellationToken,
    req_timeout: Duration,
) {
    loop {
        // Check if cancelled
        if cancel_token.is_cancelled() {
            return;
        }

        send_once(&tx, &client, &request, req_timeout).await;

        // Check again after request
        if cancel_token.is_cancelled() {
            return;
        }
    }
}

async fn submit_task_requests(
    tx: mpsc::Sender<WorkerResult>,
    client: HttpClient,
    request: Request<Full<Bytes>>,
    total_requests: u64,
    progress: ProgressTracker,
    req_timeout: Duration,
) {
    for _ in 0..total_requests {
        send_once(&tx, &client, &request, req_timeout).await;

        // Update progress counter
        progress.increment();
    }
}

/// Send one request and report the outcome.
/// The timing (and the timeout) cover the full response: headers plus body,
/// because the body is drained so the connection returns to the pool and can
/// be reused by subsequent requests.
async fn send_once(
    tx: &mpsc::Sender<WorkerResult>,
    client: &HttpClient,
    request: &Request<Full<Bytes>>,
    req_timeout: Duration,
) {
    let now = Instant::now();
    let result = timeout(req_timeout, async {
        let res = client.request(request.clone()).await.map_err(|e| e.to_string())?;
        let status_code = res.status().as_u16();
        let content_len = get_content_length(&res);
        res.into_body().collect().await.map_err(|e| e.to_string())?;
        Ok::<(u16, u64), String>((status_code, content_len))
    })
    .await;
    let elapsed = now.elapsed().as_nanos() as u64;

    match result {
        Ok(Ok((status_code, content_len))) => {
            let _ = tx
                .send(WorkerResult::Response(ResponseStat {
                    time_cost_ns: elapsed,
                    status_code,
                    content_length: content_len,
                }))
                .await;
        }
        Ok(Err(msg)) => {
            let _ = tx.send(WorkerResult::Error(msg)).await;
        }
        Err(_) => {
            let _ = tx
                .send(WorkerResult::Error("Request timeout".to_string()))
                .await;
        }
    }
}

fn get_content_length(res: &Response<Incoming>) -> u64 {
    let default_content_length = HeaderValue::from_static("0");
    let content_len_header = res
        .headers()
        .get(CONTENT_LENGTH)
        .unwrap_or(&default_content_length);
    content_len_header
        .to_str()
        .unwrap_or("0")
        .parse::<u64>()
        .unwrap_or(0)
}

fn add_error(errors: &mut Vec<ErrorStat>, message: &str) {
    if let Some(existing) = errors.iter_mut().find(|e| e.message == message) {
        existing.count += 1;
    } else {
        errors.push(ErrorStat {
            message: message.to_string(),
            count: 1,
        });
    }
}
