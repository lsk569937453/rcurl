use std::str::FromStr;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate tracing;
use clap::Parser;
use hyper::header::{HeaderName, HeaderValue};
use hyper::Request;
use hyper_util::rt::TokioIo;
use rustls::internal::msgs::handshake::Random;
use rustls::{ContentType, RootCertStore};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{Instrument, Level};
mod http;
use crate::http::handler::handle_response;
use bytes::Bytes;
use http_body_util::Full;
use hyper::header::COOKIE;
use hyper::header::HOST;
use hyper::header::RANGE;
use hyper::header::USER_AGENT;

use rustls::client::danger::HandshakeSignatureValid;
use rustls::client::danger::ServerCertVerifier;
use rustls::client::WebPkiServerVerifier as WebPkiVerifier;
use rustls::crypto::ring::default_provider;
use rustls::crypto::ring::DEFAULT_CIPHER_SUITES;
use rustls::crypto::CryptoProvider;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::ServerName;
use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct};
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

#[derive(Debug)]
pub struct NoHostnameTlsVerifier {
    verifier: Arc<WebPkiVerifier>,
}

impl ServerCertVerifier for NoHostnameTlsVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        match self.verifier.verify_server_cert(
            _end_entity,
            _intermediates,
            _server_name,
            _ocsp,
            _now,
        ) {
            Ok(res) => Ok(res),
            Err(e) => match e {
                rustls::Error::InvalidCertificate(reason) => {
                    if reason == CertificateError::NotValidForName {
                        Ok(rustls::client::danger::ServerCertVerified::assertion())
                    } else {
                        Err(rustls::Error::InvalidCertificate(reason))
                    }
                }
                _ => Err(e),
            },
        }
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
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
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
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
#[derive(Parser)]
#[command(author, version, about, long_about)]
struct Cli {
    /// The request url,like http://www.google.com
    url: String,
    /// The http method,like GET,POST,etc.
    #[arg(short = 'X', long, value_name = "HTTP Method")]
    method_option: Option<String>,
    /// The body of the http request.
    #[arg(short = 'd', long)]
    body_option: Option<String>,
    /// The http headers.
    #[arg(short = 'H', long)]
    headers: Vec<String>,
    /// The pem path.
    #[arg(short = 'c', long)]
    certificate_path_option: Option<String>,

    /// The User Agent.
    #[arg(short = 'A', long)]
    user_agent_option: Option<String>,
    /// The Cookie option.
    #[arg(short = 'b', long)]
    cookie_option: Option<String>,

    /// The downloading file path .
    #[arg(global = true, short = 'o', long, default_missing_value = "none")]
    file_path_option: Option<String>,

    /// Skip certificate validation.
    #[arg(short = 'k', long)]
    skip_certificate_validate: bool,

    /// Range validation.
    #[arg(short = 'r', long)]
    range_option: Option<String>,
    /// The debug switch.
    #[arg(short = 'v', long)]
    debug: bool,
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
    let body = cli
        .body_option
        .map_or(Full::new(Bytes::new()), |v| Full::new(Bytes::from(v)));

    let method = cli.method_option.map_or(String::from("GET"), |x| x);
    let mut request = Request::builder()
        .method(method.as_str())
        .uri(cli.url)
        .body(body)?;
    request.headers_mut().append(
        HOST,
        HeaderValue::from_str(uri.host().ok_or(anyhow!("no host"))?)?,
    );
    let user_agent = cli
        .user_agent_option
        .unwrap_or(format!("rcur/{}", env!("CARGO_PKG_VERSION").to_string()));
    request
        .headers_mut()
        .append(USER_AGENT, HeaderValue::from_str(&user_agent)?);
    if let Some(cookie) = cli.cookie_option {
        request
            .headers_mut()
            .append(COOKIE, HeaderValue::from_str(&cookie)?);
    }
    if let Some(range) = cli.range_option {
        let ranges_format = format!("bytes={}", range);
        request
            .headers_mut()
            .append(RANGE, HeaderValue::from_str(&ranges_format)?);
    }
    for x in cli.headers {
        let split: Vec<String> = x.splitn(2, ':').map(|s| s.to_string()).collect();
        if split.len() == 2 {
            let key = &split[0];
            let value = &split[1];
            request.headers_mut().append(
                HeaderName::from_str(key.as_str())?,
                HeaderValue::from_str(value.as_str())?,
            );
        } else {
            return Err(anyhow!("header error"));
        }
    }
    if cli.debug {
        for (key, value) in request.headers().iter() {
            println!("> {}: {}", key, value.to_str()?);
        }
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
    handle_response(cli.file_path_option, res).await?;

    Ok(())
}
