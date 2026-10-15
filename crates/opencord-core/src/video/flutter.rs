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
    #[cfg(target_os = "linux")]
    {
        let tiles = lock(&super::gl::TILES);
        if let Some(tile) = tiles
            .as_ref()
            .and_then(|tiles| tiles.get(&texture_id)?.upgrade())
        {
            return Some((
                tile.presented.load(Ordering::Relaxed),
                tile.drawn.load(Ordering::Relaxed),
            ));
        }
    }
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
            // Blank until the first picture; as wide as Metal needs on macOS.
            let width = if cfg!(target_os = "macos") { 64 } else { 2 };
            Arc::new(Rows {
                width,
                height: 2,
                data: vec![0; width as usize * 2 * 4],
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
        #[cfg(target_os = "macos")]
        let (width, rgba) = metal_rows(width, height, &rgba);
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

/// What irondash 0.5's macOS surfaces need (docs/plans/macos.md §7, M4):
/// BGRA, the format it gives them (its pixel data says RGBA), and rows a
/// multiple of 256 bytes, which Metal needs to draw from them, while
/// irondash makes each row exactly the picture's width. So the picture is
/// stretched to the next width of a multiple of 64 pixels, red and blue
/// swapped; Flutter scales the texture to its tile anyway.
#[cfg(target_os = "macos")]
fn metal_rows(width: u32, height: u32, rgba: &[u8]) -> (u32, Vec<u8>) {
    const ALIGN: u32 = 64;
    let aligned = width.div_ceil(ALIGN).max(1) * ALIGN;
    let (from, to) = (width as usize, aligned as usize);
    let columns: Vec<usize> = (0..to).map(|x| x * from / to).collect();
    let mut bgra = Vec::with_capacity(to * height as usize * 4);
    for row in rgba.chunks_exact(from * 4).take(height as usize) {
        for &x in &columns {
            let [r, g, b, a] = [row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]];
            bgra.extend_from_slice(&[b, g, r, a]);
        }
    }
    (aligned, bgra)
}

impl Drop for FlutterSink {
    fn drop(&mut self) {
        // A texture is released on the thread that made it.
        if let Some(texture) = self.texture.take() {
            self.main.send(move || drop(texture));
        }
    }
}

/// A GL texture's sink (`gl.rs`): pictures in memory are uploaded, those
/// on the GPU drawn from it.
#[cfg(target_os = "linux")]
struct GlSink {
    id: i64,
    tile: Arc<super::gl::Tile>,
    texture: Option<Arc<SendableTexture<irondash_texture::BoxedGLTexture>>>,
    main: RunLoopSender,
}

#[cfg(target_os = "linux")]
impl FrameSink for GlSink {
    fn id(&self) -> i64 {
        self.id
    }

    fn present(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.tile.present(width, height, rgba);
    }

    fn takes_gpu(&self) -> bool {
        true
    }

    fn present_gpu(
        &self,
        picture: opencord_media::video::codec::dmabuf::DmaBufPicture,
        size: (u32, u32),
    ) -> bool {
        self.tile.present_gpu(picture, size)
    }
}

#[cfg(target_os = "linux")]
impl Drop for GlSink {
    fn drop(&mut self) {
        if let Some(texture) = self.texture.take() {
            self.main.send(move || drop(texture));
        }
    }
}

#[cfg(target_os = "linux")]
impl FlutterTextures {
    fn create_gl(&self, main: RunLoopSender) -> Option<Arc<dyn FrameSink>> {
        let engine = self.engine;
        let made = main.send_and_wait(move || {
            let tile = super::gl::Tile::new();
            let provider: Arc<dyn PayloadProvider<irondash_texture::BoxedGLTexture>> = tile.clone();
            match Texture::new_with_provider(engine, provider) {
                Ok(texture) => Ok((texture.id(), tile, texture.into_sendable_texture())),
                Err(error) => Err(format!("{error:?}")),
            }
        });
        match made {
            Ok((id, tile, texture)) => {
                tile.set_texture(Arc::clone(&texture));
                let mut tiles = lock(&super::gl::TILES);
                let tiles = tiles.get_or_insert_with(HashMap::new);
                tiles.retain(|_, tile| tile.strong_count() > 0);
                tiles.insert(id, Arc::downgrade(&tile));
                Some(Arc::new(GlSink {
                    id,
                    tile,
                    texture: Some(texture),
                    main,
                }))
            }
            Err(error) => {
                tracing::warn!(%error, "a GL texture could not be made");
                None
            }
        }
    }
}

impl Textures for FlutterTextures {
    fn create(&self) -> Option<Arc<dyn FrameSink>> {
        let main = RunLoop::sender_for_main_thread()
            .inspect_err(|error| tracing::warn!(?error, "no Flutter main thread for textures"))
            .ok()?;
        #[cfg(target_os = "linux")]
        if super::gl::enabled() {
            return self.create_gl(main);
        }
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn macos_textures_get_bgra_rows_of_a_multiple_of_64_pixels() {
        // Two rows of three pixels: red, green, blue.
        let rgba = [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
            .repeat(2)
            .concat();

        let (width, bgra) = metal_rows(3, 2, &rgba);

        assert_eq!(width, 64);
        assert_eq!(bgra.len(), 64 * 2 * 4);
        assert_eq!(&bgra[..4], &[0, 0, 255, 255], "red, as BGRA");
        assert_eq!(&bgra[63 * 4..64 * 4], &[255, 0, 0, 255], "blue at the end");
        assert_eq!(metal_rows(128, 1, &[0; 128 * 4]).0, 128, "already aligned");
    }
}
