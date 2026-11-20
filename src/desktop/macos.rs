use crate::{
    files::{self, Format},
    raster::{BLACK, Bitmap, p, valid_size},
};
use objc2::rc::autoreleasepool;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSScreen,
    NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSData, NSDictionary, NSString, NSURL};
use std::fs;

pub fn clipboard_put(image: &Bitmap) {
    if let Ok(bytes) = files::encode_clipboard(image) {
        // The pasteboard retains the encoded bytes independently of this event.
        autoreleasepool(|_| unsafe {
            let clipboard = NSPasteboard::generalPasteboard();
            clipboard.clearContents();
            clipboard.setData_forType(Some(&NSData::from_vec(bytes)), NSPasteboardTypePNG);
        });
    }
}

pub fn clipboard_get() -> Option<Bitmap> {
    autoreleasepool(|_| unsafe {
        let clipboard = NSPasteboard::generalPasteboard();
        [NSPasteboardTypePNG, NSPasteboardTypeTIFF]
            .into_iter()
            .find_map(|format| files::decode_clipboard(clipboard.dataForType(format)?.bytes()).ok())
    })
}

pub fn clipboard_text_put(text: &str) {
    if !text.is_empty() {
        autoreleasepool(|_| unsafe {
            let clipboard = NSPasteboard::generalPasteboard();
            clipboard.clearContents();
            clipboard.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
        });
    }
}

pub fn clipboard_text_get() -> Option<String> {
    autoreleasepool(|_| unsafe {
        let clipboard = NSPasteboard::generalPasteboard();
        clipboard
            .stringForType(NSPasteboardTypeString)
            .map(|text| text.to_string())
    })
}

pub fn wallpaper(image: &Bitmap, tiled: bool) -> Result<(), String> {
    let main = MainThreadMarker::new().ok_or("Wallpaper must be set from a window.")?;
    let folder = super::data_dir().ok_or("Could not find the desktop data directory.")?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    autoreleasepool(|_| {
        let screens = NSScreen::screens(main);
        if screens.is_empty() {
            return Err("No display is available.".into());
        }
        // AppKit scales its source image to fit the screen. Compose centered or tiled pixels at
        // the display's aspect ratio first so this works independently of the prior setting.
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        for (index, screen) in screens.iter().enumerate() {
            let size = screen.frame().size;
            let scale = screen.backingScaleFactor();
            let (mut w, mut h) = ((size.width * scale) as i32, (size.height * scale) as i32);
            while !valid_size(w, h) && (w > 1 || h > 1) {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            if !valid_size(w, h) {
                return Err("Invalid display dimensions.".into());
            }
            let mut wallpaper = Bitmap::new(w, h, BLACK);
            if tiled {
                for y in (0..h).step_by(image.h as usize) {
                    for x in (0..w).step_by(image.w as usize) {
                        wallpaper.blit(image, p(x, y), None);
                    }
                }
            } else {
                wallpaper.blit(image, p((w - image.w) / 2, (h - image.h) / 2), None);
            }
            let path = folder.join(format!("wallpaper-{index}.png"));
            files::write_atomic(&path, &files::encode(&wallpaper, Format::Png)?)?;
            let path = NSString::from_str(&path.to_string_lossy());
            // Each screen has a separate persistent path, which AppKit can read after exit.
            unsafe {
                workspace
                    .setDesktopImageURL_forScreen_options_error(
                        &NSURL::fileURLWithPath(&path),
                        screen,
                        &NSDictionary::new(),
                    )
                    .map_err(|error| error.localizedDescription().to_string())?;
            }
        }
        Ok(())
    })
}
