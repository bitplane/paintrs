use crate::{
    files::{self, Format, Page},
    raster::{Bitmap, HOLE, WHITE},
};
use std::{ffi::OsStr, fs, io, os::windows::ffi::OsStrExt, ptr, slice};
use windows_sys::Win32::{
    Foundation::{ERROR_SUCCESS, GlobalFree},
    Graphics::{Gdi::*, Printing::*},
    Storage::Xps::*,
    System::{DataExchange::*, Memory::*, Registry::*},
    UI::{Input::KeyboardAndMouse::GetActiveWindow, WindowsAndMessaging::*},
};

const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;
const CF_UNICODETEXT: u32 = 13;
const MAX_CLIPBOARD_BYTES: usize = 128 * 1024 * 1024;

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain([0]).collect()
}

#[cfg(not(debug_assertions))]
pub fn startup_error(error: &str) {
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            wide(error).as_ptr(),
            wide("Paintrs").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

struct Clipboard;

impl Clipboard {
    fn open() -> Option<Self> {
        // Clipboard writes need a real owner window. These calls run in the window event loop.
        unsafe { (OpenClipboard(GetActiveWindow()) != 0).then_some(Self) }
    }

    fn read(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let size = GlobalSize(handle);
            if size == 0 || size > MAX_CLIPBOARD_BYTES {
                return None;
            }
            let address = GlobalLock(handle);
            if address.is_null() {
                return None;
            }
            let bytes = slice::from_raw_parts(address.cast::<u8>(), size).to_vec();
            GlobalUnlock(handle);
            Some(bytes)
        }
    }

    fn write(&self, format: u32, bytes: &[u8]) {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if handle.is_null() {
                return;
            }
            let address = GlobalLock(handle);
            if address.is_null() {
                GlobalFree(handle);
                return;
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), address.cast(), bytes.len());
            GlobalUnlock(handle);
            // Successful SetClipboardData transfers ownership to Windows.
            if SetClipboardData(format, handle).is_null() {
                GlobalFree(handle);
            }
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

fn png_format() -> u32 {
    unsafe { RegisterClipboardFormatW(wide("PNG").as_ptr()) }
}

pub fn clipboard_put(image: &Bitmap) {
    let Ok(png) = files::encode_clipboard(image) else {
        return;
    };
    let Ok(bmp) = files::encode(image, Format::Bitmap) else {
        return;
    };
    if let Some(clipboard) = Clipboard::open()
        && unsafe { EmptyClipboard() } != 0
    {
        let format = png_format();
        if format != 0 {
            clipboard.write(format, &png);
        }
        clipboard.write(CF_DIB, &bmp[14..]);
    }
}

pub fn clipboard_get() -> Option<Bitmap> {
    let clipboard = Clipboard::open()?;
    if let Some(image) = clipboard
        .read(png_format())
        .and_then(|data| files::decode_clipboard(&data).ok())
    {
        return Some(image);
    }
    [CF_DIBV5, CF_DIB].into_iter().find_map(|format| {
        clipboard
            .read(format)
            .and_then(|data| files::decode_dib(&data).ok())
    })
}

pub fn clipboard_text_put(text: &str) {
    if !text.is_empty()
        && let Some(clipboard) = Clipboard::open()
        && unsafe { EmptyClipboard() } != 0
    {
        let data: Vec<_> = wide(text).into_iter().flat_map(u16::to_le_bytes).collect();
        clipboard.write(CF_UNICODETEXT, &data);
    }
}

pub fn clipboard_text_get() -> Option<String> {
    let bytes = Clipboard::open()?.read(CF_UNICODETEXT)?;
    let utf16: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .copied()
        .map(u16::from_le_bytes)
        .take_while(|&unit| unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&utf16))
}

pub fn wallpaper(image: &Bitmap, tiled: bool) -> Result<(), String> {
    let folder = super::data_dir().ok_or("Could not find the desktop data directory.")?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join("wallpaper.bmp");
    files::write_atomic(&file, &files::encode(image, Format::Bitmap)?)?;
    let subkey = wide("Control Panel\\Desktop");
    for (name, value) in [
        ("WallpaperStyle", "0"),
        ("TileWallpaper", if tiled { "1" } else { "0" }),
    ] {
        let value = wide(value);
        let result = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                wide(name).as_ptr(),
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32).to_string());
        }
    }
    let mut path = wide(&file);
    let applied = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            path.as_mut_ptr().cast(),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if applied == 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(())
}

struct Printer(PRINTER_HANDLE);
impl Drop for Printer {
    fn drop(&mut self) {
        unsafe {
            ClosePrinter(self.0);
        }
    }
}

struct PrintContext(HDC);
impl Drop for PrintContext {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}

fn printer_name(printer: &str) -> Result<Vec<u16>, String> {
    if !printer.is_empty() {
        return Ok(wide(printer));
    }
    unsafe {
        let mut length = 0;
        GetDefaultPrinterW(ptr::null_mut(), &mut length);
        if length == 0 {
            return Err("No default printer is configured.".into());
        }
        let mut buffer = vec![0; length as usize];
        if GetDefaultPrinterW(buffer.as_mut_ptr(), &mut length) == 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        Ok(buffer)
    }
}

fn printer_context(name: &[u16], page: &Page) -> Result<PrintContext, String> {
    unsafe {
        let mut handle = PRINTER_HANDLE::default();
        if OpenPrinterW(name.as_ptr(), &mut handle, ptr::null()) == 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        let printer = Printer(handle);
        let size = DocumentPropertiesW(
            ptr::null_mut(),
            printer.0,
            name.as_ptr(),
            ptr::null_mut(),
            ptr::null(),
            0,
        );
        if size < size_of::<DEVMODEW>() as i32 {
            return Err("Could not read the printer settings.".into());
        }
        // The driver can append private data to DEVMODEW. Allocate the reported size with DEVMODEW alignment.
        let mut storage =
            vec![DEVMODEW::default(); (size as usize).div_ceil(size_of::<DEVMODEW>())];
        let mode = storage.as_mut_ptr();
        if DocumentPropertiesW(
            ptr::null_mut(),
            printer.0,
            name.as_ptr(),
            mode,
            ptr::null(),
            DM_OUT_BUFFER,
        ) != IDOK
        {
            return Err("Could not read the printer settings.".into());
        }
        (*mode).dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
        (*mode).dmFields &= !(DM_PAPERLENGTH | DM_PAPERWIDTH);
        if (*mode).dmFields & DM_COPIES != 0 {
            (*mode).Anonymous1.Anonymous1.dmCopies = 1;
        }
        (*mode).Anonymous1.Anonymous1.dmOrientation = if page.landscape {
            DMORIENT_LANDSCAPE
        } else {
            DMORIENT_PORTRAIT
        } as i16;
        (*mode).Anonymous1.Anonymous1.dmPaperSize =
            if page.a4 { DMPAPER_A4 } else { DMPAPER_LETTER } as i16;
        if DocumentPropertiesW(
            ptr::null_mut(),
            printer.0,
            name.as_ptr(),
            mode,
            mode,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        ) != IDOK
        {
            return Err("The printer did not accept the page settings.".into());
        }
        let dc = CreateDCW(wide("WINSPOOL").as_ptr(), name.as_ptr(), ptr::null(), mode);
        if dc.is_null() {
            return Err(io::Error::last_os_error().to_string());
        }
        Ok(PrintContext(dc))
    }
}

pub fn print(image: &Bitmap, page: &Page, printer: &str, copies: u16) -> Result<(), String> {
    let dc = printer_context(&printer_name(printer)?, page)?;
    let title = wide("Paintrs");
    let job = DOCINFOW {
        cbSize: size_of::<DOCINFOW>() as i32,
        lpszDocName: title.as_ptr(),
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.w,
            biHeight: -image.h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    // Bitmap's 0xRRGGBB pixels are already in the BGRX byte order GDI expects.
    let flattened = image.pixels.contains(&HOLE).then(|| {
        image
            .pixels
            .iter()
            .map(|&c| if c == HOLE { WHITE } else { c })
            .collect::<Vec<_>>()
    });
    let pixels = flattened.as_deref().unwrap_or(&image.pixels);
    let (x, y, w, h) = page.image_box(image);
    unsafe {
        let dpi_x = f64::from(GetDeviceCaps(dc.0, LOGPIXELSX as i32)) / 72.0;
        let dpi_y = f64::from(GetDeviceCaps(dc.0, LOGPIXELSY as i32)) / 72.0;
        let left = (x * dpi_x).round() as i32 - GetDeviceCaps(dc.0, PHYSICALOFFSETX as i32);
        let top = (y * dpi_y).round() as i32 - GetDeviceCaps(dc.0, PHYSICALOFFSETY as i32);
        if StartDocW(dc.0, &job) <= 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        for _ in 0..copies {
            if StartPage(dc.0) <= 0
                || StretchDIBits(
                    dc.0,
                    left,
                    top,
                    (w * dpi_x).round().max(1.0) as i32,
                    (h * dpi_y).round().max(1.0) as i32,
                    0,
                    0,
                    image.w,
                    image.h,
                    pixels.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                    SRCCOPY,
                ) == GDI_ERROR
                || EndPage(dc.0) <= 0
            {
                let error = io::Error::last_os_error().to_string();
                AbortDoc(dc.0);
                return Err(error);
            }
        }
        if EndDoc(dc.0) <= 0 {
            return Err(io::Error::last_os_error().to_string());
        }
    }
    Ok(())
}
