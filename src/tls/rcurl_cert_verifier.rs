use pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::client::danger::HandshakeSignatureValid;
use rustls::client::danger::ServerCertVerified;
use rustls::client::danger::ServerCertVerifier;
use rustls::crypto::CryptoProvider;
use rustls::DigitallySignedStruct;
use rustls::Error;
use rustls::RootCertStore;
use rustls::SignatureScheme;
use std::sync::Arc;
use x509_parser::parse_x509_certificate;
#[derive(Debug)]
pub struct RcurlCertVerifier {
    verifier: Arc<dyn ServerCertVerifier>,
    verbosity: u8,
    skip_validate: bool,
}

impl RcurlCertVerifier {
    pub fn new(
        verbosity: u8,
        skip_validate: bool,
        provider: Arc<CryptoProvider>,
        root_store: &RootCertStore,
    ) -> Result<Self, anyhow::Error> {
        let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
            root_store.clone().into(),
            provider,
        )
        .build()?; // ← 关键点：使用 ?

        Ok(Self {
            verifier,
            verbosity,
            skip_validate,
        })
    }
}

impl ServerCertVerifier for RcurlCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if self.verbosity >= 1 {
            debug!("\n[rcurl: debug] Server certificate received:");
            match parse_x509_certificate(end_entity.as_ref()) {
                Ok((_, cert)) => {
                    debug!("  Subject: {}", cert.subject());
                    debug!("  Issuer: {}", cert.issuer());
                    debug!(
                        "  Validity: {} - {}",
                        cert.validity().not_before,
                        cert.validity().not_after
                    );
                    if let Ok(Some(san)) = cert.tbs_certificate.subject_alternative_name() {
                        debug!("  Subject Alternative Names:");
                        for name in &san.value.general_names {
                            debug!("    - {}", name);
                        }
                    }
                }
                Err(e) => error!("[rcurl: debug] Failed to parse server certificate: {e}"),
            }
            println!();
        }

        if self.skip_validate {
            Ok(ServerCertVerified::assertion())
        } else {
            self.verifier
                .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.verifier.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}
