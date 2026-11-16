//! `opencord --after-pid <pid> --cover` (design: "The launcher holds the
//! cover", steps 2, 4 and 6). The app is restarting for an update with its
//! window covered. The launcher shows that last covered frame in a window
//! of its own, in the same place and size, writes `cover.shown`, and only
//! then waits for the app to exit: the app waits for `cover.shown` before
//! it exits, so waiting first would deadlock. It then switches and starts
//! the new version as any restart does, and keeps its window up until the
//! new app writes `cover.taken` (or [`TAKEN_WAIT`] passes).
//!
//! `cover.rgba` is `b"OCCV"`, then the width, the height and the stride in
//! bytes (each a little-endian `u32`), then `height` rows of `stride` bytes
//! whose first `width × 4` are RGBA pixels. Both are physical pixels, and
//! the picture is the window's visible content: on Linux, without its
//! 14 px transparent frame margin. `restart.json` and the `cover.*` files
//! are removed by the new app; a `cover.taken` left over from an earlier
//! restart is removed here first, so it can't close this cover at once.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use opencord_update::layout::{self, Layout};
use opencord_update::restart::WindowNote;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId, WindowLevel};

use crate::log;
use crate::splash::{self, OPAQUE, PaintError, Surface};

/// How long the cover waits for the new app to take it over.
pub const TAKEN_WAIT: Duration = Duration::from_secs(30);
/// Between looks for `cover.taken`.
const POLL_EVERY: Duration = Duration::from_millis(50);
/// How long the restart waits for the window to be on screen. The app
/// itself waits 3 s for `cover.shown`.
const SHOW_WAIT: Duration = Duration::from_secs(5);

const MAGIC: &[u8; 4] = b"OCCV";
const HEADER: usize = 16;
/// Limits on what `cover.rgba` may describe: wider than any screen, and
/// a file no bigger than an 8K frame needs, with room to spare.
const MAX_SIDE: u32 = 16_384;
const MAX_BYTES: u64 = 256 << 20;

/// The fill's middle (design: "The fill"), under anything the picture
/// doesn't cover and under its transparent pixels.
const FILL: u32 = OPAQUE | 0x4D_55D6;

/// The picture from `cover.rgba`, as opaque `0RGB` pixels row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    /// Shared, so handing the picture to the window's thread copies none.
    pixels: Arc<[u32]>,
}

/// Why `cover.rgba` can't be shown.
#[derive(Debug)]
pub enum ImageError {
    Read(io::Error),
    /// Doesn't start with the header.
    NotACover,
    /// A side of 0 or over [`MAX_SIDE`], or rows shorter than the width.
    Size {
        width: u32,
        height: u32,
        stride: u32,
    },
    /// The file isn't the header and `height` rows of `stride` bytes.
    Length {
        expected: u64,
        found: u64,
    },
    TooLarge,
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "can't read cover.rgba: {error}"),
            Self::NotACover => f.write_str("cover.rgba has no OCCV header"),
            Self::Size {
                width,
                height,
                stride,
            } => write!(
                f,
                "cover.rgba gives a {width}×{height} picture with {stride}-byte rows"
            ),
            Self::Length { expected, found } => {
                write!(f, "cover.rgba is {found} bytes, not {expected}")
            }
            Self::TooLarge => write!(f, "cover.rgba is over {MAX_BYTES} bytes"),
        }
    }
}

impl Image {
    /// Reads `cover.rgba` at `path`.
    pub fn read(path: &Path) -> Result<Self, ImageError> {
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
            .map_err(ImageError::Read)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(ImageError::TooLarge);
        }
        Self::parse(&bytes)
    }

    fn parse(bytes: &[u8]) -> Result<Self, ImageError> {
        if bytes.len() < HEADER || !bytes.starts_with(MAGIC) {
            return Err(ImageError::NotACover);
        }
        let word = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let (width, height, stride) = (word(4), word(8), word(12));
        let row = u64::from(width) * 4;
        if width == 0
            || height == 0
            || width > MAX_SIDE
            || height > MAX_SIDE
            || u64::from(stride) < row
        {
            return Err(ImageError::Size {
                width,
                height,
                stride,
            });
        }
        let expected = HEADER as u64 + u64::from(stride) * u64::from(height);
        if bytes.len() as u64 != expected {
            return Err(ImageError::Length {
                expected,
                found: bytes.len() as u64,
            });
        }
        // Within the file's length, which fits in memory, so in `usize`.
        let (stride, row) = (stride as usize, row as usize);
        let pixels = (0..height as usize)
            .flat_map(|y| {
                let start = HEADER + y * stride;
                bytes[start..start + row]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(opaque)
            })
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Draws the picture 1:1 from the top left of a `width` × `height`
    /// buffer, the fill around it should the window differ in size.
    fn draw(&self, buffer: &mut [u32], width: usize, height: usize) {
        buffer.fill(FILL);
        let columns = width.min(self.width as usize);
        let rows = height.min(self.height as usize);
        for (y, line) in self
            .pixels
            .chunks_exact(self.width as usize)
            .take(rows)
            .enumerate()
        {
            buffer[y * width..y * width + columns].copy_from_slice(&line[..columns]);
        }
    }
}

/// One RGBA pixel over the fill, as an opaque `0RGB` one.
fn opaque(&[red, green, blue, alpha]: &[u8; 4]) -> u32 {
    let alpha = u32::from(alpha);
    let channel = |value: u8, shift: u32| {
        let under = (FILL >> shift) & 0xFF;
        ((u32::from(value) * alpha + under * (255 - alpha) + 127) / 255) << shift
    };
    OPAQUE | channel(red, 16) | channel(green, 8) | channel(blue, 0)
}

/// The window that holds the cover. Tests stand in for it.
pub trait CoverWindow {
    /// Shows `image` 1:1 in a borderless window at `at`, above other
    /// windows and without taking the focus. Returns once it is on the
    /// screen, with whether it got there.
    fn show(&mut self, image: &Image, at: &WindowNote) -> bool;
    /// Takes the window down. Called once, at the end of every cover.
    fn close(&mut self);
}

/// Time, for the wait on `cover.taken`. Tests stand in for it.
pub trait Clock {
    /// Since the clock was made.
    fn elapsed(&self) -> Duration;
    fn sleep(&self, time: Duration);
}

pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&self, time: Duration) {
        thread::sleep(time);
    }
}

/// Holds the cover across an update restart: shows it, writes
/// `cover.shown`, runs `wait_for_app` (the old app's exit) and `start`
/// (the switch and the new app), then waits for `cover.taken` for up to
/// [`TAKEN_WAIT`] and closes the window. Without a picture or a place to
/// show it, it shows nothing and carries on, `cover.shown` included, so
/// the app doesn't wait. Returns what `start` returned.
pub fn hold(
    layout: &Layout,
    window: &mut dyn CoverWindow,
    clock: &dyn Clock,
    wait_for_app: impl FnOnce(),
    start: impl FnOnce() -> bool,
) -> bool {
    remove_stale_taken(layout);
    let shown = match what_to_show(layout) {
        Ok((image, at)) => {
            log::line(format!(
                "showing the cover: a {}×{} picture in a {}×{} window at ({}, {})",
                image.width, image.height, at.width, at.height, at.x, at.y
            ));
            window.show(&image, &at)
        }
        Err(error) => {
            log::line(format!("no cover: {error}"));
            false
        }
    };
    if let Err(error) = layout::write_whole(&layout.cover_shown(), b"") {
        // The app goes on after 3 s without it.
        log::line(format!("can't write cover.shown: {error}"));
    }
    wait_for_app();
    let started = start();
    if shown && started {
        wait_until_taken(layout, clock);
    }
    window.close();
    started
}

/// What `restart.json` and `cover.rgba` say to show, or why there's
/// nothing to.
fn what_to_show(layout: &Layout) -> Result<(Image, WindowNote), String> {
    let restart = layout
        .read_restart()
        .ok_or("restart.json is missing or can't be read")?;
    let at = restart.window.ok_or("restart.json gives no window")?;
    if at.width == 0 || at.height == 0 || at.width > MAX_SIDE || at.height > MAX_SIDE {
        return Err(format!("the window is {}×{}", at.width, at.height));
    }
    let image = Image::read(&layout.cover_image()).map_err(|error| error.to_string())?;
    Ok((image, at))
}

/// Removes a `cover.taken` an earlier restart left behind.
fn remove_stale_taken(layout: &Layout) {
    match fs::remove_file(layout.cover_taken()) {
        Ok(()) => log::line("removed an old cover.taken"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => log::line(format!("can't remove an old cover.taken: {error}")),
    }
}

/// Waits until the new app writes `cover.taken`, for up to
/// [`TAKEN_WAIT`].
fn wait_until_taken(layout: &Layout, clock: &dyn Clock) {
    let from = clock.elapsed();
    loop {
        let waited = clock.elapsed().saturating_sub(from);
        match layout.cover_taken().try_exists() {
            Ok(true) => {
                log::line(format!(
                    "the new app took the cover after {} ms",
                    waited.as_millis()
                ));
                return;
            }
            Ok(false) => {}
            // Its window can't be seen coming: closing now beats holding
            // a window over the new app that takes its clicks.
            Err(error) => {
                log::line(format!("can't look for cover.taken: {error}"));
                return;
            }
        }
        if waited >= TAKEN_WAIT {
            log::line(format!(
                "cover.taken didn't come in {} s: closing the cover",
                TAKEN_WAIT.as_secs()
            ));
            return;
        }
        clock.sleep(POLL_EVERY.min(TAKEN_WAIT - waited));
    }
}

/// Runs [`hold`] with a real window: the window's event loop on this
/// thread, which must be the main one (macOS), and the restart on another.
pub fn across_restart(
    layout: Layout,
    wait_for_app: impl FnOnce() + Send + 'static,
    start: impl FnOnce() -> bool + Send + 'static,
) -> bool {
    let event_loop = match EventLoop::<Command>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            log::line(format!("no cover window ({error})"));
            return hold(
                &layout,
                &mut NoWindow,
                &SystemClock::new(),
                wait_for_app,
                start,
            );
        }
    };
    let mut remote = Remote {
        proxy: event_loop.create_proxy(),
        closed: false,
    };
    let worker = thread::spawn(move || {
        hold(
            &layout,
            &mut remote,
            &SystemClock::new(),
            wait_for_app,
            start,
        )
    });
    let mut app = CoverApp::default();
    if let Err(error) = event_loop.run_app(&mut app) {
        log::line(format!("the cover's window ended: {error}"));
    }
    // The restart's panic is the launcher's.
    worker
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// Without a display: nothing to show.
struct NoWindow;

impl CoverWindow for NoWindow {
    fn show(&mut self, _: &Image, _: &WindowNote) -> bool {
        false
    }

    fn close(&mut self) {}
}

/// What the restart's thread asks of the window's.
enum Command {
    Show {
        image: Image,
        at: WindowNote,
        shown: mpsc::Sender<bool>,
    },
    Close,
}

/// The window, driven from the restart's thread.
struct Remote {
    proxy: EventLoopProxy<Command>,
    closed: bool,
}

impl CoverWindow for Remote {
    fn show(&mut self, image: &Image, at: &WindowNote) -> bool {
        let (shown, answer) = mpsc::channel();
        let command = Command::Show {
            image: image.clone(),
            at: at.clone(),
            shown,
        };
        if self.proxy.send_event(command).is_err() {
            log::line("the cover's window had already gone");
            return false;
        }
        match answer.recv_timeout(SHOW_WAIT) {
            Ok(shown) => shown,
            Err(error) => {
                log::line(format!("the cover's window didn't answer: {error}"));
                false
            }
        }
    }

    fn close(&mut self) {
        self.closed = true;
        if self.proxy.send_event(Command::Close).is_err() {
            log::line("the cover's window had already gone");
        }
    }
}

impl Drop for Remote {
    /// Ends the window's loop even if the restart panicked before
    /// [`CoverWindow::close`], so the launcher can end.
    fn drop(&mut self) {
        if !self.closed {
            self.close();
        }
    }
}

#[derive(Default)]
struct CoverApp {
    window: Option<Rc<Window>>,
    surface: Option<Surface>,
    image: Option<Image>,
}

impl CoverApp {
    /// Opens the window, draws the picture and only then shows it, so it
    /// never flashes empty.
    fn open(
        &mut self,
        event_loop: &ActiveEventLoop,
        image: Image,
        at: &WindowNote,
    ) -> Result<(), String> {
        let attributes = Window::default_attributes()
            .with_title("Opencord")
            .with_position(PhysicalPosition::new(at.x, at.y))
            .with_inner_size(PhysicalSize::new(at.width, at.height))
            .with_decorations(false)
            .with_resizable(false)
            .with_active(false)
            // Above whatever the system raises when the app's window
            // closes. Over the app's own window it changes nothing: both
            // show the same picture.
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_visible(false);
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("no window: {error}"))?,
        );
        let surface = splash::surface(&window).map_err(|error| format!("no surface: {error}"))?;
        self.window = Some(window.clone());
        self.surface = Some(surface);
        self.image = Some(image);
        // Drawn now, not on a requested redraw: Windows sends none to a
        // hidden window.
        self.redraw().map_err(|error| error.to_string())?;
        window.set_visible(true);
        Ok(())
    }

    fn redraw(&mut self) -> Result<(), PaintError> {
        let (Some(window), Some(surface), Some(image)) =
            (&self.window, &mut self.surface, &self.image)
        else {
            return Ok(());
        };
        splash::paint(window, surface, |buffer, width, height| {
            image.draw(buffer, width, height);
        })
    }

    fn close_window(&mut self) {
        self.surface = None;
        self.window = None;
    }
}

impl ApplicationHandler<Command> for CoverApp {
    fn resumed(&mut self, _: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, command: Command) {
        match command {
            Command::Show { image, at, shown } => {
                let opened = self.open(event_loop, image, &at);
                if let Err(error) = &opened {
                    log::line(format!("can't show the cover: {error}"));
                    self.close_window();
                }
                if shown.send(opened.is_ok()).is_err() {
                    log::line("the cover showed after the restart stopped waiting for it");
                }
            }
            Command::Close => {
                self.close_window();
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    log::line(format!("can't draw the cover: {error}"));
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            // Closed by hand (Alt+F4): the restart goes on without it.
            WindowEvent::CloseRequested => self.close_window(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    use opencord_update::restart::RestartNote;
    use semver::Version;

    use super::*;

    /// The image Task 15's Dart test writes the same bytes of: 3 × 2, with
    /// 16-byte rows (4 bytes of padding each) and a transparent last pixel.
    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cover.rgba")
    }

    const AT: WindowNote = WindowNote {
        x: 120,
        y: -40,
        width: 3,
        height: 2,
    };

    /// An install folder mid-restart: `restart.json` with the window, and
    /// the fixture as `cover.rgba`.
    fn restarting(dir: &Path) -> Layout {
        let layout = Layout::new(dir);
        layout
            .write_restart(&RestartNote {
                from: Version::new(0, 0, 1),
                to: Version::new(0, 0, 2),
                at: 1,
                voice: None,
                window: Some(AT),
                cover: true,
            })
            .unwrap();
        fs::copy(fixture(), layout.cover_image()).unwrap();
        layout
    }

    type Steps = Rc<RefCell<Vec<String>>>;

    /// Records what was shown and when, among the restart's steps.
    struct FakeWindow {
        steps: Steps,
        shown: Option<(Image, WindowNote)>,
    }

    impl FakeWindow {
        fn new(steps: &Steps) -> Self {
            Self {
                steps: steps.clone(),
                shown: None,
            }
        }
    }

    impl CoverWindow for FakeWindow {
        fn show(&mut self, image: &Image, at: &WindowNote) -> bool {
            self.steps.borrow_mut().push("show".into());
            self.shown = Some((image.clone(), at.clone()));
            true
        }

        fn close(&mut self) {
            self.steps.borrow_mut().push("close".into());
        }
    }

    /// Time that passes only as it is slept, and writes `cover.taken` at
    /// `taken_at` as the new app would.
    struct FakeClock {
        now: Cell<Duration>,
        taken_at: Option<(Duration, PathBuf)>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now: Cell::new(Duration::ZERO),
                taken_at: None,
            }
        }

        fn taking_at(time: Duration, layout: &Layout) -> Self {
            Self {
                taken_at: Some((time, layout.cover_taken())),
                ..Self::new()
            }
        }
    }

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.now.get()
        }

        fn sleep(&self, time: Duration) {
            self.now.set(self.now.get() + time);
            if let Some((at, path)) = &self.taken_at
                && self.now.get() >= *at
            {
                fs::write(path, b"").unwrap();
            }
        }
    }

    /// Runs a cover with steps recorded: the wait notes whether
    /// `cover.shown` was there, and the start succeeds.
    fn run_cover(layout: &Layout, clock: &FakeClock) -> (Vec<String>, Option<(Image, WindowNote)>) {
        let steps = Steps::default();
        let mut window = FakeWindow::new(&steps);
        let started = hold(
            layout,
            &mut window,
            clock,
            || {
                let shown = layout.cover_shown().is_file();
                steps
                    .borrow_mut()
                    .push(format!("wait (cover.shown {shown})"));
            },
            || {
                steps.borrow_mut().push("start".into());
                true
            },
        );
        assert!(started);
        let steps = steps.borrow().clone();
        (steps, window.shown)
    }

    // Fails if `hold` calls `wait_for_app` before `window.show`, or before
    // `layout::write_whole(&layout.cover_shown(), …)`: the old app waits
    // for `cover.shown` before it exits, so that order deadlocks.
    #[test]
    fn the_cover_is_shown_before_waiting_on_the_pid() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let clock = FakeClock::taking_at(Duration::ZERO, &layout);

        let (steps, shown) = run_cover(&layout, &clock);

        assert_eq!(steps, ["show", "wait (cover.shown true)", "start", "close"]);
        let (image, at) = shown.unwrap();
        assert_eq!(at, AT);
        assert_eq!(image, Image::read(&fixture()).unwrap());
    }

    // Fails if `wait_until_taken` doesn't look for `cover.taken` (it then
    // holds the cover the whole 30 s).
    #[test]
    fn cover_taken_closes_it() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let clock = FakeClock::taking_at(Duration::from_secs(2), &layout);

        let (steps, _) = run_cover(&layout, &clock);

        assert_eq!(steps.last().map(String::as_str), Some("close"));
        // Closed at the first look after the new app took it.
        assert!(clock.elapsed() >= Duration::from_secs(2));
        assert!(clock.elapsed() < Duration::from_secs(2) + POLL_EVERY * 2);
    }

    // Fails if `wait_until_taken` gives up before `TAKEN_WAIT` (or
    // `TAKEN_WAIT` isn't 30 s), and if it never gives up (the test hangs).
    #[test]
    fn without_cover_taken_it_closes_after_30_s() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let clock = FakeClock::new();

        let (steps, _) = run_cover(&layout, &clock);

        assert_eq!(clock.elapsed(), Duration::from_secs(30));
        assert_eq!(steps.iter().filter(|step| *step == "close").count(), 1);
        assert_eq!(steps.last().map(String::as_str), Some("close"));
    }

    // Fails if `hold` returns early without a picture (`cover.shown`
    // never written, and nothing started), and if it shows a broken one.
    #[test]
    fn a_broken_image_still_writes_cover_shown() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let mut broken = fs::read(fixture()).unwrap();
        broken.pop();
        fs::write(layout.cover_image(), broken).unwrap();
        let clock = FakeClock::new();

        let (steps, shown) = run_cover(&layout, &clock);

        assert_eq!(steps, ["wait (cover.shown true)", "start", "close"]);
        assert!(shown.is_none());
        // Nothing shown, so nothing to wait for.
        assert_eq!(clock.elapsed(), Duration::ZERO);

        // Missing geometry goes the same way.
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let mut restart = layout.read_restart().unwrap();
        restart.window = None;
        layout.write_restart(&restart).unwrap();

        let (steps, shown) = run_cover(&layout, &FakeClock::new());

        assert_eq!(steps, ["wait (cover.shown true)", "start", "close"]);
        assert!(shown.is_none());
    }

    // Fails if `Image::parse` steps from row to row by `width * 4` in
    // place of `stride`, and if `opaque` ignores alpha.
    #[test]
    fn the_image_header_is_read_with_its_stride() {
        let image = Image::read(&fixture()).unwrap();

        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(
            &image.pixels[..],
            [
                OPAQUE | 0xFF_0000,
                OPAQUE | 0x00_FF00,
                OPAQUE | 0x00_00FF,
                OPAQUE | 0xFF_FFFF,
                OPAQUE,
                // Transparent: the fill shows through.
                FILL,
            ]
        );

        // The other side: each header field is checked.
        let bytes = fs::read(fixture()).unwrap();
        let with = |at: usize, value: u32| {
            let mut bytes = bytes.clone();
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
            Image::parse(&bytes)
        };
        assert!(matches!(with(0, 0), Err(ImageError::NotACover)));
        assert!(matches!(with(4, 0), Err(ImageError::Size { .. })));
        assert!(matches!(
            with(8, MAX_SIDE + 1),
            Err(ImageError::Size { .. })
        ));
        // Rows shorter than three pixels.
        assert!(matches!(with(12, 11), Err(ImageError::Size { .. })));
        // Rows without their padding make the file too long.
        assert!(matches!(
            with(12, 12),
            Err(ImageError::Length {
                expected: 40,
                found: 48
            })
        ));
        assert!(matches!(
            Image::parse(&bytes[..HEADER - 1]),
            Err(ImageError::NotACover)
        ));
    }

    // Fails if `hold` doesn't remove a `cover.taken` left from an earlier
    // restart: this cover would close before the new app is up.
    #[test]
    fn an_old_cover_taken_doesnt_close_a_new_cover() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        fs::write(layout.cover_taken(), b"").unwrap();
        let clock = FakeClock::taking_at(Duration::from_secs(1), &layout);

        run_cover(&layout, &clock);

        assert!(clock.elapsed() >= Duration::from_secs(1));
    }

    // Fails if `hold` waits for `cover.taken` when nothing started: no
    // app will write it, and the cover would sit there 30 s.
    #[test]
    fn a_failed_start_closes_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let layout = restarting(dir.path());
        let steps = Steps::default();
        let clock = FakeClock::new();

        let started = hold(
            &layout,
            &mut FakeWindow::new(&steps),
            &clock,
            || {},
            || false,
        );

        assert!(!started);
        assert_eq!(clock.elapsed(), Duration::ZERO);
        assert_eq!(*steps.borrow(), ["show", "close"]);
    }

    // Fails if `Image::draw` scales or offsets the picture, or indexes the
    // buffer by the picture's width (a window of another size panics).
    #[test]
    fn the_picture_is_drawn_one_to_one_from_the_top_left() {
        let image = Image::read(&fixture()).unwrap();

        let mut larger = vec![0; 5 * 3];
        image.draw(&mut larger, 5, 3);
        assert_eq!(&larger[0..3], &image.pixels[0..3]);
        assert_eq!(&larger[5..8], &image.pixels[3..6]);
        assert!(larger[3..5].iter().all(|pixel| *pixel == FILL));
        assert!(larger[10..].iter().all(|pixel| *pixel == FILL));

        let mut smaller = vec![0; 2];
        image.draw(&mut smaller, 2, 1);
        assert_eq!(smaller, image.pixels[0..2]);
    }
}
