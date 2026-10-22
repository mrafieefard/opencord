//! Finding the server's public address (Phase 3 plan §5.1): ask a lookup
//! service (ipify by default) what address a request came from.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

/// Each lookup gets this long.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);
/// A global IPv6 address to route towards, to see whether this machine has
/// IPv6 at all (Google's resolver; nothing is sent).
const SOME_GLOBAL_V6: Ipv6Addr = Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888);

/// What a lookup found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Found {
    pub v4: Option<Ipv4Addr>,
    pub v6: Option<Ipv6Addr>,
}

/// Where to ask: plain-text services answering with the caller's address.
/// An empty one is not asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lookup {
    pub v4: String,
    pub v6: String,
}

/// Asks the lookup services; IPv6 only when this machine has it. Fails
/// only when nothing was found.
pub async fn detect(
    http: &reqwest::Client,
    lookup: &Lookup,
    has_ipv6: bool,
) -> Result<Found, String> {
    if lookup.v4.is_empty() && lookup.v6.is_empty() {
        return Err("no address lookup is configured".to_owned());
    }
    let v4 = ask(http, &lookup.v4).await;
    let v6 = if has_ipv6 {
        ask(http, &lookup.v6).await
    } else {
        Err(String::new())
    };
    let found = Found {
        v4: v4.as_ref().ok().and_then(|ip| match ip {
            IpAddr::V4(v4) => Some(*v4),
            IpAddr::V6(_) => None,
        }),
        v6: v6.as_ref().ok().and_then(|ip| match ip {
            IpAddr::V6(v6) => Some(*v6),
            IpAddr::V4(_) => None,
        }),
    };
    match (found, v4) {
        (Found { v4: None, v6: None }, Err(error)) => Err(error),
        (Found { v4: None, v6: None }, Ok(ip)) => Err(format!("the lookup answered {ip}")),
        (found, _) => Ok(found),
    }
}

async fn ask(http: &reqwest::Client, url: &str) -> Result<IpAddr, String> {
    if url.is_empty() {
        return Err("not configured".to_owned());
    }
    let response = http
        .get(url)
        .timeout(LOOKUP_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("{url} could not be reached: {}", plain(&error)))?;
    if !response.status().is_success() {
        return Err(format!("{url} answered {}", response.status()));
    }
    let text = response
        .text()
        .await
        .map_err(|error| format!("{url} stopped answering: {}", plain(&error)))?;
    text.trim()
        .parse::<IpAddr>()
        .map(|ip| ip.to_canonical())
        .map_err(|_| format!("{url} did not answer with an address"))
}

/// An HTTP error without reqwest's wrapping, for people to read.
fn plain(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "no answer in time".to_owned();
    }
    let mut source: &dyn std::error::Error = error;
    while let Some(inner) = source.source() {
        source = inner;
    }
    source.to_string()
}

/// Whether this machine has a global IPv6 address to reach the internet
/// with.
pub fn has_ipv6() -> bool {
    let Ok(socket) = std::net::UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)) else {
        return false;
    };
    if socket.connect((SOME_GLOBAL_V6, 53)).is_err() {
        return false;
    }
    match socket.local_addr().map(|address| address.ip()) {
        Ok(IpAddr::V6(local)) => is_global_v6(local),
        _ => false,
    }
}

fn is_global_v6(ip: Ipv6Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.to_ipv4_mapped().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::get;

    async fn answering(body: &'static str) -> String {
        let app = Router::new().route("/", get(move || async move { body }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        url
    }

    #[tokio::test]
    async fn the_lookup_answers_with_the_address() {
        let http = crate::tls::http_client().unwrap();
        let lookup = Lookup {
            v4: answering("203.0.113.10\n").await,
            v6: answering("2001:db8::10").await,
        };

        let found = detect(&http, &lookup, true).await.unwrap();

        assert_eq!(found.v4, Some(Ipv4Addr::new(203, 0, 113, 10)));
        assert_eq!(found.v6, Some("2001:db8::10".parse().unwrap()));
        let without_v6 = detect(&http, &lookup, false).await.unwrap();
        assert_eq!(without_v6.v6, None);
    }

    #[tokio::test]
    async fn nonsense_or_silence_is_an_error() {
        let http = crate::tls::http_client().unwrap();
        let nonsense = Lookup {
            v4: answering("<html>").await,
            v6: String::new(),
        };
        assert!(detect(&http, &nonsense, false).await.is_err());
        let nobody = Lookup {
            v4: "http://127.0.0.1:9/".to_owned(),
            v6: String::new(),
        };
        let error = detect(&http, &nobody, false).await.unwrap_err();
        assert!(error.contains("could not be reached"), "{error}");
        let none = Lookup {
            v4: String::new(),
            v6: String::new(),
        };
        assert!(detect(&http, &none, true).await.is_err());
    }

    /// Asks the real ipify, through the server's own HTTPS client.
    #[tokio::test]
    #[ignore = "reaches api.ipify.org"]
    async fn ipify_answers() {
        let http = crate::tls::http_client().unwrap();
        let lookup = Lookup {
            v4: "https://api.ipify.org".to_owned(),
            v6: "https://api6.ipify.org".to_owned(),
        };
        let found = detect(&http, &lookup, has_ipv6()).await.unwrap();
        println!("{found:?}");
        assert!(found.v4.is_some());
    }

    #[test]
    fn only_global_v6_counts() {
        assert!(is_global_v6("2001:db8::1".parse().unwrap()));
        assert!(!is_global_v6("fd00::1".parse().unwrap()));
        assert!(!is_global_v6("fe80::1".parse().unwrap()));
        assert!(!is_global_v6("::ffff:1.2.3.4".parse().unwrap()));
    }
}
