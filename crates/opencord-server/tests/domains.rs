//! Domains, Phase 3 D2 (plan §6, §7.5–7.6, §21.1): adding, checking against
//! DNS, activating, the primary, re-checks, invites through a domain and the
//! web invite page. DNS is a fake server on 127.0.0.1.

mod common;

use std::time::Duration;

use common::domains::*;
use common::*;
use opencord_proto::v1 as proto;
use opencord_server::domains::dns::fake::FakeDns;
use proto::request::Kind;
use proto::response::Result as Response;

#[tokio::test]
async fn domains_are_checked_and_the_first_verified_one_becomes_primary() {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a("chat.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let code = invite(&mut owner, None).await;
    let (mut member, ready) = join(&server, 2, &code).await;
    assert_eq!(ready.server_addresses.unwrap().primary, None);

    let id = add(&mut owner, "Chat.Example.COM").await;
    let checked = check(&mut owner, id).await;

    assert_eq!(result(&checked), proto::CheckResult::Verified);
    assert_eq!(checked.status, proto::DomainStatus::Verified as i32);
    assert!(checked.active && checked.primary);
    let addresses = member
        .wait_for(|event| match event {
            proto::event::Kind::ServerAddressesUpdate(update) => update.addresses.clone(),
            _ => None,
        })
        .await;
    let primary = addresses.primary.unwrap();
    assert_eq!(primary.host, "chat.example.com");
    assert_eq!(primary.port, u32::from(server.handle.local_addr.port()));
    assert!(!primary.trusted_certificate, "no certificate yet");
    assert_eq!(addresses.ip.unwrap().host, OURS);
    // Whoever connects next learns it in Ready.
    let (_, later) = join(&server, 3, &code).await;
    assert_eq!(
        later.server_addresses.unwrap().primary.unwrap().host,
        "chat.example.com"
    );
}

#[tokio::test]
async fn wrong_records_say_what_is_wrong_and_cannot_be_activated() {
    let dns = FakeDns::start().await;
    dns.zone("example.com")
        .a("elsewhere.example.com", "198.51.100.1")
        .a("proxied.example.com", "104.26.12.205");
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;

    let elsewhere = add(&mut owner, "elsewhere.example.com").await;
    let proxied = add(&mut owner, "proxied.example.com").await;
    let missing = add(&mut owner, "missing.example.com").await;

    let checked = check(&mut owner, elsewhere).await;
    assert_eq!(result(&checked), proto::CheckResult::PointsElsewhere);
    assert!(
        checked.last_check.unwrap().message.contains("198.51.100.1"),
        "says where it points"
    );
    assert_eq!(
        result(&check(&mut owner, proxied).await),
        proto::CheckResult::CloudflareProxied
    );
    assert_eq!(
        result(&check(&mut owner, missing).await),
        proto::CheckResult::NoRecord
    );
    let refused = owner
        .error(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: elsewhere,
            active: Some(true),
            ..Default::default()
        }))
        .await;
    assert_eq!(refused.code, proto::ErrorCode::DomainNotVerified as i32);
}

#[tokio::test]
async fn names_are_checked_and_limited() {
    let dns = FakeDns::start().await;
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _) = join(&server, 2, &code).await;

    let add_kind = |host: &str| {
        Kind::AddDomain(proto::AddDomain {
            hostname: host.to_owned(),
            kind: proto::DomainKind::Own as i32,
            public_port: None,
        })
    };
    let forbidden = member.error(add_kind("chat.example.com")).await;
    assert_eq!(forbidden.code, proto::ErrorCode::Forbidden as i32);
    for bad in ["203.0.113.10", "localhost", "bad_name.example.com", ""] {
        let error = owner.error(add_kind(bad)).await;
        assert_eq!(error.code, proto::ErrorCode::DomainInvalid as i32, "{bad}");
    }
    let unicode = info(owner.ok(add_kind("bücher.example")).await);
    assert!(
        unicode
            .domains
            .iter()
            .any(|d| d.hostname == "xn--bcher-kva.example")
    );
    let again = owner.error(add_kind("XN--BCHER-KVA.example")).await;
    assert_eq!(again.code, proto::ErrorCode::DomainExists as i32);
    for n in 1..20 {
        owner.ok(add_kind(&format!("d{n}.example.com"))).await;
    }
    let full = owner.error(add_kind("d21.example.com")).await;
    assert_eq!(full.code, proto::ErrorCode::DomainLimit as i32);
}

#[tokio::test]
async fn checking_by_hand_waits_ten_seconds() {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a("chat.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, "chat.example.com").await;
    check(&mut owner, id).await;

    let again = owner
        .error(Kind::CheckDomain(proto::CheckDomain { domain_id: id }))
        .await;

    assert_eq!(again.code, proto::ErrorCode::RateLimited as i32);
    assert!(again.retry_after_ms.unwrap() > 5_000);
}

#[tokio::test]
async fn a_broken_record_marks_a_dns_problem_but_the_domain_stays_active() {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a("chat.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, "chat.example.com").await;
    check(&mut owner, id).await;

    dns.repoint("chat.example.com", "198.51.100.1");
    tokio::time::sleep(Duration::from_millis(10_100)).await;
    owner
        .ok(Kind::CheckDomain(proto::CheckDomain { domain_id: id }))
        .await;
    // The owner hears of it, and the page shows it.
    let mut notice = None;
    let broken = loop {
        match owner.next_event().await {
            proto::event::Kind::ServerNotice(found) => notice = Some(found),
            proto::event::Kind::NetworkStatus(status) => {
                let info = status.info.unwrap();
                let listed = domain(&info, "chat.example.com");
                if !listed.checking && listed.status == proto::DomainStatus::DnsProblem as i32 {
                    break listed.clone();
                }
            }
            _ => {}
        }
    };

    assert!(broken.problem_since_ms > 0);
    assert!(
        broken.active && broken.primary,
        "a short DNS problem breaks nothing"
    );
    let notice = notice.expect("a notice before the page's update");
    assert_eq!(notice.key, format!("domain.{id}.dns"));
    let history = match owner
        .ok(Kind::FetchDomainChecks(proto::FetchDomainChecks {
            domain_id: id,
        }))
        .await
    {
        Response::DomainChecks(history) => history.checks,
        other => panic!("{other:?}"),
    };
    assert_eq!(history.len(), 2);
    assert_eq!(
        history[0].result,
        proto::CheckResult::PointsElsewhere as i32
    );
    assert_eq!(history[1].result, proto::CheckResult::Verified as i32);
}

#[tokio::test]
async fn the_primary_moves_before_it_can_be_turned_off() {
    let dns = FakeDns::start().await;
    dns.zone("example.com")
        .a("one.example.com", OURS)
        .a("two.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let one = add(&mut owner, "one.example.com").await;
    let two = add(&mut owner, "two.example.com").await;
    check(&mut owner, one).await;
    check(&mut owner, two).await;
    owner
        .ok(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: two,
            active: Some(true),
            ..Default::default()
        }))
        .await;

    let refused = owner
        .error(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: one,
            active: Some(false),
            ..Default::default()
        }))
        .await;
    assert_eq!(refused.code, proto::ErrorCode::Conflict as i32);
    owner
        .ok(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: two,
            primary: Some(true),
            ..Default::default()
        }))
        .await;
    let off = info(
        owner
            .ok(Kind::UpdateDomain(proto::UpdateDomain {
                domain_id: one,
                active: Some(false),
                ..Default::default()
            }))
            .await,
    );

    let one = domain(&off, "one.example.com");
    assert!(!one.active && !one.primary);
    assert!(one.deactivated_at_ms > 0);
    assert!(domain(&off, "two.example.com").primary);
}

#[tokio::test]
async fn invites_go_through_a_domain_and_die_with_it() {
    let dns = FakeDns::start().await;
    dns.zone("example.com")
        .a("chat.example.com", OURS)
        .a("other.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let chat = add(&mut owner, "chat.example.com").await;
    let other = add(&mut owner, "other.example.com").await;
    check(&mut owner, chat).await;
    check(&mut owner, other).await;
    owner
        .ok(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: other,
            active: Some(true),
            ..Default::default()
        }))
        .await;

    let invite = |domain_id| {
        Kind::CreateInvite(proto::CreateInvite {
            max_uses: None,
            expires_in_s: None,
            domain_id,
        })
    };
    let Response::Invite(primary) = owner.ok(invite(None)).await else {
        panic!("an invite");
    };
    let Response::Invite(through_other) = owner.ok(invite(Some(other))).await else {
        panic!("an invite");
    };
    assert_eq!(primary.address.unwrap().host, "chat.example.com");
    assert_eq!(through_other.address.unwrap().host, "other.example.com");

    // The web page, opened through the domain.
    let (status, page) = server
        .get_as(
            &format!("/invite/{}", through_other.code),
            "other.example.com",
            &[],
        )
        .await;
    assert_eq!(status, 200);
    assert!(page.contains("Test Server"));
    assert!(page.contains("1 member"));
    assert!(page.contains(&format!(
        "opencords://other.example.com/invite/{}#fp=",
        through_other.code
    )));
    assert!(page.len() < 20_000);
    assert!(!page.contains("<script"));

    owner
        .ok(Kind::UpdateDomain(proto::UpdateDomain {
            domain_id: other,
            active: Some(false),
            ..Default::default()
        }))
        .await;
    let (status, page) = server
        .get_as(
            &format!("/invite/{}", through_other.code),
            "other.example.com",
            &[],
        )
        .await;
    assert_eq!(status, 404);
    assert!(page.contains("no longer valid"));
    let mut stranger = TestClient::connect(&server, key(9)).await;
    let refused = stranger
        .identify("Stranger", Some(&through_other.code), None)
        .await;
    assert!(refused.is_err(), "the invite died with its domain");
    let not_active = owner.error(invite(Some(other))).await;
    assert_eq!(not_active.code, proto::ErrorCode::DomainNotVerified as i32);
}

#[tokio::test]
async fn people_connecting_through_a_domain_are_counted() {
    let dns = FakeDns::start().await;
    dns.zone("example.com").a("chat.example.com", OURS);
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;
    let id = add(&mut owner, "chat.example.com").await;
    check(&mut owner, id).await;
    let code = invite(&mut owner, None).await;

    let mut through = TestClient::connect_as(&server, key(5), "chat.example.com").await;
    through
        .identify("Through", Some(&code), None)
        .await
        .unwrap();
    let listed = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );

    assert_eq!(domain(&listed, "chat.example.com").users_24h, 1);
}

#[tokio::test]
async fn the_verify_token_is_served_for_http_checks() {
    let dns = FakeDns::start().await;
    let server = server_with(&dns).await;

    let (status, first) = server.get("/.well-known/opencord/verify").await;
    let (_, second) = server.get("/.well-known/opencord/verify").await;

    assert_eq!(status, 200);
    assert_eq!(first.len(), 32);
    assert_eq!(first, second);
}

#[tokio::test]
async fn a_proxy_domain_is_served_plain_on_the_main_port() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dns = FakeDns::start().await;
    let server = server_with(&dns).await;
    let (mut owner, _) = owner_of(&server, &dns).await;

    let added = info(
        owner
            .ok(Kind::AddDomain(proto::AddDomain {
                hostname: "chat.example.com".to_owned(),
                kind: proto::DomainKind::ReverseProxy as i32,
                public_port: None,
            }))
            .await,
    );

    let proxied = domain(&added, "chat.example.com");
    assert_eq!(proxied.public_port, 443, "behind a proxy, 443 by default");
    assert!(proxied.trusted_certificate);
    assert!(
        added
            .ports
            .iter()
            .all(|p| p.purpose != proto::PortPurpose::LocalProxy as i32),
        "no second port"
    );
    // The proxy, on this machine, forwards plain HTTP to the main port.
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", server.handle.local_addr.port()))
        .await
        .unwrap();
    stream
        .write_all(
            b"GET /.well-known/opencord/verify HTTP/1.1\r\nHost: chat.example.com\r\n\
              X-Forwarded-For: 198.51.100.7\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
}
