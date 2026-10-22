//! Certificates, Phase 3 D3 (plan §7): Let's Encrypt through each challenge,
//! renewal, the owner's own certificate and the DNS providers.
//!
//! The ACME tests run against Pebble, Let's Encrypt's test CA, whose
//! programs are copied out of its Docker images; they are ignored by default
//! and run with `cargo test -- --ignored`.
//! Pebble asks pebble-challtestsrv for every name, which answers 127.0.0.1,
//! so it reaches this server; the server's own checks ask a fake name
//! server.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{delete, get};
use common::domains::*;
use common::pebble::*;
use common::*;
use hickory_proto::rr::RData;
use hickory_proto::rr::rdata::TXT;
use opencord_proto::v1 as proto;
use opencord_server::domains::dns::fake::FakeDns;
use opencord_server::testing::TestCa;
use proto::request::Kind;
use rustls::pki_types::CertificateDer;
use serde_json::{Value, json};

const HOST: &str = "chat.example.com";
/// The domain's certificate status once it is no longer being requested.
async fn settled(owner: &mut TestClient, id: i64) -> proto::CertificateStatus {
    let wait = owner.wait_for(|event| match event {
        proto::event::Kind::NetworkStatus(status) => status
            .info
            .as_ref()?
            .domains
            .iter()
            .find(|domain| domain.id == id)?
            .certificate
            .clone()
            .filter(|certificate| {
                certificate.state != proto::CertificateState::Requesting as i32
                    && certificate.state != proto::CertificateState::None as i32
            }),
        _ => None,
    });
    tokio::time::timeout(Duration::from_secs(60), wait)
        .await
        .expect("the request ends")
}

/// A verified domain on a server Pebble can check.
async fn verified_domain(
    customize: impl FnOnce(&mut opencord_server::config::Config),
    pebble_directory: &str,
) -> (FakeDns, TestServer, TestClient, i64) {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a(HOST, OURS);
    let directory = pebble_directory.to_owned();
    let server = server_configured(&dns, |config| {
        config.acme.directory = directory;
        config.acme.root = Some(fixture("pebble.minica.pem"));
        config.acme.dns_propagation_s = 10;
        customize(config);
    })
    .await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, HOST).await;
    assert_eq!(
        result(&check(&mut owner, id).await),
        proto::CheckResult::Verified
    );
    (dns, server, owner, id)
}

async fn request(owner: &mut TestClient, id: i64) {
    owner
        .ok(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: id,
            email: Some("owner@example.com".to_owned()),
            agree_terms: true,
        }))
        .await;
}

#[tokio::test]
#[ignore = "needs Docker and the Pebble images"]
async fn http_01_gets_a_certificate_that_is_served_at_once() {
    let http_port = free_port();
    let pebble = Pebble::start(http_port, free_port()).await;
    let (_dns, server, mut owner, id) = verified_domain(
        |config| config.acme.http_port = http_port,
        &pebble.directory,
    )
    .await;

    request(&mut owner, id).await;
    let status = settled(&mut owner, id).await;

    assert_eq!(
        status.state,
        proto::CertificateState::Valid as i32,
        "{status:?}"
    );
    assert!(status.issuer.contains("Pebble"), "{}", status.issuer);
    assert!(!status.test && !status.uploaded);
    served(&server, HOST, pebble.root().await).await;
    // Only while it is needed.
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", http_port)).is_err(),
        "port {http_port} is closed again"
    );
    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert!(info.acme.as_ref().unwrap().exists);
    assert!(domain(&info, HOST).trusted_certificate);
}

#[tokio::test]
#[ignore = "needs Docker and the Pebble images"]
async fn tls_alpn_01_answers_on_the_main_port() {
    let main_port = free_port();
    let pebble = Pebble::start(free_port(), main_port).await;
    let (_dns, server, mut owner, id) = verified_domain(
        |config| {
            config.server.bind = SocketAddr::from(([127, 0, 0, 1], main_port));
            config.acme.tls_alpn_port = main_port;
            // Nothing listens there: only TLS-ALPN can work.
            config.acme.http_port = free_port();
        },
        &pebble.directory,
    )
    .await;

    request(&mut owner, id).await;
    let status = settled(&mut owner, id).await;

    assert_eq!(
        status.state,
        proto::CertificateState::Valid as i32,
        "{status:?}"
    );
    served(&server, HOST, pebble.root().await).await;
}

/// Cloudflare's API, as far as the DNS challenge uses it: records go to
/// Pebble's name server and to the server's fake one.
#[derive(Clone)]
struct FakeCloudflare {
    dns: FakeDns,
    pebble_dns: u16,
}

async fn fake_cloudflare(dns: FakeDns, pebble_dns: u16) -> String {
    async fn zones(
        axum::extract::Query(query): axum::extract::Query<Vec<(String, String)>>,
    ) -> Json<Value> {
        let found = query
            .iter()
            .any(|(key, value)| key == "name" && value == "example.com");
        let result = if found {
            json!([{ "id": "zone1" }])
        } else {
            json!([])
        };
        Json(json!({ "success": true, "errors": [], "result": result }))
    }
    async fn create(
        State(fake): State<FakeCloudflare>,
        UrlPath(_zone): UrlPath<String>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let name = body["name"].as_str().unwrap().to_owned();
        let value = body["content"].as_str().unwrap().to_owned();
        fake.dns
            .record(&name, RData::TXT(TXT::new(vec![value.clone()])));
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/set-txt", fake.pebble_dns))
            .json(&json!({ "host": format!("{name}."), "value": value }))
            .send()
            .await
            .unwrap();
        Json(json!({ "success": true, "errors": [], "result": { "id": "record1" } }))
    }
    async fn verify() -> Json<Value> {
        Json(json!({ "success": true, "errors": [], "result": { "status": "active" } }))
    }
    async fn remove() -> Json<Value> {
        Json(json!({ "success": true, "errors": [], "result": { "id": "record1" } }))
    }
    let app = axum::Router::new()
        .route("/zones", get(zones))
        .route("/zones/{zone}/dns_records", axum::routing::post(create))
        .route("/zones/{zone}/dns_records/{record}", delete(remove))
        .route("/user/tokens/verify", get(verify))
        .with_state(FakeCloudflare { dns, pebble_dns });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{address}")
}

#[tokio::test]
#[ignore = "needs Docker and the Pebble images"]
async fn dns_01_goes_through_a_connected_provider() {
    let pebble = Pebble::start(free_port(), free_port()).await;
    let dns = FakeDns::start().await;
    dns.zone("example.com").a(HOST, OURS);
    let cloudflare = fake_cloudflare(dns.clone(), pebble.dns_management).await;
    let server = server_configured(&dns, |config| {
        config.acme.directory = pebble.directory.clone();
        config.acme.root = Some(fixture("pebble.minica.pem"));
        config.acme.dns_propagation_s = 10;
        config.acme.cloudflare_api = cloudflare;
        // Neither HTTP-01 nor TLS-ALPN-01 could work.
        config.acme.http_port = free_port();
    })
    .await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, HOST).await;
    check(&mut owner, id).await;
    let connected = info(
        owner
            .ok(Kind::SetDnsProvider(proto::SetDnsProvider {
                kind: "cloudflare".to_owned(),
                token: "cf-token-abcd".to_owned(),
            }))
            .await,
    );
    assert_eq!(connected.dns_providers.len(), 1);
    assert_eq!(connected.dns_providers[0].masked, "••••abcd");

    request(&mut owner, id).await;
    let status = settled(&mut owner, id).await;

    assert_eq!(
        status.state,
        proto::CertificateState::Valid as i32,
        "{status:?}"
    );
    served(&server, HOST, pebble.root().await).await;
}

#[tokio::test]
#[ignore = "needs Docker and the Pebble images"]
async fn renewal_swaps_the_certificate_in_place() {
    let http_port = free_port();
    let pebble = Pebble::start(http_port, free_port()).await;
    let (_dns, server, mut owner, id) = verified_domain(
        |config| config.acme.http_port = http_port,
        &pebble.directory,
    )
    .await;
    request(&mut owner, id).await;
    settled(&mut owner, id).await;
    let root = pebble.root().await;
    let first = served(&server, HOST, root.clone()).await;

    server.handle.renew_certificates_now().await;

    let second = served(&server, HOST, root).await;
    assert_ne!(first, second, "a new certificate is served");
    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert_eq!(
        domain(&info, HOST).certificate.as_ref().unwrap().state,
        proto::CertificateState::Valid as i32
    );
}

#[tokio::test]
async fn an_uploaded_certificate_is_checked_then_served() {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a(HOST, OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, HOST).await;
    check(&mut owner, id).await;
    let ca = TestCa::new();
    let issued = ca.issue(&[HOST]);
    let other = ca.issue(&["other.example.com"]);

    let wrong_name = owner
        .error(Kind::UploadCertificate(proto::UploadCertificate {
            domain_id: id,
            chain_pem: other.cert_pem.clone(),
            key_pem: other.key_pem.clone(),
        }))
        .await;
    let wrong_key = owner
        .error(Kind::UploadCertificate(proto::UploadCertificate {
            domain_id: id,
            chain_pem: issued.cert_pem.clone(),
            key_pem: other.key_pem.clone(),
        }))
        .await;
    assert!(wrong_name.message.contains(HOST), "{}", wrong_name.message);
    assert!(wrong_key.message.contains("key"), "{}", wrong_key.message);

    let uploaded = info(
        owner
            .ok(Kind::UploadCertificate(proto::UploadCertificate {
                domain_id: id,
                chain_pem: issued.cert_pem.clone(),
                key_pem: issued.key_pem.clone(),
            }))
            .await,
    );

    let status = domain(&uploaded, HOST).certificate.clone().unwrap();
    assert_eq!(status.state, proto::CertificateState::Valid as i32);
    assert!(status.uploaded);
    let served = served(&server, HOST, CertificateDer::from(ca.ca_der())).await;
    assert_eq!(served, first_der(&issued.cert_pem).to_vec());
    // Other names still get the server's own certificate.
    let (status, _) = server.get("/").await;
    assert_ne!(status, 0);
}

/// Behind Cloudflare's proxy, apps see Cloudflare's certificate on 443 and
/// Cloudflare reaches the main port, where the owner's origin certificate
/// answers for the name. Let's Encrypt is not asked for one.
#[tokio::test]
async fn a_cloudflare_proxied_domain_takes_an_origin_certificate() {
    const PROXIED: &str = "proxied.example.com";
    let dns = FakeDns::start().await;
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let added = info(
        owner
            .ok(Kind::AddDomain(proto::AddDomain {
                hostname: PROXIED.to_owned(),
                kind: proto::DomainKind::CloudflareProxy as i32,
                public_port: None,
            }))
            .await,
    );
    let proxied = domain(&added, PROXIED).clone();
    assert_eq!(proxied.public_port, 443, "apps use Cloudflare's port");
    let status = proxied.certificate.clone().unwrap();
    assert_eq!(status.state, proto::CertificateState::Provided as i32);
    assert_eq!(status.issuer, "Cloudflare");

    let refused = owner
        .error(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: proxied.id,
            email: None,
            agree_terms: true,
        }))
        .await;
    assert!(
        refused.message.contains("origin certificate"),
        "{}",
        refused.message
    );

    let ca = TestCa::new();
    let origin = ca.issue(&[PROXIED]);
    let uploaded = info(
        owner
            .ok(Kind::UploadCertificate(proto::UploadCertificate {
                domain_id: proxied.id,
                chain_pem: origin.cert_pem.clone(),
                key_pem: origin.key_pem.clone(),
            }))
            .await,
    );
    let status = domain(&uploaded, PROXIED).certificate.clone().unwrap();
    assert_eq!(status.state, proto::CertificateState::Valid as i32);
    assert!(status.uploaded);
    // Served at once, before any check: Cloudflare needs it to pass one.
    let served = served(&server, PROXIED, CertificateDer::from(ca.ca_der())).await;
    assert_eq!(served, first_der(&origin.cert_pem).to_vec());
}

#[tokio::test]
async fn requests_need_a_verified_domain_an_agreement_and_the_owner() {
    let dns = FakeDns::start().await;
    dns.zone("example.com")
        .a(HOST, OURS)
        .a("wrong.example.com", "198.51.100.1");
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, HOST).await;
    let wrong = add(&mut owner, "wrong.example.com").await;
    check(&mut owner, id).await;
    check(&mut owner, wrong).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _) = join(&server, 2, &code).await;

    let unverified = owner
        .error(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: wrong,
            email: None,
            agree_terms: true,
        }))
        .await;
    let unagreed = owner
        .error(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: id,
            email: None,
            agree_terms: false,
        }))
        .await;
    let bad_email = owner
        .error(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: id,
            email: Some("not an email".to_owned()),
            agree_terms: true,
        }))
        .await;
    let not_owner = member
        .error(Kind::RequestCertificate(proto::RequestCertificate {
            domain_id: id,
            email: None,
            agree_terms: true,
        }))
        .await;
    let provider_by_member = member
        .error(Kind::SetDnsProvider(proto::SetDnsProvider {
            kind: "duckdns".to_owned(),
            token: "x".repeat(36),
        }))
        .await;

    assert_eq!(unverified.code, proto::ErrorCode::DomainNotVerified as i32);
    assert!(
        unagreed.message.contains("agreement"),
        "{}",
        unagreed.message
    );
    assert_eq!(bad_email.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(not_owner.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(provider_by_member.code, proto::ErrorCode::Forbidden as i32);
}

#[tokio::test]
async fn dns_providers_keep_their_tokens_to_themselves() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;

    let bad = owner
        .error(Kind::SetDnsProvider(proto::SetDnsProvider {
            kind: "duckdns".to_owned(),
            token: "short".to_owned(),
        }))
        .await;
    let unknown = owner
        .error(Kind::SetDnsProvider(proto::SetDnsProvider {
            kind: "route53".to_owned(),
            token: "x".repeat(36),
        }))
        .await;
    let token = "0123456789abcdef0123456789abcdef7890";
    let connected = info(
        owner
            .ok(Kind::SetDnsProvider(proto::SetDnsProvider {
                kind: "duckdns".to_owned(),
                token: token.to_owned(),
            }))
            .await,
    );
    let disconnected = info(
        owner
            .ok(Kind::SetDnsProvider(proto::SetDnsProvider {
                kind: "duckdns".to_owned(),
                token: String::new(),
            }))
            .await,
    );

    assert_eq!(bad.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(unknown.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(connected.dns_providers[0].masked, "••••7890");
    assert!(!format!("{connected:?}").contains(token));
    assert!(disconnected.dns_providers.is_empty());
    // Sealed on disk, too.
    let db = std::fs::read(server.dir.path().join("data/opencord.db")).unwrap_or_default();
    assert!(
        !db.windows(token.len())
            .any(|window| window == token.as_bytes())
    );
}
