use crate::cli::app_config::Cli;
use anyhow::{anyhow, Context};
use bytes::Bytes;
use form_data_builder::FormData;
use futures::StreamExt;
use http::header::{
    HeaderName, HeaderValue, ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, USER_AGENT,
};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, BodyStream, Full};
use hyper::body::{Body, Incoming};
use hyper::{Request, Response, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use indicatif::{ProgressBar, ProgressStyle};
use mime_guess::mime;
use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::ring::{default_provider, DEFAULT_CIPHER_SUITES};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore};
use std::cmp::min;
use std::convert::Infallible;
use std::fs::OpenOptions;
use std::io::Write as WriteStd;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use url::Url;

const MAX_REDIRECTS: u8 = 10;

#[derive(Debug)]
pub struct NoCertificateVerification(rustls::crypto::CryptoProvider);

impl NoCertificateVerification {
    pub fn new(provider: rustls::crypto::CryptoProvider) -> Self {
        Self(provider)
    }
}
impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub async fn http_request_with_redirects(
    cli: Cli,
) -> Result<Response<BoxBody<Bytes, Infallible>>, anyhow::Error> {
    let mut current_url: Url = cli.url.parse().context("Failed to parse initial URL")?;

    for i in 0..MAX_REDIRECTS {
        let uri: Uri = current_url.to_string().parse()?;
        let scheme = uri.scheme_str().unwrap_or("http");

        let request = build_request(&cli, &uri)?;

        let res = send_request(&cli, request, scheme).await?;

        let status = res.status();
        if status.is_redirection() {
            if let Some(location_header) = res.headers().get(http::header::LOCATION) {
                let location_str = location_header.to_str()?;
                current_url = current_url.join(location_str)?;

                if cli.debug {
                    println!(
                        "\nRedirecting to: {current_url} ({}/{MAX_REDIRECTS})\n",
                        i + 1
                    );
                }
                continue;
            } else {
                return Err(anyhow!("Redirect response missing 'location' header"));
            }
        }

        return handle_response(&cli, res).await;
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
        request_builder = request_builder.header(key.unwrap(), val);
    }

    let request = request_builder.body(Full::new(body_bytes))?;

    if cli.debug {
        println!(
            "> {} {} {:?}",
            request.method(),
            request.uri().path(),
            request.version()
        );
        for (key, value) in request.headers().iter() {
            println!("> {}: {}", key, value.to_str()?);
        }
        println!(
            "> Content-Length: {}",
            request.body().size_hint().exact().unwrap_or(0)
        );
        println!(">");
    }

    Ok(request)
}

async fn send_request(
    cli: &Cli,
    request: Request<Full<Bytes>>,
    scheme: &str,
) -> Result<Response<Incoming>, anyhow::Error> {
    let request_future = if scheme == "https" {
        // TLS 配置逻辑
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

        let mut tls_config = ClientConfig::builder_with_provider(
            rustls::crypto::CryptoProvider {
                cipher_suites: DEFAULT_CIPHER_SUITES.to_vec(),
                ..default_provider()
            }
            .into(),
        )
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)?
        .with_root_certificates(root_store)
        .with_no_client_auth();

        if cli.skip_certificate_validate {
            tls_config.dangerous().set_certificate_verifier(Arc::new(
                NoCertificateVerification::new(default_provider()),
            ));
        };

        let connector_builder = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_only();

        let https_connector = if cli.http2 {
            connector_builder.enable_all_versions().build()
        } else if cli.http2_prior_knowledge {
            connector_builder.enable_http2().build()
        } else {
            connector_builder.enable_http1().build()
        };

        let https_client: Client<_, Full<Bytes>> =
            Client::builder(TokioExecutor::new()).build(https_connector);
        https_client.request(request)
    } else {
        let http_client: Client<_, Full<Bytes>> =
            Client::builder(TokioExecutor::new()).build_http();
        http_client.request(request)
    };

    let res = timeout(Duration::from_secs(30), request_future) // 增加超时时间
        .await
        .context("Request timed out after 30 seconds")?
        .context("Failed to execute request")?;

    if cli.debug {
        println!("< {:?} {}", res.version(), res.status());
        for (key, value) in res.headers().iter() {
            println!("< {}: {}", key, value.to_str()?);
        }
        println!("<");
    }

    Ok(res)
}

async fn download_file_with_progress(
    file_path: &str,
    total_size: u64,
    mut body_stream: BodyStream<Incoming>,
) -> Result<(), anyhow::Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
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
            .map_err(|e| anyhow!("Error while downloading file stream"))?;

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
) -> Result<Response<BoxBody<Bytes, Infallible>>, anyhow::Error> {
    if cli.header_option {
        println!("{:?} {}", res.version(), res.status());
        for (key, value) in res.headers().iter() {
            println!("{}: {}", key, value.to_str()?);
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

        if let Some(length) = content_length {
            if length > 1024 * 1024 * 100 {
                return Err(anyhow!("Binary output can mess up your terminal..."));
            }
        }

        match String::from_utf8(body_bytes.to_vec()) {
            Ok(text) => print!("{text}"),
            Err(_) => {
                println!("[rcurl: warning] response body is not valid UTF-8 and was not written to a file.");
                println!("[rcurl: warning] to save to a file, use `-o <filename>`");
            }
        }
        std::io::stdout().flush()?; // 确保内容立即打印

        let response = Response::from_parts(parts, Full::new(body_bytes).boxed());
        Ok(response)
    }
}
