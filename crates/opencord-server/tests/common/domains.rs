//! Domains on a test server whose DNS checks ask a fake name server.

use opencord_proto::v1 as proto;
use opencord_server::config::Config;
use opencord_server::domains::dns::fake::FakeDns;
use proto::request::Kind;
use proto::response::Result as Response;

use super::{TestClient, TestServer, owner};

pub const OURS: &str = "203.0.113.10";

/// A server whose public address is [`OURS`] and whose domain checks ask
/// `dns`.
pub async fn server_with(dns: &FakeDns) -> TestServer {
    server_configured(dns, |_| {}).await
}

/// [`server_with`], with more of the config changed.
pub async fn server_configured(dns: &FakeDns, customize: impl FnOnce(&mut Config)) -> TestServer {
    let port = dns.address.port();
    TestServer::start_with(|config| {
        config.network.dns_name_server_port = port;
        customize(config);
    })
    .await
}

/// The owner, with the resolvers pointed at `dns` and the address set.
pub async fn owner_of(server: &TestServer, dns: &FakeDns) -> (TestClient, proto::Ready) {
    let (mut owner, ready) = owner(server).await;
    owner
        .ok(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            dns_resolvers: Some(proto::StringList {
                values: vec![dns.address.to_string()],
            }),
            ..Default::default()
        }))
        .await;
    owner
        .ok(Kind::SetPublicAddress(proto::SetPublicAddress {
            auto_detect: false,
            override_ipv4: OURS.to_owned(),
            override_ipv6: String::new(),
        }))
        .await;
    (owner, ready)
}

pub fn info(response: Response) -> proto::NetworkInfo {
    match response {
        Response::NetworkInfo(info) => *info,
        other => panic!("expected network info, got {other:?}"),
    }
}

pub fn domain<'a>(info: &'a proto::NetworkInfo, host: &str) -> &'a proto::Domain {
    info.domains
        .iter()
        .find(|domain| domain.hostname == host)
        .unwrap_or_else(|| panic!("{host} is listed"))
}

pub async fn add(owner: &mut TestClient, host: &str) -> i64 {
    let added = info(
        owner
            .ok(Kind::AddDomain(proto::AddDomain {
                hostname: host.to_owned(),
                kind: proto::DomainKind::Own as i32,
                public_port: None,
            }))
            .await,
    );
    domain(&added, &host.to_ascii_lowercase()).id
}

/// Checks a domain and waits for the result.
pub async fn check(owner: &mut TestClient, id: i64) -> proto::Domain {
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    owner
        .ok(Kind::CheckDomain(proto::CheckDomain { domain_id: id }))
        .await;
    owner
        .wait_for(|event| match event {
            proto::event::Kind::NetworkStatus(status) => status
                .info
                .as_ref()?
                .domains
                .iter()
                .find(|domain| domain.id == id && !domain.checking && domain.last_check.is_some())
                .cloned(),
            _ => None,
        })
        .await
}

pub fn result(domain: &proto::Domain) -> proto::CheckResult {
    proto::CheckResult::try_from(domain.last_check.as_ref().unwrap().result).unwrap()
}
