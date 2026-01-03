// This module provides a timing wrapper for HTTP connections
// Due to hyper's abstraction layers, we use a simplified approach
// to track timing at the request handler level

use std::sync::Arc;
use std::time::Instant;

/// Represents timing data collected during a request
#[derive(Debug, Clone)]
pub struct ConnectionTimings {
    pub dns_duration_ms: Option<u128>,
    pub tcp_connect_duration_ms: Option<u128>,
    pub tls_handshake_duration_ms: Option<u128>,
    pub total_duration_ms: u128,
}

impl ConnectionTimings {
    pub fn new() -> Self {
        Self {
            dns_duration_ms: None,
            tcp_connect_duration_ms: None,
            tls_handshake_duration_ms: None,
            total_duration_ms: 0,
        }
    }
}
