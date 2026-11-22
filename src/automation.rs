//! Drive the normal Paintrs interface with window-local input and a software framebuffer.

use crate::app::Paint;
use crate::files::{self, Format};
use crate::raster::{BLACK, Bitmap, WHITE, p};
use crate::ui;
use std::path::Path;
use std::time::Instant;
use winit::keyboard::{Key, ModifiersState};
use winit::window::CursorIcon;

pub use winit::keyboard::NamedKey;

/// The modifier keys applied to an automated keypress.
pub use winit::keyboard::ModifiersState as Modifiers;

/// A single editor session with no host clipboard, filesystem dialogs or desktop actions.
pub struct Session {
    paint: Paint,
    frame: Bitmap,
}

impl Session {
    /// Create a drawing session at the given UI size in pixels.
    pub fn new(width: u32, height: u32) -> Result<Self, &'static str> {
        if !(600..=4096).contains(&width) || !(475..=4096).contains(&height) {
            return Err("Unsupported Paintrs window dimensions.");
        }
        let mut paint = Paint::with_desktop(false);
        paint.dims = p(width as i32, height as i32);
        let mut session = Self {
            paint,
            frame: Bitmap::new(width as i32, height as i32, BLACK),
        };
        session.render();
        Ok(session)
    }

    /// Move the pointer to a position in window coordinates.
    pub fn move_pointer(&mut self, x: i32, y: i32) {
        let point = p(x, y);
        self.paint
            .pointer_moved(if self.frame.bounds().contains(point) {
                point
            } else {
                p(-1, -1)
            });
    }

    /// Press or release a mouse button.
    pub fn button(&mut self, right: bool, pressed: bool) {
        if pressed {
            self.paint.pointer_down(right);
        } else {
            self.paint.pointer_up(right);
        }
    }

    /// Press a mouse button with an explicit single- or double-click classification.
    ///
    /// Pass `true` for `double_click` to send one press treated as the second press
    /// of a double-click. This bypasses timing-based classification. Release the
    /// button with `button(right, false)`.
    pub fn press(&mut self, right: bool, double_click: bool) {
        self.paint
            .pointer_down_with_click(right, Some(double_click));
    }

    /// Scroll by the given number of steps.
    pub fn scroll(&mut self, x: i32, y: i32) {
        self.paint.wheel(x.clamp(-4096, 4096), y.clamp(-4096, 4096));
    }

    /// Send a keyboard key and its modifiers.
    pub fn key(&mut self, key: Key, modifiers: Modifiers) {
        self.paint.mods = modifiers;
        self.paint.key(&key, key.to_text());
        self.paint.mods = ModifiersState::empty();
    }

    /// Insert text in the editor's focused input.
    pub fn text(&mut self, text: &str) {
        self.paint.text_input(text);
    }

    /// Advance timed editor effects.
    pub fn animate(&mut self, now: Instant) -> bool {
        self.paint.animate(now)
    }

    /// Draw the window and update hit targets. Pixels use the format 0x00RRGGBB.
    pub fn render(&mut self) -> &[u32] {
        ui::render(&mut self.paint, &mut self.frame);
        let at = self.paint.pointer;
        if self.frame.bounds().contains(at) {
            match self.paint.cursor() {
                CursorIcon::Crosshair | CursorIcon::Cell => {
                    for offset in -6i32..=6 {
                        let color = if offset.abs() >= 5 { WHITE } else { BLACK };
                        self.frame.put(p(at.x + offset, at.y), color);
                        self.frame.put(p(at.x, at.y + offset), color);
                    }
                    self.frame.put(at, WHITE);
                }
                _ => {
                    for (y, row) in [
                        "X",
                        "XX",
                        "X.X",
                        "X..X",
                        "X...X",
                        "X....X",
                        "X.....X",
                        "X......X",
                        "X.......X",
                        "X....XXXXX",
                        "X..X.X",
                        "X.X X.X",
                        "XX  X.X",
                        "X    XX",
                    ]
                    .iter()
                    .enumerate()
                    {
                        for (x, symbol) in row.bytes().enumerate() {
                            let color = match symbol {
                                b'X' => BLACK,
                                b'.' => WHITE,
                                _ => continue,
                            };
                            self.frame.put(p(at.x + x as i32, at.y + y as i32), color);
                        }
                    }
                }
            }
        }
        &self.frame.pixels
    }

    /// Return the last rendered frame in 0x00RRGGBB format.
    pub fn pixels(&self) -> &[u32] {
        &self.frame.pixels
    }

    /// Encode the last rendered frame as PNG.
    pub fn screenshot(&self) -> Result<Vec<u8>, String> {
        files::encode(&self.frame, Format::Png)
    }

    /// Commit pending drawing operations and save the picture as PNG.
    pub fn finish(&mut self, path: &Path) -> Result<(), String> {
        self.paint.editor.settle(&mut self.paint.fonts);
        let png = files::encode(self.paint.editor.bitmap(), Format::Png)?;
        files::write_atomic(path, &png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Command;

    #[test]
    fn drawing_uses_window_coordinates_and_produces_a_png() {
        let mut session = Session::new(1000, 700).unwrap();
        assert!(!session.paint.enabled(Command::Open));
        assert!(!session.paint.enabled(Command::SaveAs));
        assert!(!session.paint.enabled(Command::Print));

        session.move_pointer(22, 151);
        session.button(false, true);
        session.button(false, false);
        session.render();
        session.move_pointer(180, 145);
        session.button(false, true);
        session.move_pointer(280, 100);
        session.button(false, false);
        session.render();

        let start = session.paint.pixel(p(180, 145));
        assert_eq!(session.paint.editor.bitmap().get(start), BLACK);
        assert!(
            session
                .screenshot()
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
    }

    #[test]
    fn explicit_presses_close_a_polygon_without_starting_another() {
        let mut session = Session::new(1000, 700).unwrap();
        session
            .paint
            .editor
            .set_tool(crate::editor::Tool::Polygon, &mut session.paint.fonts);
        session.paint.editor.fill_mode = crate::raster::FillMode::Solid;
        session.render();

        for (x, y, double) in [
            (170, 110, false),
            (280, 110, false),
            (225, 209, false),
            (225, 209, true),
            (340, 110, false),
            (450, 110, false),
            (395, 209, true),
        ] {
            session.move_pointer(x, y);
            session.press(false, double);
            session.button(false, false);
        }
        for (x, y) in [(225, 150), (395, 150)] {
            assert_eq!(
                session
                    .paint
                    .editor
                    .bitmap()
                    .get(session.paint.pixel(p(x, y))),
                BLACK
            );
        }
    }
}
