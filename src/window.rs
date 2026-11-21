use crate::app::{self, Paint, WindowAction};
use crate::raster::{BLACK, Bitmap, p};
use crate::ui;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Fullscreen, Icon, Window, WindowId};

struct Platform {
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    window: Rc<Window>,
    frame: Bitmap,
    scale: u32,
    size: PhysicalSize<u32>,
    title: String,
    full_screen: bool,
    #[cfg(target_os = "macos")]
    resize: Option<MacResize>,
}

impl Platform {
    fn new(event_loop: &ActiveEventLoop, title: String) -> Result<Self, String> {
        let scale = std::env::var("PAINTRS_SCALE")
            .ok()
            .and_then(|raw| raw.parse::<u32>().ok())
            .filter(|n| (1..=3).contains(n))
            .unwrap_or_else(|| {
                event_loop.primary_monitor().map_or(1, |m| {
                    let size = m.size();
                    let scale = m.scale_factor().round().clamp(1.0, 3.0) as u32;
                    if size.width < 800 * scale || size.height < 560 * scale {
                        1
                    } else {
                        scale
                    }
                })
            });
        let fit = event_loop.primary_monitor().map(|m| m.size());
        let desired = PhysicalSize::new(
            fit.map_or(1000 * scale, |m| {
                (1000 * scale)
                    .min(m.width.saturating_sub(44))
                    .max(600 * scale)
            }),
            fit.map_or(700 * scale, |m| {
                (700 * scale)
                    .min(m.height.saturating_sub(85))
                    .max(475 * scale)
            }),
        );
        let options = Window::default_attributes()
            .with_title(&title)
            .with_decorations(false)
            .with_inner_size(desired)
            .with_min_inner_size(PhysicalSize::new(600 * scale, 475 * scale))
            .with_window_icon(Icon::from_rgba(ui::icon(), 16, 16).ok());
        let window = Rc::new(
            event_loop
                .create_window(options)
                .map_err(|e| e.to_string())?,
        );
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface =
            softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        let size = window.inner_size();
        Ok(Self {
            surface,
            window,
            frame: Bitmap::new(1, 1, BLACK),
            scale,
            size,
            title,
            full_screen: false,
            #[cfg(target_os = "macos")]
            resize: None,
        })
    }

    fn render(&mut self, app: &mut Paint) -> Result<(), softbuffer::SoftBufferError> {
        let (Some(w), Some(h)) = (
            NonZeroU32::new(self.size.width),
            NonZeroU32::new(self.size.height),
        ) else {
            return Ok(());
        };
        self.surface.resize(w, h)?;
        let width = self.size.width.div_ceil(self.scale) as i32;
        let height = self.size.height.div_ceil(self.scale) as i32;
        if (width, height) != (self.frame.w, self.frame.h) {
            self.frame = Bitmap::new(width, height, BLACK);
            app.dims = p(width, height);
            app.constrain_scroll();
        }
        app.maximized = self.window.is_maximized();
        ui::render(app, &mut self.frame);
        let mut displayed = self.surface.buffer_mut()?;
        if self.scale == 1 {
            displayed.copy_from_slice(&self.frame.pixels);
        } else {
            let stride = width as usize;
            let scale = self.scale as usize;
            for (y, row) in displayed
                .chunks_exact_mut(self.size.width as usize)
                .enumerate()
            {
                let start = (y / scale) * stride;
                for (x, target) in row.iter_mut().enumerate() {
                    *target = self.frame.pixels[start + x / scale];
                }
            }
        }
        self.window.pre_present_notify();
        displayed.present()
    }

    fn synchronize(&mut self, app: &mut Paint, event_loop: &ActiveEventLoop) {
        let title = app.title();
        if self.title != title {
            self.window.set_title(&title);
            self.title = title;
        }
        if self.full_screen != app.viewing {
            self.full_screen = app.viewing;
            self.window
                .set_fullscreen(app.viewing.then_some(Fullscreen::Borderless(None)));
        }
        self.window.set_cursor(app.cursor());
        if let Some(action) = app.window_action.take() {
            match action {
                WindowAction::Drag => {
                    let _ = self.window.drag_window();
                }
                WindowAction::Resize(direction) => {
                    #[cfg(not(target_os = "macos"))]
                    let _ = self.window.drag_resize_window(direction);
                    #[cfg(target_os = "macos")]
                    {
                        self.resize = MacResize::start(&self.window, direction);
                    }
                }
                WindowAction::Minimize => self.window.set_minimized(true),
                WindowAction::Maximize => self.window.set_maximized(!self.window.is_maximized()),
                WindowAction::Exit => event_loop.exit(),
            }
        }
    }
}

#[cfg(target_os = "macos")]
struct MacResize {
    direction: winit::window::ResizeDirection,
    origin: winit::dpi::LogicalPosition<f64>,
    size: winit::dpi::LogicalSize<f64>,
    start: (f64, f64),
}

#[cfg(target_os = "macos")]
impl MacResize {
    fn pointer() -> (f64, f64) {
        // AppKit reports screen coordinates with an upward-pointing y axis.
        let location = unsafe { objc2_app_kit::NSEvent::mouseLocation() };
        (location.x, -location.y)
    }

    fn start(window: &Window, direction: winit::window::ResizeDirection) -> Option<Self> {
        let dpi = window.scale_factor();
        Some(Self {
            direction,
            origin: window.outer_position().ok()?.to_logical(dpi),
            size: window.inner_size().to_logical(dpi),
            start: Self::pointer(),
        })
    }

    fn update(&self, window: &Window, ui_scale: u32) {
        use winit::dpi::{LogicalPosition, LogicalSize};
        use winit::window::ResizeDirection::*;
        let (x, y) = Self::pointer();
        let (dx, dy) = (x - self.start.0, y - self.start.1);
        let (mut width, mut height) = (self.size.width, self.size.height);
        let (mut left, mut top) = (self.origin.x, self.origin.y);
        let minimum = f64::from(ui_scale) / window.scale_factor();
        if matches!(self.direction, East | NorthEast | SouthEast) {
            width = (width + dx).max(600.0 * minimum);
        } else if matches!(self.direction, West | NorthWest | SouthWest) {
            width = (width - dx).max(600.0 * minimum);
            left += self.size.width - width;
        }
        if matches!(self.direction, South | SouthWest | SouthEast) {
            height = (height + dy).max(475.0 * minimum);
        } else if matches!(self.direction, North | NorthWest | NorthEast) {
            height = (height - dy).max(475.0 * minimum);
            top += self.size.height - height;
        }
        let _ = window.request_inner_size(LogicalSize::new(width, height));
        window.set_outer_position(LogicalPosition::new(left, top));
    }
}

struct Desktop {
    app: Paint,
    platform: Option<Platform>,
    error: Option<String>,
}

impl ApplicationHandler for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.platform.is_none() {
            match Platform::new(event_loop, self.app.title()) {
                Ok(platform) => {
                    platform.window.request_redraw();
                    self.platform = Some(platform);
                }
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window: WindowId, event: WindowEvent) {
        let Some(platform) = &mut self.platform else {
            return;
        };
        if platform.window.id() != window {
            return;
        }

        match event {
            WindowEvent::RedrawRequested => {
                if let Err(error) = platform.render(&mut self.app) {
                    self.error = Some(error.to_string());
                    event_loop.exit();
                }
                platform.window.set_cursor(self.app.cursor());
                return;
            }
            WindowEvent::Resized(size) => platform.size = size,
            WindowEvent::Focused(focus) => {
                self.app.focused = focus;
                if !focus {
                    self.app.lost_focus();
                    #[cfg(target_os = "macos")]
                    {
                        platform.resize = None;
                    }
                }
            }
            WindowEvent::CloseRequested => self.app.invoke(app::Command::Exit),
            WindowEvent::CursorMoved { position, .. } => {
                #[cfg(target_os = "macos")]
                if let Some(resize) = &platform.resize {
                    resize.update(&platform.window, platform.scale);
                }
                let scale = f64::from(platform.scale);
                self.app.pointer_moved(p(
                    (position.x / scale).floor() as i32,
                    (position.y / scale).floor() as i32,
                ));
            }
            WindowEvent::CursorLeft { .. } if self.app.editor.gesture.is_none() => {
                self.app.pointer_moved(p(-1, -1))
            }
            WindowEvent::MouseInput {
                button: button @ (MouseButton::Left | MouseButton::Right),
                state,
                ..
            } => {
                let right = button == MouseButton::Right;
                match state {
                    ElementState::Pressed => self.app.pointer_down(right),
                    ElementState::Released => {
                        self.app.pointer_up(right);
                        #[cfg(target_os = "macos")]
                        if !right {
                            platform.resize = None;
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x.round() as i32, y.round() as i32),
                    MouseScrollDelta::PixelDelta(delta) => (
                        (delta.x / 24.0).round() as i32,
                        (delta.y / 24.0).round() as i32,
                    ),
                };
                self.app.wheel(x, y);
            }
            WindowEvent::ModifiersChanged(modifiers) => self.app.mods = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.app.key(&event.logical_key, event.text.as_deref());
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.app.ime.clear();
                self.app.text_input(&text);
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.app.ime = text,
            WindowEvent::DroppedFile(path) => self.app.dropped(path),
            _ => return,
        }
        let accepts_text = self.app.editor.text.is_some()
            || matches!(
                self.app.dialog,
                Some(app::Dialog::File(_) | app::Dialog::Form(_))
            );
        if accepts_text && self.app.ime.is_empty() {
            platform.window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(
                    self.app.pointer.x * platform.scale as i32,
                    self.app.pointer.y * platform.scale as i32,
                ),
                PhysicalSize::new(1, 18 * platform.scale),
            );
        }
        platform.window.set_ime_allowed(accepts_text);
        platform.synchronize(&mut self.app, event_loop);
        platform.window.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.animate(Instant::now())
            && let Some(platform) = &self.platform
        {
            platform.window.request_redraw();
        }
        event_loop.set_control_flow(
            self.app
                .animation_deadline()
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next();
    if path
        .as_deref()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!(
            "Paintrs {}\nUsage: paintrs [IMAGE]",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    if args.next().is_some() {
        return Err("Usage: paintrs [IMAGE]".into());
    }
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        // Keep quit routed through Paint's unsaved-changes prompt.
        builder.with_default_menu(false);
    }
    let event_loop = builder.build().map_err(|error| error.to_string())?;
    let mut app = Paint::new();
    if let Some(path) = path {
        app.open_initial(PathBuf::from(path));
    }
    let mut desktop = Desktop {
        app,
        platform: None,
        error: None,
    };
    event_loop
        .run_app(&mut desktop)
        .map_err(|error| error.to_string())?;
    desktop.error.map_or(Ok(()), Err)
}

/// Runs the standalone Paintrs command with the process arguments.
pub fn run_cli() {
    if let Err(error) = run() {
        eprintln!("Paintrs: {error}");
        #[cfg(all(windows, not(debug_assertions)))]
        crate::desktop::startup_error(&error);
        std::process::exit(1);
    }
}
