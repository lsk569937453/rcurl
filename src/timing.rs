use std::fmt;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone)]
pub struct RequestTimings {
    pub dns_start: Option<Instant>,
    pub dns_end: Option<Instant>,
    pub tcp_connect_start: Option<Instant>,
    pub tcp_connect_end: Option<Instant>,
    pub tls_start: Option<Instant>,
    pub tls_end: Option<Instant>,
    pub total_start: Option<Instant>,
    pub total_end: Option<Instant>,
}

impl RequestTimings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_total(&mut self) {
        self.total_start = Some(Instant::now());
    }

    pub fn end_total(&mut self) {
        self.total_end = Some(Instant::now());
    }

    pub fn start_dns(&mut self) {
        self.dns_start = Some(Instant::now());
    }

    pub fn end_dns(&mut self) {
        self.dns_end = Some(Instant::now());
    }

    pub fn start_tcp_connect(&mut self) {
        self.tcp_connect_start = Some(Instant::now());
    }

    pub fn end_tcp_connect(&mut self) {
        self.tcp_connect_end = Some(Instant::now());
    }

    pub fn start_tls(&mut self) {
        self.tls_start = Some(Instant::now());
    }

    pub fn end_tls(&mut self) {
        self.tls_end = Some(Instant::now());
    }

    pub fn dns_duration(&self) -> Option<Duration> {
        Some(self.dns_end?.duration_since(self.dns_start?))
    }

    pub fn tcp_connect_duration(&self) -> Option<Duration> {
        Some(self.tcp_connect_end?.duration_since(self.tcp_connect_start?))
    }

    pub fn tls_duration(&self) -> Option<Duration> {
        Some(self.tls_end?.duration_since(self.tls_start?))
    }

    pub fn total_duration(&self) -> Option<Duration> {
        Some(self.total_end?.duration_since(self.total_start?))
    }
}

impl fmt::Display for RequestTimings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\nTime breakdown:")?;

        if let Some(dns) = self.dns_duration() {
            writeln!(f, "  DNS lookup:      {:>8} ms", dns.as_millis())?;
        } else {
            writeln!(f, "  DNS lookup:      N/A")?;
        }

        if let Some(tcp) = self.tcp_connect_duration() {
            writeln!(f, "  TCP connect:     {:>8} ms", tcp.as_millis())?;
        } else {
            writeln!(f, "  TCP connect:     N/A")?;
        }

        if let Some(tls) = self.tls_duration() {
            writeln!(f, "  TLS handshake:   {:>8} ms", tls.as_millis())?;
        } else {
            writeln!(f, "  TLS handshake:   N/A")?;
        }

        if let Some(total) = self.total_duration() {
            writeln!(f, "  Total time:      {:>8} ms", total.as_millis())?;
        } else {
            writeln!(f, "  Total time:      N/A")?;
        }

        Ok(())
    }
}
