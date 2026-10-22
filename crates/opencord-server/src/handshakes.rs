//! TLS handshakes the other side gave up on because it did not trust the
//! certificate. A Lavalink node that does not trust the server's
//! certificate fails exactly so, and the connection test (Phase 3 plan
//! §12.9) says so instead of "nothing connected".

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum_server::accept::Accept;
use axum_server::tls_rustls::RustlsAcceptor;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

/// Refusals kept, and for how long.
const KEPT: usize = 64;
const KEPT_FOR: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub ip: IpAddr,
    pub at: Instant,
    /// The alert it sent, as rustls names it.
    pub alert: String,
}

/// The recent refusals.
#[derive(Debug, Default)]
pub struct Refusals {
    recent: Mutex<VecDeque<Refusal>>,
}

impl Refusals {
    pub fn record(&self, ip: IpAddr, alert: String) {
        let now = Instant::now();
        let mut recent = self.recent.lock().unwrap_or_else(PoisonError::into_inner);
        recent.retain(|refusal| now.duration_since(refusal.at) < KEPT_FOR);
        if recent.len() == KEPT {
            recent.pop_front();
        }
        recent.push_back(Refusal {
            ip: ip.to_canonical(),
            at: now,
            alert,
        });
    }

    /// Refusals since `since`, oldest first.
    pub fn since(&self, since: Instant) -> Vec<Refusal> {
        self.recent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|refusal| refusal.at >= since)
            .cloned()
            .collect()
    }
}

/// The alert of a handshake the peer refused over the certificate.
fn certificate_alert(error: &io::Error) -> Option<String> {
    use rustls::AlertDescription as Alert;
    let rustls::Error::AlertReceived(alert) = error.get_ref()?.downcast_ref::<rustls::Error>()?
    else {
        return None;
    };
    matches!(
        alert,
        Alert::BadCertificate
            | Alert::UnsupportedCertificate
            | Alert::CertificateRevoked
            | Alert::CertificateExpired
            | Alert::CertificateUnknown
            | Alert::UnknownCA
            | Alert::AccessDenied
    )
    .then(|| format!("{alert:?}"))
}

/// The main listener's TLS, noting refusals.
#[derive(Debug, Clone)]
pub struct WatchedAcceptor {
    inner: RustlsAcceptor,
    refusals: Arc<Refusals>,
}

impl WatchedAcceptor {
    pub fn new(inner: RustlsAcceptor, refusals: Arc<Refusals>) -> Self {
        Self { inner, refusals }
    }
}

type Accepted<S> = io::Result<(TlsStream<TcpStream>, S)>;

impl<S: Send + 'static> Accept<TcpStream, S> for WatchedAcceptor {
    type Stream = TlsStream<TcpStream>;
    type Service = S;
    type Future = Pin<Box<dyn Future<Output = Accepted<S>> + Send>>;

    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let peer = stream.peer_addr().ok().map(|peer| peer.ip());
        let accepting = self.inner.accept(stream, service);
        let refusals = Arc::clone(&self.refusals);
        Box::pin(async move {
            let result = accepting.await;
            if let (Err(error), Some(ip)) = (&result, peer)
                && let Some(alert) = certificate_alert(error)
            {
                tracing::debug!(%ip, alert, "a TLS client refused the certificate");
                refusals.record(ip, alert);
            }
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_certificate_alerts_count() {
        let refused = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::AlertReceived(rustls::AlertDescription::CertificateUnknown),
        );
        assert_eq!(
            certificate_alert(&refused).as_deref(),
            Some("CertificateUnknown")
        );
        let other = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::AlertReceived(rustls::AlertDescription::HandshakeFailure),
        );
        assert_eq!(certificate_alert(&other), None);
        assert_eq!(certificate_alert(&io::Error::other("reset")), None);
    }

    #[test]
    fn refusals_are_kept_a_while() {
        let refusals = Refusals::default();
        let before = Instant::now();
        refusals.record("::ffff:10.0.0.2".parse().unwrap(), "UnknownCA".to_owned());
        let seen = refusals.since(before);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].ip, "10.0.0.2".parse::<IpAddr>().unwrap());
        assert!(
            refusals
                .since(Instant::now() + Duration::from_secs(1))
                .is_empty()
        );
    }
}
