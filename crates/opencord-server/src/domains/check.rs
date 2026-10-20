//! What a domain check concludes (Phase 3 plan §6.2), in plain words.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

use super::dns::{DnsError, Resolved};
use crate::network::PublicIps;
use crate::network::settings::IpRange;

/// Cloudflare's proxy addresses (cloudflare.com/ips, 2026-10).
const CLOUDFLARE: [&str; 22] = [
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
    "2400:cb00::/32",
    "2606:4700::/32",
    "2803:f800::/32",
    "2405:b500::/32",
    "2405:8100::/32",
    "2a06:98c0::/29",
    "2c0f:f248::/32",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckResult {
    Verified,
    PointsElsewhere,
    NoRecord,
    CloudflareProxied,
    PartlyCorrect,
    /// The check could not be made: no answer, no public address known.
    Failed,
    /// The owner confirmed it by hand, after an HTTP check that could not
    /// loop back to the server's own address.
    ConfirmedByOwner,
}

impl CheckResult {
    pub fn passed(self) -> bool {
        matches!(self, Self::Verified | Self::ConfirmedByOwner)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub result: CheckResult,
    /// What happened, for the owner.
    pub message: String,
    /// Worth knowing even when it passed: a stale cache, a missing AAAA.
    pub note: Option<String>,
    #[serde(default)]
    pub found: Vec<IpAddr>,
    pub checked_at_ms: i64,
}

impl CheckOutcome {
    pub fn new(result: CheckResult, message: impl Into<String>, checked_at_ms: i64) -> Self {
        Self {
            result,
            message: message.into(),
            note: None,
            found: Vec::new(),
            checked_at_ms,
        }
    }
}

/// The outcome of a DNS check of `host`: whether its records point at
/// `expected`, and a note when public resolvers still give something else.
pub fn dns_outcome(
    host: &str,
    found: Result<Resolved, DnsError>,
    resolver: Option<&Resolved>,
    expected: PublicIps,
    now_ms: i64,
) -> CheckOutcome {
    let found = match found {
        Ok(found) => found,
        Err(DnsError::NoSuchDomain(_)) => {
            return CheckOutcome::new(
                CheckResult::NoRecord,
                format!("{host} has no DNS records yet. Create the records shown when adding it."),
                now_ms,
            );
        }
        Err(error) => {
            return CheckOutcome::new(
                CheckResult::Failed,
                format!("The DNS check could not finish: {error}."),
                now_ms,
            );
        }
    };
    let mut outcome = classify(host, &found, expected, now_ms);
    if let Some(resolver) = resolver
        && outcome.result != CheckResult::Failed
        && !same_addresses(resolver, &found)
    {
        let cached = list(resolver.addresses());
        let extra = match cached.is_empty() {
            true => "Public resolvers do not know it yet".to_owned(),
            false => format!("Public resolvers still give {cached}"),
        };
        outcome.note = Some(join_note(
            outcome.note.take(),
            format!("{extra}; they catch up within the record's time to live."),
        ));
    }
    outcome
}

/// Every record must point at this server, and at least one does.
fn classify(host: &str, found: &Resolved, expected: PublicIps, now_ms: i64) -> CheckOutcome {
    let addresses: Vec<IpAddr> = found.addresses().collect();
    let with = |result, message: String| CheckOutcome {
        found: addresses.clone(),
        ..CheckOutcome::new(result, message, now_ms)
    };
    if found.is_empty() {
        return with(
            CheckResult::NoRecord,
            format!("{host} has no A or AAAA record."),
        );
    }
    if expected.v4.is_none() && expected.v6.is_none() {
        return with(
            CheckResult::Failed,
            "This server's public address is not known yet: detect it or set it under Public address."
                .to_owned(),
        );
    }
    let v4_match = expected.v4.is_some_and(|ip| found.v4.contains(&ip));
    let v6_match = expected.v6.is_some_and(|ip| found.v6.contains(&ip));
    let wrong_v4: Vec<Ipv4Addr> = found
        .v4
        .iter()
        .copied()
        .filter(|ip| Some(*ip) != expected.v4)
        .collect();
    let wrong_v6: Vec<Ipv6Addr> = found
        .v6
        .iter()
        .copied()
        .filter(|ip| Some(*ip) != expected.v6)
        .collect();
    if !v4_match && !v6_match {
        if addresses.iter().all(|ip| is_cloudflare(*ip)) {
            return with(
                CheckResult::CloudflareProxied,
                "It points to Cloudflare's proxy. Set the record to DNS only (grey cloud), or add it \
                 again as a domain through Cloudflare's proxy: the proxy forwards only web traffic \
                 on a few ports, and no UDP."
                    .to_owned(),
            );
        }
        return with(
            CheckResult::PointsElsewhere,
            format!(
                "It points to {}, not this server ({}).",
                list(addresses.iter().copied()),
                list(expected_addresses(expected))
            ),
        );
    }
    if !wrong_v4.is_empty() || !wrong_v6.is_empty() {
        let mut problems = Vec::new();
        if !wrong_v4.is_empty() {
            problems.push(match expected.v4 {
                Some(ours) => format!(
                    "an A record points to {} instead of {ours}",
                    list(wrong_v4.iter().map(|ip| IpAddr::V4(*ip)))
                ),
                None => format!(
                    "an A record points to {}, but this server has no IPv4 address",
                    list(wrong_v4.iter().map(|ip| IpAddr::V4(*ip)))
                ),
            });
        }
        if !wrong_v6.is_empty() {
            problems.push(match expected.v6 {
                Some(ours) => format!(
                    "an AAAA record points to {} instead of {ours}",
                    list(wrong_v6.iter().map(|ip| IpAddr::V6(*ip)))
                ),
                None => format!(
                    "an AAAA record points to {}, but this server has no IPv6: remove it",
                    list(wrong_v6.iter().map(|ip| IpAddr::V6(*ip)))
                ),
            });
        }
        return with(
            CheckResult::PartlyCorrect,
            format!("Partly right: {}.", problems.join(", and ")),
        );
    }
    let mut verified = with(
        CheckResult::Verified,
        "It points to this server.".to_owned(),
    );
    if let Some(v6) = expected.v6
        && found.v6.is_empty()
    {
        verified.note = Some(format!(
            "No AAAA record: add one for {v6} so people on IPv6 connect directly."
        ));
    }
    verified
}

fn expected_addresses(expected: PublicIps) -> impl Iterator<Item = IpAddr> {
    expected
        .v4
        .map(IpAddr::V4)
        .into_iter()
        .chain(expected.v6.map(IpAddr::V6))
}

fn same_addresses(a: &Resolved, b: &Resolved) -> bool {
    let mut left: Vec<IpAddr> = a.addresses().collect();
    let mut right: Vec<IpAddr> = b.addresses().collect();
    left.sort();
    right.sort();
    left == right
}

pub fn is_cloudflare(ip: IpAddr) -> bool {
    CLOUDFLARE
        .iter()
        .filter_map(|range| IpRange::parse(range))
        .any(|range| range.contains(ip))
}

fn list(addresses: impl Iterator<Item = IpAddr>) -> String {
    addresses
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_note(existing: Option<String>, more: String) -> String {
    match existing {
        Some(existing) => format!("{existing} {more}"),
        None => more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: &str = "chat.example.com";
    const NOW: i64 = 1_700_000_000_000;

    fn ours() -> PublicIps {
        PublicIps {
            v4: Some("203.0.113.10".parse().unwrap()),
            v6: None,
        }
    }

    fn found(addresses: &[&str]) -> Resolved {
        let mut resolved = Resolved::default();
        for address in addresses {
            match address.parse::<IpAddr>().unwrap() {
                IpAddr::V4(v4) => resolved.v4.push(v4),
                IpAddr::V6(v6) => resolved.v6.push(v6),
            }
        }
        resolved
    }

    fn outcome(addresses: &[&str], expected: PublicIps) -> CheckOutcome {
        dns_outcome(HOST, Ok(found(addresses)), None, expected, NOW)
    }

    #[test]
    fn records_at_this_server_pass() {
        let passed = outcome(&["203.0.113.10"], ours());
        assert_eq!(passed.result, CheckResult::Verified);
        assert_eq!(passed.note, None);
    }

    #[test]
    fn records_elsewhere_say_where() {
        let elsewhere = outcome(&["198.51.100.1"], ours());
        assert_eq!(elsewhere.result, CheckResult::PointsElsewhere);
        assert!(elsewhere.message.contains("198.51.100.1"));
        assert!(elsewhere.message.contains("203.0.113.10"));
    }

    #[test]
    fn cloudflares_proxy_is_recognised() {
        let proxied = outcome(&["104.26.12.205", "2606:4700::6810:1"], ours());
        assert_eq!(proxied.result, CheckResult::CloudflareProxied);
        assert!(proxied.message.contains("grey cloud"));
    }

    #[test]
    fn a_wrong_aaaa_is_partly_correct_and_a_missing_one_a_note() {
        let dual = PublicIps {
            v4: ours().v4,
            v6: Some("2001:db8::10".parse().unwrap()),
        };
        let partly = outcome(&["203.0.113.10", "2001:db8::99"], dual);
        assert_eq!(partly.result, CheckResult::PartlyCorrect);
        assert!(
            partly.message.contains("2001:db8::99"),
            "{}",
            partly.message
        );
        let stray = outcome(&["203.0.113.10", "2001:db8::99"], ours());
        assert_eq!(stray.result, CheckResult::PartlyCorrect);
        assert!(stray.message.contains("no IPv6"), "{}", stray.message);
        let no_aaaa = outcome(&["203.0.113.10"], dual);
        assert_eq!(no_aaaa.result, CheckResult::Verified);
        assert!(no_aaaa.note.unwrap().contains("AAAA"));
        let both = outcome(&["203.0.113.10", "2001:db8::10"], dual);
        assert_eq!(both.result, CheckResult::Verified);
    }

    #[test]
    fn a_second_host_in_round_robin_is_partly_correct() {
        let partly = outcome(&["203.0.113.10", "198.51.100.1"], ours());
        assert_eq!(partly.result, CheckResult::PartlyCorrect);
    }

    #[test]
    fn nothing_found_or_nothing_known() {
        assert_eq!(outcome(&[], ours()).result, CheckResult::NoRecord);
        assert_eq!(
            outcome(&["203.0.113.10"], PublicIps::default()).result,
            CheckResult::Failed
        );
        let missing = dns_outcome(
            HOST,
            Err(DnsError::NoSuchDomain(HOST.to_owned())),
            None,
            ours(),
            NOW,
        );
        assert_eq!(missing.result, CheckResult::NoRecord);
        let silent = dns_outcome(
            HOST,
            Err(DnsError::NoAnswer("timeout".to_owned())),
            None,
            ours(),
            NOW,
        );
        assert_eq!(silent.result, CheckResult::Failed);
    }

    #[test]
    fn a_stale_resolver_is_noted() {
        let cached = found(&["198.51.100.1"]);
        let outcome = dns_outcome(
            HOST,
            Ok(found(&["203.0.113.10"])),
            Some(&cached),
            ours(),
            NOW,
        );

        assert_eq!(outcome.result, CheckResult::Verified);
        assert!(outcome.note.unwrap().contains("198.51.100.1"));
    }

    #[test]
    fn outcomes_survive_storage() {
        let outcome = outcome(&["198.51.100.1"], ours());
        let stored = serde_json::to_string(&outcome).unwrap();
        assert_eq!(
            serde_json::from_str::<CheckOutcome>(&stored).unwrap(),
            outcome
        );
    }
}
