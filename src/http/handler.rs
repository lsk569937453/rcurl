use crate::cli::app_config::Cli;
use bytes::Bytes;

use http::header::ACCEPT;
use http_body_util::Full;
use hyper::header::CONTENT_TYPE;
use hyper::header::COOKIE;
use hyper::header::REFERER;
use hyper::header::{HeaderName, HeaderValue};
use hyper::Request;
use hyper_util::rt::TokioExecutor;
use mime_guess::mime;
use rustls::RootCertStore;
use std::time::Duration;

use tokio::time::timeout;

use hyper::header::RANGE;
use std::path::Path;

use hyper::header::USER_AGENT;

use rustls::client::danger::HandshakeSignatureValid;

use form_data_builder::FormData;
use http_body_util::BodyStream;
use hyper::body::Incoming;
use hyper::header::CONTENT_LENGTH;
use hyper::Response;
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use rustls::crypto::ring::default_provider;

use futures::StreamExt;
use http_body_util::BodyExt;

use http_body_util::combinators::BoxBody;
use hyper::body::Buf;
use hyper::HeaderMap;
use hyper_util::client::legacy::Client;
use rustls::crypto::ring::DEFAULT_CIPHER_SUITES;
use rustls::crypto::CryptoProvider;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::ServerName;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct};
use std::cmp::min;
use std::convert::From;
use std::convert::Infallible;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::Write as WriteStd;
use std::str::FromStr;
use std::sync::Arc;
#[derive(Debug)]
pub struct NoCertificateVerification(CryptoProvider);

impl NoCertificateVerification {
    pub fn new(provider: CryptoProvider) -> Self {
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
        verify_tls12_signature(
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
        verify_tls13_signature(
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
    let mut body_for_test = Full::new(Bytes::from("")).boxed();

    let content_length_option = parts.headers.get(CONTENT_LENGTH);

    if let Some(content_lenth_str) = content_length_option {
        let content_length = content_lenth_str.to_str()?.parse::<u64>()?;

        if let Some(file_path) = cli.file_path_option.clone() {
            let mut body_streaming = BodyStream::new(incoming);
            let mut downloaded = 0;
            let total_size = content_length;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .open(file_path)?;
            let pb = ProgressBar::new(total_size);
            pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        ?
        .progress_chars("#>-"));

            while let Some(Ok(t)) = body_streaming.next().await {
                if let Ok(bytes) = t.into_data() {
                    let new = min(downloaded + bytes.len() as u64, total_size);
                    downloaded = new;
                    pb.set_position(new);
                    file.write_all(&bytes)?;
                }
            }
            pb.finish_with_message("downloaded");
        } else {
            if content_length > 1024 * 1024 * 100 {
                return Err(anyhow!(
                    "Binary output can mess up your terminal. Use '--output -' to tell
rcurl to output it to your terminal anyway, or consider '--output
<FILE>' to save to a file."
                ));
            }
            let mut body = incoming.collect().await?.aggregate();
            let dst = body.copy_to_bytes(content_length as usize);
            let response_string = String::from_utf8_lossy(&dst);
            body_for_test = Full::new(Bytes::from(response_string.clone().to_string())).boxed();
            println!("{response_string}");
        }
    }
    let res = Response::from_parts(parts, body_for_test);
    Ok(res)
}
pub async fn http_request(
    cli: Cli,
    scheme: &str,
) -> Result<Response<BoxBody<Bytes, Infallible>>, anyhow::Error> {
    let mut root_store = RootCertStore::empty();

    if let Some(file_path) = cli.certificate_path_option.clone() {
        let f = std::fs::File::open(file_path.clone())?;
        let mut rd = std::io::BufReader::new(f);
        for cert in rustls_pemfile::certs(&mut rd) {
            root_store.add(cert?)?;
        }
    } else {
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    };
    let versions = rustls::DEFAULT_VERSIONS.to_vec();
    let mut tls_config = ClientConfig::builder_with_provider(
        CryptoProvider {
            cipher_suites: DEFAULT_CIPHER_SUITES.to_vec(),
            ..default_provider()
        }
        .into(),
    )
    .with_protocol_versions(&versions)?
    .with_root_certificates(root_store)
    .with_no_client_auth();

    tls_config.key_log = Arc::new(rustls::KeyLogFile::new());

    if cli.skip_certificate_validate {
        tls_config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification::new(default_provider())));
    };
    let uri: hyper::Uri = cli.url.parse()?;
    let mut method = String::from("GET");
    let mut content_type_option = None;

    if cli.body_option.is_some() {
        method = String::from("POST");
        content_type_option = Some(String::from("application/x-www-form-urlencoded"));
    }
    if cli.uploadfile_option.is_some() {
        method = String::from("PUT");
    }
    if let Some(method_userdefined) = cli.method_option.clone() {
        method = method_userdefined;
    }
    let mut request_builder = Request::builder()
        .method(method.as_str())
        .uri(cli.url.clone());
    if cli.http2_prior_knowledge {
        request_builder = request_builder.version(hyper::Version::HTTP_2);
    }
    let mut header_map = HeaderMap::new();
    if let Some(content_type) = content_type_option {
        header_map.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
    }
    header_map.insert(ACCEPT, HeaderValue::from_str("*/*")?);
    // header_map.insert(
    //     HOST,
    //     HeaderValue::from_str(uri.host().ok_or(anyhow!("no host"))?)?,
    // );
    let user_agent = cli
        .user_agent_option
        .clone()
        .unwrap_or(format!("rcurl/{}", env!("CARGO_PKG_VERSION")));
    header_map.insert(USER_AGENT, HeaderValue::from_str(&user_agent)?);
    if let Some(cookie) = cli.cookie_option.clone() {
        header_map.insert(COOKIE, HeaderValue::from_str(&cookie)?);
    }
    if let Some(range) = cli.range_option.clone() {
        let ranges_format = format!("bytes={range}");
        header_map.insert(RANGE, HeaderValue::from_str(&ranges_format)?);
    }
    if let Some(refer) = cli.refer_option.clone() {
        header_map.insert(REFERER, HeaderValue::from_str(&refer)?);
    }
    let mut body_bytes = Bytes::new();
    if !cli.form_option.is_empty() {
        let mut form = FormData::new(Vec::new()); // use a Vec<u8> as a writer
        let form_header = form.content_type_header(); // add this `Content-Type` header to your HTTP request

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
    } else if let Some(body) = cli.body_option.clone() {
        body_bytes = Bytes::from(body);
    } else if let Some(upload_file) = cli.uploadfile_option.clone() {
        let byte_vec = tokio::fs::read(upload_file).await?;
        body_bytes = Bytes::from(byte_vec);
    }

    if cli.header_option {
        request_builder = request_builder.method("HEAD");
    }
    for x in cli.headers.clone() {
        let split: Vec<String> = x.splitn(2, ':').map(|s| s.to_string()).collect();

        if split.len() == 2 {
            let key = &split[0];
            let value = &split[1];
            let new_value = value.trim_start();

            header_map.insert(
                HeaderName::from_str(key.as_str())?,
                HeaderValue::from_str(new_value)?,
            );
        } else {
            return Err(anyhow!("header error"));
        }
    }
    for (key, val) in header_map {
        request_builder = request_builder.header(key.ok_or(anyhow!(""))?, val);
    }
    let content_length = body_bytes.len();
    let body = Full::new(body_bytes);
    let request = request_builder.body(body)?;

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
        println!("> Content-Length: {content_length}");
    }

    let span = tracing::info_span!("Rcurl");
    let _enter = span.enter();
    let request_future = {
        trace!("Start request");
        let fut = if scheme == "https" {
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

            let https_clientt: Client<_, Full<Bytes>> =
                Client::builder(TokioExecutor::new()).build(https_connector);
            https_clientt.request(request)
        } else {
            let mut http_client_builder = Client::builder(TokioExecutor::new());
            http_client_builder
                .http1_title_case_headers(true)
                .http1_preserve_header_case(true);
            let https_connector = if cli.http2 {
                http_client_builder.http2_only(false).build_http()
            } else if cli.http2_prior_knowledge {
                http_client_builder.http2_only(true).build_http()
            } else {
                http_client_builder.build_http()
            };
            https_connector.request(request)
        };
        fut
    };
    let res = timeout(Duration::from_secs(5), request_future)
        .await
        .map_err(|e| anyhow!("Request timeout in 5 seconds, {}", e))?
        .map_err(|e| anyhow!("Request failed , {}", e))?;
    if cli.debug {
        let status = res.status();
        println!("< {:?} {}", res.version(), status);
        for (key, value) in res.headers().iter() {
            println!("< {}: {}", key, value.to_str()?);
        }
    }

    let parts = handle_response(&cli, res).await?;

    Ok(parts)
}
