use hyper_util::client::legacy::connect::dns::Name;
use std::future::Future;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::task::{Context as OtherContext, Poll};
use std::vec;
use tower_service::Service;
#[derive(Clone, Debug)]
pub struct DnsLoggingResolver;

impl DnsLoggingResolver {
    pub fn new() -> Self {
        Self
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
        Box::pin(async move {
            let host = name.as_str().to_string();
            let host_clone_for_log = host.clone();

            debug!("Resolving DNS for: {}", &host_clone_for_log);

            let addrs_iter =
                tokio::task::spawn_blocking(move || (host, 0).to_socket_addrs()).await??;
            info!("Resolved DNS for {}: {:?}", &host_clone_for_log, addrs_iter);

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

            Ok(addresses.into_iter())
        })
    }
}
