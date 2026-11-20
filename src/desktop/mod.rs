use std::path::PathBuf;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::print;
#[cfg(target_os = "linux")]
pub use unix::{clipboard_get, clipboard_put, clipboard_text_get, clipboard_text_put, wallpaper};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

pub fn data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("paintrs"))
}
