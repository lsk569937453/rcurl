use hyper::Uri;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::rt::TokioIo;

use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tower_service::Service;

/// HTTPS proxy connector using hyper-util's official Tunnel implementation.
///
/// This connector uses the HTTP CONNECT method to establish a tunnel through
/// an HTTP proxy for HTTPS traffic.
#[derive(Clone)]
pub struct HttpProxyConnector {
    inner: Tunnel<HttpConnector>,
}

impl HttpProxyConnector {
    /// Create a new HTTPS proxy connector using hyper-util's official Tunnel.
    ///
    /// # Arguments
    /// * `proxy_addr` - The proxy address in format "host:port" (e.g., "127.0.0.1:7890")
    pub fn new(proxy_addr: String) -> Self {
        let proxy_dst = format!("http://{}", proxy_addr).parse().unwrap_or_default();
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        Self {
            inner: Tunnel::new(proxy_dst, http),
        }
    }
}

impl Service<Uri> for HttpProxyConnector {
    type Response = <Tunnel<HttpConnector> as Service<Uri>>::Response;
    type Error = <Tunnel<HttpConnector> as Service<Uri>>::Error;
    type Future = <Tunnel<HttpConnector> as Service<Uri>>::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Uri) -> Self::Future {
        self.inner.call(req)
    }
}

/// Read proxy address from environment variables.
///
/// Checks the following environment variables in order:
/// - `HTTPS_PROXY` / `https_proxy` for HTTPS URLs
/// - `HTTP_PROXY` / `http_proxy` for HTTP URLs
/// - `ALL_PROXY` / `all_proxy` as a fallback
///
/// Supports formats like:
/// - `http://127.0.0.1:7890`
/// - `http://proxy.example.com:8080`
/// - `socks5://127.0.0.1:1080` (not supported yet, returns None)
///
/// Returns `None` if no proxy is configured or if the proxy type is not supported.
pub fn get_proxy_from_env(scheme: &str) -> Option<String> {
    let env_var = if scheme == "https" {
        std::env::var("HTTPS_PROXY")
            .or_else(|_| std::env::var("https_proxy"))
            .or_else(|_| std::env::var("ALL_PROXY"))
            .or_else(|_| std::env::var("all_proxy"))
            .ok()
    } else {
        std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("http_proxy"))
            .or_else(|_| std::env::var("ALL_PROXY"))
            .or_else(|_| std::env::var("all_proxy"))
            .ok()
    }?;

    eprintln!("* Proxy env var found for {}: {}", scheme, env_var);

    // Parse the proxy URL to extract host and port
    if let Ok(url) = url::Url::parse(&env_var) {
        match url.scheme() {
            "http" => {
                let host = url.host_str()?;
                let port = url.port().unwrap_or(80);
                Some(format!("{}:{}", host, port))
            }
            "https" => {
                let host = url.host_str()?;
                let port = url.port().unwrap_or(443);
                Some(format!("{}:{}", host, port))
            }
            _ => None, // socks5, socks4, etc. not supported yet
        }
    } else {
        // Fallback: assume the value is already in host:port format
        Some(env_var)
    }
}

/// Check if a URL should bypass proxy based on NO_PROXY environment variable.
///
/// Supports patterns like:
/// - `example.com` - exact domain match
/// - `.example.com` - domain suffix match
/// - `192.168.1.1` - exact IP match
/// - `192.168.1.0/24` - CIDR notation (not supported yet)
pub fn should_bypass_proxy(host: Option<&str>) -> bool {
    let host = match host {
        Some(h) => h,
        None => return false,
    };

    let no_proxy = match std::env::var("NO_PROXY").or_else(|_| std::env::var("no_proxy")) {
        Ok(val) => val,
        Err(_) => return false,
    };

    for pattern in no_proxy.split(',') {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        if pattern.starts_with('.') {
            // Domain suffix match
            if host.ends_with(pattern) || host == &pattern[1..] {
                return true;
            }
        } else if host == pattern {
            // Exact match
            return true;
        }
    }

    false
}

/// HTTP forward proxy connector.
///
/// This connector connects to an HTTP proxy and forwards the request through it.
/// Unlike HTTPS CONNECT proxy, the HTTP proxy receives the full URL in the request.
#[derive(Clone)]
pub struct HttpForwardProxyConnector {
    pub proxy_addr: String,
}

impl HttpForwardProxyConnector {
    pub fn new(proxy_addr: String) -> Self {
        Self { proxy_addr }
    }
}
use futures::future::BoxFuture;

impl Service<Uri> for HttpForwardProxyConnector {
    type Response = TokioIo<TcpStream>;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let proxy = self.proxy_addr.clone();

        Box::pin(async move {
            // Connect directly to the proxy
            let stream = TcpStream::connect(&proxy).await?;
            Ok(TokioIo::new(stream))
        })
    }
}
