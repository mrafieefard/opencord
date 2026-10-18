//! Free subdomains, Phase 3 D4 (plan §9.1): pointed here through the
//! provider, verified, kept pointed here when the address changes, and
//! given a certificate by themselves. DuckDNS is a fake API that writes
//! into the fake name server (and Pebble's, for the DNS challenge).

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::routing::get;
use common::domains::*;
use common::pebble::*;
use common::*;
use hickory_proto::rr::RData;
use hickory_proto::rr::rdata::TXT;
use opencord_proto::v1 as proto;
use opencord_server::domains::dns::fake::FakeDns;
use proto::request::Kind;
use serde_json::json;

const HOST: &str = "mychat.duckdns.org";
const TOKEN: &str = "0123456789abcdef0123456789abcdef7890";
const NEW_IP: &str = "203.0.113.77";

/// DuckDNS's `/update`, as far as Opencord uses it.
#[derive(Clone)]
struct FakeDuckDns {
    dns: FakeDns,
    pebble_dns: Option<u16>,
    updates: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

impl FakeDuckDns {
    async fn start(dns: FakeDns, pebble_dns: Option<u16>) -> (Self, String) {
        async fn update(
            State(fake): State<FakeDuckDns>,
            Query(query): Query<HashMap<String, String>>,
        ) -> &'static str {
            if query.get("token").map(String::as_str) != Some(TOKEN) {
                return "KO";
            }
            let host = format!("{}.duckdns.org", query["domains"]);
            if let Some(ip) = query.get("ip") {
                fake.dns.repoint(&host, ip);
            }
            if let Some(value) = query.get("txt").filter(|value| !value.is_empty()) {
                let name = format!("_acme-challenge.{host}");
                fake.dns
                    .record(&name, RData::TXT(TXT::new(vec![value.clone()])));
                if let Some(port) = fake.pebble_dns {
                    reqwest::Client::new()
                        .post(format!("http://127.0.0.1:{port}/set-txt"))
                        .json(&json!({ "host": format!("{name}."), "value": value }))
                        .send()
                        .await
                        .unwrap();
                }
            }
            fake.updates.lock().unwrap().push(query);
            "OK"
        }
        let fake = Self {
            dns,
            pebble_dns,
            updates: Arc::default(),
        };
        let app = axum::Router::new()
            .route("/update", get(update))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        (fake, format!("http://{address}"))
    }

    fn ips(&self) -> Vec<String> {
        self.updates
            .lock()
            .unwrap()
            .iter()
            .filter_map(|query| query.get("ip").cloned())
            .collect()
    }
}

/// The domain once `pick` finds what it waits for.
async fn domain_when(
    owner: &mut TestClient,
    pick: impl Fn(&proto::Domain) -> bool,
    wait: Duration,
) -> proto::Domain {
    let found = owner.wait_for(|event| match event {
        proto::event::Kind::NetworkStatus(status) => status
            .info
            .as_ref()?
            .domains
            .iter()
            .find(|domain| domain.hostname == HOST && pick(domain))
            .cloned(),
        _ => None,
    });
    tokio::time::timeout(wait, found)
        .await
        .expect("the domain gets there")
}

fn add_request(token: &str, agree: bool) -> Kind {
    Kind::AddFreeSubdomain(proto::AddFreeSubdomain {
        provider: "duckdns".to_owned(),
        name: "MyChat".to_owned(),
        token: token.to_owned(),
        email: None,
        agree_terms: agree,
    })
}

#[tokio::test]
async fn a_free_subdomain_is_pointed_here_verified_and_kept_pointed_here() {
    let dns = FakeDns::start().await;
    dns.zone("duckdns.org");
    let (duckdns, api) = FakeDuckDns::start(dns.clone(), None).await;
    let server = server_configured(&dns, |config| config.acme.duckdns_api = api).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;

    let wrong = owner
        .error(add_request("wrong-token-wrong-token-wrong-token", true))
        .await;
    let unagreed = owner.error(add_request(TOKEN, false)).await;
    assert_eq!(wrong.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(unagreed.code, proto::ErrorCode::AcmeFailed as i32);
    let after_wrong = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert!(
        after_wrong.dns_providers.is_empty(),
        "a refused token is not kept"
    );
    assert!(after_wrong.domains.is_empty());
    owner.ok(add_request(TOKEN, true)).await;
    let verified = domain_when(
        &mut owner,
        |domain| domain.status == proto::DomainStatus::Verified as i32,
        Duration::from_secs(10),
    )
    .await;

    assert_eq!(verified.kind, proto::DomainKind::FreeSubdomain as i32);
    assert!(verified.active && verified.primary);
    assert_eq!(duckdns.ips(), [OURS]);
    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert_eq!(
        info.dns_providers[0].kind, "duckdns",
        "connected on the way"
    );

    // The address changes: the name follows, and still checks out.
    owner
        .ok(Kind::SetPublicAddress(proto::SetPublicAddress {
            auto_detect: false,
            override_ipv4: NEW_IP.to_owned(),
            override_ipv6: String::new(),
        }))
        .await;
    let rechecked = domain_when(
        &mut owner,
        |domain| {
            domain
                .last_check
                .as_ref()
                .is_some_and(|check| check.found.iter().any(|ip| ip == NEW_IP))
        },
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(duckdns.ips(), [OURS, NEW_IP]);
    assert_eq!(rechecked.status, proto::DomainStatus::Verified as i32);
}

#[tokio::test]
async fn names_and_providers_are_checked() {
    let dns = FakeDns::start().await;
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;

    let bad_name = owner
        .error(Kind::AddFreeSubdomain(proto::AddFreeSubdomain {
            provider: "duckdns".to_owned(),
            name: "my_chat".to_owned(),
            token: TOKEN.to_owned(),
            email: None,
            agree_terms: true,
        }))
        .await;
    let cloudflare = owner
        .error(Kind::AddFreeSubdomain(proto::AddFreeSubdomain {
            provider: "cloudflare".to_owned(),
            name: "chat".to_owned(),
            token: TOKEN.to_owned(),
            email: None,
            agree_terms: true,
        }))
        .await;
    let no_token = owner.error(add_request("", true)).await;

    assert_eq!(bad_name.code, proto::ErrorCode::DomainInvalid as i32);
    assert_eq!(cloudflare.code, proto::ErrorCode::InvalidArgument as i32);
    assert!(no_token.message.contains("token"), "{}", no_token.message);
    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert!(info.domains.is_empty(), "nothing is left behind");
}

#[tokio::test]
#[ignore = "needs Docker and the Pebble images"]
async fn a_free_subdomain_gets_its_certificate_by_itself() {
    let pebble = Pebble::start(free_port(), free_port()).await;
    let dns = FakeDns::start().await;
    dns.zone("duckdns.org");
    let (_duckdns, api) = FakeDuckDns::start(dns.clone(), Some(pebble.dns_management)).await;
    let directory = pebble.directory.clone();
    let server = server_configured(&dns, |config| {
        config.acme.duckdns_api = api;
        config.acme.directory = directory;
        config.acme.root = Some(fixture("pebble.minica.pem"));
        config.acme.dns_propagation_s = 10;
        // Only the DNS challenge can work.
        config.acme.http_port = free_port();
    })
    .await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;

    owner.ok(add_request(TOKEN, true)).await;
    let done = domain_when(
        &mut owner,
        |domain| {
            domain.certificate.as_ref().is_some_and(|certificate| {
                certificate.state == proto::CertificateState::Valid as i32
                    || certificate.state == proto::CertificateState::Failed as i32
            })
        },
        Duration::from_secs(60),
    )
    .await;

    let certificate = done.certificate.unwrap();
    assert_eq!(
        certificate.state,
        proto::CertificateState::Valid as i32,
        "{}",
        certificate.error
    );
    served(&server, HOST, pebble.root().await).await;
}
