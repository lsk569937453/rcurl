use anyhow::anyhow;
use hyper_util::rt::TokioIo;
use hyper::Uri;
use hyper_util::client::legacy::connect::HttpConnector;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tower_service::Service;

#[derive(Clone)]
pub struct HttpProxyConnector {
    pub proxy_addr: String,
    direct: HttpConnector,
}

impl HttpProxyConnector {
    pub fn new(proxy_addr: String) -> Self {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        Self {
            proxy_addr,
            direct: http,
        }
    }
}

impl Service<Uri> for HttpProxyConnector {
    type Response = TokioIo<TcpStream>;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let proxy = self.proxy_addr.clone();

        Box::pin(async move {
            let host = uri.host().ok_or(anyhow!("missing host"))?;
            let port = uri.port_u16().unwrap_or(443);

            // 1. 连接代理
            let mut stream = TcpStream::connect(&proxy).await?;

            // 2. 发送 CONNECT
            let req = format!(
                "CONNECT {}:{} HTTP/1.1\r\n\
                 Host: {}:{}\r\n\
                 Proxy-Connection: Keep-Alive\r\n\r\n",
                host, port, host, port
            );

            tokio::io::AsyncWriteExt::write_all(&mut stream, req.as_bytes()).await?;

            // 3. 读取响应直到空行（headers 结束）
            let mut response_buf = Vec::new();
            let mut header_buf = [0u8; 1];

            loop {
                let n = tokio::io::AsyncReadExt::read(&mut stream, &mut header_buf).await?;
                if n == 0 {
                    return Err(anyhow!("proxy connection closed unexpectedly"));
                }

                let byte = header_buf[0];
                response_buf.push(byte);

                // 检查是否到达 header 结束标记 (\r\n\r\n)
                let len = response_buf.len();
                if len >= 4 {
                    if &response_buf[len - 4..] == b"\r\n\r\n" {
                        break;
                    }
                }
            }

            let resp = std::str::from_utf8(&response_buf)?;

            if !resp.starts_with("HTTP/1.1 200") && !resp.starts_with("HTTP/1.0 200") {
                return Err(anyhow!("proxy CONNECT failed: {}", resp));
            }

            eprintln!("* CONNECT response: {}", resp.lines().next().unwrap_or(""));

            Ok(TokioIo::new(stream))
        })
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

impl Service<Uri> for HttpForwardProxyConnector {
    type Response = TokioIo<TcpStream>;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

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
