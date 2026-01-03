#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;

mod ftp;
mod tls;
use crate::app::run::main_with_error;
mod app;
mod cli;
mod disk;
mod dns;
mod history;
mod http;
mod ping;
mod response;
mod telnet;
mod timing;

#[tokio::main]
async fn main() {
    if let Err(e) = main_with_error().await {
        error!("An error occurred:\n{:?}", e);
    }
}
