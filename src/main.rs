#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use crate::cli::app_config::Cli;
use crate::ftp::handler::ftp_request;
use crate::response::res::RcurlResponse;
use clap::Parser;
use http::handler::http_request_with_redirects;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;
mod ftp;
mod tls;

mod http;

mod cli;

mod response;

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();

    if let Err(e) = do_request(cli).await {
        error!("An error occurred:\n{:?}", e);
    }
}

async fn do_request(cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let log_level = match cli.verbosity {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = tracing_subscriber::fmt()
        .with_level(true)
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_span_events(FmtSpan::NONE)
        .with_max_level(log_level)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let url = cli.url.clone();
    let uri: hyper::Uri = url.parse()?;
    if let Some(scheme) = uri.scheme() {
        let scheme_string = scheme.to_string();
        let scheme_str = scheme_string.as_str();
        let s = match scheme_str {
            "http" | "https" => {
                let http_parts = http_request_with_redirects(cli).await?;
                RcurlResponse::Http(http_parts)
            }
            "ftp" | "ftps" | "sftp" => {
                ftp_request(cli, scheme_str).await?;
                RcurlResponse::Ftp(())
            }
            _ => Err(anyhow!("Can not find scheme in the uri:{}.", uri))?,
        };
        Ok(s)
    } else {
        Err(anyhow!("Can not find scheme in the uri:{}.", uri))?
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use crate::response::res::RcurlResponse;
    use crate::{cli::app_config::Cli, do_request};
    use ::http::StatusCode;
    use bytes::Bytes;
    use http_body_util::combinators::BoxBody;
    use http_body_util::BodyExt;

    async fn assert_http_ok(cli: Cli) -> hyper::Response<BoxBody<Bytes, Infallible>> {
        let response = do_request(cli).await.expect("Request should not fail");

        if let RcurlResponse::Http(http_response) = response {
            assert_eq!(
                http_response.status(),
                StatusCode::OK,
                "HTTP status should be 200 OK"
            );
            http_response
        } else {
            panic!("Expected an HTTP response, but got: {:?}", response);
        }
    }

    async fn assert_ftp_ok(cli: Cli) {
        let response = do_request(cli).await.expect("Request should not fail");

        assert!(
            matches!(response, RcurlResponse::Ftp(_)),
            "Expected an FTP response, but got: {:?}",
            response
        );
    }
    async fn response_to_string(response: hyper::Response<BoxBody<Bytes, Infallible>>) -> String {
        let (_, incoming) = response.into_parts();
        let body_bytes = incoming.collect().await.unwrap().to_bytes();

        String::from_utf8(body_bytes.to_vec()).unwrap()
    }
    #[tokio::test]
    async fn test_https_get_ok() {
        let cli = Cli {
            url: "https://httpbin.org/get".to_string(),
            ..Default::default()
        };
        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_get_debug_ok() {
        let cli = Cli {
            url: "https://httpbin.org/get".to_string(),
            verbosity: 0,
            skip_certificate_validate: true,
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_get_debug_ok2() {
        let cli = Cli {
            url: "https://httpbin.org/get".to_string(),
            file_path_option: Some("test.html".to_string()),
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_post_ok() {
        let cli = Cli {
            url: "https://httpbin.org/post".to_string(),
            method_option: Some("POST".to_string()),
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_post_body_ok() {
        let cli = Cli {
            url: "https://httpbin.org/post".to_string(),
            body_option: Some(r#"{"a":"b"}"#.to_string()),
            headers: vec!["Content-Type:application/json".to_string()],
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_post_form() {
        let default_form = "a=b";
        let cli = Cli {
            url: "https://httpbin.org/post".to_string(),
            form_option: vec![default_form.to_string()],
            ..Default::default()
        };

        let response = assert_http_ok(cli).await;
        let body_str = response_to_string(response).await;
        assert!(body_str.contains(r#""a": "b""#));
    }

    #[tokio::test]
    async fn test_http_post_form_with_file() {
        let default_form = "a=@LICENSE";
        let cli = Cli {
            url: "https://httpbin.org/post".to_string(),
            form_option: vec![default_form.to_string()],
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_put_ok() {
        let cli = Cli {
            url: "http://httpbin.org/put".to_string(),
            method_option: Some("PUT".to_string()),
            ..Default::default()
        };

        assert_http_ok(cli).await;
    }

    #[tokio::test]
    async fn test_http_user_agent_ok() {
        let default_agent = "rcurl-test-useragent";

        let cli = Cli {
            url: "http://httpbin.org/get".to_string(),
            user_agent_option: Some(default_agent.to_string()),
            ..Default::default()
        };

        let response = assert_http_ok(cli).await;
        let body_str = response_to_string(response).await;
        assert!(body_str.contains(default_agent));
    }

    #[tokio::test]
    async fn test_http_referer_ok() {
        let default_refer = "default_refer";
        let cli = Cli {
            url: "http://httpbin.org/get".to_string(),
            refer_option: Some(default_refer.to_string()),
            ..Default::default()
        };

        let response = assert_http_ok(cli).await;
        let body_str = response_to_string(response).await;
        assert!(body_str.contains(default_refer));
    }

    #[tokio::test]
    async fn test_http_head_request_ok() {
        let cli = Cli {
            url: "http://httpbin.org/get".to_string(),
            header_option: true,
            ..Default::default()
        };

        let response = assert_http_ok(cli).await;
        let (_, incoming) = response.into_parts();
        let body_bytes = incoming.collect().await.unwrap().to_bytes();
        assert!(body_bytes.is_empty(), "HEAD request body should be empty");
    }

    #[tokio::test]
    async fn test_ftp_list_ok() {
        let cli = Cli {
            url: "ftp://test.rebex.net:21/".to_string(),
            authority_option: Some("demo:password".to_string()),
            ..Default::default()
        };
        assert_ftp_ok(cli).await;
    }

    #[tokio::test]
    async fn test_ftp_download_file_ok() {
        let cli = Cli {
            url: "ftp://test.rebex.net:21/".to_string(),
            file_path_option: Some("test.html".to_string()),
            range_option: Some("0-1000".to_string()),
            authority_option: Some("demo:password".to_string()),
            ..Default::default()
        };

        assert_ftp_ok(cli).await;
    }
}
