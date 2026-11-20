use crate::raster::*;
use flate2::{Compression, write::ZlibEncoder};
use image::{ImageFormat, ImageReader, Limits, RgbImage, RgbaImage};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bitmap,
    Indexed8,
    Indexed4,
    Monochrome,
    Png,
    Jpeg,
    Gif,
    Tiff,
    Icon,
}

impl Format {
    pub const ALL: [Self; 9] = [
        Self::Bitmap,
        Self::Indexed8,
        Self::Indexed4,
        Self::Monochrome,
        Self::Png,
        Self::Jpeg,
        Self::Gif,
        Self::Tiff,
        Self::Icon,
    ];
    pub fn extension(self) -> &'static str {
        match self {
            Self::Bitmap | Self::Indexed8 | Self::Indexed4 | Self::Monochrome => "bmp",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Tiff => "tif",
            Self::Icon => "ico",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Bitmap => "24-bit Bitmap (*.bmp)",
            Self::Indexed8 => "256 Color Bitmap (*.bmp)",
            Self::Indexed4 => "16 Color Bitmap (*.bmp)",
            Self::Monochrome => "Monochrome Bitmap (*.bmp)",
            Self::Png => "PNG Image (*.png)",
            Self::Jpeg => "JPEG Image (*.jpg)",
            Self::Gif => "GIF Image (*.gif)",
            Self::Tiff => "TIFF Image (*.tif)",
            Self::Icon => "Icon (*.ico)",
        }
    }
    pub fn from_path(path: &Path) -> Option<Self> {
        Some(
            match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
                "bmp" | "dib" => Self::Bitmap,
                "png" => Self::Png,
                "jpg" | "jpeg" | "jpe" => Self::Jpeg,
                "gif" => Self::Gif,
                "tiff" | "tif" => Self::Tiff,
                "ico" => Self::Icon,
                _ => return None,
            },
        )
    }
}

fn read_native(bytes: &[u8], keep_transparent: bool) -> Result<Bitmap, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE as u32);
    limits.max_image_height = Some(MAX_SIDE as u32);
    limits.max_alloc = Some(MAX_PIXELS as u64 * 8);
    reader.limits(limits);
    let rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    let (w, h) = (rgba.width() as i32, rgba.height() as i32);
    if !valid_size(w, h) {
        return Err("Image dimensions are too large.".into());
    }
    let pixels = rgba
        .pixels()
        .map(|pixel| {
            let [r, g, b, a] = pixel.0;
            if keep_transparent && a == 0 {
                HOLE
            } else {
                mix(
                    WHITE,
                    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
                    a.into(),
                )
            }
        })
        .collect();
    Ok(Bitmap { w, h, pixels })
}

pub fn decode(bytes: &[u8]) -> Result<Bitmap, String> {
    read_native(bytes, false)
}

pub fn open(path: &Path) -> Result<(Bitmap, Format), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 * 1024 {
        return Err("Image file is too large.".into());
    }
    let mut format = Format::from_path(path).unwrap_or(Format::Png);
    if bytes.starts_with(b"BM") {
        format = match bytes.get(28..30) {
            Some([1, 0]) => Format::Monochrome,
            Some([4, 0]) => Format::Indexed4,
            Some([8, 0]) => Format::Indexed8,
            _ => Format::Bitmap,
        };
    }
    Ok((decode(&bytes)?, format))
}

fn rgb(color: Color) -> [u8; 3] {
    let c = if color == HOLE { WHITE } else { color };
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

fn encode_native(bitmap: &Bitmap, format: ImageFormat) -> Result<Vec<u8>, String> {
    let rgb = bitmap.pixels.iter().flat_map(|&c| rgb(c)).collect();
    let image =
        RgbImage::from_raw(bitmap.w as u32, bitmap.h as u32, rgb).ok_or("Invalid dimensions.")?;
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, format)
        .map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

pub const EGA: [Color; 16] = [
    BLACK, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xc0c0c0, 0x808080,
    0xff0000, 0x00ff00, 0xffff00, 0x0000ff, 0xff00ff, 0x00ffff, WHITE,
];

fn indexed(bitmap: &Bitmap, depth: u32) -> Vec<u8> {
    let palette: Vec<Color> = match depth {
        1 => vec![BLACK, WHITE],
        4 => EGA.to_vec(),
        _ => (0..256)
            .map(|i| {
                (((i >> 5) * 255 / 7) << 16)
                    | ((((i >> 2) & 7) * 255 / 7) << 8)
                    | ((i & 3) * 255 / 3)
            })
            .collect(),
    };
    let stride = ((bitmap.w as u32 * depth).div_ceil(32) * 4) as usize;
    let offset = 54 + palette.len() * 4;
    let mut out = vec![0; offset + stride * bitmap.h as usize];
    let len = out.len() as u32;
    out[0..2].copy_from_slice(b"BM");
    out[2..6].copy_from_slice(&len.to_le_bytes());
    out[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    out[14..18].copy_from_slice(&40u32.to_le_bytes());
    out[18..22].copy_from_slice(&bitmap.w.to_le_bytes());
    out[22..26].copy_from_slice(&bitmap.h.to_le_bytes());
    out[26] = 1;
    out[28] = depth as u8;
    out[46..50].copy_from_slice(&(palette.len() as u32).to_le_bytes());
    for (slot, &color) in out[54..offset]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(&palette)
    {
        let [red, green, blue] = rgb(color);
        slot.copy_from_slice(&[blue, green, red, 0]);
    }
    let mut lookup = std::collections::HashMap::new();
    for (y, row) in out[offset..].chunks_exact_mut(stride).rev().enumerate() {
        for x in 0..bitmap.w as usize {
            let [red, green, blue] = rgb(bitmap.get(p(x as i32, y as i32)));
            let index = match depth {
                1 => u8::from(monochrome(bitmap.get(p(x as i32, y as i32))) == WHITE),
                4 => *lookup.entry([red, green, blue]).or_insert_with(|| {
                    palette
                        .iter()
                        .enumerate()
                        .min_by_key(|&(_, &color)| {
                            rgb(color)
                                .into_iter()
                                .zip([red, green, blue])
                                .map(|(a, b)| (i32::from(a) - i32::from(b)).pow(2))
                                .sum::<i32>()
                        })
                        .expect("16 colors")
                        .0 as u8
                }),
                _ => {
                    let red = (u16::from(red) * 7 + 127) / 255;
                    let green = (u16::from(green) * 7 + 127) / 255;
                    let blue = (u16::from(blue) * 3 + 127) / 255;
                    (red << 5 | green << 2 | blue) as u8
                }
            };
            let bit = x * depth as usize;
            row[bit / 8] |= index << (8 - depth as usize - bit % 8);
        }
    }
    out
}

pub fn monochrome(color: Color) -> Color {
    let [r, g, b] = rgb(color).map(u32::from);
    if r * 299 + g * 587 + b * 114 >= 128_000 {
        WHITE
    } else {
        BLACK
    }
}

pub fn encode(bitmap: &Bitmap, format: Format) -> Result<Vec<u8>, String> {
    match format {
        Format::Bitmap => encode_native(bitmap, ImageFormat::Bmp),
        Format::Png => encode_native(bitmap, ImageFormat::Png),
        Format::Icon => {
            let pixels = bitmap
                .pixels
                .iter()
                .flat_map(|&c| {
                    let [r, g, b] = rgb(c);
                    [r, g, b, if c == HOLE { 0 } else { 255 }]
                })
                .collect();
            let rgba = RgbaImage::from_raw(bitmap.w as u32, bitmap.h as u32, pixels)
                .ok_or("Invalid dimensions.")?;
            let mut output = Cursor::new(Vec::new());
            rgba.write_to(&mut output, ImageFormat::Ico)
                .map_err(|e| e.to_string())?;
            Ok(output.into_inner())
        }
        Format::Monochrome => Ok(indexed(bitmap, 1)),
        Format::Indexed4 => Ok(indexed(bitmap, 4)),
        Format::Indexed8 => Ok(indexed(bitmap, 8)),
        Format::Jpeg => encode_native(bitmap, ImageFormat::Jpeg),
        Format::Gif => encode_native(bitmap, ImageFormat::Gif),
        Format::Tiff => encode_native(bitmap, ImageFormat::Tiff),
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Enter a file name.")?;
    let name = path
        .file_name()
        .ok_or("Enter a file name.")?
        .to_string_lossy();
    for suffix in 0..100 {
        let temp = parent.join(format!(".{name}.paintrs-{}-{suffix}", std::process::id()));
        let file = OpenOptions::new().write(true).create_new(true).open(&temp);
        let mut file = match file {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        };
        let result = file.write_all(bytes).and_then(|()| {
            drop(file);
            fs::rename(&temp, path)
        });
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        return result.map_err(|e| e.to_string());
    }
    Err("Could not create a temporary file in this folder.".into())
}

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

pub fn clipboard_put(image: &Bitmap) {
    let pixels = image
        .pixels
        .iter()
        .flat_map(|&c| {
            let [r, g, b] = rgb(c);
            [r, g, b, if c == HOLE { 0 } else { 255 }]
        })
        .collect();
    let Some(rgba) = RgbaImage::from_raw(image.w as u32, image.h as u32, pixels) else {
        return;
    };
    let mut payload = Cursor::new(Vec::new());
    if rgba.write_to(&mut payload, ImageFormat::Png).is_ok() {
        clipboard_write(&payload.into_inner(), Some("image/png"));
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
            && let Ok(image) = read_native(&output.stdout, true)
        {
            return Some(image);
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct Page {
    pub landscape: bool,
    pub a4: bool,
    pub margins: [f64; 4],
    pub center: bool,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            landscape: false,
            a4: false,
            margins: [0.5; 4],
            center: true,
        }
    }
}

impl Page {
    pub fn dimensions(&self) -> (f64, f64) {
        let (w, h) = if self.a4 {
            (595.28, 841.89)
        } else {
            (612.0, 792.0)
        };
        if self.landscape { (h, w) } else { (w, h) }
    }
    pub fn image_box(&self, image: &Bitmap) -> (f64, f64, f64, f64) {
        let (page_w, page_h) = self.dimensions();
        let [left, top, right, bottom] = self.margins.map(|m| m * 72.0);
        let w = page_w - left - right;
        let h = page_h - top - bottom;
        let scale = (w / f64::from(image.w))
            .min(h / f64::from(image.h))
            .min(0.75);
        let (iw, ih) = (f64::from(image.w) * scale, f64::from(image.h) * scale);
        let (x, y) = if self.center {
            ((w - iw) / 2.0, (h - ih) / 2.0)
        } else {
            (0.0, 0.0)
        };
        (x + left, y + top, iw, ih)
    }
}

pub fn pdf(image: &Bitmap, page: &Page) -> Result<Vec<u8>, String> {
    let (pw, ph) = page.dimensions();
    let (x, y, w, h) = page.image_box(image);
    let drawing = format!(
        "q\n{w:.3} 0 0 {h:.3} {x:.3} {:.3} cm\n/Picture Do\nQ\n",
        ph - y - h
    );
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    let mut row = Vec::with_capacity(image.w as usize * 3);
    for colors in image.pixels.chunks_exact(image.w as usize) {
        row.clear();
        row.extend(colors.iter().flat_map(|&c| rgb(c)));
        encoder.write_all(&row).map_err(|e| e.to_string())?;
    }
    let compressed = encoder.finish().map_err(|e| e.to_string())?;
    let stream = |dict: &str, data: &[u8]| {
        let mut body = format!("<< /Length {} {dict} >>\nstream\n", data.len()).into_bytes();
        body.extend_from_slice(data);
        body.extend_from_slice(b"\nendstream");
        body
    };
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Picture 5 0 R >> >> /Contents 4 0 R >>").into_bytes(),
        stream("", drawing.as_bytes()),
        stream(&format!("/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode", image.w, image.h), &compressed),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj", i + 1).unwrap();
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        out,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    )
    .unwrap();
    Ok(out)
}

pub fn print(image: &Bitmap, page: &Page, printer: &str, copies: u16) -> Result<(), String> {
    let copies_arg = format!("-#{copies}");
    let printer_arg = format!("-P{printer}");
    let mut args = vec![copies_arg.as_str()];
    if !printer.is_empty() {
        args.push(printer_arg.as_str());
    }
    filter("lpr", &args, &pdf(image, page)?).map(|_| ())
}

pub fn data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|p| p.join("paintrs"))
}

pub fn wallpaper(image: &Bitmap, tiled: bool) -> Result<(), String> {
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("GNOME"))
    {
        return Err("Wallpaper commands require a GNOME desktop.".into());
    }
    let folder = data_dir().ok_or("Could not find the desktop data directory.")?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = folder.join("wallpaper.png");
    write_atomic(&path, &encode(image, Format::Png)?)?;
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
