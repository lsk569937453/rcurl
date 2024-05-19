use std::str::FromStr;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use clap::Parser;
use futures::TryStreamExt;
use hyper::header::{HeaderName, HeaderValue};
use hyper::Request;
use hyper_util::rt::TokioIo;
use mime_guess::mime;
use rustls::RootCertStore;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{Instrument, Level};
mod http;
use crate::http::handler::handle_response;
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::CONTENT_TYPE;
use hyper::header::COOKIE;
use hyper::header::HOST;
mod cli;
use hyper::header::RANGE;
use std::path::Path;

use hyper::header::USER_AGENT;

use rustls::client::danger::HandshakeSignatureValid;

use bytes::BytesMut;

use crate::cli::app_config::Cli;
use form_data_builder::FormData;
use rustls::crypto::ring::default_provider;
use rustls::crypto::ring::DEFAULT_CIPHER_SUITES;
use rustls::crypto::CryptoProvider;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::ServerName;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct};
use std::convert::From;
use std::sync::Arc;
use tokio_rustls::TlsConnector;
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

#[tokio::main]
async fn main() {
    let cli: Cli = Cli::parse();
    let log_level_hyper = if cli.debug { Level::TRACE } else { Level::INFO };

    tracing_subscriber::fmt()
        // Configure formatting settings.
        .with_level(true)
        .with_max_level(log_level_hyper)
        // Set the subscriber as the default.
        .init();
    if let Err(e) = do_request(cli).await {
        println!("{}", e);
    }
}
async fn do_request(cli: Cli) -> Result<(), anyhow::Error> {
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
    let host = uri.host().expect("uri has no host");
    let default_port = if let Some(port) = uri.port_u16() {
        port
    } else if uri.scheme_str() == Some("https") {
        443
    } else {
        80
    };
    let mut method = String::from("GET");
    let mut content_type_option = None;

    if cli.body_option.is_some() {
        method = String::from("POST");
        content_type_option = Some(String::from("application/x-www-form-urlencoded"));
    }
    if let Some(method_userdefined) = cli.method_option.clone() {
        method = method_userdefined;
    }
    let mut request_builder = Request::builder()
        .method(method.as_str())
        .uri(cli.url.clone());
    if let Some(content_type) = content_type_option {
        request_builder =
            request_builder.header(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
    }
    request_builder = request_builder.header(
        HOST,
        HeaderValue::from_str(uri.host().ok_or(anyhow!("no host"))?)?,
    );
    let user_agent = cli
        .user_agent_option
        .clone()
        .unwrap_or(format!("rcur/{}", env!("CARGO_PKG_VERSION").to_string()));
    request_builder = request_builder.header(USER_AGENT, HeaderValue::from_str(&user_agent)?);
    if let Some(cookie) = cli.cookie_option.clone() {
        request_builder = request_builder.header(COOKIE, HeaderValue::from_str(&cookie)?);
    }
    if let Some(range) = cli.range_option.clone() {
        let ranges_format = format!("bytes={}", range);
        request_builder = request_builder.header(RANGE, HeaderValue::from_str(&ranges_format)?);
    }
    let mut body_bytes = Bytes::new();
    if cli.form_option.len() != 0 {
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
    }

    if cli.header_option {
        request_builder = request_builder.method("HEAD");
    }
    for x in cli.headers.clone() {
        let split: Vec<String> = x.splitn(2, ':').map(|s| s.to_string()).collect();
        if split.len() == 2 {
            let key = &split[0];
            let value = &split[1];
            request_builder = request_builder.header(
                HeaderName::from_str(key.as_str())?,
                HeaderValue::from_str(value.as_str())?,
            );
        } else {
            return Err(anyhow!("header error"));
        }
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
        println!("> Content-Length: {}", content_length);
    }

    let port = uri.port_u16().unwrap_or(default_port);
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr.clone()).await?;
    let remote_addr = stream.peer_addr()?.to_string();
    let local_addr = stream.local_addr()?.to_string();
    let span = tracing::info_span!("Rcurl");
    let _enter = span.enter();
    let request_future = {
        trace!("Start request");
        let fut = if uri.scheme_str() == Some("https") {
            let connector = TlsConnector::from(Arc::new(tls_config));
            let domain = pki_types::ServerName::try_from(host)
                .map_err(|e| anyhow!("{}", e))?
                .to_owned();
            let tls_stream = connector.connect(domain, stream).await?;
            let stream_io = TokioIo::new(tls_stream);

            let (mut sender, conn) = hyper::client::conn::http1::handshake(stream_io)
                .instrument(info_span!("Https Handshake"))
                .await?;
            tokio::task::spawn(async move {
                if let Err(err) = conn
                    .instrument(info_span!(
                        "rcurl",
                        localAddr=%local_addr,
                         remoteAddr=remote_addr,

                    ))
                    .await
                {
                    println!("Connection failed: {:?}", err);
                }
            });
            sender.send_request(request)
        } else {
            let stream_io = TokioIo::new(stream);

            let (mut sender, conn) = hyper::client::conn::http1::handshake(stream_io)
                .instrument(info_span!("Http Handshake"))
                .await?;
            tokio::task::spawn(
                async move {
                    if let Err(err) = conn.await {
                        println!("Connection failed: {:?}", err);
                    }
                }
                .instrument(
                    info_span!(
                        "addr",
                        localAddr=%local_addr,
                        remoteAddr=remote_addr,

                    )
                    .or_current(),
                ),
            );
            sender.send_request(request)
        };
        fut
    };
    let res = timeout(Duration::from_secs(5), request_future)
        .await
        .map_err(|e| anyhow!("Request timeout in 5 seconds, {}", e))??;
    if cli.debug {
        let status = res.status();
        println!("< {:?} {}", res.version(), status);
        for (key, value) in res.headers().iter() {
            println!("< {}: {}", key, value.to_str()?);
        }
    }

    handle_response(&cli, res).await?;

    Ok(())
}
