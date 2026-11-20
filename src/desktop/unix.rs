use crate::{
    files::{self, Page},
    raster::Bitmap,
};
#[cfg(target_os = "linux")]
use std::{env, fs, path::Path};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn filter(program: &str, args: &[&str], bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut input = child.stdin.take().expect("piped input");
    let (result, write) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || input.write_all(bytes));
        (
            child.wait_with_output(),
            writer.join().expect("writer thread"),
        )
    });
    let output = result.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    write.map_err(|e| e.to_string())?;
    Ok(output.stdout)
}

#[cfg(target_os = "linux")]
mod linux_clipboard {
    use super::*;
    pub fn clipboard_put(image: &Bitmap) {
        if let Ok(payload) = files::encode_clipboard(image) {
            clipboard_write(&payload, Some("image/png"));
        }
    }

    fn clipboard_write(payload: &[u8], mime: Option<&str>) {
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            let args = mime.map(|mime| vec!["--type", mime]).unwrap_or_default();
            if detached_input("wl-copy", &args, payload) {
                return;
            }
        }
        let mut args = vec!["-selection", "clipboard", "-i"];
        if let Some(mime) = mime {
            args.extend(["-t", mime]);
        }
        detached_input("xclip", &args, payload);
    }

    fn detached_input(program: &str, args: &[&str], payload: &[u8]) -> bool {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let written = child
            .stdin
            .take()
            .is_some_and(|mut input| input.write_all(payload).is_ok());
        let status = child.wait();
        written && status.is_ok_and(|status| status.success())
    }

    pub fn clipboard_get() -> Option<Bitmap> {
        let mut commands = vec![(
            "xclip",
            vec!["-selection", "clipboard", "-t", "image/png", "-o"],
        )];
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            commands.insert(0, ("wl-paste", vec!["--no-newline", "--type", "image/png"]));
        }
        for (program, args) in commands {
            if let Ok(output) = Command::new("timeout")
                .args(["0.7", program])
                .args(args)
                .output()
                && output.status.success()
                && let Ok(image) = files::decode_clipboard(&output.stdout)
            {
                return Some(image);
            }
        }
        None
    }

    pub fn clipboard_text_put(text: &str) {
        if !text.is_empty() {
            clipboard_write(text.as_bytes(), None);
        }
    }

    pub fn clipboard_text_get() -> Option<String> {
        let mut commands = vec![("xclip", vec!["-selection", "clipboard", "-o"])];
        if env::var_os("WAYLAND_DISPLAY").is_some() {
            commands.insert(0, ("wl-paste", vec!["--no-newline"]));
        }
        for (program, args) in commands {
            if let Ok(output) = Command::new("timeout")
                .args(["0.7", program])
                .args(args)
                .output()
                && output.status.success()
                && let Ok(value) = String::from_utf8(output.stdout)
            {
                return Some(value);
            }
        }
        None
    }
}

#[cfg(target_os = "linux")]
pub use linux_clipboard::{clipboard_get, clipboard_put, clipboard_text_get, clipboard_text_put};

pub fn print(image: &Bitmap, page: &Page, printer: &str, copies: u16) -> Result<(), String> {
    let copies_arg = format!("-#{copies}");
    let printer_arg = format!("-P{printer}");
    let media = if page.a4 { "media=A4" } else { "media=Letter" };
    let mut args = vec![copies_arg.as_str(), "-o", media];
    if !printer.is_empty() {
        args.push(printer_arg.as_str());
    }
    filter("lpr", &args, &files::pdf(image, page)?).map(|_| ())
}

#[cfg(target_os = "linux")]
pub fn wallpaper(image: &Bitmap, tiled: bool) -> Result<(), String> {
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("GNOME"))
    {
        return Err("Wallpaper commands require a GNOME desktop.".into());
    }
    let folder = super::data_dir().ok_or("Could not find the desktop data directory.")?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("wallpaper.png");
    files::write_atomic(&path, &files::encode(image, files::Format::Png)?)?;
    let path = url_path(&path);
    let quote = format!("'file://{path}'");
    for (key, value) in [
        ("picture-uri", quote.as_str()),
        (
            "picture-options",
            if tiled { "'wallpaper'" } else { "'centered'" },
        ),
    ] {
        filter(
            "gsettings",
            &["set", "org.gnome.desktop.background", key, value],
            &[],
        )?;
    }
    let _ = filter(
        "gsettings",
        &[
            "set",
            "org.gnome.desktop.background",
            "picture-uri-dark",
            &quote,
        ],
        &[],
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn url_path(path: &Path) -> String {
    let mut result = String::new();
    for byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(byte) {
            result.push(char::from(*byte));
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}
