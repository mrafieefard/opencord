//! TLS certificate: a configured one, or a self-signed one generated on first
//! start and kept in the data directory.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opencord_common::address::Fingerprint;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};

use crate::config::TlsSection;

pub const CERT_FILE: &str = "cert.pem";
pub const KEY_FILE: &str = "key.pem";

#[derive(Debug)]
pub struct TlsMaterial {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub key: PrivateKeyDer<'static>,
    /// Fingerprint of the leaf certificate, which clients pin.
    pub fingerprint: Fingerprint,
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a valid PEM file: {source}")]
    Pem {
        path: PathBuf,
        source: rustls::pki_types::pem::Error,
    },
    #[error("{path} contains no certificate")]
    NoCertificate { path: PathBuf },
    #[error("tls.cert and tls.key must be set together")]
    IncompleteConfig,
    #[error("could not generate a certificate: {0}")]
    Generate(#[from] rcgen::Error),
    #[error("invalid certificate or key: {0}")]
    Rustls(#[from] rustls::Error),
}

/// Loads the configured certificate, or the generated one in `tls_dir`,
/// generating it for `hostnames` if it does not exist yet.
pub fn load_or_generate(
    tls: &TlsSection,
    tls_dir: &Path,
    hostnames: &[String],
) -> Result<TlsMaterial, TlsError> {
    match (&tls.cert, &tls.key) {
        (Some(cert), Some(key)) => load(cert, key),
        (None, None) => {
            let cert_path = tls_dir.join(CERT_FILE);
            let key_path = tls_dir.join(KEY_FILE);
            if !cert_path.exists() || !key_path.exists() {
                generate(tls_dir, &cert_path, &key_path, hostnames)?;
            }
            load(&cert_path, &key_path)
        }
        _ => Err(TlsError::IncompleteConfig),
    }
}

/// SHA-256 of the DER-encoded certificate.
pub fn fingerprint(cert: &CertificateDer<'_>) -> Fingerprint {
    Sha256::digest(cert.as_ref()).into()
}

pub fn server_config(material: &TlsMaterial) -> Result<rustls::ServerConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(material.cert_chain.clone(), material.key.clone_key())?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn load(cert_path: &Path, key_path: &Path) -> Result<TlsMaterial, TlsError> {
    let pem_error = |path: &Path| {
        let path = path.to_owned();
        move |source| TlsError::Pem { path, source }
    };
    let cert_chain = CertificateDer::pem_file_iter(cert_path)
        .map_err(pem_error(cert_path))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(pem_error(cert_path))?;
    let leaf = cert_chain.first().ok_or_else(|| TlsError::NoCertificate {
        path: cert_path.to_owned(),
    })?;
    let fingerprint = fingerprint(leaf);
    let key = PrivateKeyDer::from_pem_file(key_path).map_err(pem_error(key_path))?;
    Ok(TlsMaterial {
        cert_chain,
        key,
        fingerprint,
    })
}

fn generate(
    dir: &Path,
    cert_path: &Path,
    key_path: &Path,
    hostnames: &[String],
) -> Result<(), TlsError> {
    fs::create_dir_all(dir).map_err(|source| TlsError::Io {
        path: dir.to_owned(),
        source,
    })?;
    let certified = rcgen::generate_simple_self_signed(hostnames.to_vec())?;
    fs::write(cert_path, certified.cert.pem()).map_err(|source| TlsError::Io {
        path: cert_path.to_owned(),
        source,
    })?;
    write_private(key_path, certified.signing_key.serialize_pem().as_bytes()).map_err(|source| {
        TlsError::Io {
            path: key_path.to_owned(),
            source,
        }
    })
}

/// Creates the file readable only by its owner.
pub(crate) fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hostnames() -> Vec<String> {
        vec!["localhost".to_owned(), "127.0.0.1".to_owned()]
    }

    #[test]
    fn generates_a_certificate_once_and_reuses_it() {
        let dir = tempfile::tempdir().unwrap();

        let first = load_or_generate(&TlsSection::default(), dir.path(), &hostnames()).unwrap();
        let second = load_or_generate(&TlsSection::default(), dir.path(), &hostnames()).unwrap();

        assert!(dir.path().join(CERT_FILE).exists());
        assert!(dir.path().join(KEY_FILE).exists());
        assert_eq!(first.fingerprint, second.fingerprint);
        assert_eq!(first.fingerprint, fingerprint(&first.cert_chain[0]));
    }

    #[cfg(unix)]
    #[test]
    fn generated_key_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();

        load_or_generate(&TlsSection::default(), dir.path(), &hostnames()).unwrap();

        let mode = fs::metadata(dir.path().join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn uses_the_configured_certificate() {
        let generated_dir = tempfile::tempdir().unwrap();
        let generated =
            load_or_generate(&TlsSection::default(), generated_dir.path(), &hostnames()).unwrap();
        let configured = TlsSection {
            cert: Some(generated_dir.path().join(CERT_FILE)),
            key: Some(generated_dir.path().join(KEY_FILE)),
        };
        let unused_dir = tempfile::tempdir().unwrap();

        let loaded = load_or_generate(&configured, unused_dir.path(), &hostnames()).unwrap();

        assert_eq!(loaded.fingerprint, generated.fingerprint);
        assert!(!unused_dir.path().join(CERT_FILE).exists());
    }

    #[test]
    fn requires_both_cert_and_key() {
        let dir = tempfile::tempdir().unwrap();
        let half = TlsSection {
            cert: Some(dir.path().join("cert.pem")),
            key: None,
        };

        assert!(matches!(
            load_or_generate(&half, dir.path(), &hostnames()),
            Err(TlsError::IncompleteConfig)
        ));
    }

    #[test]
    fn builds_a_server_config() {
        let dir = tempfile::tempdir().unwrap();
        let material = load_or_generate(&TlsSection::default(), dir.path(), &hostnames()).unwrap();

        assert!(server_config(&material).is_ok());
    }
}
