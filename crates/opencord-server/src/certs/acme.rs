//! Getting a certificate from Let's Encrypt (Phase 3 plan §7.1–7.2): the
//! account, the order, one challenge, the certificate. The challenge is
//! chosen by what can work here: DNS-01 through a connected DNS provider
//! (no open ports), TLS-ALPN-01 when this server answers on the port the
//! CA checks it on (443), else HTTP-01 on port 80, open only meanwhile.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, Problem, RetryPolicy,
};
use rcgen::{CertificateParams, CustomExtension, DistinguishedName, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio_util::sync::CancellationToken;

use super::pem;
use super::providers::{Endpoints, Provider, TxtRecord};
use super::store::{CertStore, certified_key};
use crate::domains::dns::DnsChecker;

/// The steps shown on the domain's row while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Account,
    Order,
    Challenge,
    Validation,
    Issuing,
}

/// Which challenge was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Http,
    TlsAlpn,
    Dns,
}

/// Everything a request needs from the server.
pub struct Request<'a> {
    pub host: &'a str,
    pub directory: &'a str,
    pub root: Option<&'a PathBuf>,
    /// The account saved for this directory, if any.
    pub account: Option<AccountCredentials>,
    pub email: Option<&'a str>,
    /// The subscriber agreement, needed to make an account.
    pub agreed: bool,
    pub providers: &'a [Provider],
    pub endpoints: &'a Endpoints,
    pub http: &'a reqwest::Client,
    /// The ports the CA checks, and this server's main port.
    pub http_port: u16,
    pub tls_alpn_port: u16,
    pub main_port: u16,
    pub store: &'a CertStore,
    pub http01: &'a Http01,
    pub dns: Option<DnsChecker>,
    pub dns_wait: Duration,
}

pub struct Issued {
    pub chain_pem: String,
    pub key_pem: String,
    pub leaf: pem::Leaf,
    pub method: Method,
    /// Set when an account was made: to save.
    pub new_account: Option<AccountCredentials>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub message: String,
    /// The CA's limit: retry after this.
    pub rate_limited: bool,
    pub needs_agreement: bool,
}

impl Failure {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            rate_limited: false,
            needs_agreement: false,
        }
    }
}

/// Requests a certificate for `request.host`; `step` hears progress.
pub async fn issue(mut request: Request<'_>, step: impl Fn(Step)) -> Result<Issued, Failure> {
    step(Step::Account);
    let builder = match request.root {
        Some(root) => Account::builder_with_root(root),
        None => Account::builder(),
    }
    .map_err(|error| Failure::plain(format!("The ACME client could not start: {error}")))?;
    let (account, new_account) = match request.account.take() {
        Some(credentials) => (
            builder
                .from_credentials(credentials)
                .await
                .map_err(|error| acme_failure(&error, None))?,
            None,
        ),
        None => {
            if !request.agreed {
                return Err(Failure {
                    needs_agreement: true,
                    ..Failure::plain("Agree to Let's Encrypt's subscriber agreement first.")
                });
            }
            let contact = request.email.map(|email| format!("mailto:{email}"));
            let contacts: Vec<&str> = contact.iter().map(String::as_str).collect();
            let (account, credentials) = builder
                .create(
                    &NewAccount {
                        contact: &contacts,
                        terms_of_service_agreed: true,
                        only_return_existing: false,
                    },
                    request.directory.to_owned(),
                    None,
                )
                .await
                .map_err(|error| acme_failure(&error, None))?;
            (account, Some(credentials))
        }
    };

    step(Step::Order);
    let identifiers = [Identifier::Dns(request.host.to_owned())];
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .map_err(|error| acme_failure(&error, None))?;

    step(Step::Challenge);
    let mut cleanup = Cleanup::default();
    let result = async {
        let mut method = None;
        let mut authorizations = order.authorizations();
        while let Some(authorization) = authorizations.next().await {
            let mut authorization = authorization.map_err(|error| acme_failure(&error, None))?;
            if authorization.status == AuthorizationStatus::Valid {
                continue;
            }
            let chosen = set_up(&request, &mut authorization, &mut cleanup).await?;
            method = Some(chosen);
        }
        Ok::<_, Failure>(method)
    }
    .await;
    let method = match result {
        Ok(method) => method,
        Err(failure) => {
            cleanup.undo(&request).await;
            return Err(failure);
        }
    };

    step(Step::Validation);
    let status = order
        .poll_ready(&RetryPolicy::new().timeout(Duration::from_secs(120)))
        .await;
    cleanup.undo(&request).await;
    let status = status.map_err(|error| acme_failure(&error, method))?;
    if status != OrderStatus::Ready {
        return Err(why_invalid(&mut order, method).await);
    }

    step(Step::Issuing);
    let key = KeyPair::generate()
        .map_err(|error| Failure::plain(format!("A key could not be made: {error}")))?;
    let mut params = CertificateParams::new(vec![request.host.to_owned()])
        .map_err(|_| Failure::plain(format!("{} cannot be in a certificate", request.host)))?;
    params.distinguished_name = DistinguishedName::new();
    let csr = params
        .serialize_request(&key)
        .map_err(|error| Failure::plain(format!("The request could not be made: {error}")))?;
    order
        .finalize_csr(csr.der())
        .await
        .map_err(|error| acme_failure(&error, method))?;
    let chain_pem = order
        .poll_certificate(&RetryPolicy::new().timeout(Duration::from_secs(60)))
        .await
        .map_err(|error| acme_failure(&error, method))?;
    let chain = pem::chain(&chain_pem).map_err(|error| Failure::plain(error.to_string()))?;
    let leaf = pem::leaf(&chain[0]).map_err(|error| Failure::plain(error.to_string()))?;
    Ok(Issued {
        chain_pem,
        key_pem: key.serialize_pem(),
        leaf,
        method: method.unwrap_or(Method::Http),
        new_account,
    })
}

/// Sets up one authorization's challenge, the best that can work here. A
/// handle gives out one challenge, so the kind is chosen first.
async fn set_up<'a>(
    request: &Request<'_>,
    authorization: &'a mut instant_acme::AuthorizationHandle<'a>,
    cleanup: &mut Cleanup,
) -> Result<Method, Failure> {
    let host = request.host;
    let name = format!("_acme-challenge.{host}");
    // DNS-01, through the first provider that has the name.
    let mut provider = None;
    for candidate in request.providers {
        match candidate.has(request.http, request.endpoints, &name).await {
            Ok(true) => {
                provider = Some(candidate.clone());
                break;
            }
            Ok(false) => {}
            Err(error) => {
                return Err(Failure::plain(format!(
                    "{}: {error}",
                    candidate.kind.label()
                )));
            }
        }
    }
    if let Some(provider) = provider {
        let mut challenge = authorization
            .challenge(ChallengeType::Dns01)
            .ok_or_else(|| Failure::plain("Let's Encrypt offered no DNS challenge."))?;
        let value = challenge.key_authorization().dns_value();
        let record = provider
            .set_txt(request.http, request.endpoints, &name, &value)
            .await
            .map_err(|error| Failure::plain(format!("{}: {error}", provider.kind.label())))?
            .ok_or_else(|| Failure::plain(format!("{} lost the name", provider.kind.label())))?;
        cleanup.txt.push((provider, record));
        wait_for_txt(request, &name, &value).await;
        challenge
            .set_ready()
            .await
            .map_err(|error| acme_failure(&error, Some(Method::Dns)))?;
        return Ok(Method::Dns);
    }
    if request.main_port == request.tls_alpn_port {
        let mut challenge = authorization
            .challenge(ChallengeType::TlsAlpn01)
            .ok_or_else(|| Failure::plain("Let's Encrypt offered no TLS-ALPN challenge."))?;
        let digest = challenge.key_authorization().digest();
        let key = alpn_certificate(host, digest.as_ref())?;
        request.store.set_challenge(host, Some(key));
        cleanup.alpn.push(host.to_owned());
        challenge
            .set_ready()
            .await
            .map_err(|error| acme_failure(&error, Some(Method::TlsAlpn)))?;
        return Ok(Method::TlsAlpn);
    }
    let mut challenge = authorization
        .challenge(ChallengeType::Http01)
        .ok_or_else(|| {
            Failure::plain("Let's Encrypt offered no challenge this server can answer.")
        })?;
    let token = challenge.token.clone();
    let answer = challenge.key_authorization().as_str().to_owned();
    request
        .http01
        .serve(request.http_port, token.clone(), answer)
        .await
        .map_err(|_| {
            Failure::plain(format!(
                "Port {} is in use or not allowed here, so Let's Encrypt cannot check the domain \
                 on it. Free it (see the guide on ports below 1024), run Opencord on port 443, \
                 or connect your DNS provider to use the DNS challenge.",
                request.http_port
            ))
        })?;
    cleanup.http.push(token);
    challenge
        .set_ready()
        .await
        .map_err(|error| acme_failure(&error, Some(Method::Http)))?;
    Ok(Method::Http)
}

/// Waits, a while at most, until the domain's name servers give the TXT
/// record: the CA asks them directly.
async fn wait_for_txt(request: &Request<'_>, name: &str, value: &str) {
    let Some(dns) = &request.dns else {
        return;
    };
    let deadline = tokio::time::Instant::now() + request.dns_wait;
    while tokio::time::Instant::now() < deadline {
        if let Ok(records) = dns.authoritative_txt(name).await
            && records.iter().any(|record| record == value)
        {
            return;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// A TLS-ALPN-01 certificate (RFC 8737): for `host`, carrying the key
/// authorization's digest.
fn alpn_certificate(host: &str, digest: &[u8]) -> Result<Arc<rustls::sign::CertifiedKey>, Failure> {
    let failed = |error: rcgen::Error| Failure::plain(format!("The challenge failed: {error}"));
    let mut params = CertificateParams::new(vec![host.to_owned()]).map_err(failed)?;
    params.custom_extensions = vec![CustomExtension::new_acme_identifier(digest)];
    let key = KeyPair::generate().map_err(failed)?;
    let certificate = params.self_signed(&key).map_err(failed)?;
    certified_key(
        vec![CertificateDer::from(certificate.der().to_vec())],
        &PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    )
    .map_err(|error| Failure::plain(error.to_string()))
}

/// What was set up, to take down again whatever happens.
#[derive(Default)]
struct Cleanup {
    txt: Vec<(Provider, TxtRecord)>,
    alpn: Vec<String>,
    http: Vec<String>,
}

impl Cleanup {
    async fn undo(&mut self, request: &Request<'_>) {
        for (provider, record) in self.txt.drain(..) {
            if let Err(error) = provider
                .clear_txt(request.http, request.endpoints, &record)
                .await
            {
                tracing::warn!(%error, "a challenge's TXT record could not be removed");
            }
        }
        for host in self.alpn.drain(..) {
            request.store.set_challenge(&host, None);
        }
        for token in self.http.drain(..) {
            request.http01.done(&token);
        }
    }
}

/// Why an order became invalid: the failed challenge's problem.
async fn why_invalid(order: &mut instant_acme::Order, method: Option<Method>) -> Failure {
    let mut authorizations = order.authorizations();
    while let Some(Ok(authorization)) = authorizations.next().await {
        if let Some(problem) = authorization
            .challenges
            .iter()
            .find_map(|challenge| challenge.error.as_ref())
        {
            return explain(problem, method);
        }
    }
    Failure::plain("Let's Encrypt did not accept the challenge.")
}

fn acme_failure(error: &instant_acme::Error, method: Option<Method>) -> Failure {
    match error {
        instant_acme::Error::Api(problem) => explain(problem, method),
        error => Failure::plain(format!("Let's Encrypt could not be reached: {error}")),
    }
}

/// A CA's problem in plain words (plan §7.2).
pub fn explain(problem: &Problem, method: Option<Method>) -> Failure {
    let kind = problem
        .r#type
        .as_deref()
        .and_then(|kind| kind.rsplit(':').next())
        .unwrap_or_default();
    let detail = problem.detail.clone().unwrap_or_default();
    let message = match kind {
        "connection" | "incorrectResponse" => match method {
            Some(Method::Http) => format!(
                "Port 80 isn't reachable from the internet. Open or forward it, or connect your \
                 DNS provider to use the DNS challenge. ({detail})"
            ),
            Some(Method::TlsAlpn) => format!(
                "Port 443 isn't reachable from the internet. Open or forward it, or connect your \
                 DNS provider. ({detail})"
            ),
            _ => format!("Let's Encrypt could not reach the server: {detail}"),
        },
        "dns" => format!("The domain doesn't point to this server: {detail}"),
        "unauthorized" => match method {
            Some(Method::Dns) => format!("The DNS challenge record was not found: {detail}"),
            _ => format!("Let's Encrypt reached another server for the domain: {detail}"),
        },
        "rateLimited" => format!(
            "Let's Encrypt's limit was reached; try again later, or use its test environment \
             meanwhile. ({detail})"
        ),
        "caa" => format!("The domain's CAA records do not allow Let's Encrypt: {detail}"),
        "rejectedIdentifier" => format!("Let's Encrypt does not issue for this name: {detail}"),
        "userActionRequired" => format!("Let's Encrypt asks for something first: {detail}"),
        _ if detail.is_empty() => "Let's Encrypt refused the request.".to_owned(),
        _ => format!("Let's Encrypt refused the request: {detail}"),
    };
    Failure {
        message,
        rate_limited: kind == "rateLimited",
        needs_agreement: kind == "userActionRequired",
    }
}

/// Answers HTTP-01 on port 80, only while a challenge is under way, only
/// for its exact paths.
#[derive(Debug, Default)]
pub struct Http01 {
    answers: Arc<Mutex<HashMap<String, String>>>,
    running: tokio::sync::Mutex<Option<(u16, CancellationToken)>>,
}

impl Http01 {
    /// Serves `answer` for `token`, listening on `port` if not already.
    pub async fn serve(&self, port: u16, token: String, answer: String) -> std::io::Result<()> {
        let mut running = self.running.lock().await;
        if running.as_ref().is_none_or(|(on, _)| *on != port) {
            if let Some((_, stop)) = running.take() {
                stop.cancel();
            }
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
            let stop = CancellationToken::new();
            let app = Router::new()
                .route("/.well-known/acme-challenge/{token}", get(answer_challenge))
                .with_state(Arc::clone(&self.answers))
                .into_make_service_with_connect_info::<SocketAddr>();
            let stopped = stop.clone();
            tokio::spawn(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async move { stopped.cancelled().await })
                    .await;
            });
            *running = Some((port, stop));
        }
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(token, answer);
        Ok(())
    }

    /// The challenge is over: its path goes, and the port closes when it
    /// was the last.
    pub fn done(&self, token: &str) {
        let empty = {
            let mut answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
            answers.remove(token);
            answers.is_empty()
        };
        if empty
            && let Ok(mut running) = self.running.try_lock()
            && let Some((_, stop)) = running.take()
        {
            stop.cancel();
        }
    }
}

async fn answer_challenge(
    State(answers): State<Arc<Mutex<HashMap<String, String>>>>,
    Path(token): Path<String>,
) -> Result<String, StatusCode> {
    answers
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&token)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(kind: &str) -> Problem {
        serde_json::from_value(serde_json::json!({
            "type": format!("urn:ietf:params:acme:error:{kind}"),
            "detail": "the detail",
        }))
        .unwrap()
    }

    #[test]
    fn problems_read_as_what_to_do() {
        let port = explain(&problem("connection"), Some(Method::Http));
        assert!(
            port.message.starts_with("Port 80 isn't reachable"),
            "{}",
            port.message
        );
        let dns = explain(&problem("dns"), Some(Method::Http));
        assert!(dns.message.contains("doesn't point to this server"));
        let limit = explain(&problem("rateLimited"), None);
        assert!(limit.rate_limited);
        assert!(
            explain(&problem("other"), None)
                .message
                .contains("the detail")
        );
    }

    #[tokio::test]
    async fn port_80_answers_only_its_challenge_and_only_meanwhile() {
        let http01 = Http01::default();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        http01
            .serve(port, "tok".to_owned(), "tok.thumb".to_owned())
            .await
            .unwrap();
        let client = crate::tls::http_client().unwrap();
        let url = |path: &str| format!("http://127.0.0.1:{port}{path}");

        let answer = client
            .get(url("/.well-known/acme-challenge/tok"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let other = client
            .get(url("/.well-known/acme-challenge/x"))
            .send()
            .await
            .unwrap();
        let elsewhere = client.get(url("/gateway")).send().await.unwrap();

        assert_eq!(answer, "tok.thumb");
        assert_eq!(other.status(), 404);
        assert_eq!(elsewhere.status(), 404);
        http01.done("tok");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            client
                .get(url("/.well-known/acme-challenge/tok"))
                .send()
                .await
                .is_err()
        );
    }
}
