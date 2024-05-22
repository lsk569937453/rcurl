#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use clap::Parser;
mod ftp;
use http::handler::http_request;
use hyper_util::client::legacy::Error;
use tracing::Level;
mod http;
use crate::ftp::handler::ftp_request;
mod cli;
use clap::CommandFactory;
mod response;
use crate::cli::app_config::Cli;
use crate::response::res::Response;

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();

    if let Err(e) = do_request(cli).await {
        println!("{}", e);
    }
}
async fn do_request(cli: Cli) -> Result<Response, anyhow::Error> {
    let url = cli.url.clone();
    let uri: hyper::Uri = url.parse()?;
    if let Some(scheme) = uri.scheme() {
        let scheme_string = scheme.to_string();
        let scheme_str = scheme_string.as_str();
        let s = match scheme_str {
            "http" | "https" => {
                let http_parts = http_request(cli, scheme_str).await?;
                Response::Http(http_parts)
            }
            "ftp" | "ftps" | "sftp" => {
                let ftp_res = ftp_request(cli, scheme_str).await?;
                Response::Ftp(ftp_res)
            }
            _ => Err(anyhow!("Can not find scheme in the uri:{}.", uri))?,
        };
        return Ok(s);
    } else {
        Err(anyhow!("Can not find scheme in the uri:{}.", uri))?
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::res::RcurlResponse;
    use crate::{cli::app_config::Cli, do_request};
    use ::http::StatusCode;
    #[tokio::test]
    async fn test_http_get_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/get".to_string();
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let RcurlResponse::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_post_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/post".to_string();
        cli.method_option = Some("POST".to_string());
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let Response::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_post_body_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/post".to_string();
        cli.body_option = Some(r#"{"a":"b"}"#.to_string());
        cli.headers = vec!["Content-Type:application/json".to_string()];
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let Response::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_post_form() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/post".to_string();
        cli.form_option = vec!["a=b".to_string()];
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let Response::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_put_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/put".to_string();
        cli.method_option = Some("PUT".to_string());
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let Response::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_user_agent_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/get".to_string();
        let default_agent = "rcurl-test-useragent";
        cli.user_agent_option = Some(default_agent.to_string());
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let Response::Http(parts) = res {
            assert_eq!(parts.status, StatusCode::OK);
            println!("{:?}", parts);
            let user_agent_option = parts.headers.get("User-Agent");
            assert_eq!(user_agent_option.unwrap().to_str().unwrap(), default_agent);
        } else {
            assert!(false);
        }
    }
}
