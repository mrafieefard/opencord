//! Plugins' internet access (spec §10.7, plan Task 10).
//!
//! A plugin reaches only its manifest's `domains`, by name, over http or
//! https. Every new connection resolves the name again through [`Resolve`]
//! and keeps only the addresses plugins may reach ([`is_forbidden`]), so a
//! DNS answer that changes between checks gains nothing, and every
//! redirect's address is checked again. The connector dials an IP literal
//! without asking the resolver, so URLs naming an address are refused before
//! sending and at every redirect.

use std::error::Error as StdError;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use opencord_plugins::limits::{
    HTTP_CONCURRENT, HTTP_MAX_REDIRECTS, HTTP_MAX_RESPONSE_BYTES, HTTP_TIMEOUT,
};
use opencord_proto::v1 as proto;
use reqwest::header::{HeaderMap, HeaderName};
use reqwest::{Method, Url};
use tokio::sync::Semaphore;

use super::runtime::PluginEnv;

const MB: u64 = 1024 * 1024;

/// IPv4 networks plugins may not reach: this network, private and shared
/// networks, loopback, link-local (cloud metadata), protocol assignments,
/// documentation, 6to4 relays, benchmarking, multicast and reserved.
const V4_FORBIDDEN: &[([u8; 4], u32)] = &[
    ([0, 0, 0, 0], 8),
    ([10, 0, 0, 0], 8),
    ([100, 64, 0, 0], 10),
    ([127, 0, 0, 0], 8),
    ([169, 254, 0, 0], 16),
    ([172, 16, 0, 0], 12),
    ([192, 0, 0, 0], 24),
    ([192, 0, 2, 0], 24),
    ([192, 88, 99, 0], 24),
    ([192, 168, 0, 0], 16),
    ([198, 18, 0, 0], 15),
    ([198, 51, 100, 0], 24),
    ([203, 0, 113, 0], 24),
    ([224, 0, 0, 0], 4),
    ([240, 0, 0, 0], 4),
];

/// IPv6 networks plugins may not reach, past those judged by the IPv4
/// address inside them ([`embedded_v4`]): discard, Teredo, NAT64 for local
/// use, documentation, unique local, link-local, site-local and multicast.
const V6_FORBIDDEN: &[([u16; 8], u32)] = &[
    ([0x0100, 0, 0, 0, 0, 0, 0, 0], 64),
    ([0x2001, 0, 0, 0, 0, 0, 0, 0], 32),
    ([0x0064, 0xff9b, 1, 0, 0, 0, 0, 0], 48),
    ([0x2001, 0x0db8, 0, 0, 0, 0, 0, 0], 32),
    ([0xfc00, 0, 0, 0, 0, 0, 0, 0], 7),
    ([0xfe80, 0, 0, 0, 0, 0, 0, 0], 10),
    ([0xfec0, 0, 0, 0, 0, 0, 0, 0], 10),
    ([0xff00, 0, 0, 0, 0, 0, 0, 0], 8),
];

/// Whether plugins may not reach `ip`.
pub fn is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_forbidden(v4),
        IpAddr::V6(v6) => v6_forbidden(v6),
    }
}

fn v4_forbidden(ip: Ipv4Addr) -> bool {
    V4_FORBIDDEN
        .iter()
        .any(|&(network, len)| in_v4(ip, Ipv4Addr::from(network), len))
}

fn v6_forbidden(ip: Ipv6Addr) -> bool {
    match embedded_v4(ip) {
        Some(v4) => v4_forbidden(v4),
        None => V6_FORBIDDEN
            .iter()
            .any(|&(network, len)| in_v6(ip, Ipv6Addr::from(network), len)),
    }
}

/// The IPv4 address an IPv6 one leads to: IPv4-compatible (`::/96`, which
/// holds `::` and `::1`), IPv4-mapped (`::ffff:0:0/96`), NAT64
/// (`64:ff9b::/96`) and 6to4 (`2002::/16`).
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let last = Ipv4Addr::from(u128::from(ip) as u32);
    let segments = ip.segments();
    if in_v6(ip, Ipv6Addr::UNSPECIFIED, 96)
        || in_v6(ip, Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0, 0), 96)
        || in_v6(ip, Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96)
    {
        Some(last)
    } else if segments[0] == 0x2002 {
        Some(Ipv4Addr::from(
            u32::from(segments[1]) << 16 | u32::from(segments[2]),
        ))
    } else {
        None
    }
}

fn in_v4(ip: Ipv4Addr, network: Ipv4Addr, len: u32) -> bool {
    let mask = u32::MAX.checked_shl(32 - len).unwrap_or(0);
    u32::from(ip) & mask == u32::from(network) & mask
}

fn in_v6(ip: Ipv6Addr, network: Ipv6Addr, len: u32) -> bool {
    let mask = u128::MAX.checked_shl(128 - len).unwrap_or(0);
    u128::from(ip) & mask == u128::from(network) & mask
}

/// Why a request didn't happen or didn't finish.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    /// The host isn't one of the manifest's domains, or is an address.
    #[error("{0} isn't in the plugin's domains")]
    NotInDomains(String),
    #[error("{0}")]
    Network(String),
}

enum DomainRule {
    /// `api.example.com`: that name.
    Exact(String),
    /// `*.example.com`, kept as `.example.com`: any name ending in it.
    Below(String),
}

/// The names a plugin may reach: its manifest's `domains`.
pub struct NetGuard {
    rules: Vec<DomainRule>,
}

impl NetGuard {
    pub fn new(domains: &[String]) -> NetGuard {
        let rules = domains
            .iter()
            .map(|domain| match domain.strip_prefix("*.") {
                Some(parent) => DomainRule::Below(format!(".{}", parent.to_ascii_lowercase())),
                None => DomainRule::Exact(domain.to_ascii_lowercase()),
            })
            .collect();
        NetGuard { rules }
    }

    /// `host` exactly, or a name below a `*.x` domain (never `x` itself).
    pub fn allows_host(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        self.rules.iter().any(|rule| match rule {
            DomainRule::Exact(name) => host == *name,
            DomainRule::Below(suffix) => host.len() > suffix.len() && host.ends_with(suffix),
        })
    }

    /// Whether `url` may be asked for: http or https, to a name (never an
    /// address) the domains allow.
    pub fn check(&self, url: &Url) -> Result<(), NetError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(NetError::Network(format!(
                "only http and https addresses can be reached, not {}",
                url.scheme()
            )));
        }
        // `domain` is `None` for an address, however the URL wrote it.
        match url.domain() {
            Some(host) if self.allows_host(host) => Ok(()),
            _ => Err(NetError::NotInDomains(
                url.host_str().unwrap_or_default().to_owned(),
            )),
        }
    }
}

/// Looks names up; the system's resolver, or a test's.
pub trait Resolve: Send + Sync {
    fn resolve(&self, host: &str) -> BoxFuture<'_, io::Result<Vec<IpAddr>>>;
}

/// The system's resolver.
pub struct SystemResolver;

impl Resolve for SystemResolver {
    fn resolve(&self, host: &str) -> BoxFuture<'_, io::Result<Vec<IpAddr>>> {
        let host = host.to_owned();
        Box::pin(async move {
            let found = tokio::net::lookup_host((host.as_str(), 0)).await?;
            Ok(found.map(|address| address.ip()).collect())
        })
    }
}

/// Which addresses connections may go to. Only `#[cfg(test)]` code can
/// build [`AddressPolicy::LoopbackForTests`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressPolicy {
    Public,
    /// Public addresses and this machine's, for a test's local site.
    #[cfg(test)]
    LoopbackForTests,
}

impl AddressPolicy {
    fn allows(self, ip: IpAddr) -> bool {
        match self {
            AddressPolicy::Public => !is_forbidden(ip),
            #[cfg(test)]
            AddressPolicy::LoopbackForTests => ip.is_loopback() || !is_forbidden(ip),
        }
    }
}

/// reqwest's resolver: the guard's names, through `resolver`, keeping only
/// the addresses `policy` allows.
struct Guarded {
    guard: Arc<NetGuard>,
    resolver: Arc<dyn Resolve>,
    policy: AddressPolicy,
}

impl reqwest::dns::Resolve for Guarded {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let (guard, resolver, policy) = (self.guard.clone(), self.resolver.clone(), self.policy);
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found = reachable(&guard, &*resolver, policy, &host).await?;
            let addresses: reqwest::dns::Addrs =
                Box::new(found.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addresses)
        })
    }
}

/// The addresses of `host` a connection may go to.
async fn reachable(
    guard: &NetGuard,
    resolver: &dyn Resolve,
    policy: AddressPolicy,
    host: &str,
) -> Result<Vec<IpAddr>, NetError> {
    // Checked before every request and redirect too; here the client
    // holds to it whoever calls it.
    if !guard.allows_host(host) {
        return Err(NetError::NotInDomains(host.to_owned()));
    }
    let found = resolver
        .resolve(host)
        .await
        .map_err(|error| NetError::Network(format!("could not look up {host}: {error}")))?;
    if found.is_empty() {
        return Err(NetError::Network(format!("{host} has no addresses")));
    }
    let allowed: Vec<IpAddr> = found.into_iter().filter(|&ip| policy.allows(ip)).collect();
    if allowed.is_empty() {
        return Err(NetError::Network(format!(
            "{host} resolves only to addresses plugins may not reach"
        )));
    }
    Ok(allowed)
}

/// The client for one plugin's requests: names through `resolver`, public
/// addresses only, redirects within the domains, no proxy.
pub fn client(
    guard: Arc<NetGuard>,
    resolver: Arc<dyn Resolve>,
) -> Result<reqwest::Client, NetError> {
    build_client(guard, resolver, AddressPolicy::Public)
}

fn build_client(
    guard: Arc<NetGuard>,
    resolver: Arc<dyn Resolve>,
    policy: AddressPolicy,
) -> Result<reqwest::Client, NetError> {
    let tls = crate::tls::client_config(None)
        .map_err(|error| NetError::Network(format!("could not set up TLS: {error}")))?;
    let redirects = guard.clone();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .user_agent(concat!("Opencord/", env!("CARGO_PKG_VERSION")))
        // A proxy would look the names up itself, past the guard.
        .no_proxy()
        .dns_resolver(Guarded {
            guard,
            resolver,
            policy,
        })
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > HTTP_MAX_REDIRECTS {
                let error = NetError::Network(format!("more than {HTTP_MAX_REDIRECTS} redirects"));
                return attempt.error(error);
            }
            match redirects.check(attempt.url()) {
                Ok(()) => attempt.follow(),
                Err(error) => attempt.error(error),
            }
        }))
        // The next site would learn the address, query string and all.
        .referer(false)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| NetError::Network(format!("could not set up the client: {error}")))
}

/// One VM's way out: its domains, its client and its request slots.
pub struct PluginNet {
    guard: Arc<NetGuard>,
    client: reqwest::Client,
    slots: Semaphore,
}

impl PluginNet {
    /// Through the system's resolver, to public addresses only.
    pub fn new(guard: Arc<NetGuard>) -> Result<PluginNet, NetError> {
        let client = client(guard.clone(), Arc::new(SystemResolver))?;
        Ok(PluginNet::with_client(guard, client))
    }

    fn with_client(guard: Arc<NetGuard>, client: reqwest::Client) -> PluginNet {
        PluginNet {
            guard,
            client,
            slots: Semaphore::new(HTTP_CONCURRENT),
        }
    }
}

/// What a plugin asks for, already checked by its Lua call.
pub struct HttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    /// At most [`HTTP_TIMEOUT`].
    pub timeout: Duration,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

/// Headers the client sets itself, or that would reach a proxy.
pub fn may_set_header(name: &HeaderName) -> bool {
    let name = name.as_str();
    !matches!(
        name,
        "host" | "connection" | "content-length" | "transfer-encoding"
    ) && !name.starts_with("proxy-")
}

/// Sends a plugin's request and logs it as an action, without its query
/// string or bodies.
pub async fn request(env: &PluginEnv, request: HttpRequest) -> Result<HttpResponse, NetError> {
    let line = format!("{} {}", request.method, shown(&request.url));
    let result = send(&env.net, request).await;
    let outcome = match &result {
        Ok(response) => response.status.to_string(),
        Err(error) => format!("failed: {error}"),
    };
    env.log.push(
        proto::PluginLogLevel::Action,
        &format!("{line} → {outcome}"),
    );
    result
}

async fn send(net: &PluginNet, request: HttpRequest) -> Result<HttpResponse, NetError> {
    net.guard.check(&request.url)?;
    let _slot = net
        .slots
        .acquire()
        .await
        .map_err(|_| NetError::Network("the plugin is stopping".to_owned()))?;
    let timeout = request.timeout.min(HTTP_TIMEOUT);
    let mut sending = net
        .client
        .request(request.method, request.url)
        .headers(request.headers);
    if let Some(body) = request.body {
        sending = sending.body(body);
    }
    let exchange = async {
        let mut response = sending.send().await.map_err(failure)?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > HTTP_MAX_RESPONSE_BYTES)
        {
            return Err(too_large());
        }
        let headers = std::mem::take(response.headers_mut());
        let mut body = Vec::new();
        while let Some(piece) = response.chunk().await.map_err(failure)? {
            if (body.len() + piece.len()) as u64 > HTTP_MAX_RESPONSE_BYTES {
                return Err(too_large());
            }
            body.extend_from_slice(&piece);
        }
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    };
    tokio::time::timeout(timeout, exchange)
        .await
        .unwrap_or_else(|_| Err(timed_out(timeout)))
}

fn too_large() -> NetError {
    NetError::Network(format!(
        "the response is larger than {} MB",
        HTTP_MAX_RESPONSE_BYTES / MB
    ))
}

fn timed_out(after: Duration) -> NetError {
    NetError::Network(format!("no answer within {} s", after.as_secs_f64()))
}

/// What went wrong, from the error's causes: the guard's own refusal as it
/// was, else the innermost cause. reqwest's own text names the URL, query
/// string and all.
fn failure(error: reqwest::Error) -> NetError {
    let mut reason = None;
    let mut cause = error.source();
    while let Some(current) = cause {
        if let Some(refused) = current.downcast_ref::<NetError>() {
            return refused.clone();
        }
        reason = Some(current.to_string());
        cause = current.source();
    }
    if error.is_timeout() {
        return timed_out(HTTP_TIMEOUT);
    }
    let connecting = error.is_connect();
    let reason = reason.unwrap_or_else(|| error.without_url().to_string());
    if connecting {
        NetError::Network(format!("could not connect: {reason}"))
    } else {
        NetError::Network(reason)
    }
}

/// `host[:port]/path`, as the plugin's log shows a request.
fn shown(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}{}", url.path()),
        None => format!("{host}{}", url.path()),
    }
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
