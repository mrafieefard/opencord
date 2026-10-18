//! The DNS checker against [`fake::FakeDns`] (plan §21.1): matching,
//! mismatching, CNAME-chained, truncated and missing records, and silence.

use std::time::Duration;

use hickory_proto::rr::RecordType;

use super::fake::FakeDns;
use super::*;

async fn checker(fake: &FakeDns) -> DnsChecker {
    DnsChecker {
        resolvers: vec![fake.address],
        name_server_port: fake.address.port(),
        timeout: Duration::from_millis(300),
    }
}

#[tokio::test]
async fn the_name_servers_are_asked_directly() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .a("chat.example.com", "203.0.113.10")
        .aaaa("chat.example.com", "2001:db8::10");
    let checker = checker(&fake).await;

    let found = checker.authoritative("Chat.Example.com").await.unwrap();

    assert_eq!(found.v4, ["203.0.113.10".parse::<Ipv4Addr>().unwrap()]);
    assert_eq!(found.v6, ["2001:db8::10".parse::<Ipv6Addr>().unwrap()]);
    assert_eq!(found.chain, ["chat.example.com"]);
    // The addresses came from a question without recursion.
    assert!(
        fake.asked()
            .iter()
            .any(|(name, kind, recursion)| name == "chat.example.com"
                && *kind == RecordType::A
                && !recursion)
    );
}

#[tokio::test]
async fn cnames_are_followed_within_and_across_zones() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .zone("hosting.net")
        .cname("chat.example.com", "www.example.com")
        .cname("www.example.com", "box7.hosting.net")
        .a("box7.hosting.net", "198.51.100.7");
    let checker = checker(&fake).await;

    let found = checker.authoritative("chat.example.com").await.unwrap();

    assert_eq!(found.v4, ["198.51.100.7".parse::<Ipv4Addr>().unwrap()]);
    assert_eq!(
        found.chain,
        ["chat.example.com", "www.example.com", "box7.hosting.net"]
    );
}

#[tokio::test]
async fn a_cname_loop_stops() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .cname("a.example.com", "b.example.com")
        .cname("b.example.com", "a.example.com");
    let checker = checker(&fake).await;

    let error = checker.authoritative("a.example.com").await.unwrap_err();

    assert_eq!(error, DnsError::TooManyCnames);
}

#[tokio::test]
async fn a_truncated_answer_is_asked_again_over_tcp() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .a("big.example.com", "203.0.113.10")
        .truncate("big.example.com");
    let checker = checker(&fake).await;

    let found = checker.authoritative("big.example.com").await.unwrap();

    assert_eq!(found.v4, ["203.0.113.10".parse::<Ipv4Addr>().unwrap()]);
}

#[tokio::test]
async fn a_missing_record_or_domain_is_said_so() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .a("other.example.com", "203.0.113.10");
    let checker = checker(&fake).await;

    let missing = checker.authoritative("chat.example.com").await;
    let unknown = checker.authoritative("chat.nowhere.test").await;

    assert_eq!(
        missing.unwrap_err(),
        DnsError::NoSuchDomain("chat.example.com".to_owned())
    );
    assert!(matches!(unknown, Err(DnsError::NoSuchDomain(_))));
}

#[tokio::test]
async fn a_name_with_other_records_only_has_no_addresses() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .aaaa("v6only.example.com", "2001:db8::1");
    let checker = checker(&fake).await;

    let found = checker.authoritative("v6only.example.com").await.unwrap();

    assert!(found.v4.is_empty());
    assert_eq!(found.v6.len(), 1);
}

#[tokio::test]
async fn silence_times_out() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .a("chat.example.com", "203.0.113.10")
        .silence("chat.example.com");
    let checker = checker(&fake).await;
    let started = std::time::Instant::now();

    let error = checker.authoritative("chat.example.com").await.unwrap_err();

    assert!(matches!(error, DnsError::NoAnswer(_)), "{error:?}");
    // Three tries of 300 ms, then it gives up.
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn the_resolvers_view_can_differ_from_the_name_servers() {
    let fake = FakeDns::start().await;
    fake.zone("example.com")
        .a("chat.example.com", "203.0.113.10");
    let checker = checker(&fake).await;

    let recursive = checker.recursive("chat.example.com").await.unwrap();

    assert_eq!(recursive.v4, ["203.0.113.10".parse::<Ipv4Addr>().unwrap()]);
    assert!(
        fake.asked()
            .iter()
            .any(|(name, _, recursion)| name == "chat.example.com" && *recursion)
    );
}

#[tokio::test]
async fn a_bad_name_is_refused_before_asking() {
    let fake = FakeDns::start().await;
    let checker = checker(&fake).await;

    let error = checker.authoritative("not a name!").await.unwrap_err();

    assert!(matches!(error, DnsError::InvalidName(_)));
    assert!(fake.asked().is_empty());
}

/// The real thing, through public resolvers: a plain name, and one behind
/// CNAMEs in other zones.
#[tokio::test]
#[ignore = "asks the internet's DNS"]
async fn real_names_resolve_at_their_name_servers() {
    let checker = DnsChecker::new(
        ["1.1.1.1:53", "8.8.8.8:53"]
            .iter()
            .map(|server| server.parse().unwrap())
            .collect(),
    );
    for host in [
        "github.com",
        "www.github.com",
        "api.ipify.org",
        "duckdns.org",
    ] {
        let authoritative = checker.authoritative(host).await;
        let recursive = checker.recursive(host).await;
        println!("{host}: {authoritative:?}\n  resolver: {recursive:?}");
        let found = authoritative.unwrap();
        assert!(!found.is_empty(), "{host}");
    }
}
