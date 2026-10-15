//! Reading certificates: a PEM chain and key, and what the leaf says.

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::{FromDer, X509Certificate};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PemError {
    #[error("no certificate in the chain")]
    NoCertificate,
    #[error("the certificate cannot be read: {0}")]
    BadCertificate(String),
    #[error("no private key, or one that cannot be read")]
    BadKey,
    #[error("the private key does not belong to the certificate")]
    KeyMismatch,
    #[error("the certificate is not for {0}")]
    WrongName(String),
    #[error("the certificate has expired")]
    Expired,
}

/// What the first certificate of a chain says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    /// Who issued it: the organisation, else the common name.
    pub issuer: String,
    pub not_before_ms: i64,
    pub not_after_ms: i64,
    pub names: Vec<String>,
}

impl Leaf {
    /// Whether it is for `host`, a `*.` wildcard included.
    pub fn covers(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.names.iter().any(|name| {
            let name = name.to_ascii_lowercase();
            match name.strip_prefix("*.") {
                Some(parent) => host
                    .split_once('.')
                    .is_some_and(|(label, rest)| !label.is_empty() && rest == parent),
                None => name == host,
            }
        })
    }
}

pub fn chain(pem: &str) -> Result<Vec<CertificateDer<'static>>, PemError> {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|error| PemError::BadCertificate(error.to_string()))?;
    if chain.is_empty() {
        return Err(PemError::NoCertificate);
    }
    Ok(chain)
}

pub fn key(pem: &str) -> Result<PrivateKeyDer<'static>, PemError> {
    PrivateKeyDer::from_pem_slice(pem.as_bytes()).map_err(|_| PemError::BadKey)
}

pub fn leaf(certificate: &CertificateDer<'_>) -> Result<Leaf, PemError> {
    let (_, parsed) = X509Certificate::from_der(certificate.as_ref())
        .map_err(|error| PemError::BadCertificate(error.to_string()))?;
    let issuer = parsed
        .issuer()
        .iter_organization()
        .chain(parsed.issuer().iter_common_name())
        .find_map(|attribute| attribute.as_str().ok().map(str::to_owned))
        .unwrap_or_else(|| "Unknown".to_owned());
    let mut names: Vec<String> = parsed
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    GeneralName::DNSName(name) => Some((*name).to_owned()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        names.extend(
            parsed
                .subject()
                .iter_common_name()
                .filter_map(|attribute| attribute.as_str().ok().map(str::to_owned)),
        );
    }
    let validity = parsed.validity();
    Ok(Leaf {
        issuer,
        not_before_ms: validity.not_before.timestamp() * 1000,
        not_after_ms: validity.not_after.timestamp() * 1000,
        names,
    })
}

/// Checks an uploaded chain and key for `host` (plan §7.4): readable, the
/// key the certificate's, for that name, not expired.
pub fn check_upload(
    host: &str,
    chain_pem: &str,
    key_pem: &str,
    now_ms: i64,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>, Leaf), PemError> {
    let chain = chain(chain_pem)?;
    let key = key(key_pem)?;
    let leaf = leaf(&chain[0])?;
    let certified =
        super::store::certified_key(chain.clone(), &key).map_err(|_| PemError::BadKey)?;
    if certified.keys_match().is_err() {
        return Err(PemError::KeyMismatch);
    }
    if !leaf.covers(host) {
        return Err(PemError::WrongName(host.to_owned()));
    }
    if leaf.not_after_ms <= now_ms {
        return Err(PemError::Expired);
    }
    Ok((chain, key, leaf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestCa;

    #[test]
    fn a_leaf_says_who_issued_it_for_what_and_until_when() {
        let ca = TestCa::new();
        let issued = ca.issue(&["chat.example.com", "*.example.org"]);

        let leaf = leaf(&chain(&issued.cert_pem).unwrap()[0]).unwrap();

        assert_eq!(leaf.issuer, "Opencord test CA");
        assert!(leaf.not_after_ms > leaf.not_before_ms);
        assert!(leaf.covers("Chat.Example.com"));
        assert!(leaf.covers("a.example.org"));
        assert!(!leaf.covers("example.org"));
        assert!(!leaf.covers("a.b.example.org"));
    }

    #[test]
    fn uploads_are_checked() {
        let ca = TestCa::new();
        let issued = ca.issue(&["chat.example.com"]);
        let other = ca.issue(&["chat.example.com"]);

        assert!(check_upload("chat.example.com", &issued.cert_pem, &issued.key_pem, 0).is_ok());
        assert_eq!(
            check_upload("chat.example.com", &issued.cert_pem, &other.key_pem, 0).unwrap_err(),
            PemError::KeyMismatch
        );
        assert_eq!(
            check_upload("x.example.com", &issued.cert_pem, &issued.key_pem, 0).unwrap_err(),
            PemError::WrongName("x.example.com".to_owned())
        );
        assert_eq!(
            check_upload(
                "chat.example.com",
                &issued.cert_pem,
                &issued.key_pem,
                i64::MAX
            )
            .unwrap_err(),
            PemError::Expired
        );
        assert_eq!(
            check_upload("chat.example.com", "", &issued.key_pem, 0).unwrap_err(),
            PemError::NoCertificate
        );
    }
}
