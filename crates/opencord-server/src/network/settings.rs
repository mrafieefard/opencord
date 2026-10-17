//! The owner's network settings (Phase 3 plan §5, §8), kept in
//! `server_meta` under `network.`.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use opencord_proto::v1 as proto;
use sqlx::SqliteConnection;

use crate::db::meta;
use crate::error::ApiError;

const PREFIX: &str = "network.";
const MAIN_PORT: &str = "main_port";
const UPNP: &str = "upnp";
const LOCAL_LISTENER_PORT: &str = "local_listener_port";
const TRUSTED_PROXIES: &str = "trusted_proxies";
const DNS_RESOLVERS: &str = "dns_resolvers";
const ACME_STAGING: &str = "acme_staging";
const AUTO_DETECT: &str = "auto_detect";
const OVERRIDE_V4: &str = "override_ipv4";
const OVERRIDE_V6: &str = "override_ipv6";
const DETECTED_V4: &str = "detected_ipv4";
const DETECTED_V6: &str = "detected_ipv6";
const DETECTED_AT: &str = "detected_at_ms";

pub const DEFAULT_LOCAL_LISTENER_PORT: u16 = 7719;
/// Public resolvers that answer from anywhere (plan §6.2).
pub const DEFAULT_RESOLVERS: [&str; 3] = ["1.1.1.1", "8.8.8.8", "9.9.9.9"];
/// More would only slow a check down.
const MAX_LIST: usize = 16;

/// An address, or a range of them in CIDR notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpRange {
    pub base: IpAddr,
    pub prefix: u8,
}

impl IpRange {
    pub fn parse(text: &str) -> Option<Self> {
        let (address, prefix) = match text.trim().split_once('/') {
            Some((address, prefix)) => (address, Some(prefix.parse::<u8>().ok()?)),
            None => (text.trim(), None),
        };
        let base: IpAddr = address.parse().ok()?;
        let most = if base.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(most);
        (prefix <= most).then_some(Self { base, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.base, ip.to_canonical()) {
            (IpAddr::V4(base), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(base) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(base), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(base) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for IpRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let most = if self.base.is_ipv4() { 32 } else { 128 };
        if self.prefix == most {
            write!(f, "{}", self.base)
        } else {
            write!(f, "{}/{}", self.base, self.prefix)
        }
    }
}

/// A resolver for domain checks: an address, with port 53 unless given.
pub fn parse_resolver(text: &str) -> Option<SocketAddr> {
    let text = text.trim();
    text.parse::<SocketAddr>().ok().or_else(|| {
        text.parse::<IpAddr>()
            .ok()
            .map(|ip| SocketAddr::new(ip, 53))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    /// The main port from the next start.
    pub main_port: Option<u16>,
    pub upnp: bool,
    pub local_listener_port: u16,
    pub trusted_proxies: Vec<IpRange>,
    pub dns_resolvers: Vec<SocketAddr>,
    pub acme_staging: bool,
    pub auto_detect: bool,
    pub override_v4: Option<IpAddr>,
    pub override_v6: Option<IpAddr>,
    /// What detection found last, kept for the next start.
    pub detected_v4: Option<IpAddr>,
    pub detected_v6: Option<IpAddr>,
    pub detected_at_ms: i64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            main_port: None,
            upnp: false,
            local_listener_port: DEFAULT_LOCAL_LISTENER_PORT,
            // The local listener's peers: tunnels and proxies on this machine.
            trusted_proxies: ["127.0.0.1", "::1"]
                .into_iter()
                .filter_map(IpRange::parse)
                .collect(),
            dns_resolvers: DEFAULT_RESOLVERS
                .into_iter()
                .filter_map(parse_resolver)
                .collect(),
            acme_staging: false,
            auto_detect: true,
            override_v4: None,
            override_v6: None,
            detected_v4: None,
            detected_v6: None,
            detected_at_ms: 0,
        }
    }
}

impl NetworkSettings {
    pub async fn load(conn: &mut SqliteConnection) -> Result<Self, meta::MetaError> {
        let values: HashMap<String, String> = meta::load_prefixed(conn, PREFIX).await?;
        let get = |key: &str| values.get(&format!("{PREFIX}{key}")).map(String::as_str);
        let corrupt = |key: &'static str| meta::MetaError::Corrupt { key };
        let flag = |key: &'static str, default: bool| match get(key) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(_) => Err(corrupt(key)),
        };
        let ip = |key: &'static str| {
            get(key)
                .map(|value| value.parse::<IpAddr>().map_err(|_| corrupt(key)))
                .transpose()
        };
        let list = |key: &'static str| get(key).map(|value| value.split(',').collect::<Vec<_>>());
        let defaults = Self::default();
        Ok(Self {
            main_port: get(MAIN_PORT)
                .map(|value| value.parse().map_err(|_| corrupt(MAIN_PORT)))
                .transpose()?,
            upnp: flag(UPNP, defaults.upnp)?,
            local_listener_port: get(LOCAL_LISTENER_PORT)
                .map_or(Ok(defaults.local_listener_port), |value| {
                    value.parse().map_err(|_| corrupt(LOCAL_LISTENER_PORT))
                })?,
            trusted_proxies: match list(TRUSTED_PROXIES) {
                None => defaults.trusted_proxies,
                Some(items) => items
                    .into_iter()
                    .filter(|item| !item.is_empty())
                    .map(|item| IpRange::parse(item).ok_or(corrupt(TRUSTED_PROXIES)))
                    .collect::<Result<_, _>>()?,
            },
            dns_resolvers: match list(DNS_RESOLVERS) {
                None => defaults.dns_resolvers,
                Some(items) => items
                    .into_iter()
                    .filter(|item| !item.is_empty())
                    .map(|item| parse_resolver(item).ok_or(corrupt(DNS_RESOLVERS)))
                    .collect::<Result<_, _>>()?,
            },
            acme_staging: flag(ACME_STAGING, defaults.acme_staging)?,
            auto_detect: flag(AUTO_DETECT, defaults.auto_detect)?,
            override_v4: ip(OVERRIDE_V4)?,
            override_v6: ip(OVERRIDE_V6)?,
            detected_v4: ip(DETECTED_V4)?,
            detected_v6: ip(DETECTED_V6)?,
            detected_at_ms: get(DETECTED_AT).map_or(Ok(0), |value| {
                value.parse().map_err(|_| corrupt(DETECTED_AT))
            })?,
        })
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let joined = |items: Vec<String>| items.join(",");
        let entries = [
            (MAIN_PORT, self.main_port.map(|port| port.to_string())),
            (UPNP, Some(self.upnp.to_string())),
            (
                LOCAL_LISTENER_PORT,
                Some(self.local_listener_port.to_string()),
            ),
            (
                TRUSTED_PROXIES,
                Some(joined(
                    self.trusted_proxies
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                )),
            ),
            (
                DNS_RESOLVERS,
                Some(joined(
                    self.dns_resolvers.iter().map(ToString::to_string).collect(),
                )),
            ),
            (ACME_STAGING, Some(self.acme_staging.to_string())),
            (AUTO_DETECT, Some(self.auto_detect.to_string())),
            (OVERRIDE_V4, self.override_v4.map(|ip| ip.to_string())),
            (OVERRIDE_V6, self.override_v6.map(|ip| ip.to_string())),
            (DETECTED_V4, self.detected_v4.map(|ip| ip.to_string())),
            (DETECTED_V6, self.detected_v6.map(|ip| ip.to_string())),
            (DETECTED_AT, Some(self.detected_at_ms.to_string())),
        ];
        for (key, value) in entries {
            meta::put(conn, &format!("{PREFIX}{key}"), value.as_deref()).await?;
        }
        Ok(())
    }

    /// These settings with `request`'s changes, or why they cannot be.
    pub fn apply(&self, request: &proto::UpdateNetworkSettings) -> Result<Self, ApiError> {
        let mut next = self.clone();
        if let Some(port) = request.main_port {
            next.main_port = match port {
                0 => None,
                port => Some(
                    u16::try_from(port)
                        .map_err(|_| ApiError::invalid_argument("the main port is 1 to 65535"))?,
                ),
            };
        }
        if let Some(upnp) = request.upnp_enabled {
            next.upnp = upnp;
        }
        if let Some(port) = request.local_listener_port {
            next.local_listener_port = u16::try_from(port)
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| {
                    ApiError::invalid_argument("the local listener port is 1 to 65535")
                })?;
        }
        if let Some(list) = &request.trusted_proxies {
            next.trusted_proxies = parse_list(&list.values, IpRange::parse, "a trusted proxy")?;
        }
        if let Some(list) = &request.dns_resolvers {
            let resolvers = parse_list(&list.values, parse_resolver, "a DNS resolver")?;
            if resolvers.is_empty() {
                return Err(ApiError::invalid_argument(
                    "at least one DNS resolver is needed",
                ));
            }
            next.dns_resolvers = resolvers;
        }
        if let Some(staging) = request.acme_staging {
            next.acme_staging = staging;
        }
        Ok(next)
    }

    pub fn to_proto(&self) -> proto::NetworkSettings {
        proto::NetworkSettings {
            main_port: self.main_port.map_or(0, u32::from),
            upnp_enabled: self.upnp,
            local_listener_port: u32::from(self.local_listener_port),
            trusted_proxies: self
                .trusted_proxies
                .iter()
                .map(ToString::to_string)
                .collect(),
            dns_resolvers: self
                .dns_resolvers
                .iter()
                .map(|resolver| match resolver.port() {
                    53 => resolver.ip().to_string(),
                    _ => resolver.to_string(),
                })
                .collect(),
            acme_staging: self.acme_staging,
        }
    }

    /// Whether `ip`'s `X-Forwarded-For` is believed.
    pub fn trusts(&self, ip: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|range| range.contains(ip))
    }
}

fn parse_list<T>(
    values: &[String],
    parse: impl Fn(&str) -> Option<T>,
    what: &str,
) -> Result<Vec<T>, ApiError> {
    if values.len() > MAX_LIST {
        return Err(ApiError::invalid_argument(format!(
            "at most {MAX_LIST} entries"
        )));
    }
    values
        .iter()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            parse(value)
                .ok_or_else(|| ApiError::invalid_argument(format!("{value:?} is not {what}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_pool;

    #[test]
    fn ranges_match_their_addresses() {
        let range = IpRange::parse("10.0.0.0/8").unwrap();
        assert!(range.contains("10.200.1.2".parse().unwrap()));
        assert!(!range.contains("11.0.0.1".parse().unwrap()));
        let one = IpRange::parse("::1").unwrap();
        assert!(one.contains("::1".parse().unwrap()));
        assert!(!one.contains("127.0.0.1".parse().unwrap()));
        // An IPv4 peer seen on an IPv6 socket.
        assert!(range.contains("::ffff:10.1.1.1".parse().unwrap()));
        assert!(
            IpRange::parse("0.0.0.0/0")
                .unwrap()
                .contains("8.8.8.8".parse().unwrap())
        );
        assert_eq!(IpRange::parse("10.0.0.0/33"), None);
        assert_eq!(IpRange::parse("example.com"), None);
        assert_eq!(
            IpRange::parse("10.0.0.0/8").unwrap().to_string(),
            "10.0.0.0/8"
        );
    }

    #[test]
    fn resolvers_take_port_53_unless_given() {
        assert_eq!(
            parse_resolver("1.1.1.1"),
            Some("1.1.1.1:53".parse().unwrap())
        );
        assert_eq!(
            parse_resolver("[2606:4700::1111]:5353"),
            Some("[2606:4700::1111]:5353".parse().unwrap())
        );
        assert_eq!(parse_resolver("dns.google"), None);
    }

    #[tokio::test]
    async fn settings_save_and_load_back() {
        let (pool, _dir) = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        assert_eq!(
            NetworkSettings::load(&mut conn).await.unwrap(),
            NetworkSettings::default()
        );
        let changed = NetworkSettings {
            main_port: Some(443),
            upnp: true,
            trusted_proxies: vec![IpRange::parse("172.16.0.0/12").unwrap()],
            dns_resolvers: vec![parse_resolver("9.9.9.9").unwrap()],
            override_v4: Some("203.0.113.10".parse().unwrap()),
            detected_v6: Some("2001:db8::10".parse().unwrap()),
            detected_at_ms: 1_700_000_000_000,
            ..NetworkSettings::default()
        };

        changed.save(&mut conn).await.unwrap();

        assert_eq!(NetworkSettings::load(&mut conn).await.unwrap(), changed);
    }

    #[test]
    fn changes_are_checked() {
        let settings = NetworkSettings::default();
        let request = |change: fn(&mut proto::UpdateNetworkSettings)| {
            let mut request = proto::UpdateNetworkSettings::default();
            change(&mut request);
            settings.apply(&request)
        };
        assert!(request(|r| r.main_port = Some(70_000)).is_err());
        assert!(request(|r| r.local_listener_port = Some(0)).is_err());
        assert!(
            request(|r| r.trusted_proxies = Some(proto::StringList {
                values: vec!["not an address".into()]
            }))
            .is_err()
        );
        assert!(request(|r| r.dns_resolvers = Some(proto::StringList { values: vec![] })).is_err());
        let applied = request(|r| {
            r.main_port = Some(443);
            r.trusted_proxies = Some(proto::StringList {
                values: vec!["10.0.0.0/8".into(), " ".into()],
            });
        })
        .unwrap();
        assert_eq!(applied.main_port, Some(443));
        assert_eq!(applied.trusted_proxies.len(), 1);
        assert_eq!(request(|r| r.main_port = Some(0)).unwrap().main_port, None);
    }
}
