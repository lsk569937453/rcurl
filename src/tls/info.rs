use anyhow::Result;
use pki_types::ServerName;
use rustls::crypto::ring::{DEFAULT_CIPHER_SUITES, default_provider};
use rustls::{ClientConfig, RootCertStore};
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use x509_parser::parse_x509_certificate;

/// TLS connection information
#[derive(Debug)]
pub struct TlsInfo {
    pub tls_version: String,
    pub cipher_suite: String,
    pub peer_certificates: Vec<CertificateInfo>,
}

/// Certificate information
#[derive(Debug)]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub validity_not_before: String,
    pub validity_not_after: String,
    pub subject_alt_names: Vec<String>,
    pub key_size: Option<usize>,
    pub signature_algorithm: String,
    pub public_key_algorithm: String,
}

impl std::fmt::Display for TlsInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "TLS Handshake Information:")?;
        writeln!(f, "  TLS Version: {}", self.tls_version)?;
        writeln!(f, "  Cipher Suite: {}", self.cipher_suite)?;
        writeln!(f, "  Number of Certificates: {}", self.peer_certificates.len())?;
        Ok(())
    }
}

impl std::fmt::Display for CertificateInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Certificate Information:")?;
        writeln!(f, "  Subject: {}", self.subject)?;
        writeln!(f, "  Issuer: {}", self.issuer)?;
        writeln!(f, "  Serial Number: {}", self.serial_number)?;
        writeln!(f, "  Valid From: {}", self.validity_not_before)?;
        writeln!(f, "  Valid To: {}", self.validity_not_after)?;
        if !self.subject_alt_names.is_empty() {
            writeln!(f, "  Subject Alternative Names:")?;
            for san in &self.subject_alt_names {
                writeln!(f, "    - {}", san)?;
            }
        }
        if let Some(key_size) = self.key_size {
            writeln!(f, "  Public Key Size: {} bits", key_size)?;
        }
        writeln!(f, "  Signature Algorithm: {}", self.signature_algorithm)?;
        writeln!(f, "  Public Key Algorithm: {}", self.public_key_algorithm)?;
        Ok(())
    }
}

/// Get TLS and certificate information by performing a TLS handshake
pub async fn get_tls_info(
    host: &str,
    port: u16,
    skip_verify: bool,
    cert_path: Option<&String>,
) -> Result<(TlsInfo, Option<CertificateInfo>)> {
    // Resolve host address
    let addr = format!("{}:{}", host, port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("Failed to resolve address: {}:{}", host, port))?;

    // Connect TCP stream
    let stream = TcpStream::connect(&addr).await?;

    // Setup TLS config
    let mut root_store = RootCertStore::empty();
    if let Some(file_path) = cert_path {
        let f = std::fs::File::open(file_path)?;
        let mut rd = std::io::BufReader::new(f);
        for cert in rustls_pemfile::certs(&mut rd) {
            root_store.add(cert?)?;
        }
    } else {
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    let provider = Arc::new(rustls::crypto::CryptoProvider {
        cipher_suites: DEFAULT_CIPHER_SUITES.to_vec(),
        ..default_provider()
    });

    let config = if skip_verify {
        // For insecure mode, use normal config but skip cert verify
        let mut builder = ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)?
            .with_root_certificates(root_store.clone())
            .with_no_client_auth();

        // Skip certificate verification
        builder.dangerous().set_certificate_verifier(Arc::new(
            crate::tls::rcurl_cert_verifier::RcurlCertVerifier::new(
                0,
                true,
                provider,
                &root_store,
            )?
        ));
        builder
    } else {
        ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)?
            .with_root_certificates(root_store)
            .with_no_client_auth()
    };

    // Create TLS connector
    let connector = TlsConnector::from(Arc::new(config));
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|_| anyhow::anyhow!("Invalid server name: {}", host))?;

    // Perform TLS handshake
    let tls_stream = connector.connect(server_name, stream).await?;

    // Get TLS session info from the connection
    let tls_info = extract_tls_info_from_connection(&tls_stream);

    // Get certificate info
    let cert_info = extract_cert_info_from_connection(&tls_stream)?;

    Ok((tls_info, cert_info))
}

/// Extract TLS information from the TLS stream
fn extract_tls_info_from_connection<S>(
    stream: &tokio_rustls::client::TlsStream<S>,
) -> TlsInfo {
    // Get negotiated protocol version and cipher suite
    let tls_version = format!("{:?}", stream.get_ref().1.protocol_version());
    let cipher_suite = format!("{:?}", stream.get_ref().1.negotiated_cipher_suite());

    TlsInfo {
        tls_version,
        cipher_suite,
        peer_certificates: vec![],
    }
}

/// Extract certificate info from TLS connection
fn extract_cert_info_from_connection<S>(
    stream: &tokio_rustls::client::TlsStream<S>,
) -> Result<Option<CertificateInfo>> {
    if let Some(certs) = stream.get_ref().1.peer_certificates() {
        if let Some(end_entity) = certs.first() {
            let cert_info = extract_certificate_info(end_entity.as_ref())?;
            return Ok(Some(cert_info));
        }
    }
    Ok(None)
}

/// Extract certificate information from DER-encoded certificate
fn extract_certificate_info(der: &[u8]) -> Result<CertificateInfo> {
    let (_, cert) = parse_x509_certificate(der)
        .map_err(|e| anyhow::anyhow!("Failed to parse certificate: {}", e))?;

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();
    let serial_number = cert.serial.to_string();

    // Format validity dates
    let validity_not_before = format!("{}", cert.validity().not_before);
    let validity_not_after = format!("{}", cert.validity().not_after);

    // Extract Subject Alternative Names
    let subject_alt_names = if let Ok(Some(san)) = cert.tbs_certificate.subject_alternative_name() {
        san.value
            .general_names
            .iter()
            .map(|name| format!("{:?}", name))
            .collect()
    } else {
        vec![]
    };

    // Extract key information
    let public_key = cert.public_key();
    let public_key_algorithm = format!("{:?}", public_key.algorithm);
    let key_size = extract_key_size(&public_key);

    // Extract signature algorithm from OID
    let signature_algorithm = format!("{:?}", cert.signature_algorithm.algorithm);

    Ok(CertificateInfo {
        subject,
        issuer,
        serial_number,
        validity_not_before,
        validity_not_after,
        subject_alt_names,
        key_size,
        signature_algorithm,
        public_key_algorithm,
    })
}

/// Extract key size from public key info
fn extract_key_size(
    public_key: &x509_parser::x509::SubjectPublicKeyInfo,
) -> Option<usize> {
    // Try to get key size based on algorithm
    let algorithm_str = format!("{:?}", public_key.algorithm.algorithm);

    // Check for RSA by OID string
    if algorithm_str.contains("2.5.8.1.1") || algorithm_str.contains("rsa") {
        // RSA: calculate key size from modulus
        if let Ok(pk) = public_key.parsed() {
            if let x509_parser::public_key::PublicKey::RSA(rsa) = pk {
                return Some(rsa.modulus.len() * 8);
            }
        }
        None
    } else if algorithm_str.contains("ecPublicKey") || algorithm_str.contains("1.2.840.10045.2.1") {
        // ECDSA: approximate key size based on curve
        Some(256)
    } else if algorithm_str.contains("ed25519") || algorithm_str.contains("1.3.101.112") {
        Some(256)
    } else if algorithm_str.contains("ed448") || algorithm_str.contains("1.3.101.113") {
        Some(448)
    } else {
        None
    }
}
