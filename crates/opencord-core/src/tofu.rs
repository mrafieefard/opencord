//! TLS trust. Certificates are pinned by SHA-256 fingerprint on first use,
//! like SSH host keys; certificates from a public CA are accepted without a
//! pin.

use std::sync::{Arc, Mutex, PoisonError};

use opencord_common::address::Fingerprint;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use sha2::{Digest, Sha256};

/// What the verifier concluded about the server's certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Matches the pinned fingerprint.
    Pinned(Fingerprint),
    /// Not pinned, but valid for a public certificate authority.
    Authority(Fingerprint),
    /// Not pinned and not from a public authority; the user must decide.
    Untrusted(Fingerprint),
    /// Differs from the pinned fingerprint. Never accept silently.
    Mismatch {
        expected: Fingerprint,
        presented: Fingerprint,
    },
}

impl Verdict {
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Pinned(_) | Self::Authority(_))
    }
}

/// The trust decision, without any TLS machinery.
pub fn decide(
    expected: Option<Fingerprint>,
    presented: Fingerprint,
    valid_for_authority: impl FnOnce() -> bool,
) -> Verdict {
    match expected {
        Some(expected) if expected == presented => Verdict::Pinned(presented),
        Some(expected) => Verdict::Mismatch {
            expected,
            presented,
        },
        None if valid_for_authority() => Verdict::Authority(presented),
        None => Verdict::Untrusted(presented),
    }
}

pub fn fingerprint(cert: &CertificateDer<'_>) -> Fingerprint {
    Sha256::digest(cert.as_ref()).into()
}

/// A client config that verifies with [`decide`] and remembers the verdict.
#[derive(Debug, Clone)]
pub struct TlsTrust {
    pub config: Arc<ClientConfig>,
    verdict: Arc<Mutex<Option<Verdict>>>,
}

impl TlsTrust {
    pub fn new(expected: Option<Fingerprint>) -> Self {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let authority =
            WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
                .build()
                .expect("the bundled root certificates are valid");
        let verdict = Arc::new(Mutex::new(None));
        let verifier = Arc::new(TofuVerifier {
            expected,
            authority,
            algorithms: provider.signature_verification_algorithms,
            verdict: Arc::clone(&verdict),
        });
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("ring supports the default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();
        Self {
            config: Arc::new(config),
            verdict,
        }
    }

    /// Set once the server's certificate has been checked.
    pub fn verdict(&self) -> Option<Verdict> {
        *self.verdict.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct TofuVerifier {
    expected: Option<Fingerprint>,
    authority: Arc<WebPkiServerVerifier>,
    algorithms: WebPkiSupportedAlgorithms,
    verdict: Arc<Mutex<Option<Verdict>>>,
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let verdict = decide(self.expected, fingerprint(end_entity), || {
            self.authority
                .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
                .is_ok()
        });
        *self.verdict.lock().unwrap_or_else(PoisonError::into_inner) = Some(verdict);
        if verdict.is_accepted() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Fingerprint = [0xaa; 32];
    const B: Fingerprint = [0xbb; 32];

    #[test]
    fn a_matching_pin_is_accepted_without_asking_authorities() {
        let verdict = decide(Some(A), A, || panic!("authorities are not consulted"));

        assert_eq!(verdict, Verdict::Pinned(A));
        assert!(verdict.is_accepted());
    }

    #[test]
    fn a_pin_beats_a_valid_authority_certificate() {
        let verdict = decide(Some(A), B, || true);

        assert_eq!(
            verdict,
            Verdict::Mismatch {
                expected: A,
                presented: B
            }
        );
        assert!(!verdict.is_accepted());
    }

    #[test]
    fn unpinned_certificates_need_an_authority() {
        assert_eq!(decide(None, A, || true), Verdict::Authority(A));
        assert_eq!(decide(None, A, || false), Verdict::Untrusted(A));
        assert!(!Verdict::Untrusted(A).is_accepted());
    }
}
