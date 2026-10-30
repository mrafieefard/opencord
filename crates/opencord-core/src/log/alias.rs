//! Stand-ins for what identifies people in the session log (D82): servers
//! are `server-A`, addresses `public-ip-1` or `lan-ip-2`, users `user-1`
//! and the signed-in one `you`, channels `channel-1`, other hosts
//! `host-1`. Each stays the same for the whole session; the real values
//! live only in memory, never in the file.
//!
//! Code that logs a server, an address, a user or a channel passes it
//! through here; the filter (`scrub`) catches what it doesn't.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};

static ALIASES: Mutex<Option<Aliases>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Aliases) -> T) -> T {
    let mut slot = ALIASES.lock().unwrap_or_else(PoisonError::into_inner);
    f(slot.get_or_insert_with(Aliases::default))
}

/// A server, from its key (`host:port`) or host: `server-A`.
pub fn server(key: &str) -> String {
    with(|aliases| aliases.server(key))
}

/// Logs once what's known about a server, beside its alias:
/// `server-A is https, port 443, behind Cloudflare, version 0.0.1`.
pub fn server_facts(key: &str, facts: &str) {
    let (alias, new) = with(|aliases| {
        let alias = aliases.server(key);
        (
            alias.clone(),
            aliases.described.insert(alias, facts.to_owned()).as_deref() != Some(facts),
        )
    });
    if new {
        tracing::info!(target: "opencord_core::connection", "{alias} is {facts}");
    }
}

/// An IP address: `public-ip-1`, `lan-ip-2`, `loopback`, `ipv6-lan-1`…
pub fn address(ip: IpAddr) -> String {
    with(|aliases| aliases.address(ip))
}

/// Any host or address: a server's alias, an address's, or `host-1`.
pub fn host(host: &str) -> String {
    with(|aliases| aliases.host(host))
}

/// A user: `user-1`, or `you` for the signed-in user.
pub fn user(id: i64) -> String {
    with(|aliases| aliases.user(id))
}

/// The signed-in user's id, which reads `you` from now on.
pub fn set_self(id: i64) {
    with(|aliases| aliases.set_self(id));
}

/// A channel: `channel-1`.
pub fn channel(id: i64) -> String {
    with(|aliases| aliases.channel(id))
}

pub(super) fn scrub(text: &str, home: Option<&str>) -> String {
    with(|aliases| super::scrub::line_with(aliases, text, home).into_owned())
}

#[derive(Default)]
pub(crate) struct Aliases {
    /// Lowercased host → `server-X`.
    servers: HashMap<String, String>,
    addresses: HashMap<IpAddr, String>,
    hosts: HashMap<String, String>,
    users: HashMap<i64, String>,
    channels: HashMap<i64, String>,
    counts: HashMap<&'static str, usize>,
    described: HashMap<String, String>,
}

impl Aliases {
    pub(crate) fn server(&mut self, key: &str) -> String {
        let host = host_of(key).to_lowercase();
        if let Some(alias) = self.servers.get(&host) {
            return alias.clone();
        }
        let alias = format!("server-{}", letters(self.servers.len()));
        self.servers.insert(host, alias.clone());
        alias
    }

    /// Hosts registered as servers, for the filter to replace wherever
    /// they appear.
    pub(crate) fn server_hosts(&self) -> impl Iterator<Item = (&str, &str)> {
        self.servers
            .iter()
            .map(|(host, alias)| (host.as_str(), alias.as_str()))
    }

    pub(crate) fn address(&mut self, ip: IpAddr) -> String {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 => v4,
        };
        if ip.is_unspecified() {
            return ip.to_string();
        }
        if ip.is_loopback() {
            return "loopback".to_owned();
        }
        if let Some(alias) = self.addresses.get(&ip) {
            return alias.clone();
        }
        let kind = match ip {
            IpAddr::V4(v4) => {
                let [a, b, ..] = v4.octets();
                // Private, link-local and carrier-grade NAT (Tailscale).
                if v4.is_private() || v4.is_link_local() || (a == 100 && (64..128).contains(&b)) {
                    "lan-ip"
                } else {
                    "public-ip"
                }
            }
            IpAddr::V6(v6) => {
                let first = v6.segments()[0];
                // Unique local (fc00::/7) and link-local (fe80::/10).
                if first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80 {
                    "ipv6-lan"
                } else {
                    "ipv6-public"
                }
            }
        };
        let alias = format!("{kind}-{}", self.next(kind));
        self.addresses.insert(ip, alias.clone());
        alias
    }

    /// Any host: a server's alias, an address's, or `host-N`.
    pub(crate) fn host(&mut self, host: &str) -> String {
        if let Some(inner) = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            return format!("[{}]", self.host(inner));
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            return self.address(ip);
        }
        let lower = host.to_lowercase();
        if let Some(alias) = self.servers.get(&lower) {
            return alias.clone();
        }
        if let Some(alias) = self.hosts.get(&lower) {
            return alias.clone();
        }
        let alias = format!("host-{}", self.next("host"));
        self.hosts.insert(lower, alias.clone());
        alias
    }

    pub(crate) fn user(&mut self, id: i64) -> String {
        if let Some(alias) = self.users.get(&id) {
            return alias.clone();
        }
        let alias = format!("user-{}", self.next("user"));
        self.users.insert(id, alias.clone());
        alias
    }

    pub(crate) fn set_self(&mut self, id: i64) {
        self.users.insert(id, "you".to_owned());
    }

    pub(crate) fn channel(&mut self, id: i64) -> String {
        if let Some(alias) = self.channels.get(&id) {
            return alias.clone();
        }
        let alias = format!("channel-{}", self.next("channel"));
        self.channels.insert(id, alias.clone());
        alias
    }

    fn next(&mut self, kind: &'static str) -> usize {
        let count = self.counts.entry(kind).or_default();
        *count += 1;
        *count
    }
}

/// `host:port`, `[v6]:port` or a bare host → the host.
fn host_of(key: &str) -> &str {
    if let Some(rest) = key.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match key.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => host,
        _ => key,
    }
}

/// 0 → A, 25 → Z, 26 → AA.
fn letters(mut index: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn same_host_same_alias() {
        let mut aliases = Aliases::default();
        assert_eq!(aliases.server("opencord2.mamad.xyz:443"), "server-A");
        assert_eq!(aliases.server("chat.example.org"), "server-B");
        assert_eq!(aliases.server("OPENCORD2.mamad.xyz:7700"), "server-A");
    }

    #[test]
    fn servers_past_z_get_two_letters() {
        let mut aliases = Aliases::default();
        for index in 0..26 {
            aliases.server(&format!("host{index}.example.org"));
        }
        assert_eq!(aliases.server("one-more.example.org"), "server-AA");
    }

    #[test]
    fn addresses_keep_kind() {
        let mut aliases = Aliases::default();
        assert_eq!(aliases.address(ip("192.168.1.5")), "lan-ip-1");
        assert_eq!(aliases.address(ip("8.8.8.8")), "public-ip-1");
        assert_eq!(aliases.address(ip("100.111.246.56")), "lan-ip-2");
        assert_eq!(aliases.address(ip("192.168.1.5")), "lan-ip-1");
        assert_eq!(aliases.address(ip("127.0.0.1")), "loopback");
        assert_eq!(aliases.address(ip("::1")), "loopback");
        assert_eq!(aliases.address(ip("fd00::1")), "ipv6-lan-1");
        assert_eq!(aliases.address(ip("2001:4860::8888")), "ipv6-public-1");
        assert_eq!(aliases.address(ip("::ffff:8.8.8.8")), "public-ip-1");
        assert_eq!(aliases.address(ip("0.0.0.0")), "0.0.0.0");
    }

    #[test]
    fn users_and_self() {
        let mut aliases = Aliases::default();
        assert_eq!(aliases.user(41), "user-1");
        aliases.set_self(7);
        assert_eq!(aliases.user(7), "you");
        assert_eq!(aliases.user(99), "user-2");
        assert_eq!(aliases.user(41), "user-1");
        assert_eq!(aliases.channel(5), "channel-1");
    }

    #[test]
    fn hosts_that_are_addresses_or_servers() {
        let mut aliases = Aliases::default();
        aliases.server("chat.example.org:443");
        assert_eq!(aliases.host("chat.example.org"), "server-A");
        assert_eq!(aliases.host("10.0.0.2"), "lan-ip-1");
        assert_eq!(aliases.host("cdn.other.net"), "host-1");
        assert_eq!(aliases.host("[fd00::2]"), "[ipv6-lan-1]");
    }
}
