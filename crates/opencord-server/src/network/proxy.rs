//! Tunnels and reverse proxies (Phase 3 plan §9.2–9.3): who a request
//! really came from, and the plain listener on this machine they forward
//! to.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use tokio_util::sync::CancellationToken;

use super::settings::NetworkSettings;
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
        Ok(Self(client_address(
            &state.network.settings(),
            peer,
            &parts.headers,
        )))
    }
}

/// The client behind `peer`: the last address in `X-Forwarded-For` that
/// is not itself a trusted proxy, when `peer` is one.
pub fn client_address(
    settings: &NetworkSettings,
    peer: SocketAddr,
    headers: &HeaderMap,
) -> SocketAddr {
    if !settings.trusts(peer.ip()) {
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
        .find(|ip| !settings.trusts(*ip))
        .map_or(peer, |ip| SocketAddr::new(ip.to_canonical(), 0))
}

/// Marks requests that came through the local listener: from a tunnel or a
/// reverse proxy.
#[derive(Debug, Clone, Copy)]
pub struct ThroughLocalListener;

/// The plain listener on this machine that tunnels and reverse proxies
/// forward to: the same routes as the main port, without TLS.
#[derive(Debug)]
pub struct LocalListener {
    pub port: u16,
    stop: CancellationToken,
    serving: Option<tokio::task::JoinHandle<()>>,
}

impl LocalListener {
    pub async fn start(state: Arc<AppState>, port: u16) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
        let port = listener.local_addr()?.port();
        let stop = CancellationToken::new();
        let app = crate::http::router(Arc::clone(&state))
            .layer(axum::Extension(ThroughLocalListener))
            .into_make_service_with_connect_info::<SocketAddr>();
        let stopped = stop.clone();
        let serving = tokio::spawn(async move {
            let serve = axum::serve(listener, app)
                .with_graceful_shutdown(async move { stopped.cancelled().await });
            if let Err(error) = serve.await {
                tracing::warn!(%error, "the local listener stopped");
            }
        });
        Ok(Self {
            port,
            stop,
            serving: Some(serving),
        })
    }

    pub fn stop(&self) {
        self.stop.cancel();
    }

    /// Stops it and waits, a moment at most, until its port is free.
    pub async fn close(mut self) {
        self.stop.cancel();
        if let Some(serving) = self.serving.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), serving).await;
        }
    }
}

impl Drop for LocalListener {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::settings::IpRange;

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

        let client = client_address(&settings, peer, &headers(&["198.51.100.7, 10.1.1.1"]));

        assert_eq!(client, "198.51.100.7:0".parse().unwrap());
    }

    #[test]
    fn anyone_else_is_taken_as_they_are() {
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "203.0.113.5:4000".parse().unwrap();

        assert_eq!(
            client_address(&settings, peer, &headers(&["1.2.3.4"])),
            peer
        );
    }

    #[test]
    fn a_spoofed_address_before_the_real_one_is_ignored() {
        // The client sent "6.6.6.6" itself; the proxy appended its address.
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        let client = client_address(&settings, peer, &headers(&["6.6.6.6", "198.51.100.7"]));

        assert_eq!(client.ip(), "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn without_a_header_the_proxy_itself_is_the_client() {
        let settings = settings(&["127.0.0.1"]);
        let peer: SocketAddr = "127.0.0.1:5000".parse().unwrap();

        assert_eq!(client_address(&settings, peer, &HeaderMap::new()), peer);
        assert_eq!(
            client_address(&settings, peer, &headers(&["garbage"])),
            peer
        );
    }
}
