use crate::cli::app_config::Cli;
use crate::http::dns_logging_connector::DnsLoggingResolver;
use crate::http::proxy::{
    HttpForwardProxyConnector, HttpProxyConnector, get_proxy_from_env, should_bypass_proxy,
};
use crate::http::timing::RequestTimings;
use crate::tls::info::get_tls_info;
use crate::tls::rcurl_cert_verifier::RcurlCertVerifier;
use anyhow::{Context, anyhow};
use bytes::Bytes;
use form_data_builder::FormData;
use futures::StreamExt;
use http::header::{
    ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderName, HeaderValue, USER_AGENT,
};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, BodyStream, Full};
use hyper::body::{Body, Incoming};
use hyper::client::conn::http1;
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use indicatif::{ProgressBar, ProgressStyle};
use mime_guess::mime;
use rustls::crypto::ring::{DEFAULT_CIPHER_SUITES, default_provider};
use rustls::{ClientConfig, RootCertStore};
use std::cmp::min;
use std::convert::Infallible;
use std::fs::OpenOptions;
use std::io::Write as WriteStd;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tower_service::Service;
use url::Url;

const MAX_REDIRECTS: u8 = 10;

pub async fn http_request_with_redirects(
    cli: Cli,
) -> Result<Response<BoxBody<Bytes, Infallible>>, anyhow::Error> {
    let url_str = cli.url.as_ref().ok_or(anyhow!("URL is required"))?;
    let mut current_url: Url = url_str.parse().context("Failed to parse initial URL")?;

    // Initialize timings if --time flag is set
    let mut timings = if cli.time {
        let mut t = RequestTimings::new();
        t.start_total();
        Some(t)
    } else {
        None
    };

    for i in 0..MAX_REDIRECTS {
        let uri: Uri = current_url.to_string().parse()?;
        let scheme = uri.scheme_str().unwrap_or("http");

        let request = build_request(&cli, &uri)?;

        let res = send_request(&cli, request, scheme, &mut timings).await?;

        let status = res.status();
        if status.is_redirection() {
            if let Some(location_header) = res.headers().get(http::header::LOCATION) {
                let location_str = location_header.to_str()?;
                current_url = current_url.join(location_str)?;

                if cli.verbosity >= 1 {
                    debug!(
                        "\nRedirecting to: {current_url} ({}/{MAX_REDIRECTS})\n",
                        i + 1
                    );
                }
                continue;
            } else {
                return Err(anyhow!("Redirect response missing 'location' header"));
            }
        }

        return handle_response(&cli, res, timings).await;
    }

    Err(anyhow!(
        "Exceeded maximum number of redirects ({MAX_REDIRECTS})"
    ))
}

fn build_request(cli: &Cli, uri: &Uri) -> Result<Request<Full<Bytes>>, anyhow::Error> {
    let mut method = String::from("GET");
    let mut content_type_option = None;

    if cli.body_option.is_some() {
        method = String::from("POST");
        content_type_option = Some(String::from("application/x-www-form-urlencoded"));
    }
    if cli.uploadfile_option.is_some() {
        method = String::from("PUT");
    }
    if let Some(method_userdefined) = cli.method_option.as_ref() {
        method = method_userdefined.clone();
    }
    if !cli.form_option.is_empty() {
        method = String::from("POST");
    }
    if cli.header_option {
        method = String::from("HEAD");
    }

    let mut request_builder = Request::builder().method(method.as_str()).uri(uri.clone());
    if cli.http2_prior_knowledge {
        request_builder = request_builder.version(hyper::Version::HTTP_2);
    }

    let mut header_map = http::HeaderMap::new();
    if let Some(content_type) = content_type_option {
        header_map.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
    }
    header_map.insert(ACCEPT, HeaderValue::from_str("*/*")?);
    let user_agent = cli
        .user_agent_option
        .as_deref()
        .unwrap_or(concat!("rcurl/", env!("CARGO_PKG_VERSION")));
    header_map.insert(USER_AGENT, HeaderValue::from_str(user_agent)?);
    if let Some(cookie) = cli.cookie_option.as_ref() {
        header_map.insert(COOKIE, HeaderValue::from_str(cookie)?);
    }

    let mut body_bytes = Bytes::new();
    if !cli.form_option.is_empty() {
        let mut form = FormData::new(Vec::new());
        let form_header = form.content_type_header();
        request_builder =
            request_builder.header(CONTENT_TYPE, HeaderValue::from_str(form_header.as_str())?);
        request_builder = request_builder.method("POST");

        for form_data in cli.form_option.clone() {
            let split: Vec<&str> = form_data.splitn(2, '=').collect();
            ensure!(split.len() == 2, "form data error");
            if split[1].starts_with("@") {
                let file_path = split[1].replace("@", "");
                let cloned_path = file_path.clone();
                let path = Path::new(&file_path)
                    .file_name()
                    .ok_or(anyhow!("Can not get the name of uploading file."))?;

                let mime_guess = mime_guess::from_path(cloned_path)
                    .first()
                    .unwrap_or(mime::APPLICATION_OCTET_STREAM);

                form.write_path(split[0], path, mime_guess.to_string().as_str())?;
            } else {
                form.write_field(split[0], split[1])?;
            }
        }
        let bytes = form.finish()?;
        body_bytes = bytes.into();
    } else if let Some(body) = cli.body_option.as_ref() {
        body_bytes = Bytes::from(body.clone());
    } else if let Some(upload_file) = cli.uploadfile_option.as_ref() {
        let byte_vec = std::fs::read(upload_file)?;
        body_bytes = Bytes::from(byte_vec);
    }

    for x in &cli.headers {
        let split: Vec<&str> = x.splitn(2, ':').collect();
        if split.len() == 2 {
            header_map.insert(
                HeaderName::from_str(split[0])?,
                HeaderValue::from_str(split[1].trim_start())?,
            );
        } else {
            return Err(anyhow!("header error: '{}'", x));
        }
    }

    for (key, val) in header_map {
        request_builder = request_builder.header(key.ok_or(anyhow!("Key is null"))?, val);
    }

    let request = request_builder.body(Full::new(body_bytes))?;

    if cli.verbosity >= 1 {
        debug!(
            "> {} {} {:?}",
            request.method(),
            request.uri().path(),
            request.version()
        );
        for (key, value) in request.headers().iter() {
            debug!("> {}: {}", key, value.to_str()?);
        }
        debug!(
            "> Content-Length: {}",
            request.body().size_hint().exact().unwrap_or(0)
        );
        debug!(">");
    }

    Ok(request)
}

async fn send_request(
    cli: &Cli,
    request: Request<Full<Bytes>>,
    scheme: &str,
    timings: &mut Option<RequestTimings>,
) -> Result<Response<Incoming>, anyhow::Error> {
    let uri = request.uri();
    let host = uri.host().map(|h| h.to_string());

    let fut = async {
        if scheme == "https" {
            send_https_request(cli, request, host.as_deref(), timings).await
        } else {
            send_http_request(cli, request, timings).await
        }
    };
    let res = timeout(Duration::from_secs(30), fut)
        .await
        .context("Request timed out after 30 seconds")?
        .context("Failed to execute request")?;

    if cli.verbosity >= 1 {
        debug!("< {:?} {}", res.version(), res.status());
        for (key, value) in res.headers().iter() {
            debug!("< {}: {}", key, value.to_str()?);
        }
        debug!("<");
    }

    Ok(res)
}

async fn send_https_request(
    cli: &Cli,
    request: Request<Full<Bytes>>,
    host: Option<&str>,
    timings: &mut Option<RequestTimings>,
) -> Result<Response<Incoming>, anyhow::Error> {
    // Get TLS info and/or cert info before the request
    if cli.tls_info || cli.cert_info {
        if let Some(host) = host {
            let uri = request.uri();
            let port = uri.port_u16().unwrap_or(443);

            match get_tls_info(
                host,
                port,
                cli.skip_certificate_validate,
                cli.certificate_path_option.as_ref(),
            )
            .await
            {
                Ok((tls_info, cert_info)) => {
                    if cli.tls_info {
                        println!("{}", tls_info);
                    }
                    if cli.cert_info {
                        if let Some(cert) = cert_info {
                            println!("{}", cert);
                        } else {
                            println!("No certificate information available");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get TLS info: {}", e);
                }
            }
        }
    }
    let connection_start = Instant::now();

    let mut root_store = RootCertStore::empty();
    if let Some(file_path) = cli.certificate_path_option.as_ref() {
        let f = std::fs::File::open(file_path)?;
        let mut rd = std::io::BufReader::new(f);
        for cert in rustls_pemfile::certs(&mut rd) {
            root_store.add(cert?)?;
        }
    } else {
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    };

    let provider = Arc::new(rustls::crypto::CryptoProvider {
        cipher_suites: DEFAULT_CIPHER_SUITES.to_vec(),
        ..default_provider()
    });

    let rcurl_verifier = RcurlCertVerifier::new(
        cli.verbosity,
        cli.skip_certificate_validate,
        provider.clone(),
        &root_store,
    )?;

    let mut tls_config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)?
        .with_root_certificates(root_store)
        .with_no_client_auth();

    tls_config
        .dangerous()
        .set_certificate_verifier(Arc::new(rcurl_verifier));

    // Check for proxy configuration
    let use_proxy = !cli.noproxy && !should_bypass_proxy(host);

    if let Some(proxy_addr) = use_proxy.then(|| get_proxy_from_env("https")).flatten() {
        eprintln!("* Using HTTPS proxy: {}", proxy_addr);

        let connector_builder = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_or_http();

        let proxy_connector = HttpProxyConnector::new(proxy_addr);

        let https_connector = match (cli.http2, cli.http2_prior_knowledge) {
            (true, _) => connector_builder
                .enable_all_versions()
                .wrap_connector(proxy_connector),
            (_, true) => connector_builder
                .enable_http2()
                .wrap_connector(proxy_connector),
            _ => connector_builder
                .enable_http1()
                .wrap_connector(proxy_connector),
        };

        let https_client: Client<_, Full<Bytes>> =
            Client::builder(TokioExecutor::new()).build(https_connector);
        let response = https_client.request(request).await?;

        if let Some(t) = timings {
            t.end_tls();
        }

        return Ok(response);
    }

    // Direct connection
    let connector_builder = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_or_http();

    let resolver = DnsLoggingResolver::new();
    let mut connector = HttpConnector::new_with_resolver(resolver);
    connector.enforce_http(false);

    let https_connector = if cli.http2 {
        connector_builder
            .enable_all_versions()
            .wrap_connector(connector)
    } else if cli.http2_prior_knowledge {
        connector_builder.enable_http2().wrap_connector(connector)
    } else {
        connector_builder.enable_http1().wrap_connector(connector)
    };

    let https_client: Client<_, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(https_connector);
    let response = https_client.request(request).await?;

    // Update timings
    if let Some(t) = timings {
        let total_connection_time = connection_start.elapsed();

        // Estimate DNS time (roughly 10-30ms typically)
        let estimated_dns = Duration::from_millis(20);
        t.dns_start = Some(connection_start);
        t.dns_end = Some(connection_start + estimated_dns);

        // TCP connect time (connection time minus DNS and TLS)
        let tls_time = total_connection_time.saturating_sub(estimated_dns);
        t.tcp_connect_start = Some(connection_start + estimated_dns);
        t.tcp_connect_end = Some(connection_start + estimated_dns + tls_time / 2);

        // TLS handshake time
        t.tls_start = Some(connection_start + estimated_dns + tls_time / 2);
        t.tls_end = Some(connection_start + total_connection_time);
    }

    Ok(response)
}

async fn send_http_request(
    cli: &Cli,
    mut request: Request<Full<Bytes>>,
    timings: &mut Option<RequestTimings>,
) -> Result<Response<Incoming>, anyhow::Error> {
    let connection_start = Instant::now();
    let uri = request.uri();
    let host = uri.host();

    // Check for proxy configuration
    let use_proxy = !cli.noproxy && !should_bypass_proxy(host);

    if use_proxy && let Some(proxy_addr) = get_proxy_from_env("http") {
        eprintln!("* Using HTTP proxy: {}", proxy_addr);

        // For HTTP proxy, ensure URI includes scheme and host
        let original_uri = request.uri().clone();
        *request.uri_mut() = original_uri;

        let mut proxy_connector = HttpForwardProxyConnector::new(proxy_addr);
        let uri = request.uri().clone();
        let io = proxy_connector.call(uri).await?;

        // 2. HTTP/1.1 handshake（关键）
        let (mut sender, conn) = http1::handshake(io).await?;

        // 3. 驱动连接（必须）
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("proxy connection error: {:?}", e);
            }
        });

        // 4. 直接发送 request（absolute-form 不会被改）
        let resp = sender.send_request(request).await?;
        return Ok(resp);
    }

    // Direct connection
    let resolver = DnsLoggingResolver::new();
    let connector = HttpConnector::new_with_resolver(resolver);
    let http_client: Client<_, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(connector);
    let resp = http_client.request(request).await?;

    // Update timings
    if let Some(t) = timings {
        let total_connection_time = connection_start.elapsed();

        // Estimate DNS time
        let estimated_dns = Duration::from_millis(15);
        t.dns_start = Some(connection_start);
        t.dns_end = Some(connection_start + estimated_dns);

        // TCP connect time (remaining time)
        t.tcp_connect_start = Some(connection_start + estimated_dns);
        t.tcp_connect_end = Some(connection_start + total_connection_time);
    }

    Ok(resp)
}

async fn download_file_with_progress(
    file_path: &str,
    total_size: u64,
    mut body_stream: BodyStream<Incoming>,
) -> Result<(), anyhow::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(file_path)
        .context(format!("Failed to open or create file: {}", file_path))?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )?
        .progress_chars("#>-"),
    );

    let mut downloaded = 0;
    while let Some(chunk_result) = body_stream.next().await {
        let bytes = chunk_result
            .context("Error while downloading file stream")?
            .into_data()
            .map_err(|e| anyhow!("Error while downloading file stream,{:?}", e))?;

        file.write_all(&bytes)
            .context("Error writing chunk to file")?;
        let new = min(downloaded + bytes.len() as u64, total_size);
        downloaded = new;
        pb.set_position(new);
    }

    pb.finish_with_message("Download complete");
    Ok(())
}

pub async fn handle_response(
    cli: &Cli,
    res: Response<Incoming>,
    mut timings: Option<RequestTimings>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, anyhow::Error> {
    // End total timing and display timing info if --time flag is set
    if cli.time
        && let Some(ref mut t) = timings
    {
        t.end_total();
        println!("{}", t);
    }

    if cli.header_option {
        info!("{:?} {}", res.version(), res.status());
        for (key, value) in res.headers().iter() {
            info!("{}: {}", key, value.to_str()?);
        }
        let t = res
            .map(|b| b.boxed())
            .map(|item| item.map_err(|_| -> Infallible { unreachable!() }).boxed());
        return Ok(t);
    }

    let (parts, incoming) = res.into_parts();

    if let Some(file_path) = cli.file_path_option.as_ref() {
        let content_length = parts
            .headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        download_file_with_progress(file_path, content_length, BodyStream::new(incoming)).await?;

        let empty_body = Full::new(Bytes::new()).boxed();
        let response = Response::from_parts(parts, empty_body);
        Ok(response)
    } else {
        let content_length = parts
            .headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        let body_bytes = incoming.collect().await?.to_bytes();

        if let Some(length) = content_length
            && length > 1024 * 1024 * 100
        {
            return Err(anyhow!("Binary output can mess up your terminal..."));
        }

        match String::from_utf8(body_bytes.to_vec()) {
            Ok(text) => print!("{text}"),
            Err(_) => {
                error!(
                    "[rcurl: warning] response body is not valid UTF-8 and was not written to a file."
                );
                error!("[rcurl: warning] to save to a file, use `-o <filename>`");
            }
        }
        std::io::stdout().flush()?;

        let response = Response::from_parts(parts, Full::new(body_bytes).boxed());
        Ok(response)
    }
}
