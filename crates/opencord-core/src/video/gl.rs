//! Zero-copy video textures on Linux (plan §7.11, V10), switched on with
//! `OPENCORD_ZERO_COPY=1` until they have been looked at on more machines.
//!
//! A tile's texture is a GL texture Flutter draws. A worker thread holds a
//! GL context of its own that shares textures with Flutter's: a picture the
//! hardware decoder left on the GPU (DMA-BUF planes, luma and chroma) is
//! imported as two EGL images and drawn through a BT.601 shader into one of
//! the tile's three RGBA textures, at the tile's size; a picture in memory
//! is uploaded instead. Either way the worker leaves a fence, and Flutter's
//! raster thread waits on it before drawing the texture. Nothing touches
//! Flutter's own GL state but one texture binding, put back at once.
//!
//! Our context is made from Flutter's on its raster thread, the first time
//! a tile is drawn: only there are Flutter's EGL display and context
//! current. Pictures before that are dropped, which is one frame.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use glow::HasContext;
use irondash_texture::{BoxedGLTexture, GLTexture, GLTextureProvider, PayloadProvider};
use khronos_egl as egl;
use opencord_media::video::codec::dmabuf::DmaBufPicture;

use super::lock;

type Egl = egl::DynamicInstance<egl::EGL1_4>;

/// EGL_EXT_image_dma_buf_import and its modifiers extension.
const LINUX_DMA_BUF: egl::Enum = 0x3270;
const LINUX_DRM_FOURCC: egl::Int = 0x3271;
const DMA_BUF_PLANE0_FD: egl::Int = 0x3272;
const DMA_BUF_PLANE0_OFFSET: egl::Int = 0x3273;
const DMA_BUF_PLANE0_PITCH: egl::Int = 0x3274;
const DMA_BUF_PLANE0_MODIFIER_LO: egl::Int = 0x3443;
const DMA_BUF_PLANE0_MODIFIER_HI: egl::Int = 0x3444;
/// DRM's "no modifier given": the import leaves it out.
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// Whether zero-copy textures are on (`OPENCORD_ZERO_COPY=1`).
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("OPENCORD_ZERO_COPY").is_ok_and(|value| value == "1"))
}

fn egl() -> Option<&'static Egl> {
    static EGL: OnceLock<Option<Egl>> = OnceLock::new();
    EGL.get_or_init(|| {
        // SAFETY: loads the system's libEGL, the one Flutter uses too.
        unsafe { Egl::load_required() }
            .inspect_err(|error| tracing::info!(%error, "no EGL: textures stay in memory"))
            .ok()
    })
    .as_ref()
}

/// GL functions, through EGL.
fn gl_functions(egl: &'static Egl) -> glow::Context {
    // SAFETY: names EGL resolves are the GL entry points of the current
    // context's API; glow calls them with the arguments GL defines.
    unsafe {
        glow::Context::from_loader_function(|name| {
            egl.get_proc_address(name)
                .map_or(std::ptr::null(), |function| function as *const c_void)
        })
    }
}

/// A GL sync object, waited on by another thread of the same share group.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fence(glow::NativeFence);
// SAFETY: sync objects are shared across the share group; only its handle
// crosses threads, and each use happens with a context of the group current.
unsafe impl Send for Fence {}

/// The newest picture of a tile, ready to draw.
#[derive(Clone, Copy)]
struct Shown {
    name: u32,
    width: u32,
    height: u32,
    fence: Option<Fence>,
}

/// One tile's GL side.
pub(super) struct Tile {
    shown: Mutex<Option<Shown>>,
    /// Made available to Flutter: marks a new picture.
    texture: Mutex<Option<Arc<irondash_texture::SendableTexture<BoxedGLTexture>>>>,
    pub(super) presented: AtomicU64,
    pub(super) drawn: AtomicU64,
    /// Its textures, the worker's: three, used in turn.
    ring: Mutex<Ring>,
}

#[derive(Default)]
struct Ring {
    textures: Vec<(glow::NativeTexture, u32, u32)>,
    next: usize,
}

impl Tile {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            shown: Mutex::new(None),
            texture: Mutex::new(None),
            presented: AtomicU64::new(0),
            drawn: AtomicU64::new(0),
            ring: Mutex::new(Ring::default()),
        })
    }

    pub(super) fn set_texture(
        &self,
        texture: Arc<irondash_texture::SendableTexture<BoxedGLTexture>>,
    ) {
        *lock(&self.texture) = Some(texture);
    }

    fn mark(&self) {
        if let Some(texture) = lock(&self.texture).as_ref() {
            texture.mark_frame_available();
        }
    }

    /// A picture in memory, RGBA rows.
    pub(super) fn present(self: &Arc<Self>, width: u32, height: u32, rgba: Vec<u8>) {
        match world() {
            Some(world) => world.send(Job::Upload {
                tile: Arc::clone(self),
                width,
                height,
                rgba,
            }),
            // Flutter draws it first, and the worker starts then.
            None => self.mark(),
        }
    }

    /// A picture on the GPU, drawn at `size`; `false` once imports have
    /// failed here, so pictures come in memory from then on.
    pub(super) fn present_gpu(self: &Arc<Self>, picture: DmaBufPicture, size: (u32, u32)) -> bool {
        match world() {
            Some(world) if !world.imports_fail.load(Ordering::Relaxed) => {
                world.send(Job::Import {
                    tile: Arc::clone(self),
                    picture,
                    size,
                });
                true
            }
            Some(_) => false,
            None => {
                self.mark();
                true
            }
        }
    }
}

impl Drop for Tile {
    fn drop(&mut self) {
        let textures = std::mem::take(&mut lock(&self.ring).textures);
        if let Some(world) = world() {
            world.send(Job::Release(
                textures
                    .into_iter()
                    .map(|(texture, _, _)| texture)
                    .collect(),
            ));
        }
    }
}

/// What Flutter draws for a tile now.
struct Frame {
    name: u32,
    width: i32,
    height: i32,
}

impl GLTextureProvider for Frame {
    fn get(&self) -> GLTexture<'_> {
        GLTexture {
            target: glow::TEXTURE_2D,
            name: &self.name,
            width: self.width,
            height: self.height,
        }
    }
}

thread_local! {
    /// GL functions on Flutter's raster thread.
    static RASTER: OnceCell<Option<glow::Context>> = const { OnceCell::new() };
    /// A black texture in Flutter's context, for tiles with no picture yet.
    static BLANK: OnceCell<Option<u32>> = const { OnceCell::new() };
}

impl PayloadProvider<BoxedGLTexture> for Tile {
    /// On Flutter's raster thread, its GL context current.
    fn get_payload(&self) -> BoxedGLTexture {
        self.drawn.fetch_add(1, Ordering::Relaxed);
        start_world();
        let shown = {
            let mut shown = lock(&self.shown);
            let current = *shown;
            if let Some(shown) = shown.as_mut() {
                shown.fence = None;
            }
            current
        };
        RASTER.with(|raster| {
            let Some(gl) = raster.get_or_init(|| egl().map(gl_functions)) else {
                return blank_frame(None);
            };
            match shown {
                Some(shown) => {
                    if let Some(Fence(fence)) = shown.fence {
                        // SAFETY: a fence of our share group, waited on once
                        // and deleted, in Flutter's current context.
                        unsafe {
                            gl.wait_sync(fence, 0, glow::TIMEOUT_IGNORED);
                            gl.delete_sync(fence);
                        }
                    }
                    Box::new(Frame {
                        name: shown.name,
                        width: shown.width as i32,
                        height: shown.height as i32,
                    }) as BoxedGLTexture
                }
                None => blank_frame(Some(gl)),
            }
        })
    }
}

fn blank_frame(gl: Option<&glow::Context>) -> BoxedGLTexture {
    let name = BLANK.with(|blank| {
        *blank.get_or_init(|| {
            let gl = gl?;
            // SAFETY: in Flutter's current context: one texture made and
            // filled, its previous binding put back.
            unsafe {
                let before = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
                let texture = gl.create_texture().ok()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    1,
                    1,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&[0, 0, 0, 255])),
                );
                gl.bind_texture(
                    glow::TEXTURE_2D,
                    std::num::NonZeroU32::new(before as u32).map(glow::NativeTexture),
                );
                Some(texture.0.get())
            }
        })
    });
    Box::new(Frame {
        name: name.unwrap_or(0),
        width: 1,
        height: 1,
    })
}

/// The worker and what it is told.
struct World {
    jobs: Mutex<mpsc::Sender<Job>>,
    /// An import failed: this GPU cannot take the decoder's buffers.
    imports_fail: AtomicBool,
}

impl World {
    fn send(&self, job: Job) {
        let _ = lock(&self.jobs).send(job);
    }
}

enum Job {
    Upload {
        tile: Arc<Tile>,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    Import {
        tile: Arc<Tile>,
        picture: DmaBufPicture,
        size: (u32, u32),
    },
    Release(Vec<glow::NativeTexture>),
}

static WORLD: OnceLock<Option<World>> = OnceLock::new();
/// Set once the first tile has been drawn and the worker is starting.
static STARTING: AtomicU32 = AtomicU32::new(0);

fn world() -> Option<&'static World> {
    WORLD.get().and_then(Option::as_ref)
}

/// Makes our context from Flutter's current one and starts the worker;
/// once, on the raster thread.
fn start_world() {
    if STARTING.swap(1, Ordering::SeqCst) == 1 {
        return;
    }
    let world = make_world()
        .inspect_err(|error| tracing::info!(%error, "zero-copy textures are off"))
        .ok();
    let _ = WORLD.set(world);
}

fn make_world() -> Result<World, String> {
    let egl = egl().ok_or("no EGL")?;
    let display = egl.get_current_display().ok_or("no current EGL display")?;
    let flutter = egl.get_current_context().ok_or("no current EGL context")?;
    let query = |attribute| {
        egl.query_context(display, flutter, attribute)
            .map_err(|error| error.to_string())
    };
    let api = query(egl::CONTEXT_CLIENT_TYPE)? as egl::Enum;
    let config_id = query(egl::CONFIG_ID)?;
    let config = egl
        .choose_first_config(display, &[egl::CONFIG_ID, config_id, egl::NONE])
        .map_err(|error| error.to_string())?
        .ok_or("no EGL config")?;
    let es = api == egl::OPENGL_ES_API;
    let mut attributes = Vec::new();
    if es {
        attributes.extend([
            egl::CONTEXT_CLIENT_VERSION,
            query(egl::CONTEXT_CLIENT_VERSION)?,
        ]);
    }
    attributes.push(egl::NONE);
    egl.bind_api(api).map_err(|error| error.to_string())?;
    let context = egl
        .create_context(display, config, Some(flutter), &attributes)
        .map_err(|error| error.to_string())?;
    let (jobs, received) = mpsc::channel();
    let (display_ptr, context_ptr) = (display.as_ptr() as usize, context.as_ptr() as usize);
    std::thread::Builder::new()
        .name("opencord-gl".to_owned())
        .spawn(move || {
            // SAFETY: the display and context made above, alive for the
            // process; this thread is the only one that makes it current.
            let (display, context) = unsafe {
                (
                    egl::Display::from_ptr(display_ptr as egl::EGLDisplay),
                    egl::Context::from_ptr(context_ptr as egl::EGLContext),
                )
            };
            match Worker::new(egl, display, context, api, es) {
                Ok(mut worker) => worker.run(&received),
                Err(error) => tracing::info!(%error, "the GL worker did not start"),
            }
        })
        .map_err(|error| error.to_string())?;
    tracing::info!(es, "zero-copy video textures");
    Ok(World {
        jobs: Mutex::new(jobs),
        imports_fail: AtomicBool::new(false),
    })
}

type CreateImage = unsafe extern "system" fn(
    egl::EGLDisplay,
    egl::EGLContext,
    egl::Enum,
    *mut c_void,
    *const egl::Int,
) -> *mut c_void;
type DestroyImage = unsafe extern "system" fn(egl::EGLDisplay, *mut c_void) -> egl::Boolean;
type ImageTargetTexture = unsafe extern "system" fn(u32, *mut c_void);

/// Our context's side: the converting program and what it draws with.
struct Worker {
    egl: &'static Egl,
    display: egl::Display,
    gl: glow::Context,
    program: glow::NativeProgram,
    framebuffer: glow::NativeFramebuffer,
    planes: [glow::NativeTexture; 2],
    create_image: CreateImage,
    destroy_image: DestroyImage,
    image_target: ImageTargetTexture,
}

impl Worker {
    fn new(
        egl: &'static Egl,
        display: egl::Display,
        context: egl::Context,
        api: egl::Enum,
        es: bool,
    ) -> Result<Self, String> {
        egl.bind_api(api).map_err(|error| error.to_string())?;
        egl.make_current(display, None, None, Some(context))
            .map_err(|error| format!("our context did not become current: {error}"))?;
        let function = |name: &str| {
            egl.get_proc_address(name)
                .ok_or_else(|| format!("no {name}"))
        };
        // SAFETY: the EGL and GL entry points of these names, with the
        // signatures their extensions define.
        let (create_image, destroy_image, image_target) = unsafe {
            (
                std::mem::transmute::<extern "system" fn(), CreateImage>(function(
                    "eglCreateImageKHR",
                )?),
                std::mem::transmute::<extern "system" fn(), DestroyImage>(function(
                    "eglDestroyImageKHR",
                )?),
                std::mem::transmute::<extern "system" fn(), ImageTargetTexture>(function(
                    "glEGLImageTargetTexture2DOES",
                )?),
            )
        };
        let gl = gl_functions(egl);
        // SAFETY: GL calls in our current context.
        unsafe {
            let program = program(&gl, es)?;
            let framebuffer = gl.create_framebuffer()?;
            let planes = [texture(&gl)?, texture(&gl)?];
            let quad = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad));
            let corners: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            let bytes: Vec<u8> = corners
                .iter()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            gl.use_program(Some(program));
            let position = gl
                .get_attrib_location(program, "position")
                .ok_or("no position attribute")?;
            gl.enable_vertex_attrib_array(position);
            gl.vertex_attrib_pointer_f32(position, 2, glow::FLOAT, false, 8, 0);
            gl.uniform_1_i32(gl.get_uniform_location(program, "luma").as_ref(), 0);
            gl.uniform_1_i32(gl.get_uniform_location(program, "chroma").as_ref(), 1);
            Ok(Self {
                egl,
                display,
                gl,
                program,
                framebuffer,
                planes,
                create_image,
                destroy_image,
                image_target,
            })
        }
    }

    fn run(&mut self, jobs: &mpsc::Receiver<Job>) {
        while let Ok(job) = jobs.recv() {
            match job {
                Job::Upload {
                    tile,
                    width,
                    height,
                    rgba,
                } => self.upload(&tile, width, height, &rgba),
                Job::Import {
                    tile,
                    picture,
                    size,
                } => {
                    if let Err(error) = self.import(&tile, &picture, size) {
                        tracing::info!(%error, "decoded pictures cannot be drawn from the GPU here");
                        if let Some(world) = world() {
                            world.imports_fail.store(true, Ordering::Relaxed);
                        }
                    }
                }
                Job::Release(textures) => {
                    for texture in textures {
                        // SAFETY: a texture of ours, no longer drawn.
                        unsafe { self.gl.delete_texture(texture) };
                    }
                }
            }
        }
    }

    /// The tile's next texture, at this size.
    fn next_texture(
        &self,
        tile: &Tile,
        width: u32,
        height: u32,
    ) -> Result<glow::NativeTexture, String> {
        let gl = &self.gl;
        let mut ring = lock(&tile.ring);
        if ring.textures.len() < 3 {
            // SAFETY: GL calls in our current context.
            let texture = unsafe { texture(gl)? };
            ring.textures.push((texture, 0, 0));
        }
        let index = ring.next % ring.textures.len();
        ring.next = ring.next.wrapping_add(1);
        let (texture, have_width, have_height) = ring.textures[index];
        if (have_width, have_height) != (width, height) {
            // SAFETY: GL calls in our current context; storage for the size.
            unsafe {
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA as i32,
                    width as i32,
                    height as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
            }
            ring.textures[index] = (texture, width, height);
        }
        Ok(texture)
    }

    fn upload(&self, tile: &Arc<Tile>, width: u32, height: u32, rgba: &[u8]) {
        let Ok(texture) = self.next_texture(tile, width, height) else {
            return;
        };
        // SAFETY: GL calls in our current context; `rgba` holds the rows.
        unsafe {
            let gl = &self.gl;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(rgba)),
            );
        }
        self.show(tile, texture, width, height);
    }

    fn import(
        &self,
        tile: &Arc<Tile>,
        picture: &DmaBufPicture,
        size: (u32, u32),
    ) -> Result<(), String> {
        let images = [
            self.image(picture, 0, picture.width, picture.height)?,
            self.image(picture, 1, picture.width / 2, picture.height / 2)?,
        ];
        let (width, height) = size;
        let target = self.next_texture(tile, width, height)?;
        let gl = &self.gl;
        // SAFETY: GL calls in our current context; each EGL image is bound
        // to a plane texture, which keeps it, and then destroyed.
        unsafe {
            for (unit, (plane, image)) in self.planes.iter().zip(images).enumerate() {
                gl.active_texture(glow::TEXTURE0 + unit as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(*plane));
                (self.image_target)(glow::TEXTURE_2D, image);
                (self.destroy_image)(self.display.as_ptr(), image);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(target),
                0,
            );
            gl.viewport(0, 0, width as i32, height as i32);
            gl.use_program(Some(self.program));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.active_texture(glow::TEXTURE0);
        }
        // The decoder's surface goes back to it only once the GPU is done
        // reading it.
        let fence = self.show(tile, target, width, height);
        if let Some(Fence(fence)) = fence {
            // SAFETY: a fence just made in our context.
            unsafe {
                gl.client_wait_sync(
                    fence,
                    glow::SYNC_FLUSH_COMMANDS_BIT,
                    wait_ns(Duration::from_millis(100)),
                );
            }
        }
        Ok(())
    }

    /// One plane of `picture` as an EGL image.
    fn image(
        &self,
        picture: &DmaBufPicture,
        index: usize,
        width: u32,
        height: u32,
    ) -> Result<*mut c_void, String> {
        let plane = &picture.planes[index];
        let mut attributes = vec![
            egl::WIDTH,
            width as egl::Int,
            egl::HEIGHT,
            height as egl::Int,
            LINUX_DRM_FOURCC,
            plane.fourcc as egl::Int,
            DMA_BUF_PLANE0_FD,
            plane.fd.as_raw_fd(),
            DMA_BUF_PLANE0_OFFSET,
            plane.offset as egl::Int,
            DMA_BUF_PLANE0_PITCH,
            plane.pitch as egl::Int,
        ];
        if plane.modifier != DRM_FORMAT_MOD_INVALID {
            attributes.extend([
                DMA_BUF_PLANE0_MODIFIER_LO,
                (plane.modifier & 0xffff_ffff) as u32 as egl::Int,
                DMA_BUF_PLANE0_MODIFIER_HI,
                (plane.modifier >> 32) as u32 as egl::Int,
            ]);
        }
        attributes.push(egl::NONE);
        // SAFETY: an EGL_LINUX_DMA_BUF_EXT import of a live buffer (`plane`
        // holds its descriptor open), with no context, as the extension
        // requires.
        let image = unsafe {
            (self.create_image)(
                self.display.as_ptr(),
                egl::NO_CONTEXT,
                LINUX_DMA_BUF,
                std::ptr::null_mut(),
                attributes.as_ptr(),
            )
        };
        if image.is_null() {
            let error = self
                .egl
                .get_error()
                .map_or_else(|| "none".to_owned(), |error| error.to_string());
            return Err(format!("EGL refused a DMA-BUF plane ({error})"));
        }
        Ok(image)
    }

    /// Makes `texture` the tile's newest picture, behind a fence.
    fn show(
        &self,
        tile: &Arc<Tile>,
        texture: glow::NativeTexture,
        width: u32,
        height: u32,
    ) -> Option<Fence> {
        // SAFETY: GL calls in our current context.
        let fence = unsafe {
            let fence = self.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).ok();
            self.gl.flush();
            fence.map(Fence)
        };
        let replaced = lock(&tile.shown).replace(Shown {
            name: texture.0.get(),
            width,
            height,
            fence,
        });
        // A fence Flutter never waited on is ours to delete.
        if let Some(Shown {
            fence: Some(Fence(old)),
            ..
        }) = replaced
        {
            // SAFETY: a fence of our share group nobody waits on any more.
            unsafe { self.gl.delete_sync(old) };
        }
        tile.presented.fetch_add(1, Ordering::Relaxed);
        tile.mark();
        fence
    }
}

fn wait_ns(duration: Duration) -> i32 {
    i32::try_from(duration.as_nanos()).unwrap_or(i32::MAX)
}

/// A texture sampled linearly and clamped, as tiles are drawn.
///
/// # Safety
/// A GL context must be current.
unsafe fn texture(gl: &glow::Context) -> Result<glow::NativeTexture, String> {
    // SAFETY: the caller's context is current.
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        for (parameter, value) in [
            (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
            (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
            (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
            (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
        ] {
            gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
        }
        Ok(texture)
    }
}

/// NV12 to RGBA, BT.601 video range, the decoder's: luma from the first
/// plane, blue and red difference from the second (its R and G).
///
/// # Safety
/// A GL context must be current.
unsafe fn program(gl: &glow::Context, es: bool) -> Result<glow::NativeProgram, String> {
    let header = if es {
        "#version 100\nprecision mediump float;\n"
    } else {
        "#version 120\n"
    };
    let vertex = format!(
        "{header}attribute vec2 position;\nvarying vec2 uv;\n\
         void main() {{ uv = position * 0.5 + 0.5; gl_Position = vec4(position, 0.0, 1.0); }}\n"
    );
    let fragment = format!(
        "{header}varying vec2 uv;\nuniform sampler2D luma;\nuniform sampler2D chroma;\n\
         void main() {{\n\
           float y = 1.16438 * (texture2D(luma, uv).r - 0.06275);\n\
           vec2 c = texture2D(chroma, uv).rg - vec2(0.50196);\n\
           gl_FragColor = vec4(y + 1.59603 * c.y, y - 0.39176 * c.x - 0.81297 * c.y,\n\
                               y + 2.01723 * c.x, 1.0);\n\
         }}\n"
    );
    // SAFETY: the caller's context is current.
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.bind_attrib_location(program, 0, "position");
        gl.link_program(program);
        for shader in shaders {
            gl.delete_shader(shader);
        }
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        Ok(program)
    }
}

/// Live GL tiles' counters, by texture id, for `stats`.
pub(super) static TILES: Mutex<Option<HashMap<i64, std::sync::Weak<Tile>>>> = Mutex::new(None);

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use opencord_media::video::codec::EncoderConfig;
    use opencord_media::video::codec::decoder::{Decoded, Decoder};
    use opencord_media::video::codec::encoder::{Backend, Encoder};
    use opencord_media::video::codec::scale::Scaler;
    use opencord_media::video::pattern::scene;

    use super::*;

    /// EGL_MESA_platform_surfaceless: a display with no window system.
    const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

    /// A GL ES context with nothing to draw on but textures, current here.
    fn headless() -> Option<(egl::Display, egl::Context)> {
        let step =
            |what: &str, error: &dyn std::fmt::Debug| eprintln!("headless EGL: {what}: {error:?}");
        // SAFETY: loads the system's libEGL.
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|error| step("load", &error))
            .ok()?;
        // SAFETY: the surfaceless platform takes no native display.
        let display = unsafe {
            egl.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }
        .map_err(|error| step("display", &error))
        .ok()?;
        egl.initialize(display)
            .map_err(|error| step("initialize", &error))
            .ok()?;
        // The decoder's buffers are the Intel or AMD GPU's, which Mesa
        // drives; another vendor's EGL (NVIDIA's, first under GLVND here)
        // cannot import them, and the app falls back to uploads there.
        let vendor = egl.query_string(Some(display), egl::VENDOR).ok()?;
        if !vendor.to_string_lossy().contains("Mesa") {
            eprintln!("headless EGL: {vendor:?} is not Mesa");
            return None;
        }
        egl.bind_api(egl::OPENGL_ES_API)
            .map_err(|error| step("bind", &error))
            .ok()?;
        let config = egl
            .choose_first_config(
                display,
                &[
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    // No window here: the default asks for one.
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::NONE,
                ],
            )
            .map_err(|error| step("config", &error))
            .ok()??;
        let context = egl
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE],
            )
            .map_err(|error| step("context", &error))
            .ok()?;
        Some((display, context))
    }

    /// The moving scene at 1280×720, encoded, and its last picture decoded
    /// twice: kept on the GPU, and in memory.
    fn the_same_picture_twice() -> Option<(DmaBufPicture, opencord_media::video::picture::Picture)>
    {
        let config = EncoderConfig {
            width: 1280,
            height: 720,
            fps: 30,
            bitrate: 4_000_000,
        };
        let mut encoder = Encoder::open(config, &Backend::ALL).ok()?;
        let mut gpu = Decoder::open(true).ok()?;
        if !gpu.is_hardware() {
            return None;
        }
        let mut memory = Decoder::open(false).ok()?;
        let start = Instant::now();
        let (mut on_gpu, mut in_memory) = (None, None);
        for number in 0..10 {
            let picture = scene(1280, 720, number, start);
            let Some(encoded) = encoder.encode(&picture, number == 0).ok()? else {
                continue;
            };
            for decoded in gpu.decode_gpu(&encoded.nal_units, start).ok()? {
                if let Decoded::Gpu(picture) = decoded {
                    on_gpu = Some(picture);
                }
            }
            if let Some(picture) = memory.decode(&encoded.nal_units, start).ok()?.pop() {
                in_memory = Some(picture);
            }
        }
        on_gpu.zip(in_memory)
    }

    #[test]
    fn a_picture_drawn_from_the_gpu_matches_the_one_converted_in_memory() {
        let Some((display, context)) = headless() else {
            eprintln!("skipped: no surfaceless EGL here");
            return;
        };
        let Some((on_gpu, in_memory)) = the_same_picture_twice() else {
            eprintln!("skipped: no hardware video here");
            return;
        };
        let worker = Worker::new(egl().unwrap(), display, context, egl::OPENGL_ES_API, true)
            .expect("the GL worker starts");
        let tile = Tile::new();
        let (width, height) = (640, 360);

        worker
            .import(&tile, &on_gpu, (width, height))
            .expect("the planes import");

        let name = lock(&tile.shown).expect("a picture shown").name;
        let mut drawn = vec![0u8; (width * height * 4) as usize];
        // SAFETY: GL calls in the worker's current context, reading the
        // texture it drew into.
        unsafe {
            let gl = &worker.gl;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(worker.framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                std::num::NonZeroU32::new(name).map(glow::NativeTexture),
                0,
            );
            gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut drawn)),
            );
        }
        let expected = Scaler::new().rgba(&in_memory, width, height).unwrap();

        // Row 0 is the top in both: the same picture, the same way up.
        let difference = drawn
            .chunks(4)
            .zip(expected.chunks(4))
            .map(|(a, b)| (0..3).map(|c| a[c].abs_diff(b[c]) as u64).sum::<u64>())
            .sum::<u64>() as f64
            / f64::from(width * height * 3);
        println!("mean difference per channel: {difference:.2} levels of 255");
        assert!(difference < 6.0, "{difference:.2}");
        let red = |rgba: &[u8], x: u32, y: u32| rgba[((y * width + x) * 4) as usize];
        assert!(
            red(&drawn, 20, 20).abs_diff(red(&expected, 20, 20)) < 24,
            "the top-left corner agrees"
        );
    }
}
