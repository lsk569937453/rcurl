#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use clap::Parser;
mod ftp;
use http::handler::http_request;
use http_body_util::BodyExt;
use hyper_util::client::legacy::Error;
use tracing::Level;
mod http;
use crate::ftp::handler::ftp_request;
mod cli;
use clap::CommandFactory;
mod response;
use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();

    if let Err(e) = do_request(cli).await {
        println!("{}", e);
    }
}
async fn do_request(cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let url = cli.url.clone();
    let uri: hyper::Uri = url.parse()?;
    if let Some(scheme) = uri.scheme() {
        let scheme_string = scheme.to_string();
        let scheme_str = scheme_string.as_str();
        let s = match scheme_str {
            "http" | "https" => {
                let http_parts = http_request(cli, scheme_str).await?;
                RcurlResponse::Http(http_parts)
            }
            "ftp" | "ftps" | "sftp" => {
                let ftp_res = ftp_request(cli, scheme_str).await?;
                RcurlResponse::Ftp(ftp_res)
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
        let rcurl_res = result.unwrap();
        if let RcurlResponse::Http(response) = rcurl_res {
            assert_eq!(response.status(), StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_https_get_ok() {
        let mut cli = Cli::new();
        cli.url = "https://httpbin.org/get".to_string();
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let rcurl_res = result.unwrap();
        if let RcurlResponse::Http(response) = rcurl_res {
            assert_eq!(response.status(), StatusCode::OK)
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
        if let RcurlResponse::Http(parts) = res {
            assert_eq!(parts.status(), StatusCode::OK)
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
        if let RcurlResponse::Http(parts) = res {
            assert_eq!(parts.status(), StatusCode::OK)
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_post_form() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/post".to_string();
        let default_form = "a=b";
        cli.form_option = vec![default_form.to_string()];
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        if let RcurlResponse::Http(response) = res {
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body();
            let s = body.collect().await.unwrap();
            let body_str = String::from_utf8(s.to_bytes().to_vec()).unwrap();
            assert!(body_str.contains(r#""a": "b""#));
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
        if let RcurlResponse::Http(parts) = res {
            assert_eq!(parts.status(), StatusCode::OK)
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
        let rcurl_response = result.unwrap();
        if let RcurlResponse::Http(response) = rcurl_response {
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body();
            let s = body.collect().await.unwrap();
            let body_str = String::from_utf8(s.to_bytes().to_vec()).unwrap();
            assert!(body_str.contains(default_agent));
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_referer_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/get".to_string();
        let default_refer = "default_refer";
        cli.refer_option = Some(default_refer.to_string());
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let rcurl_response = result.unwrap();
        if let RcurlResponse::Http(response) = rcurl_response {
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body();
            let s = body.collect().await.unwrap();
            let body_str = String::from_utf8(s.to_bytes().to_vec()).unwrap();
            assert!(body_str.contains(default_refer));
        } else {
            assert!(false);
        }
    }
    #[tokio::test]
    async fn test_http_head_request_ok() {
        let mut cli = Cli::new();
        cli.url = "http://httpbin.org/get".to_string();
        cli.header_option = true;
        let result = do_request(cli).await;
        assert!(result.is_ok());
        let rcurl_response = result.unwrap();
        if let RcurlResponse::Http(response) = rcurl_response {
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body();
            let s = body.collect().await.unwrap().to_bytes();
            assert!(s.len() == 0);
        } else {
            assert!(false);
        }
    }
}
