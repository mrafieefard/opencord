//! The app's textures through irondash (plan §7.11): pixel buffers that
//! Flutter draws, made and released on its main thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use irondash_run_loop::{RunLoop, RunLoopSender};
use irondash_texture::{
    BoxedPixelData, PayloadProvider, PixelData, PixelDataProvider, SendableTexture, Texture,
};

use super::{FrameSink, Textures, lock};

/// Textures for one Flutter engine.
pub struct FlutterTextures {
    engine: i64,
}

impl FlutterTextures {
    /// `engine` from the app's `EngineContext.getEngineHandle()`.
    pub fn new(engine: i64) -> Self {
        Self { engine }
    }
}

/// The newest picture, shared with Flutter's raster thread, and how many
/// were handed over and drawn.
#[derive(Default)]
struct Latest {
    picture: Mutex<Option<Arc<Rows>>>,
    presented: AtomicU64,
    drawn: AtomicU64,
}

/// Every live texture's counters, by id, for diagnostics and tests.
static LIVE: Mutex<Option<HashMap<i64, Weak<Latest>>>> = Mutex::new(None);

/// Pictures handed to a texture and pictures Flutter drew from it.
pub fn stats(texture_id: i64) -> Option<(u64, u64)> {
    let live = lock(&LIVE);
    let latest = live.as_ref()?.get(&texture_id)?.upgrade()?;
    Some((
        latest.presented.load(Ordering::Relaxed),
        latest.drawn.load(Ordering::Relaxed),
    ))
}

struct Rows {
    width: i32,
    height: i32,
    data: Vec<u8>,
}

/// A picture handed to Flutter without copying it.
struct Shared(Arc<Rows>);

impl PixelDataProvider for Shared {
    fn get(&self) -> PixelData<'_> {
        PixelData {
            width: self.0.width,
            height: self.0.height,
            data: &self.0.data,
        }
    }
}

impl PayloadProvider<BoxedPixelData> for Latest {
    fn get_payload(&self) -> BoxedPixelData {
        self.drawn.fetch_add(1, Ordering::Relaxed);
        let picture = lock(&self.picture).clone().unwrap_or_else(|| {
            Arc::new(Rows {
                width: 2,
                height: 2,
                data: vec![0; 16],
            })
        });
        Box::new(Shared(picture))
    }
}

struct FlutterSink {
    id: i64,
    latest: Arc<Latest>,
    texture: Option<Arc<SendableTexture<BoxedPixelData>>>,
    main: RunLoopSender,
}

impl FrameSink for FlutterSink {
    fn id(&self) -> i64 {
        self.id
    }

    fn present(&self, width: u32, height: u32, rgba: Vec<u8>) {
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return;
        };
        *lock(&self.latest.picture) = Some(Arc::new(Rows {
            width,
            height,
            data: rgba,
        }));
        self.latest.presented.fetch_add(1, Ordering::Relaxed);
        if let Some(texture) = &self.texture {
            texture.mark_frame_available();
        }
    }
}

impl Drop for FlutterSink {
    fn drop(&mut self) {
        // A texture is released on the thread that made it.
        if let Some(texture) = self.texture.take() {
            self.main.send(move || drop(texture));
        }
    }
}

impl Textures for FlutterTextures {
    fn create(&self) -> Option<Arc<dyn FrameSink>> {
        let main = RunLoop::sender_for_main_thread()
            .inspect_err(|error| tracing::warn!(?error, "no Flutter main thread for textures"))
            .ok()?;
        let engine = self.engine;
        let made = main.send_and_wait(move || {
            let latest = Arc::new(Latest::default());
            let provider: Arc<dyn PayloadProvider<BoxedPixelData>> = latest.clone();
            match Texture::new_with_provider(engine, provider) {
                Ok(texture) => Ok((texture.id(), latest, texture.into_sendable_texture())),
                Err(error) => Err(format!("{error:?}")),
            }
        });
        match made {
            Ok((id, latest, texture)) => {
                let mut live = lock(&LIVE);
                let live = live.get_or_insert_with(HashMap::new);
                live.retain(|_, latest| latest.strong_count() > 0);
                live.insert(id, Arc::downgrade(&latest));
                Some(Arc::new(FlutterSink {
                    id,
                    latest,
                    texture: Some(texture),
                    main,
                }))
            }
            Err(error) => {
                tracing::warn!(%error, "a texture could not be made");
                None
            }
        }
    }
}
