use hyper_util::client::legacy::connect::dns::Name;
use std::future::Future;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as OtherContext, Poll};
use std::vec;
use tower_service::Service;

#[derive(Clone, Debug)]
pub struct DnsLoggingResolver {
    pub timings: Arc<std::sync::Mutex<Option<crate::http::timing::RequestTimings>>>,
}

impl DnsLoggingResolver {
    pub fn new() -> Self {
        Self {
            timings: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn with_timings() -> (Self, Arc<std::sync::Mutex<Option<crate::http::timing::RequestTimings>>>) {
        let timings = Arc::new(std::sync::Mutex::new(None));
        (
            Self {
                timings: timings.clone(),
            },
            timings,
        )
    }

    pub fn reset_timings(&self) {
        *self.timings.lock().unwrap() = None;
    }

    pub fn start_dns(&self) {
        let mut timings = self.timings.lock().unwrap();
        if timings.is_none() {
            *timings = Some(crate::http::timing::RequestTimings::new());
        }
        if let Some(ref mut t) = *timings {
            t.start_dns();
        }
    }

    pub fn end_dns(&self) {
        if let Some(ref mut t) = *self.timings.lock().unwrap() {
            t.end_dns();
        }
    }
}

impl Service<Name> for DnsLoggingResolver {
    type Response = vec::IntoIter<SocketAddr>;
    type Error = anyhow::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, _cx: &mut OtherContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, name: Name) -> Self::Future {
        let timings = self.timings.clone();
        self.start_dns();
        Box::pin(async move {
            let host = name.as_str().to_string();
            let host_clone_for_log = host.clone();
            debug!("Resolving DNS for: {}", &host_clone_for_log);
            let addrs_iter =
                tokio::task::spawn_blocking(move || (host, 0).to_socket_addrs()).await??;
            debug!("Resolved DNS for {}: {:?}", &host_clone_for_log, addrs_iter);

            let addresses: Vec<_> = addrs_iter.collect();

            if addresses.is_empty() {
                error!(
                    "DNS resolution for {} returned no addresses.",
                    &host_clone_for_log
                );
                return Err(anyhow!(
                    "No IP addresses found for host {}",
                    host_clone_for_log
                ));
            }

            for (i, addr) in addresses.iter().enumerate() {
                if i == 0 {
                    debug!("Resolved IP: {}", addr.ip());
                } else {
                    debug!("Resolved IP (alternative): {}", addr.ip());
                }
            }

            // End DNS timing
            if let Some(ref mut t) = *timings.lock().unwrap() {
                t.end_dns();
            }

            Ok(addresses.into_iter())
        })
    }
}
