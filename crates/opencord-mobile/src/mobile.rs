//! The facade's process state: the core's client, its runtime, the event
//! pump and the cache folder. JavaScript reaches it through the exported
//! functions in `api`; tests build a [`Mobile`] directly.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use opencord_core::client::Client;
use opencord_core::media::MediaOptions;
use tokio::runtime::{Handle, Runtime};

use crate::MobileError;
use crate::events::{EventPump, MobileEvent};
use crate::types::ViewOutputRecord;
use crate::views::ViewPump;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static MOBILE: OnceLock<Mobile> = OnceLock::new();

pub struct Mobile {
    client: Client,
    events: EventPump,
    views: ViewPump,
    cache_dir: PathBuf,
    runtime: Handle,
}

impl Mobile {
    /// Opens the core's local data in `data_dir`. `open_devices` is off in
    /// tests, so they never open a microphone.
    pub fn start(
        data_dir: &Path,
        cache_dir: &Path,
        runtime: Handle,
        open_devices: bool,
    ) -> Result<Self, MobileError> {
        let (client, core_events) = Client::new(data_dir, runtime.clone())?;
        let media_events = client.enable_media(MediaOptions { open_devices });
        Ok(Self {
            client,
            events: EventPump::new(core_events, media_events),
            views: ViewPump::default(),
            cache_dir: cache_dir.to_owned(),
            runtime,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn attach(&self) -> u64 {
        self.events.attach()
    }

    pub async fn next_events(&self, generation: u64, max: u32) -> Vec<MobileEvent> {
        self.events.next(generation, max).await
    }

    /// Starts a new generation of the plugin view stream; a
    /// `next_view_outputs` waiting under an older one returns empty at once.
    /// Views the earlier generation left open (a JavaScript reload) are
    /// closed first: nothing is left to draw them.
    pub fn view_outputs_attach(&self) -> u64 {
        let (generation, left) = self.views.attach(self.client.view_outputs());
        for view_id in left {
            tracing::info!(view = %view_id, "closing a view an earlier start left open");
            self.client.view_close(&view_id);
        }
        generation
    }

    pub async fn next_view_outputs(&self, generation: u64, max: u32) -> Vec<ViewOutputRecord> {
        self.views.next(generation, max).await
    }

    /// A view this JavaScript start opened, to close if it reloads.
    pub fn note_view_opened(&self, view_id: &str) {
        self.views.opened(view_id);
    }

    pub fn note_view_closed(&self, view_id: &str) {
        self.views.closed(view_id);
    }

    /// The views opened and not closed since the last attach, ascending.
    pub fn open_view_ids(&self) -> Vec<String> {
        self.views.open_ids()
    }

    pub fn set_plugin_features(&self, features: Vec<String>) -> Result<(), MobileError> {
        Ok(self.client.set_plugin_features(features)?)
    }

    /// Runs `work` on the core's runtime, where its connections live.
    pub async fn run<T, F>(&self, work: impl FnOnce(Client) -> F) -> Result<T, MobileError>
    where
        F: Future<Output = Result<T, MobileError>> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime
            .spawn(work(self.client.clone()))
            .await
            .map_err(|error| MobileError::Other {
                message: error.to_string(),
            })?
    }

    /// Runs blocking file work off the runtime's async threads.
    pub async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, MobileError> + Send + 'static,
    ) -> Result<T, MobileError> {
        self.runtime
            .spawn_blocking(work)
            .await
            .map_err(|error| MobileError::Other {
                message: error.to_string(),
            })?
    }
}

/// Opens the core once; later calls do nothing.
pub(crate) fn init(data_dir: &Path, cache_dir: &Path) -> Result<(), MobileError> {
    if MOBILE.get().is_some() {
        return Ok(());
    }
    let runtime = match RUNTIME.get() {
        Some(runtime) => runtime,
        None => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("opencord-mobile")
                .build()
                .map_err(|error| MobileError::Other {
                    message: error.to_string(),
                })?;
            RUNTIME.get_or_init(|| runtime)
        }
    };
    let mobile = Mobile::start(data_dir, cache_dir, runtime.handle().clone(), true)?;
    if MOBILE.set(mobile).is_err() {
        // A racing first call won; its `Mobile` is the one kept.
        tracing::debug!("the core was opened twice at once; the first is kept");
    }
    Ok(())
}

pub(crate) fn mobile() -> Result<&'static Mobile, MobileError> {
    MOBILE.get().ok_or(MobileError::NotInitialized)
}

/// `work` on the core's runtime, with the client.
pub(crate) async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, MobileError>
where
    F: Future<Output = Result<T, MobileError>> + Send + 'static,
    T: Send + 'static,
{
    mobile()?.run(work).await
}
