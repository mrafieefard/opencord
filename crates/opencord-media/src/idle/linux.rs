//! Linux: the first source this session offers, found once.
//! - Wayland compositors with ext-idle-notify-v1 (KDE, Hyprland, Sway and
//!   other wlroots ones) say when the seat has been idle for [`THRESHOLD`]
//!   and when it resumes; a thread of our own listens.
//! - GNOME has Mutter's IdleMonitor on the session bus.
//! - X11 has the MIT-SCREEN-SAVER extension. Under Wayland it would see
//!   only X clients' input, so it is used only without Wayland.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use ashpd::zbus;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{
    self, ExtIdleNotificationV1,
};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
use x11rb::connection::Connection as _;
use x11rb::protocol::screensaver::ConnectionExt as _;

use super::THRESHOLD;

enum Source {
    /// Since when the seat has been idle, as the compositor last said.
    Wayland(Arc<Mutex<Option<Instant>>>),
    Mutter,
    X11,
    Nothing,
}

static SOURCE: tokio::sync::OnceCell<Source> = tokio::sync::OnceCell::const_new();

pub async fn idle_time() -> Option<Duration> {
    match SOURCE.get_or_init(find).await {
        Source::Wayland(since) => Some(
            since
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .map_or(Duration::ZERO, |since| since.elapsed()),
        ),
        Source::Mutter => mutter().await,
        Source::X11 => tokio::task::spawn_blocking(x11).await.ok().flatten(),
        Source::Nothing => None,
    }
}

async fn find() -> Source {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland
        && let Some(since) = tokio::task::spawn_blocking(wayland_watch)
            .await
            .ok()
            .flatten()
    {
        return Source::Wayland(since);
    }
    if mutter().await.is_some() {
        return Source::Mutter;
    }
    if !wayland
        && tokio::task::spawn_blocking(x11)
            .await
            .ok()
            .flatten()
            .is_some()
    {
        return Source::X11;
    }
    tracing::info!(
        "this desktop does not say how long it has been idle; nobody is moved for being away"
    );
    Source::Nothing
}

async fn mutter() -> Option<Duration> {
    static BUS: OnceLock<Option<zbus::Connection>> = OnceLock::new();
    let connection = match BUS.get() {
        Some(connection) => connection.clone()?,
        None => {
            let connection = zbus::Connection::session().await.ok();
            BUS.get_or_init(|| connection).clone()?
        }
    };
    let reply = connection
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .await
        .ok()?;
    let milliseconds: u64 = reply.body().deserialize().ok()?;
    Some(Duration::from_millis(milliseconds))
}

fn x11() -> Option<Duration> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let info = connection.screensaver_query_info(root).ok()?.reply().ok()?;
    Some(Duration::from_millis(u64::from(info.ms_since_user_input)))
}

/// Asks the compositor to say when the seat has been idle for
/// [`THRESHOLD`], and listens on a thread of its own for the rest of the
/// process. `None` when it does not offer ext-idle-notify-v1.
fn wayland_watch() -> Option<Arc<Mutex<Option<Instant>>>> {
    let connection = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<Watch>(&connection).ok()?;
    let handle = queue.handle();
    let notifier: ExtIdleNotifierV1 = globals.bind(&handle, 1..=1, ()).ok()?;
    let seat: wl_seat::WlSeat = globals.bind(&handle, 1..=1, ()).ok()?;
    let timeout = u32::try_from(THRESHOLD.as_millis()).ok()?;
    let notification = notifier.get_idle_notification(timeout, &seat, &handle, ());
    let since = Arc::new(Mutex::new(None));
    let mut watch = Watch {
        since: Arc::clone(&since),
    };
    queue.roundtrip(&mut watch).ok()?;
    std::thread::Builder::new()
        .name("idle watch".to_owned())
        .spawn(move || {
            let _keep = (notifier, seat, notification, connection);
            while queue.blocking_dispatch(&mut watch).is_ok() {}
        })
        .ok()?;
    Some(since)
}

struct Watch {
    since: Arc<Mutex<Option<Instant>>>,
}

impl Dispatch<ExtIdleNotificationV1, ()> for Watch {
    fn event(
        watch: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let since = match event {
            ext_idle_notification_v1::Event::Idled => Instant::now().checked_sub(THRESHOLD),
            _ => None,
        };
        *watch.since.lock().unwrap_or_else(PoisonError::into_inner) = since;
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Watch {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Watch {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for Watch {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a desktop session; run with `--ignored`.
    #[tokio::test]
    #[ignore = "needs a desktop session"]
    async fn this_desktop_says_how_long_it_has_been_idle() {
        let idle = idle_time().await;
        println!("idle: {idle:?}");
        assert!(idle.is_some(), "no idle source on this desktop");
    }

    /// Needs a desktop session nobody touches for a minute; run with
    /// `--ignored`.
    #[tokio::test]
    #[ignore = "needs an untouched desktop for a minute"]
    async fn an_untouched_desktop_reads_idle_past_the_threshold() {
        assert!(idle_time().await.is_some());
        tokio::time::sleep(THRESHOLD + Duration::from_secs(5)).await;

        let idle = idle_time().await.unwrap();
        println!("idle: {idle:?}");
        assert!(idle >= THRESHOLD, "{idle:?}");
    }
}
