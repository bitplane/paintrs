//! Paintrs bitmap editor and automation interface.

mod app;
mod desktop;
mod editor;
mod files;
mod raster;
mod ui;
mod window;

pub mod automation;
pub use window::run_cli;

#[cfg(test)]
mod tests;
