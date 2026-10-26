//! The splash: a small borderless window saying "Opencord", with what the
//! launcher is doing under it and, while downloading, a progress bar. The
//! same on every system; drawn by hand into a pixel buffer, so it needs
//! no GPU and no toolkit. Escape closes it; dragging moves it.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::update::Status;

const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 180.0;

// The app's dark theme (app/lib/ui/theme/oc_colors.dart).
const BACKGROUND: u32 = 0x0E0E10;
const TEXT: u32 = 0xEDEDEF;
const TEXT_SECONDARY: u32 = 0xA1A1A8;
const TRACK: u32 = 0x27272C;
const BORDER: u32 = 0x222226;

const TITLE_FONT: &[u8] = include_bytes!("../../../app/assets/fonts/Inter-SemiBold.ttf");
const BODY_FONT: &[u8] = include_bytes!("../../../app/assets/fonts/Inter-Regular.ttf");

enum Message {
    Status(Status),
    Done,
}

/// Shows the splash while `work` runs on another thread, then returns
/// what it returned. Without a display (a terminal session), `work` runs
/// here, its statuses going to the log.
pub fn show<T: Send + 'static>(work: impl FnOnce(&dyn Fn(Status)) -> T + Send + 'static) -> T {
    let event_loop = match EventLoop::<Message>::with_user_event().build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            crate::log::line(format!("no splash ({error})"));
            return work(&|status| crate::log::line(format!("{status:?}")));
        }
    };
    let proxy = event_loop.create_proxy();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = {
        let proxy: EventLoopProxy<Message> = proxy.clone();
        thread::spawn(move || {
            let report = |status: Status| {
                let _ = proxy.send_event(Message::Status(status));
            };
            let result = work(&report);
            let _ = result_tx.send(result);
            let _ = proxy.send_event(Message::Done);
        })
    };
    let mut splash = Splash::new();
    if let Err(error) = event_loop.run_app(&mut splash) {
        crate::log::line(format!("the splash ended: {error}"));
    }
    if splash.closed_by_user {
        // The work may be mid-download: it is left, and tidied next time.
        std::process::exit(0);
    }
    let _ = worker.join();
    result_rx
        .recv()
        .expect("the launcher's work ended without a result")
}

struct Splash {
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    status: Status,
    fonts: Fonts,
    shown: bool,
    closed_by_user: bool,
}

impl Splash {
    fn new() -> Self {
        Self {
            window: None,
            surface: None,
            status: Status::Checking,
            fonts: Fonts::new(),
            shown: false,
            closed_by_user: false,
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if surface.resize(width, height).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        draw(
            &mut buffer,
            size.width as usize,
            size.height as usize,
            window.scale_factor() as f32,
            &self.status,
            &self.fonts,
        );
        let _ = buffer.present();
        // Shown once drawn, so it never flashes white first.
        if !self.shown {
            self.shown = true;
            window.set_visible(true);
        }
    }
}

impl ApplicationHandler<Message> for Splash {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_title("Opencord")
            .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
            .with_resizable(false)
            .with_decorations(false)
            .with_visible(false);
        if let Some(monitor) = event_loop.primary_monitor() {
            let scale = monitor.scale_factor();
            let area = monitor.size();
            let origin = monitor.position();
            let x = origin.x + (area.width as i32 - (WIDTH * scale) as i32) / 2;
            let y = origin.y + (area.height as i32 - (HEIGHT * scale) as i32) / 2;
            attributes = attributes.with_position(PhysicalPosition::new(x, y));
        }
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(error) => {
                crate::log::line(format!("no splash window: {error}"));
                return;
            }
        };
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        match surface {
            Ok(surface) => self.surface = Some(surface),
            Err(error) => crate::log::line(format!("no splash surface: {error}")),
        }
        self.window = Some(window);
        // Drawn now, not on a requested redraw: Windows sends none to a
        // hidden window (no WM_PAINT), so it would never be shown. Once
        // shown, it is painted again as usual.
        self.redraw();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, message: Message) {
        match message {
            Message::Status(status) => {
                self.status = status;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Message::Done => event_loop.exit(),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.closed_by_user = true;
                event_loop.exit();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(window) = &self.window {
                    let _ = window.drag_window();
                }
            }
            _ => {}
        }
    }
}

struct Fonts {
    title: FontRef<'static>,
    body: FontRef<'static>,
}

impl Fonts {
    fn new() -> Self {
        Self {
            title: FontRef::try_from_slice(TITLE_FONT).expect("Inter SemiBold reads"),
            body: FontRef::try_from_slice(BODY_FONT).expect("Inter Regular reads"),
        }
    }
}

/// What the splash says under the name.
pub fn caption(status: &Status) -> String {
    match status {
        Status::Installing => "Installing…".to_owned(),
        Status::Checking => "Checking for updates…".to_owned(),
        Status::Downloading(done, total) if *total > 0 => {
            format!("Downloading update… {}%", done * 100 / total)
        }
        Status::Downloading(..) => "Downloading update…".to_owned(),
        Status::Unpacking => "Installing update…".to_owned(),
        Status::Starting => "Starting…".to_owned(),
        Status::Retrying { offline: true } => "Can't reach GitHub. Trying again…".to_owned(),
        Status::Retrying { offline: false } => {
            "Can't install Opencord yet. Trying again…".to_owned()
        }
    }
}

/// Draws the splash into `buffer` (`0RGB` pixels, `width` × `height`) at
/// `scale` physical pixels per logical one.
fn draw(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    scale: f32,
    status: &Status,
    fonts: &Fonts,
) {
    buffer.fill(BACKGROUND);
    let mut canvas = Canvas {
        buffer,
        width,
        height,
    };
    // A hairline edge, as the window has no frame of its own.
    let edge = scale.round().max(1.0) as usize;
    canvas.rect(0, 0, width, edge, BORDER);
    canvas.rect(0, height.saturating_sub(edge), width, edge, BORDER);
    canvas.rect(0, 0, edge, height, BORDER);
    canvas.rect(width.saturating_sub(edge), 0, edge, height, BORDER);

    let centre = width as f32 / 2.0;
    let middle = height as f32 / 2.0;
    canvas.text(
        &fonts.title,
        30.0 * scale,
        "Opencord",
        centre,
        middle - 8.0 * scale,
        TEXT,
    );
    canvas.text(
        &fonts.body,
        13.0 * scale,
        &caption(status),
        centre,
        middle + 22.0 * scale,
        TEXT_SECONDARY,
    );

    if let Status::Downloading(done, total) = status {
        let bar_width = (160.0 * scale) as usize;
        let bar_height = (3.0 * scale).max(2.0) as usize;
        let x = (centre as usize).saturating_sub(bar_width / 2);
        let y = (middle + 40.0 * scale) as usize;
        canvas.rect(x, y, bar_width, bar_height, TRACK);
        let filled = if *total == 0 {
            0
        } else {
            (bar_width as u64 * (*done).min(*total) / total) as usize
        };
        canvas.rect(x, y, filled, bar_height, TEXT);
    }
}

struct Canvas<'a> {
    buffer: &'a mut [u32],
    width: usize,
    height: usize,
}

impl Canvas<'_> {
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for row in y..(y + h).min(self.height) {
            let start = row * self.width + x.min(self.width);
            let end = row * self.width + (x + w).min(self.width);
            self.buffer[start..end].fill(color);
        }
    }

    /// Draws one line of `text` centred on `centre`, its baseline at
    /// `baseline`.
    fn text(
        &mut self,
        font: &FontRef<'_>,
        size: f32,
        text: &str,
        centre: f32,
        baseline: f32,
        color: u32,
    ) {
        let font = font.as_scaled(PxScale::from(size));
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        let mut previous = None;
        for c in text.chars() {
            let id = font.glyph_id(c);
            if let Some(previous) = previous {
                x += font.kern(previous, id);
            }
            glyphs.push(id.with_scale_and_position(size, ab_glyph::point(x, 0.0)));
            x += font.h_advance(id);
            previous = Some(id);
        }
        let left = (centre - x / 2.0).round();
        for mut glyph in glyphs {
            glyph.position.x += left;
            glyph.position.y += baseline.round();
            let Some(outline) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px = bounds.min.x as i64 + gx as i64;
                let py = bounds.min.y as i64 + gy as i64;
                if px < 0 || py < 0 || px >= self.width as i64 || py >= self.height as i64 {
                    return;
                }
                let index = py as usize * self.width + px as usize;
                self.buffer[index] = blend(self.buffer[index], color, coverage);
            });
        }
    }
}

fn blend(under: u32, over: u32, alpha: f32) -> u32 {
    let alpha = alpha.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let a = ((under >> shift) & 0xFF) as f32;
        let b = ((over >> shift) & 0xFF) as f32;
        ((a + (b - a) * alpha).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captions_say_what_is_happening() {
        assert_eq!(caption(&Status::Checking), "Checking for updates…");
        assert_eq!(
            caption(&Status::Downloading(50, 200)),
            "Downloading update… 25%"
        );
        assert_eq!(caption(&Status::Downloading(0, 0)), "Downloading update…");
        assert_eq!(
            caption(&Status::Retrying { offline: true }),
            "Can't reach GitHub. Trying again…"
        );
    }

    #[test]
    fn the_name_and_the_bar_are_drawn() {
        let fonts = Fonts::new();
        let (width, height) = (600, 360);
        let mut buffer = vec![0; width * height];

        draw(
            &mut buffer,
            width,
            height,
            2.0,
            &Status::Downloading(1, 2),
            &fonts,
        );

        // Text in the upper middle.
        let text_rows = &buffer[(height / 2 - 60) * width..(height / 2) * width];
        assert!(text_rows.contains(&TEXT));
        // The bar: half filled, half track.
        let bar_row = (height / 2 + 81) * width;
        let row = &buffer[bar_row..bar_row + width];
        assert!(row.contains(&TEXT) && row.contains(&TRACK));
        // The rest is the background.
        assert_eq!(buffer[10 * width + 10], BACKGROUND);
    }

    #[test]
    fn a_tiny_window_draws_without_panicking() {
        let fonts = Fonts::new();
        let mut buffer = vec![0; 3];
        draw(&mut buffer, 3, 1, 1.0, &Status::Downloading(5, 10), &fonts);
    }

    #[test]
    fn blending_goes_from_under_to_over() {
        assert_eq!(blend(0x000000, 0xFFFFFF, 0.0), 0x000000);
        assert_eq!(blend(0x000000, 0xFFFFFF, 1.0), 0xFFFFFF);
        assert_eq!(blend(0x000000, 0x0000FF, 0.5), 0x000080);
    }
}
