//! The check for tunnel and reverse-proxy domains (Phase 3 plan §6.2): their
//! DNS points at Cloudflare or the proxy, not here, so Opencord asks for a
//! token it serves itself, through the domain, and expects it back.

use std::time::Duration;

use super::check::{CheckOutcome, CheckResult};

/// Where the token is served, on every listener.
pub const VERIFY_PATH: &str = "/.well-known/opencord/verify";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Asks `https://host[:port]/.well-known/opencord/verify` for `token`.
pub async fn check(
    http: &reqwest::Client,
    host: &str,
    port: u16,
    token: &str,
    now_ms: i64,
) -> CheckOutcome {
    let url = match port {
        443 => format!("https://{host}{VERIFY_PATH}"),
        port => format!("https://{host}:{port}{VERIFY_PATH}"),
    };
    let response = match http.get(&url).timeout(TIMEOUT).send().await {
        Ok(response) => response,
        Err(error) => {
            let why = if error.is_timeout() {
                "it did not answer in time".to_owned()
            } else if error.is_connect() {
                "nothing answered".to_owned()
            } else {
                root_cause(&error)
            };
            return CheckOutcome::new(
                CheckResult::Failed,
                format!(
                    "This server could not reach {url}: {why}. Check that the tunnel or proxy \
                     runs and forwards to Opencord. If it does, the router may not let this \
                     machine reach its own public address (no hairpin NAT): open the address \
                     from another network, and confirm the domain by hand if it works."
                ),
                now_ms,
            );
        }
    };
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() && body.trim() == token {
        return CheckOutcome::new(
            CheckResult::Verified,
            "It reaches this server.".to_owned(),
            now_ms,
        );
    }
    let what = if status.is_success() {
        "another server answered".to_owned()
    } else {
        format!("it answered {status}")
    };
    CheckOutcome::new(
        CheckResult::PointsElsewhere,
        format!(
            "{url} does not lead to this server: {what}. Check where the tunnel or proxy forwards."
        ),
        now_ms,
    )
}

fn root_cause(error: &reqwest::Error) -> String {
    let mut source: &dyn std::error::Error = error;
    while let Some(inner) = source.source() {
        source = inner;
    }
    source.to_string()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use axum::Router;
    use axum::routing::get;
    use axum_server::tls_rustls::RustlsConfig;
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    use super::*;
    use crate::testing::TestCa;

    /// An HTTPS server for `localhost`, with a certificate `ca` signed,
    /// answering the verify path with `body`.
    async fn serve(ca: &TestCa, body: &'static str) -> u16 {
        let issued = ca.issue(&["localhost"]);
        let chain = vec![CertificateDer::from_pem_slice(issued.cert_pem.as_bytes()).unwrap()];
        let key = PrivateKeyDer::from_pem_slice(issued.key_pem.as_bytes()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let app = Router::new().route(VERIFY_PATH, get(move || async move { body }));
        let handle = axum_server::Handle::<SocketAddr>::new();
        let server = axum_server::bind_rustls(
            "127.0.0.1:0".parse().unwrap(),
            RustlsConfig::from_config(Arc::new(config)),
        )
        .handle(handle.clone());
        tokio::spawn(async move { server.serve(app.into_make_service()).await });
        handle.listening().await.unwrap().port()
    }

    fn trusting(ca: &TestCa) -> reqwest::Client {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(CertificateDer::from(ca.ca_der())).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        reqwest::Client::builder()
            .tls_backend_preconfigured(config)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn the_token_through_the_domain_verifies_it() {
        let ca = TestCa::new();
        let port = serve(&ca, "the-token").await;

        let outcome = check(&trusting(&ca), "localhost", port, "the-token", 1).await;

        assert_eq!(outcome.result, CheckResult::Verified);
    }

    #[tokio::test]
    async fn another_server_or_nothing_does_not() {
        let ca = TestCa::new();
        let port = serve(&ca, "someone-else").await;

        let other = check(&trusting(&ca), "localhost", port, "the-token", 1).await;
        let nothing = check(&trusting(&ca), "localhost", 9, "the-token", 1).await;
        let untrusted = check(&trusting(&TestCa::new()), "localhost", port, "x", 1).await;

        assert_eq!(other.result, CheckResult::PointsElsewhere);
        assert_eq!(nothing.result, CheckResult::Failed);
        assert!(nothing.message.contains("hairpin"));
        assert_eq!(untrusted.result, CheckResult::Failed);
    }
}
