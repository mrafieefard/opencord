//! Releases from GitHub, found the way docs/releasing.md describes:
//! stable from `releases/latest`, which isn't rate-limited, and canary or
//! beta from the releases API (60 requests an hour per address without a
//! token, plenty for one check per launch).

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::VerifyingKey;

use crate::release::{self, Listed, Manifest};
use crate::signature;
use crate::track::Track;
use crate::update::{Error, Source};

/// The repository releases come from.
pub const REPOSITORY: &str = match option_env!("OPENCORD_UPDATE_REPOSITORY") {
    Some(repo) => repo,
    None => "mrafieefard/opencord",
};

/// How long the check may take before the installed version starts.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a download may take: an app archive at a slow 50 kB/s.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(45 * 60);
/// The most `release.json`, its signature or the listing may be.
const SMALL_LIMIT: u64 = 4 << 20;

/// Fetches bytes over HTTPS.
pub trait Http {
    /// A small body, whole.
    fn get(&self, url: &str) -> Result<Vec<u8>, Error>;
    /// A large body, streamed into `out`.
    fn download(&self, url: &str, out: &mut dyn Write) -> Result<(), Error>;
}

pub struct Github<H> {
    http: H,
    repository: String,
    key: Option<VerifyingKey>,
    platform: &'static str,
}

impl<H: Http> Github<H> {
    pub fn new(
        http: H,
        repository: &str,
        key: Option<VerifyingKey>,
        platform: &'static str,
    ) -> Self {
        Self {
            http,
            repository: repository.to_owned(),
            key,
            platform,
        }
    }

    fn manifest(&self, url: &str, tag: Option<&str>) -> Result<Manifest, Error> {
        let bytes = self.http.get(url)?;
        if let Some(key) = &self.key {
            let signature = self.http.get(&format!("{url}.sig"))?;
            if !signature::verify(key, &bytes, &signature) {
                return Err(Error::Signature);
            }
        }
        // `releases/latest` names no tag: the manifest's own is checked
        // against its version instead.
        let tag = match tag {
            Some(tag) => tag.to_owned(),
            None => serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|value| value["tag"].as_str().map(str::to_owned))
                .unwrap_or_default(),
        };
        Manifest::parse(&bytes, &tag, self.platform).map_err(Error::Manifest)
    }
}

impl<H: Http> Source for Github<H> {
    fn latest(&self, track: Track) -> Result<Manifest, Error> {
        let repo = &self.repository;
        let manifest = if track == Track::Stable {
            self.manifest(
                &format!("https://github.com/{repo}/releases/latest/download/release.json"),
                None,
            )?
        } else {
            let listing = self.http.get(&format!(
                "https://api.github.com/repos/{repo}/releases?per_page=30"
            ))?;
            let listing: Vec<Listed> = serde_json::from_slice(&listing).map_err(|error| {
                Error::Network(format!("the releases list can't be read: {error}"))
            })?;
            let (_, tag) = release::newest(&listing, track).ok_or(Error::NoRelease)?;
            self.manifest(
                &format!("https://github.com/{repo}/releases/download/{tag}/release.json"),
                Some(&tag),
            )?
        };
        // A manifest of a track this one doesn't take (`latest` is always
        // stable, but a manifest is only as good as its checks).
        match Track::of(&manifest.version) {
            Some(channel) if track.follows(channel) => Ok(manifest),
            _ => Err(Error::NoRelease),
        }
    }

    fn download(&self, url: &str, out: &mut dyn Write) -> Result<(), Error> {
        self.http.download(url, out)
    }
}

/// [`Http`] with reqwest, trusting the public certificate authorities
/// built in, so a system without a certificate store still updates.
pub struct Reqwest {
    client: reqwest::blocking::Client,
}

impl Reqwest {
    pub fn new() -> Result<Self, Error> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|error| Error::Network(error.to_string()))?
            .with_root_certificates(rustls::RootCertStore::from_iter(
                webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
            ))
            .with_no_client_auth();
        let client = reqwest::blocking::Client::builder()
            .tls_backend_preconfigured(tls)
            .user_agent(concat!("opencord-launcher/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|error| Error::Network(error.to_string()))?;
        Ok(Self { client })
    }

    fn send(&self, url: &str, timeout: Duration) -> Result<reqwest::blocking::Response, Error> {
        let response = self
            .client
            .get(url)
            .header(
                "Accept",
                "application/vnd.github+json, application/json, */*",
            )
            .timeout(timeout)
            .send()
            .map_err(|error| Error::Network(error.to_string()))?;
        let status = response.status();
        // `releases/latest` has nothing until a stable release exists.
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NoRelease);
        }
        if !status.is_success() {
            return Err(Error::Network(format!("{url} answered {status}")));
        }
        Ok(response)
    }
}

impl Http for Reqwest {
    fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        let response = self.send(url, CHECK_TIMEOUT)?;
        let mut body = Vec::new();
        response
            .take(SMALL_LIMIT + 1)
            .read_to_end(&mut body)
            .map_err(|error| Error::Network(error.to_string()))?;
        if body.len() as u64 > SMALL_LIMIT {
            return Err(Error::Network(format!("{url} is too large")));
        }
        Ok(body)
    }

    fn download(&self, url: &str, out: &mut dyn Write) -> Result<(), Error> {
        let mut response = self.send(url, DOWNLOAD_TIMEOUT)?;
        io::copy(&mut response, out).map_err(|error| match error.kind() {
            // The checking writer refuses a body that runs long.
            io::ErrorKind::InvalidData => Error::Download(error.to_string()),
            _ => Error::Network(error.to_string()),
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use data_encoding::BASE64;
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    const PLATFORM: &str = "client-linux-x64";

    #[derive(Default)]
    struct Fake {
        bodies: HashMap<String, Vec<u8>>,
        asked: RefCell<Vec<String>>,
    }

    impl Http for Fake {
        fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
            self.asked.borrow_mut().push(url.to_owned());
            self.bodies
                .get(url)
                .cloned()
                .ok_or_else(|| Error::Network(format!("{url} answered 404")))
        }

        fn download(&self, url: &str, out: &mut dyn Write) -> Result<(), Error> {
            out.write_all(&self.get(url)?).unwrap();
            Ok(())
        }
    }

    fn manifest(version: &str) -> Vec<u8> {
        serde_json::json!({
            "schema": 1,
            "version": version,
            "tag": format!("v{version}"),
            "platforms": {
                PLATFORM: {
                    "file": "opencord-linux-x64.tar.gz",
                    "url": "https://example.com/a.tar.gz",
                    "size": 3,
                    "sha256": "a".repeat(64),
                }
            }
        })
        .to_string()
        .into_bytes()
    }

    fn github(fake: Fake, key: Option<VerifyingKey>) -> Github<Fake> {
        Github::new(fake, "o/r", key, PLATFORM)
    }

    const LATEST: &str = "https://github.com/o/r/releases/latest/download/release.json";
    const LIST: &str = "https://api.github.com/repos/o/r/releases?per_page=30";

    #[test]
    fn stable_reads_the_latest_release_without_the_api() {
        let mut fake = Fake::default();
        fake.bodies.insert(LATEST.into(), manifest("0.0.2"));
        let github = github(fake, None);

        let found = github.latest(Track::Stable).unwrap();

        assert_eq!(found.version.to_string(), "0.0.2");
        assert_eq!(*github.http.asked.borrow(), [LATEST]);
    }

    #[test]
    fn beta_lists_releases_and_reads_the_newest_it_takes() {
        let mut fake = Fake::default();
        fake.bodies.insert(
            LIST.into(),
            br#"[{"tag_name":"v0.0.2-beta.2"},{"tag_name":"v0.0.2-beta.1"},{"tag_name":"v0.0.1"}]"#
                .to_vec(),
        );
        fake.bodies.insert(
            "https://github.com/o/r/releases/download/v0.0.2-beta.2/release.json".into(),
            manifest("0.0.2-beta.2"),
        );

        let found = github(fake, None).latest(Track::Beta).unwrap();

        assert_eq!(found.version.to_string(), "0.0.2-beta.2");
    }

    #[test]
    fn a_manifest_served_for_another_release_is_refused() {
        let mut fake = Fake::default();
        fake.bodies
            .insert(LIST.into(), br#"[{"tag_name":"v0.0.3-canary.1"}]"#.to_vec());
        fake.bodies.insert(
            "https://github.com/o/r/releases/download/v0.0.3-canary.1/release.json".into(),
            manifest("0.0.1"),
        );

        assert!(matches!(
            github(fake, None).latest(Track::Canary),
            Err(Error::Manifest(_))
        ));
    }

    #[test]
    fn a_stable_user_is_never_handed_a_prerelease_as_latest() {
        let mut fake = Fake::default();
        fake.bodies.insert(LATEST.into(), manifest("0.0.3-beta.1"));
        assert!(matches!(
            github(fake, None).latest(Track::Stable),
            Err(Error::NoRelease)
        ));
    }

    #[test]
    fn with_a_key_the_signature_must_hold() {
        let signer = SigningKey::from_bytes(&[3; 32]);
        let body = manifest("0.0.2");
        let signature = BASE64.encode(&signer.sign(&body).to_bytes()).into_bytes();

        let mut signed = Fake::default();
        signed.bodies.insert(LATEST.into(), body.clone());
        signed
            .bodies
            .insert(format!("{LATEST}.sig"), signature.clone());
        assert!(
            github(signed, Some(signer.verifying_key()))
                .latest(Track::Stable)
                .is_ok()
        );

        // Signed by someone else.
        let mut forged = Fake::default();
        forged.bodies.insert(LATEST.into(), body.clone());
        forged.bodies.insert(format!("{LATEST}.sig"), signature);
        let other = SigningKey::from_bytes(&[4; 32]).verifying_key();
        assert!(matches!(
            github(forged, Some(other)).latest(Track::Stable),
            Err(Error::Signature)
        ));

        // Not signed at all.
        let mut unsigned = Fake::default();
        unsigned.bodies.insert(LATEST.into(), body);
        assert!(
            github(unsigned, Some(signer.verifying_key()))
                .latest(Track::Stable)
                .is_err()
        );
    }
}
