//! How well a Lavalink node is doing, and which one gets a channel (Phase 3
//! plan §12.5–12.6). Pure functions over what the node reported, so the
//! rules are tested on their own.

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;

/// A closed WebSocket counts once it is not back within this long.
pub const RECONNECT_GRACE: Duration = Duration::from_secs(3);
/// A node back from being unavailable gets new channels after this long.
pub const RECOVERY_HOLD: Duration = Duration::from_secs(30);
/// Track-loading failures are counted over this window.
pub const FAILURE_WINDOW: Duration = Duration::from_secs(5 * 60);
/// This many failures in the window count as repeated.
const REPEATED_FAILURES: usize = 3;
const CPU_DEGRADED: f64 = 0.85;
const FRAMES_DEGRADED: f64 = 0.02;
const PING_DEGRADED_MS: u32 = 1000;
/// Frames a playing player sends per minute: 50 a second.
const FRAMES_PER_MINUTE: f64 = 3000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Preferred,
    Normal,
    Backup,
}

impl Priority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preferred => "preferred",
            Self::Normal => "normal",
            Self::Backup => "backup",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "preferred" => Self::Preferred,
            "backup" => Self::Backup,
            _ => Self::Normal,
        }
    }

    pub fn from_proto(priority: i32) -> Self {
        match proto::LavalinkPriority::try_from(priority) {
            Ok(proto::LavalinkPriority::Preferred) => Self::Preferred,
            Ok(proto::LavalinkPriority::Backup) => Self::Backup,
            _ => Self::Normal,
        }
    }

    pub fn to_proto(self) -> proto::LavalinkPriority {
        match self {
            Self::Preferred => proto::LavalinkPriority::Preferred,
            Self::Normal => proto::LavalinkPriority::Normal,
            Self::Backup => proto::LavalinkPriority::Backup,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Connecting,
    Healthy,
    Degraded,
    Unavailable,
    Draining,
    Disabled,
}

impl Health {
    pub fn to_proto(self) -> proto::LavalinkHealth {
        match self {
            Self::Connecting => proto::LavalinkHealth::Connecting,
            Self::Healthy => proto::LavalinkHealth::Healthy,
            Self::Degraded => proto::LavalinkHealth::Degraded,
            Self::Unavailable => proto::LavalinkHealth::Unavailable,
            Self::Draining => proto::LavalinkHealth::Draining,
            Self::Disabled => proto::LavalinkHealth::Disabled,
        }
    }

    /// Plays music now: channels on it stay.
    pub fn is_up(self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded | Self::Draining)
    }
}

/// The `frameStats` of the `stats` op: the last minute's frames.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameStats {
    pub sent: i64,
    pub nulled: i64,
    pub deficit: i64,
}

/// What the node last said about its load.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Load {
    pub players: u32,
    pub playing_players: u32,
    pub system_load: f64,
    pub frames: Option<FrameStats>,
}

/// What health is judged from.
#[derive(Debug, Clone, Default)]
pub struct Observed {
    pub enabled: bool,
    pub draining: bool,
    pub connected: bool,
    /// When the WebSocket closed, while it is closed.
    pub disconnected_at: Option<Instant>,
    /// The first connection attempt has not ended yet.
    pub first_attempt: bool,
    pub password_rejected: bool,
    /// REST calls that failed in a row, and the last one's error.
    pub rest_failures: u32,
    pub last_error: Option<String>,
    pub load: Option<Load>,
    pub ping_ms: Option<u32>,
    pub load_failures: Vec<Instant>,
}

/// The node's state and, unless healthy, why, in plain words.
pub fn judge(observed: &Observed, before: Health, now: Instant) -> (Health, String) {
    if !observed.enabled {
        return (Health::Disabled, String::new());
    }
    if observed.password_rejected {
        return (
            Health::Unavailable,
            "Lavalink refused the password".to_owned(),
        );
    }
    if !observed.connected {
        let error = || {
            observed
                .last_error
                .clone()
                .unwrap_or_else(|| "not connected".to_owned())
        };
        return match observed.disconnected_at {
            None if observed.first_attempt => (Health::Connecting, String::new()),
            // A blip: judged once it is not back in time.
            Some(at) if now.duration_since(at) < RECONNECT_GRACE && before.is_up() => {
                (before, "reconnecting".to_owned())
            }
            _ => (Health::Unavailable, error()),
        };
    }
    if observed.rest_failures >= 2 {
        let error = observed.last_error.clone().unwrap_or_default();
        return (Health::Unavailable, format!("REST is failing: {error}"));
    }
    if observed.draining {
        return (Health::Draining, String::new());
    }
    match degraded_because(observed, now) {
        Some(reason) => (Health::Degraded, reason),
        None => (Health::Healthy, String::new()),
    }
}

fn degraded_because(observed: &Observed, now: Instant) -> Option<String> {
    if observed.rest_failures == 1 {
        return Some("REST did not answer last time".to_owned());
    }
    if let Some(load) = observed.load {
        if load.system_load > CPU_DEGRADED {
            return Some(format!("CPU load is {:.0} %", load.system_load * 100.0));
        }
        let missing = frames_missing(load);
        if missing > FRAMES_DEGRADED {
            return Some(format!(
                "{:.1} % of audio frames are missing or late",
                missing * 100.0
            ));
        }
    }
    if let Some(ping) = observed.ping_ms.filter(|ping| *ping > PING_DEGRADED_MS) {
        return Some(format!("REST takes {ping} ms"));
    }
    let failures = recent_failures(&observed.load_failures, now);
    (failures >= REPEATED_FAILURES)
        .then(|| format!("{failures} tracks failed to load in the last 5 minutes"))
}

/// The share of the last minute's frames that were missing or empty.
fn frames_missing(load: Load) -> f64 {
    let Some(frames) = load.frames else {
        return 0.0;
    };
    let expected = FRAMES_PER_MINUTE * f64::from(load.playing_players.max(1));
    (frames.deficit.max(0) + frames.nulled.max(0)) as f64 / expected
}

pub fn recent_failures(failures: &[Instant], now: Instant) -> usize {
    failures
        .iter()
        .filter(|at| now.duration_since(**at) < FAILURE_WINDOW)
        .count()
}

/// A node as the choice sees it.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub health: Health,
    pub priority: Priority,
    /// Back from unavailable less than [`RECOVERY_HOLD`] ago.
    pub recovering: bool,
    /// Opencord channels on it, and its cap (0 for none).
    pub channels: u32,
    pub max_players: u32,
    pub sources: Vec<String>,
    pub load: Load,
    pub ping_ms: u32,
    pub recent_failures: usize,
}

impl Candidate {
    fn can_take(&self, source: Option<&str>) -> bool {
        (self.max_players == 0 || self.channels < self.max_players)
            && !self.recovering
            && source.is_none_or(|source| self.sources.iter().any(|known| known == source))
    }

    /// The penalty common in Lavalink clients, plus Opencord's own terms.
    pub fn penalty(&self) -> f64 {
        let load = self.load;
        let cpu = 1.05f64.powf(100.0 * load.system_load) * 10.0 - 10.0;
        let (deficit, nulled) = load.frames.map_or((0.0, 0.0), |frames| {
            let deficit = frames.deficit.max(0) as f64;
            let nulled = frames.nulled.max(0) as f64;
            (
                1.03f64.powf(500.0 * deficit / FRAMES_PER_MINUTE) * 600.0 - 600.0,
                (1.03f64.powf(500.0 * nulled / FRAMES_PER_MINUTE) * 300.0 - 300.0) * 2.0,
            )
        });
        f64::from(load.playing_players)
            + cpu
            + deficit
            + nulled
            + 50.0 * self.recent_failures as f64
            + f64::from(self.ping_ms) / 10.0
    }
}

/// The node a channel goes to (§12.6): healthy nodes that can play
/// `source` (degraded ones only when none is), the best priority with one,
/// the lowest penalty there; ties to fewer channels, then the lower ping.
pub fn best(candidates: &[Candidate], source: Option<&str>, except: &[i64]) -> Option<i64> {
    let usable = |health: Health| {
        candidates
            .iter()
            .filter(move |node| {
                node.health == health && !except.contains(&node.id) && node.can_take(source)
            })
            .collect::<Vec<_>>()
    };
    let mut pool = usable(Health::Healthy);
    if pool.is_empty() {
        pool = usable(Health::Degraded);
    }
    let tier = pool.iter().map(|node| node.priority).min()?;
    pool.into_iter()
        .filter(|node| node.priority == tier)
        .min_by(|a, b| {
            a.penalty()
                .total_cmp(&b.penalty())
                .then(a.channels.cmp(&b.channels))
                .then(a.ping_ms.cmp(&b.ping_ms))
        })
        .map(|node| node.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Observed {
        Observed {
            enabled: true,
            connected: true,
            ..Observed::default()
        }
    }

    fn node(id: i64) -> Candidate {
        Candidate {
            id,
            health: Health::Healthy,
            priority: Priority::Normal,
            recovering: false,
            channels: 0,
            max_players: 0,
            sources: vec!["youtube".to_owned(), "soundcloud".to_owned()],
            load: Load::default(),
            ping_ms: 20,
            recent_failures: 0,
        }
    }

    #[test]
    fn a_connected_quiet_node_is_healthy() {
        let now = Instant::now();
        assert_eq!(
            judge(&connected(), Health::Connecting, now).0,
            Health::Healthy
        );
    }

    #[test]
    fn disabled_and_draining_win_over_load() {
        let now = Instant::now();
        let mut observed = connected();
        observed.draining = true;
        observed.load = Some(Load {
            system_load: 0.99,
            ..Load::default()
        });
        assert_eq!(judge(&observed, Health::Healthy, now).0, Health::Draining);
        observed.enabled = false;
        assert_eq!(judge(&observed, Health::Healthy, now).0, Health::Disabled);
    }

    #[test]
    fn a_closed_socket_counts_after_three_seconds() {
        let now = Instant::now();
        let mut observed = connected();
        observed.connected = false;
        observed.disconnected_at = Some(now - Duration::from_secs(1));
        assert_eq!(judge(&observed, Health::Healthy, now).0, Health::Healthy);
        observed.disconnected_at = Some(now - Duration::from_secs(4));
        let (health, reason) = judge(&observed, Health::Healthy, now);
        assert_eq!(health, Health::Unavailable);
        assert_eq!(reason, "not connected");
    }

    #[test]
    fn the_first_attempt_is_connecting_and_its_failure_unavailable() {
        let now = Instant::now();
        let mut observed = Observed {
            enabled: true,
            first_attempt: true,
            ..Observed::default()
        };
        assert_eq!(
            judge(&observed, Health::Connecting, now).0,
            Health::Connecting
        );
        observed.first_attempt = false;
        observed.last_error = Some("nothing answers".to_owned());
        assert_eq!(
            judge(&observed, Health::Connecting, now),
            (Health::Unavailable, "nothing answers".to_owned())
        );
        observed.password_rejected = true;
        assert_eq!(
            judge(&observed, Health::Connecting, now).1,
            "Lavalink refused the password"
        );
    }

    #[test]
    fn load_frames_ping_and_failures_degrade() {
        let now = Instant::now();
        let mut observed = connected();
        observed.load = Some(Load {
            playing_players: 2,
            system_load: 0.9,
            ..Load::default()
        });
        assert_eq!(judge(&observed, Health::Healthy, now).1, "CPU load is 90 %");
        observed.load = Some(Load {
            playing_players: 2,
            system_load: 0.1,
            frames: Some(FrameStats {
                sent: 5800,
                nulled: 0,
                deficit: 150,
            }),
            ..Load::default()
        });
        assert_eq!(
            judge(&observed, Health::Healthy, now).1,
            "2.5 % of audio frames are missing or late"
        );
        observed.load = None;
        observed.ping_ms = Some(1500);
        assert_eq!(
            judge(&observed, Health::Healthy, now).1,
            "REST takes 1500 ms"
        );
        observed.ping_ms = Some(30);
        observed.load_failures = vec![now; 3];
        assert_eq!(judge(&observed, Health::Healthy, now).0, Health::Degraded);
        observed.load_failures = vec![now - Duration::from_secs(400); 3];
        assert_eq!(judge(&observed, Health::Healthy, now).0, Health::Healthy);
        observed.rest_failures = 2;
        observed.last_error = Some("timed out".to_owned());
        assert_eq!(
            judge(&observed, Health::Healthy, now),
            (Health::Unavailable, "REST is failing: timed out".to_owned())
        );
    }

    #[test]
    fn the_least_loaded_node_gets_the_channel() {
        let mut busy = node(1);
        busy.load.playing_players = 5;
        let mut hot = node(2);
        hot.load.system_load = 0.6;
        let quiet = node(3);
        assert_eq!(best(&[busy, hot, quiet], None, &[]), Some(3));
    }

    #[test]
    fn missing_frames_cost_more_than_players() {
        let mut lossy = node(1);
        lossy.load.frames = Some(FrameStats {
            sent: 3000,
            nulled: 30,
            deficit: 30,
        });
        let mut busy = node(2);
        busy.load.playing_players = 8;
        assert!(lossy.penalty() > busy.penalty());
        assert_eq!(best(&[lossy, busy], None, &[]), Some(2));
    }

    #[test]
    fn priority_tiers_come_before_penalty() {
        let mut preferred = node(1);
        preferred.priority = Priority::Preferred;
        preferred.load.playing_players = 30;
        let normal = node(2);
        let mut backup = node(3);
        backup.priority = Priority::Backup;
        assert_eq!(
            best(
                &[preferred.clone(), normal.clone(), backup.clone()],
                None,
                &[]
            ),
            Some(1)
        );
        assert_eq!(best(&[normal, backup.clone()], None, &[2]), Some(3));
        preferred.health = Health::Unavailable;
        assert_eq!(best(&[preferred, backup], None, &[]), Some(3));
    }

    #[test]
    fn degraded_nodes_only_when_no_healthy_one_fits() {
        let mut degraded = node(1);
        degraded.health = Health::Degraded;
        let mut healthy = node(2);
        healthy.sources = vec!["soundcloud".to_owned()];
        assert_eq!(
            best(&[degraded.clone(), healthy.clone()], None, &[]),
            Some(2)
        );
        assert_eq!(
            best(&[degraded.clone(), healthy], Some("youtube"), &[]),
            Some(1)
        );
        let mut draining = node(3);
        draining.health = Health::Draining;
        assert_eq!(best(&[draining], None, &[]), None);
    }

    #[test]
    fn caps_recovery_and_sources_rule_nodes_out() {
        let mut full = node(1);
        full.max_players = 2;
        full.channels = 2;
        let mut recovering = node(2);
        recovering.recovering = true;
        let mut no_spotify = node(3);
        no_spotify.load.playing_players = 9;
        let mut spotify = node(4);
        spotify.sources.push("spotify".to_owned());
        spotify.load.playing_players = 20;
        let all = [full, recovering, no_spotify, spotify];
        assert_eq!(best(&all, None, &[]), Some(3));
        assert_eq!(best(&all, Some("spotify"), &[]), Some(4));
        assert_eq!(best(&all, Some("deezer"), &[]), None);
    }

    #[test]
    fn ties_go_to_fewer_channels_and_ping_counts() {
        let mut more = node(1);
        more.channels = 2;
        let fewer = node(2);
        assert_eq!(best(&[more, fewer.clone()], None, &[]), Some(2));
        let mut far = node(3);
        far.ping_ms = 400;
        assert_eq!(best(&[far, fewer], None, &[]), Some(2));
    }
}
