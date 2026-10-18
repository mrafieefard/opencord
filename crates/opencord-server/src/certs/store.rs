//! Which certificate a TLS connection gets (Phase 3 plan §7.3): the one for
//! the name the client asked for (SNI), else the self-signed one. ACME's
//! TLS-ALPN-01 validators get the challenge certificate for their name.
//! Changes take effect for the next handshake, without a restart.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

/// The ALPN protocol of ACME's TLS-ALPN-01 challenge (RFC 8737).
pub const ACME_TLS_ALPN: &[u8] = b"acme-tls/1";

#[derive(Debug)]
pub struct CertStore {
    fallback: Arc<CertifiedKey>,
    by_name: RwLock<HashMap<String, Arc<CertifiedKey>>>,
    challenges: RwLock<HashMap<String, Arc<CertifiedKey>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("the private key is not one this server can use: {0}")]
    Unsupported(rustls::Error),
    #[error("the certificate chain is empty")]
    NoCertificate,
}

/// A certificate chain and its key, ready to serve.
pub fn certified_key(
    chain: Vec<CertificateDer<'static>>,
    key: &PrivateKeyDer<'static>,
) -> Result<Arc<CertifiedKey>, KeyError> {
    if chain.is_empty() {
        return Err(KeyError::NoCertificate);
    }
    let signing =
        rustls::crypto::ring::sign::any_supported_type(key).map_err(KeyError::Unsupported)?;
    Ok(Arc::new(CertifiedKey::new(chain, signing)))
}

impl CertStore {
    pub fn new(fallback: Arc<CertifiedKey>) -> Self {
        Self {
            fallback,
            by_name: RwLock::default(),
            challenges: RwLock::default(),
        }
    }

    /// Serves `key` to clients asking for `host`, or stops (`None`).
    pub fn set(&self, host: &str, key: Option<Arc<CertifiedKey>>) {
        let mut by_name = self.by_name.write().unwrap_or_else(PoisonError::into_inner);
        match key {
            Some(key) => by_name.insert(host.to_ascii_lowercase(), key),
            None => by_name.remove(&host.to_ascii_lowercase()),
        };
    }

    /// Answers a TLS-ALPN-01 validation of `host` with `key`, or stops.
    pub fn set_challenge(&self, host: &str, key: Option<Arc<CertifiedKey>>) {
        let mut challenges = self
            .challenges
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        match key {
            Some(key) => challenges.insert(host.to_ascii_lowercase(), key),
            None => challenges.remove(&host.to_ascii_lowercase()),
        };
    }

    /// Names served with their own certificate.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .by_name
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    fn choose(&self, name: Option<&str>, acme: bool) -> Option<Arc<CertifiedKey>> {
        let name = name.map(str::to_ascii_lowercase);
        if acme {
            // Only for a validation under way; nothing else answers it.
            return self
                .challenges
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(name.as_deref()?)
                .cloned();
        }
        name.and_then(|name| {
            self.by_name
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&name)
                .cloned()
        })
        .or_else(|| Some(Arc::clone(&self.fallback)))
    }
}

impl ResolvesServerCert for CertStore {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let acme = hello
            .alpn()
            .is_some_and(|mut protocols| protocols.any(|protocol| protocol == ACME_TLS_ALPN));
        self.choose(hello.server_name(), acme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestCa;
    use rustls::pki_types::pem::PemObject;

    fn key_for(ca: &TestCa, name: &str) -> Arc<CertifiedKey> {
        let issued = ca.issue(&[name]);
        certified_key(
            vec![CertificateDer::from_pem_slice(issued.cert_pem.as_bytes()).unwrap()],
            &PrivateKeyDer::from_pem_slice(issued.key_pem.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn the_name_asked_for_picks_its_certificate_and_others_get_the_fallback() {
        let ca = TestCa::new();
        let fallback = key_for(&ca, "localhost");
        let chat = key_for(&ca, "chat.example.com");
        let store = CertStore::new(Arc::clone(&fallback));

        store.set("Chat.Example.com", Some(Arc::clone(&chat)));

        assert!(Arc::ptr_eq(
            &store.choose(Some("chat.example.com"), false).unwrap(),
            &chat
        ));
        assert!(Arc::ptr_eq(
            &store.choose(Some("other.example.com"), false).unwrap(),
            &fallback
        ));
        assert!(Arc::ptr_eq(&store.choose(None, false).unwrap(), &fallback));
        store.set("chat.example.com", None);
        assert!(Arc::ptr_eq(
            &store.choose(Some("chat.example.com"), false).unwrap(),
            &fallback
        ));
    }

    #[test]
    fn only_a_validation_under_way_answers_acme() {
        let ca = TestCa::new();
        let store = CertStore::new(key_for(&ca, "localhost"));
        store.set("chat.example.com", Some(key_for(&ca, "chat.example.com")));
        assert!(store.choose(Some("chat.example.com"), true).is_none());

        let challenge = key_for(&ca, "chat.example.com");
        store.set_challenge("chat.example.com", Some(Arc::clone(&challenge)));

        assert!(Arc::ptr_eq(
            &store.choose(Some("chat.example.com"), true).unwrap(),
            &challenge
        ));
    }
}
