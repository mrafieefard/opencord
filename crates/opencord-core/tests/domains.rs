//! Phase 3 D2 (plan §7.5, §21.1): clients and domains. A server behind a
//! proxy with an authority's certificate needs no fingerprint prompt; one
//! server reached at two addresses stays one entry; a primary domain that
//! does not answer falls back to the next address; the saved addresses
//! follow the server's; invites go through a domain.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use opencord_core::api::types::{
    AddServerOutcome, ChannelKind, ConnectionState, CoreEvent, CoreEventPayload, DomainKind,
    DomainUpdate, MediaEvent, NetworkSettingsChanges, VoiceConnectionState,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::media::MediaOptions;
use opencord_server::config::Config;
use opencord_server::domains::dns::fake::FakeDns;
use opencord_server::server::{self, ServerHandle};
use opencord_server::testing::TestCa;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_rustls::TlsAcceptor;

const WAIT: Duration = Duration::from_secs(15);
const OURS: &str = "203.0.113.10";

/// One authority for the whole test binary: clients trust it before their
/// first connection.
fn ca() -> &'static TestCa {
    static CA: OnceLock<TestCa> = OnceLock::new();
    CA.get_or_init(|| {
        let ca = TestCa::new();
        assert!(opencord_core::tofu::trust_test_roots(vec![
            CertificateDer::from(ca.ca_der())
        ]));
        ca
    })
}

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

async fn start(dns: Option<&FakeDns>) -> TestServer {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.server.bind = "127.0.0.1:0".parse().unwrap();
    config.server.public_host = "localhost".to_owned();
    config.server.data_dir = dir.path().join("data");
    config.server.name = "Domains".to_owned();
    config.voice.udp_port = 0;
    config.music.audio_udp_port = 0;
    config.network.ip_lookup_v4 = String::new();
    config.network.ip_lookup_v6 = String::new();
    if let Some(dns) = dns {
        config.network.dns_name_server_port = dns.address.port();
    }
    TestServer {
        handle: server::start(config).await.unwrap(),
        _dir: dir,
    }
}

/// A TLS proxy on `localhost` with an authority's certificate, forwarding
/// to the server's plain local listener: a reverse proxy, or a tunnel.
async fn proxy(server: &TestServer) -> u16 {
    let local = server
        .handle
        .set_local_listener_on_any_port(true)
        .await
        .unwrap()
        .unwrap();
    let issued = ca().issue(&["localhost"]);
    let chain = vec![CertificateDer::from_pem_slice(issued.cert_pem.as_bytes()).unwrap()];
    let key = PrivateKeyDer::from_pem_slice(issued.key_pem.as_bytes()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let Ok(mut upstream) =
                    tokio::net::TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], local))).await
                else {
                    return;
                };
                let _ = tokio::io::copy_bidirectional(&mut tls, &mut upstream).await;
            });
        }
    });
    port
}

struct TestClient {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    _dir: tempfile::TempDir,
}

impl TestClient {
    fn new(name: &str) -> Self {
        ca();
        let dir = tempfile::tempdir().unwrap();
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(Identity::generate(), name.to_owned());
        Self {
            client,
            events,
            _dir: dir,
        }
    }

    async fn wait_for<T>(&mut self, mut pick: impl FnMut(&CoreEventPayload) -> Option<T>) -> T {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let Some(found) = pick(&event.payload) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for an event")
    }

    /// Adds by `address`, trusting its certificate when asked.
    async fn add(&self, address: &str, claim: Option<String>) -> String {
        match self
            .client
            .add_server(address, claim.clone())
            .await
            .unwrap()
        {
            AddServerOutcome::Added(server) => server.key,
            AddServerOutcome::NeedsTrust {
                address,
                fingerprint,
            } => {
                self.client
                    .trust_fingerprint(&address, &fingerprint)
                    .unwrap();
                match self.client.add_server(&address, claim).await.unwrap() {
                    AddServerOutcome::Added(server) => server.key,
                    other => panic!("{other:?}"),
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_behind_a_proxy_with_an_authoritys_certificate_needs_no_prompt() {
    let server = start(None).await;
    let port = proxy(&server).await;
    let mut owner = TestClient::new("Owner");

    let outcome = owner
        .client
        .add_server(
            &format!("localhost:{port}"),
            server.handle.claim_token.clone(),
        )
        .await
        .unwrap();

    let AddServerOutcome::Added(added) = outcome else {
        panic!("asked to trust a certificate an authority signed: {outcome:?}");
    };
    assert_eq!(added.fingerprint, None, "nothing pinned");
    let ready = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::Ready(ready) => Some(ready.clone()),
            _ => None,
        })
        .await;
    assert!(ready.self_user.id > 0, "signed in through the proxy");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_server_at_another_address_stays_one_entry() {
    let server = start(None).await;
    let port = proxy(&server).await;
    let owner = TestClient::new("Owner");
    let direct = format!("127.0.0.1:{}", server.handle.local_addr.port());
    let first = owner.add(&direct, server.handle.claim_token.clone()).await;

    let second = owner.add(&format!("localhost:{port}"), None).await;

    assert_eq!(second, first);
    assert_eq!(owner.client.servers().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_primary_that_does_not_answer_falls_back_and_the_list_follows_the_server() {
    let dns = FakeDns::start().await;
    dns.zone("example.com")
        .a("chat.example.com", OURS)
        .a("other.example.com", OURS);
    let server = start(Some(&dns)).await;
    let mut owner = TestClient::new("Owner");
    let key = owner
        .add(
            &format!("127.0.0.1:{}", server.handle.local_addr.port()),
            server.handle.claim_token.clone(),
        )
        .await;
    let client = &owner.client;
    client
        .update_network_settings(
            &key,
            NetworkSettingsChanges {
                dns_resolvers: Some(vec![dns.address.to_string()]),
                ..NetworkSettingsChanges::default()
            },
        )
        .await
        .unwrap();
    client
        .set_public_address(&key, false, OURS.to_owned(), String::new())
        .await
        .unwrap();
    let mut ids = Vec::new();
    for host in ["chat.example.com", "other.example.com"] {
        let info = client
            .add_domain(&key, host.to_owned(), DomainKind::Own, None)
            .await
            .unwrap();
        ids.push(info.domains.last().unwrap().id);
    }
    for id in &ids {
        client.check_domain(&key, *id).await.unwrap();
    }
    let addresses = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ServerAddressesUpdate(addresses) if addresses.primary.is_some() => {
                Some(addresses.clone())
            }
            _ => None,
        })
        .await;
    let port = server.handle.local_addr.port();
    assert_eq!(
        addresses.primary.unwrap().host,
        "chat.example.com",
        "the first verified is the primary"
    );
    assert_eq!(
        owner.client.servers()[0].addresses,
        [format!("chat.example.com:{port}")]
    );

    // chat.example.com does not resolve for clients: the next one does.
    server.handle.drop_connections();
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Reconnecting { .. }) => Some(()),
            _ => None,
        })
        .await;
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Connected) => Some(()),
            _ => None,
        })
        .await;

    // The other domain goes active and becomes the primary; the first goes
    // off. The saved list follows.
    owner
        .client
        .update_domain(
            &key,
            ids[1],
            DomainUpdate {
                primary: Some(true),
                ..DomainUpdate::default()
            },
        )
        .await
        .unwrap();
    owner
        .client
        .update_domain(
            &key,
            ids[0],
            DomainUpdate {
                active: Some(false),
                ..DomainUpdate::default()
            },
        )
        .await
        .unwrap();
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ServerAddressesUpdate(addresses) if addresses.domains.len() == 1 => {
                Some(())
            }
            _ => None,
        })
        .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        owner.client.servers()[0].addresses,
        [format!("other.example.com:{port}")]
    );

    // Invites go through the primary, with the fingerprint: no authority
    // vouches for the domain yet.
    let invite = owner
        .client
        .create_invite(&key, None, None, None)
        .await
        .unwrap();
    assert!(
        invite
            .link
            .starts_with(&format!("opencord://other.example.com:{port}/invite/")),
        "{}",
        invite.link
    );
    assert!(invite.link.contains("#fp="));
    assert_eq!(
        invite.web_link.unwrap(),
        format!("https://other.example.com:{port}/invite/{}", invite.code)
    );
    assert_eq!(invite.domain_id, Some(ids[1]));
}

#[tokio::test(flavor = "multi_thread")]
async fn voice_through_a_proxy_is_checked_by_its_authority_too() {
    let server = start(None).await;
    let port = proxy(&server).await;
    let mut owner = TestClient::new("Owner");
    let key = owner
        .add(
            &format!("localhost:{port}"),
            server.handle.claim_token.clone(),
        )
        .await;
    let ready = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::Ready(ready) => Some(ready.clone()),
            _ => None,
        })
        .await;
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    let mut media = owner
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    let mut voice_servers = owner.client.voice_servers();

    owner.client.voice_join(&key, voice).await.unwrap();

    let update = tokio::time::timeout(WAIT, voice_servers.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        update.gateway_url(),
        format!("wss://localhost:{port}/voice")
    );
    assert!(
        update.certificate_fingerprint.is_empty(),
        "no pin: an authority"
    );
    tokio::time::timeout(WAIT, async {
        loop {
            if let Some(MediaEvent::ConnectionState { state, .. }) = media.recv().await
                && state == VoiceConnectionState::Connected
            {
                return;
            }
        }
    })
    .await
    .expect("voice connects through the proxy");
}
