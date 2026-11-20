use crate::app::{Alert, AlertAction, Command, Dialog, Hit, Paint};
use crate::editor::{PALETTE, Tool};
use crate::files::{self, Format, Page};
use crate::raster::*;
use std::path::PathBuf;
use winit::keyboard::{Key, NamedKey};

struct Harness {
    paint: Paint,
    frame: Bitmap,
}

impl Harness {
    fn new() -> Self {
        let mut paint = Paint::new();
        paint.recent_file = None;
        paint.recent.clear();
        let mut harness = Self {
            paint,
            frame: Bitmap::new(1000, 700, BLACK),
        };
        harness.render();
        harness
    }

    fn render(&mut self) {
        crate::ui::render(&mut self.paint, &mut self.frame);
    }

    fn click(&mut self, hit: Hit) {
        let &(bounds, _) = self
            .paint
            .hits
            .iter()
            .rev()
            .find(|(_, item)| *item == hit)
            .unwrap_or_else(|| panic!("No visible control for {hit:?}"));
        let q = p(bounds.x + bounds.w / 2, bounds.y + bounds.h / 2);
        self.paint.pointer_moved(q);
        self.paint.pointer_down(false);
        self.paint.pointer_up(false);
        self.render();
    }

    fn drag(&mut self, start: Point, end: Point) {
        let view = self.paint.layout().view;
        let on_screen = |q: Point| p(q.x + view.x, q.y + view.y);
        self.paint.pointer_moved(on_screen(start));
        self.paint.pointer_down(false);
        self.paint.pointer_moved(on_screen(end));
        self.paint.pointer_up(false);
        self.render();
    }

    fn enter(&mut self) {
        self.paint.key(&Key::Named(NamedKey::Enter), None);
        self.render();
    }

    fn artifact(&self, name: &str) {
        if let Some(dir) = std::env::var_os("PAINTRS_QA_DIR") {
            let output = PathBuf::from(dir).join(name);
            files::write_atomic(&output, &files::encode(&self.frame, Format::Png).unwrap())
                .unwrap();
        }
    }
}

#[test]
fn tools_menus_and_text_use_the_same_hit_targets_as_the_desktop() {
    let mut app = Harness::new();
    app.artifact("interface.png");
    assert_eq!(app.paint.editor.tool, Tool::Pencil);
    assert_eq!(app.paint.editor.bitmap().get(p(100, 100)), WHITE);

    app.click(Hit::Color(13));
    app.click(Hit::Tool(Tool::Rectangle));
    app.drag(p(70, 40), p(286, 200));
    assert_eq!(app.paint.editor.bitmap().get(p(70, 100)), PALETTE[13]);
    assert_eq!(app.paint.editor.bitmap().get(p(140, 100)), WHITE);

    app.click(Hit::Tool(Tool::Fill));
    app.click(Hit::Color(23));
    app.drag(p(140, 100), p(140, 100));
    assert_eq!(app.paint.editor.bitmap().get(p(140, 100)), PALETTE[23]);
    assert_eq!(app.paint.editor.bitmap().get(p(55, 100)), WHITE);

    app.click(Hit::Tool(Tool::Select));
    app.drag(p(62, 34), p(295, 208));
    app.drag(p(150, 120), p(170, 130));
    assert_eq!(
        app.paint.editor.selection_bounds().unwrap().origin(),
        p(82, 44)
    );
    app.drag(p(315, 218), p(338, 240));
    assert_eq!(app.paint.editor.selection_bounds().unwrap().w, 257);
    app.enter();
    assert!(app.paint.editor.selection.is_none());

    app.click(Hit::Tool(Tool::Text));
    app.click(Hit::Color(0));
    app.click(Hit::FontSize);
    app.click(Hit::SizeChoice(28));
    app.click(Hit::Bold);
    app.drag(p(368, 53), p(660, 116));
    app.paint.text_input("Paintrs");
    app.click(Hit::Tool(Tool::Pencil));
    assert!(
        app.paint
            .editor
            .bitmap()
            .extract(r(368, 53, 120, 63))
            .pixels
            .iter()
            .any(|&c| c < 0x222222)
    );
    app.artifact("drawing.png");

    app.click(Hit::Menu(2));
    app.click(Hit::MenuZoom);
    app.click(Hit::Invoke(Command::Zoom(4)));
    assert_eq!(app.paint.zoom, 4);
    app.click(Hit::Menu(2));
    app.click(Hit::MenuZoom);
    app.click(Hit::Invoke(Command::Grid));
    assert!(app.paint.grid);

    app.click(Hit::Menu(3));
    app.artifact("image-menu.png");
    app.click(Hit::Invoke(Command::Attributes));
    app.artifact("attributes.png");
    app.paint.text_input("640");
    app.paint.key(&Key::Named(NamedKey::Tab), None);
    app.paint.text_input("360");
    app.enter();
    assert_eq!(app.paint.editor.bitmap().bounds(), r(0, 0, 640, 360));
    app.paint.invoke(Command::Undo);
    assert_eq!(app.paint.editor.bitmap().bounds(), r(0, 0, 720, 480));
}

#[test]
fn dialog_saves_and_reopens_work_and_prompts_before_discarding() {
    let mut app = Harness::new();
    let dir = std::env::temp_dir().join(format!("paintrs-test-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let file = dir.join("Saved picture.png");
    app.drag(p(10, 10), p(40, 40));
    app.click(Hit::Menu(0));
    app.click(Hit::Invoke(Command::Save));
    app.paint.text_input(file.to_str().unwrap());
    app.render();
    app.artifact("save-dialog.png");
    app.enter();
    assert_eq!(app.paint.editor.path.as_deref(), Some(file.as_path()));
    assert_eq!(files::open(&file).unwrap().0.get(p(20, 20)), BLACK);
    assert!(!app.paint.editor.dirty());

    app.drag(p(20, 10), p(50, 40));
    app.paint.invoke(Command::New);
    app.render();
    assert!(matches!(
        app.paint.dialog,
        Some(Dialog::Alert(Alert {
            action: AlertAction::Unsaved(_),
            ..
        }))
    ));
    app.click(Hit::ModalAlt);
    assert_eq!(app.paint.editor.bitmap().get(p(20, 20)), WHITE);
    app.paint.invoke(Command::Open);
    app.paint.text_input(file.to_str().unwrap());
    app.enter();
    assert_eq!(app.paint.editor.bitmap().get(p(20, 20)), BLACK);

    app.paint.invoke(Command::ExportPdf);
    let pdf = dir.join("printout.pdf");
    app.paint.text_input(pdf.to_str().unwrap());
    app.enter();
    let data = std::fs::read(&pdf).unwrap();
    assert!(data.starts_with(b"%PDF-1.4"));
    if let Some(qa) = std::env::var_os("PAINTRS_QA_DIR") {
        files::write_atomic(&PathBuf::from(qa).join("printout.pdf"), &data).unwrap();
    }
}

#[test]
fn formats_round_trip_and_indexed_bitmaps_keep_the_requested_depth() {
    let mut bitmap = Bitmap::new(37, 29, WHITE);
    bitmap.fill(r(0, 0, 18, 29), BLACK);
    bitmap.fill(r(21, 7, 10, 13), 0xff0000);
    for format in Format::ALL {
        let bytes =
            files::encode(&bitmap, format).unwrap_or_else(|error| panic!("{format:?}: {error}"));
        let reopened = files::decode(&bytes).unwrap_or_else(|error| panic!("{format:?}: {error}"));
        assert_eq!(reopened.bounds(), bitmap.bounds(), "{format:?}");
        assert!(reopened.get(p(7, 12)) < 0x161616, "{format:?}");
        assert!(reopened.get(p(35, 22)) > 0xe8e8e8, "{format:?}");
        if let Some(depth) = match format {
            Format::Bitmap => Some(24),
            Format::Indexed8 => Some(8),
            Format::Indexed4 => Some(4),
            Format::Monochrome => Some(1),
            _ => None,
        } {
            assert_eq!(bytes[28], depth);
        }
    }
    let mut page = Page {
        landscape: true,
        ..Page::default()
    };
    page.margins[1] = 1.0;
    assert_eq!(page.dimensions(), (792.0, 612.0));
    assert!(files::pdf(&bitmap, &page).unwrap().ends_with(b"%%EOF\n"));
}
