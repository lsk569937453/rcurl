use std::time::Duration;

/// Configuration for running a load test
#[derive(Clone, Debug)]
pub struct LtConfig {
    pub url: String,
    pub concurrency: u16,
    pub duration: Option<Duration>,
    pub total_requests: Option<u64>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Duration,
}

/// Per-response statistics recorded by each worker
#[derive(Clone, Copy, Debug)]
pub struct ResponseStat {
    pub time_cost_ns: u64,
    pub status_code: u16,
    pub content_length: u64,
}

/// Deduplicated error statistics
#[derive(Clone, Debug)]
pub struct ErrorStat {
    pub message: String,
    pub count: u64,
}

/// Result of a load test
#[derive(Debug)]
pub struct LtResult {
    pub duration_ns: u128,
    pub responses: Vec<ResponseStat>,
    pub errors: Vec<ErrorStat>,
    pub total_bytes: u64,
}
