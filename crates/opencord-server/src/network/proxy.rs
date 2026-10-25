//! Tunnels and reverse proxies (Phase 3 plan §9.2–9.3): who a request
//! really came from. They forward to the main port (`main_port`, D69).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};

use crate::domains::check::is_cloudflare;
use crate::state::AppState;

const FORWARDED_FOR: &str = "x-forwarded-for";

/// Where a request came from: the connection's peer, or, when the peer is
/// a trusted proxy, the client its `X-Forwarded-For` names. A forwarded
/// address comes with port 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddr(pub SocketAddr);

impl FromRequestParts<Arc<AppState>> for ClientAddr {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let ConnectInfo(peer) = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .copied()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let settings = state.network.settings();
        // Cloudflare's proxy forwards from its own addresses while a
        // domain is proxied by it.
        let cloudflare = state.domains.cloudflare_proxied();
        Ok(Self(client_address(
            |ip| settings.trusts(ip) || (cloudflare && is_cloudflare(ip)),
            peer,
            &parts.headers,
        )))
    }
}

/// The client behind `peer`: the last address in `X-Forwarded-For` that
/// is not itself a trusted proxy, when `peer` is one.
pub fn client_address(
    trusts: impl Fn(IpAddr) -> bool,
    peer: SocketAddr,
    headers: &HeaderMap,
) -> SocketAddr {
    if !trusts(peer.ip()) {
        return peer;
    }
    let forwarded: Vec<IpAddr> = headers
        .get_all(FORWARDED_FOR)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|item| item.trim().parse::<IpAddr>().ok())
        .collect();
    forwarded
        .into_iter()
        .rev()
        .find(|ip| !trusts(*ip))
        .map_or(peer, |ip| SocketAddr::new(ip.to_canonical(), 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::settings::{IpRange, NetworkSettings};

    fn headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(FORWARDED_FOR, value.parse().unwrap());
        }
        headers
    }

    fn settings(trusted: &[&str]) -> NetworkSettings {
        NetworkSettings {
            trusted_proxies: trusted.iter().filter_map(|t| IpRange::parse(t)).collect(),
            ..NetworkSettings::default()
        }
    }

    #[test]
    fn a_trusted_proxy_names_the_client() {
        let settings = settings(&["127.0.0.1", "10.0.0.0/8"]);
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let client = client_address(
            |ip| settings.trusts(ip),
            peer,
            &headers(&["198.51.100.7, 10.1.1.1"]),
        );

        assert_eq!(client, "198.51.100.7:0".parse().unwrap());
    }

    #[test]
    fn anyone_else_is_taken_as_they_are() {
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "203.0.113.5:4000".parse().unwrap();

        assert_eq!(
            client_address(|ip| settings.trusts(ip), peer, &headers(&["1.2.3.4"])),
            peer
        );
    }

    #[test]
    fn a_spoofed_address_before_the_real_one_is_ignored() {
        // The client sent "6.6.6.6" itself; the proxy appended its address.
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let client = client_address(
            |ip| settings.trusts(ip),
            peer,
            &headers(&["6.6.6.6", "198.51.100.7"]),
        );

        assert_eq!(client.ip(), "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn without_a_header_the_proxy_itself_is_the_client() {
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        assert_eq!(
            client_address(|ip| settings.trusts(ip), peer, &HeaderMap::new()),
            peer
        );
        assert_eq!(
            client_address(|ip| settings.trusts(ip), peer, &headers(&["garbage"])),
            peer
        );
    }

    #[test]
    fn cloudflare_names_the_client_only_while_a_domain_is_proxied_by_it() {
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "104.16.0.10:443".parse().unwrap();
        let forwarded = headers(&["198.51.100.7"]);
        let settings = &settings;
        let trusts = |cloudflare: bool| {
            move |ip: IpAddr| settings.trusts(ip) || (cloudflare && is_cloudflare(ip))
        };

        assert_eq!(client_address(trusts(false), peer, &forwarded), peer);
        assert_eq!(
            client_address(trusts(true), peer, &forwarded),
            "198.51.100.7:0".parse().unwrap()
        );
    }
}
