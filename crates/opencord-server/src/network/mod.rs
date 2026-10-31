//! Domains & network (Phase 3 plan §5, §8, §9): the server's public
//! address, its ports and how they are reached. The owner sees and changes
//! it on its own settings page; the work runs here, in the background, and
//! the page hears of it through `NetworkStatus` while it is open.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::{PortProtocol, PortPurpose, PortState};
use tokio::sync::{Notify, watch};

pub mod main_port;
pub mod ports;
pub mod proxy;
pub mod public_ip;
pub mod settings;
pub mod upnp;

pub use proxy::ClientAddr;
pub use public_ip::Lookup;
use settings::NetworkSettings;
use upnp::{Protocol, Upnp};

use crate::state::{AppState, Audience, now_ms};

/// How often the public address is looked up (plan §5.1).
const DETECT_EVERY: Duration = Duration::from_secs(6 * 3600);
/// Let's Encrypt's HTTP challenge port.
const ACME_HTTP_PORT: u16 = 80;

/// The public addresses in use: overrides, else what was detected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublicIps {
    pub v4: Option<Ipv4Addr>,
    pub v6: Option<Ipv6Addr>,
}

/// The ports this server listens on now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Listening {
    pub voice: Option<u16>,
    pub music: Option<u16>,
}

#[derive(Debug)]
pub struct Network {
    settings: Mutex<NetworkSettings>,
    lookup: Lookup,
    http: reqwest::Client,
    has_ipv6: AtomicBool,
    detect_error: Mutex<Option<String>>,
    public: watch::Sender<PublicIps>,
    upnp: Upnp,
    /// Wakes the background task: settings changed, or a lookup is due.
    wake: Notify,
    /// Sessions with the page open.
    watchers: Mutex<HashSet<String>>,
    main_port: AtomicU16,
    listening: Mutex<Listening>,
    /// Where domain checks ask name servers: 53, but for tests.
    name_server_port: u16,
}

impl Network {
    pub fn new(
        settings: NetworkSettings,
        lookup: Lookup,
        http: reqwest::Client,
        name_server_port: u16,
    ) -> Self {
        let public = watch::channel(effective(&settings)).0;
        Self {
            settings: Mutex::new(settings),
            lookup,
            http,
            has_ipv6: AtomicBool::new(public_ip::has_ipv6()),
            detect_error: Mutex::new(None),
            public,
            upnp: Upnp::default(),
            wake: Notify::new(),
            watchers: Mutex::new(HashSet::new()),
            main_port: AtomicU16::new(0),
            listening: Mutex::new(Listening::default()),
            name_server_port,
        }
    }

    /// The settings saved before the server started, without announcing
    /// anything.
    pub fn restore(&self, settings: NetworkSettings) {
        self.public.send_replace(effective(&settings));
        *lock(&self.settings) = settings;
    }

    pub fn settings(&self) -> NetworkSettings {
        lock(&self.settings).clone()
    }

    /// The main port in use now.
    pub fn main_port(&self) -> u16 {
        self.main_port.load(Ordering::Relaxed)
    }

    /// The server's own HTTPS client.
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// A DNS checker with the owner's resolvers (plan §6.2).
    pub fn dns_checker(&self) -> crate::domains::dns::DnsChecker {
        let mut checker = crate::domains::dns::DnsChecker::new(self.settings().dns_resolvers);
        checker.name_server_port = self.name_server_port;
        checker
    }

    /// The public addresses in use, as they change.
    pub fn public_ips(&self) -> watch::Receiver<PublicIps> {
        self.public.subscribe()
    }

    /// The public addresses in use now.
    pub fn current_public_ips(&self) -> PublicIps {
        *self.public.borrow()
    }

    /// The router's mapping of a port, with UPnP.
    pub fn mapping(&self, protocol: PortProtocol, port: u16) -> proto::MappingState {
        self.upnp.status().mapping(protocol, port).0
    }

    /// What this server listens on, once it does.
    pub fn set_listening(&self, main_port: u16, listening: Listening) {
        self.main_port.store(main_port, Ordering::Relaxed);
        *lock(&self.listening) = listening;
    }

    /// Looks the public address up now and then, and keeps the router's
    /// mappings while UPnP is on, until the server stops.
    pub fn start(state: &Arc<AppState>) {
        let state = Arc::clone(state);
        tokio::spawn(async move {
            let mut next_detect = Instant::now();
            let mut next_upnp = Instant::now();
            let mut mapped = false;
            loop {
                let now = Instant::now();
                let settings = state.network.settings();
                if settings.auto_detect && now >= next_detect {
                    detect(&state).await;
                    next_detect = now + DETECT_EVERY;
                }
                if settings.upnp && (now >= next_upnp || !mapped) {
                    let ports = state.network.mapped_ports();
                    state.network.upnp.map(&ports).await;
                    mapped = true;
                    next_upnp = now + upnp::RENEW_EVERY;
                    state.network.changed(&state);
                } else if !settings.upnp && mapped {
                    state.network.upnp.unmap().await;
                    mapped = false;
                    state.network.changed(&state);
                }
                // Only timers in use wake the task.
                let mut wake = Instant::now() + DETECT_EVERY;
                if settings.auto_detect {
                    wake = wake.min(next_detect);
                }
                if mapped {
                    wake = wake.min(next_upnp);
                }
                tokio::select! {
                    () = tokio::time::sleep_until(wake.into()) => {}
                    () = state.network.wake.notified() => {}
                    () = state.shutdown.cancelled() => {
                        if mapped {
                            let _ = tokio::time::timeout(
                                Duration::from_secs(3),
                                state.network.upnp.unmap(),
                            )
                            .await;
                        }
                        return;
                    }
                }
            }
        });
    }

    /// Settings were saved: they take effect now.
    pub fn replace_settings(&self, state: &Arc<AppState>, settings: NetworkSettings) {
        *lock(&self.settings) = settings;
        self.publish(state);
        self.wake.notify_one();
        self.changed(state);
    }

    /// Looks the public address up now; what changed is announced.
    pub async fn detect_now(&self, state: &Arc<AppState>) {
        detect(state).await;
    }

    pub fn watch(&self, session_id: &str, watching: bool) {
        let mut watchers = lock(&self.watchers);
        if watching {
            watchers.insert(session_id.to_owned());
        } else {
            watchers.remove(session_id);
        }
    }

    /// Tells sessions with the page open.
    pub fn changed(&self, state: &AppState) {
        let watchers: Vec<String> = lock(&self.watchers).iter().cloned().collect();
        if watchers.is_empty() {
            return;
        }
        let event = proto::Event {
            kind: Some(Event::NetworkStatus(Box::new(proto::NetworkStatus {
                info: Some(self.info(state)),
            }))),
        };
        let now = std::time::Instant::now();
        let mut gone = Vec::new();
        for id in watchers {
            match state.sessions.get(&id) {
                Some(session) if !session.is_ended() => session.push_event(&event, now),
                _ => gone.push(id),
            }
        }
        let mut watchers = lock(&self.watchers);
        for id in gone {
            watchers.remove(&id);
        }
    }

    pub fn info(&self, state: &AppState) -> proto::NetworkInfo {
        let settings = self.settings();
        let used = effective(&settings);
        let status = self.upnp.status();
        let main_port = self.main_port.load(Ordering::Relaxed);
        let listening = *lock(&self.listening);
        let port = |purpose: PortPurpose, protocol: PortProtocol, port: u16, state: PortState| {
            let (mapping, mapping_error) = status.mapping(protocol, port);
            proto::PortStatus {
                purpose: purpose as i32,
                protocol: protocol as i32,
                port: u32::from(port),
                state: state as i32,
                mapping: mapping as i32,
                mapping_error,
            }
        };
        let listening_or_off = |port: Option<u16>| match port {
            Some(port) => (port, PortState::Listening),
            None => (0, PortState::Off),
        };
        let (voice_port, voice_state) = listening_or_off(listening.voice);
        let (music_port, music_state) = listening_or_off(listening.music);
        let ports = vec![
            port(
                PortPurpose::Main,
                PortProtocol::Tcp,
                main_port,
                PortState::Listening,
            ),
            port(
                PortPurpose::Voice,
                PortProtocol::Udp,
                voice_port,
                voice_state,
            ),
            port(
                PortPurpose::Music,
                PortProtocol::Udp,
                music_port,
                music_state,
            ),
            port(
                PortPurpose::AcmeHttp,
                PortProtocol::Tcp,
                ACME_HTTP_PORT,
                ports::probe(PortProtocol::Tcp, ACME_HTTP_PORT),
            ),
        ];
        let next_main_port = match settings.main_port {
            Some(next) if next != main_port => ports::probe(PortProtocol::Tcp, next),
            _ => PortState::Unspecified,
        };
        let ip = |ip: Option<IpAddr>| ip.map(|ip| ip.to_string()).unwrap_or_default();
        proto::NetworkInfo {
            public_address: Some(proto::PublicAddress {
                ipv4: ip(used.v4.map(IpAddr::V4)),
                ipv6: ip(used.v6.map(IpAddr::V6)),
                detected_ipv4: ip(settings.detected_v4),
                detected_ipv6: ip(settings.detected_v6),
                override_ipv4: ip(settings.override_v4),
                override_ipv6: ip(settings.override_v6),
                auto_detect: settings.auto_detect,
                detected_at_ms: settings.detected_at_ms,
                detect_error: lock(&self.detect_error).clone().unwrap_or_default(),
                has_ipv6: self.has_ipv6.load(Ordering::Relaxed),
            }),
            ports,
            settings: Some(settings.to_proto()),
            listening_port: u32::from(main_port),
            next_main_port: next_main_port as i32,
            upnp_gateway: status
                .gateway
                .map(|gateway| gateway.ip().to_string())
                .unwrap_or_default(),
            upnp_external_ip: ip(status.external_ip),
            upnp_error: status.error.unwrap_or_default(),
            domains: state.domains.to_proto(state),
            dns_providers: state.certificates.providers(),
            acme: Some(state.certificates.account_info(state)),
            tunnel: Some(state.tunnel.status()),
        }
    }

    /// The ports the router should forward here.
    fn mapped_ports(&self) -> Vec<(Protocol, u16)> {
        let listening = *lock(&self.listening);
        let mut ports = vec![(Protocol::Tcp, self.main_port.load(Ordering::Relaxed))];
        ports.extend(listening.voice.map(|port| (Protocol::Udp, port)));
        ports.extend(listening.music.map(|port| (Protocol::Udp, port)));
        ports.retain(|(_, port)| *port != 0);
        ports
    }

    /// Hands on the addresses in use, and tells the owner when they moved.
    fn publish(&self, state: &AppState) {
        let next = effective(&self.settings());
        let previous = *self.public.borrow();
        if next == previous {
            return;
        }
        self.public.send_replace(next);
        if previous == PublicIps::default() {
            return;
        }
        let describe = |ips: PublicIps| {
            [
                ips.v4.map(|ip| ip.to_string()),
                ips.v6.map(|ip| ip.to_string()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" and ")
        };
        let owner = state.guild().meta.owner_id;
        if let Some(owner) = owner {
            state.broadcast(
                Event::ServerNotice(proto::ServerNotice {
                    key: "network.public_address".to_owned(),
                    severity: proto::NoticeSeverity::Info as i32,
                    title: "Public address changed".to_owned(),
                    body: format!(
                        "The server is now reached at {} (it was {}).",
                        describe(next),
                        describe(previous)
                    ),
                }),
                Audience::User(owner),
            );
        }
    }
}

/// Looks the public address up, keeps what was found, and announces it.
async fn detect(state: &Arc<AppState>) {
    let network = &state.network;
    network
        .has_ipv6
        .store(public_ip::has_ipv6(), Ordering::Relaxed);
    let result = public_ip::detect(
        &network.http,
        &network.lookup,
        network.has_ipv6.load(Ordering::Relaxed),
    )
    .await;
    match result {
        Ok(found) => {
            *lock(&network.detect_error) = None;
            let saved = {
                let mut settings = lock(&network.settings);
                settings.detected_v4 = found.v4.map(IpAddr::V4);
                settings.detected_v6 = found.v6.map(IpAddr::V6);
                settings.detected_at_ms = now_ms();
                settings.clone()
            };
            if let Ok(mut conn) = state.db.acquire().await
                && let Err(error) = saved.save(&mut conn).await
            {
                tracing::warn!(%error, "could not save the public address");
            }
            network.publish(state);
        }
        Err(error) => {
            tracing::info!(%error, "could not look up the public address");
            *lock(&network.detect_error) = Some(error);
        }
    }
    network.changed(state);
}

/// The addresses in use: the overrides, else the last detected.
fn effective(settings: &NetworkSettings) -> PublicIps {
    let v4 = settings
        .override_v4
        .or(settings.detected_v4)
        .and_then(|ip| match ip {
            IpAddr::V4(v4) => Some(v4),
            IpAddr::V6(_) => None,
        });
    let v6 = settings
        .override_v6
        .or(settings.detected_v6)
        .and_then(|ip| match ip {
            IpAddr::V6(v6) => Some(v6),
            IpAddr::V4(_) => None,
        });
    PublicIps { v4, v6 }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
