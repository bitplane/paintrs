use crate::editor::{Editor, TextMove, Tool};
use crate::files::{self, Format, Page};
use crate::raster::*;
use crate::ui::{self, Layout};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, ResizeDirection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    New,
    Open,
    Save,
    SaveAs,
    PageSetup,
    Preview,
    Print,
    ExportPdf,
    Wallpaper(bool),
    Exit,
    Recent(usize),
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    CopyTo,
    PasteFrom,
    Toolbox,
    ColorBox,
    Status,
    Textbar,
    Zoom(i32),
    Grid,
    Thumbnail,
    ViewBitmap,
    Turn,
    Stretch,
    Invert,
    Attributes,
    Clear,
    Crop,
    Opaque,
    Colors,
    Help,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Menu(usize),
    MenuZoom,
    Invoke(Command),
    Title,
    System,
    Minimize,
    Maximize,
    Close,
    WindowEdge(ResizeDirection),
    Tool(Tool),
    Color(usize),
    EditColor,
    Width(usize),
    Brush(usize),
    Eraser(usize),
    Spray(usize),
    Fill(usize),
    Magnify(i32),
    Opaque(bool),
    FontFamily,
    FontSize,
    FontChoice(Family),
    SizeChoice(u16),
    Bold,
    Italic,
    Underline,
    Scroll(i32, i32),
    Track(bool),
    Thumb(bool),
    Canvas,
    CanvasHandle(bool, bool),
    SelectionHandle(Point),
    ModalClose,
    ModalAccept,
    ModalCancel,
    ModalAlt,
    Field(usize),
    Option(usize),
    FileEntry(usize),
    FileUp,
    FileHome,
    FileFormat,
    Format(Format),
    ColorSample(Color),
    HelpTab(usize),
    ThumbnailClose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Drag,
    Resize(ResizeDirection),
    Minimize,
    Maximize,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    Open,
    Save,
    CopyTo,
    PasteFrom,
    Pdf,
}

impl FileMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Save => "Save As",
            Self::CopyTo => "Copy To",
            Self::PasteFrom => "Paste From",
            Self::Pdf => "Save as PDF",
        }
    }
    pub fn saving(self) -> bool {
        matches!(self, Self::Save | Self::CopyTo | Self::Pdf)
    }
}

pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
}

pub struct Field {
    pub value: String,
    pub cursor: usize,
    pub anchor: usize,
}

impl Field {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            cursor: value.len(),
            anchor: 0,
            value,
        }
    }
    pub fn range(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    pub fn insert(&mut self, text: &str) {
        let range = self.range();
        if self.value.len() + text.len() > 8192 {
            return;
        }
        self.cursor = range.start + text.len();
        self.anchor = self.cursor;
        self.value.replace_range(range, text);
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.value.len();
    }
    pub fn movement(&mut self, movement: TextMove, select: bool) {
        self.cursor = match movement {
            TextMove::Left => self.value[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i),
            TextMove::Right => {
                self.cursor
                    + self.value[self.cursor..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)
            }
            TextMove::Up | TextMove::Home => 0,
            TextMove::Down | TextMove::End => self.value.len(),
        };
        if !select {
            self.anchor = self.cursor;
        }
    }
    pub fn delete(&mut self, back: bool) {
        if self.cursor == self.anchor {
            self.movement(
                if back {
                    TextMove::Left
                } else {
                    TextMove::Right
                },
                true,
            );
        }
        self.insert("");
    }
}

pub struct FileBox {
    pub mode: FileMode,
    pub folder: PathBuf,
    pub folder_field: Field,
    pub name: Field,
    pub entries: Vec<Entry>,
    pub offset: usize,
    pub selected: Option<usize>,
    pub format: Format,
    pub dropdown: bool,
    pub focus: usize,
    pub problem: String,
    pub continuation: Option<Pending>,
}

impl FileBox {
    fn new(
        mode: FileMode,
        folder: PathBuf,
        name: &str,
        format: Format,
        continuation: Option<Pending>,
    ) -> Self {
        let mut dialog = Self {
            mode,
            folder_field: Field::new(folder.to_string_lossy()),
            folder,
            name: Field::new(name),
            entries: Vec::new(),
            offset: 0,
            selected: None,
            format,
            dropdown: false,
            focus: 1,
            problem: String::new(),
            continuation,
        };
        dialog.refresh();
        dialog
    }
    fn refresh(&mut self) {
        self.entries.clear();
        self.offset = 0;
        self.selected = None;
        self.problem.clear();
        match std::fs::read_dir(&self.folder) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let dir = path.is_dir();
                    if name.starts_with('.')
                        || (!dir
                            && Format::from_path(&path).is_none()
                            && path
                                .extension()
                                .is_none_or(|ext| !ext.eq_ignore_ascii_case("pdf")))
                    {
                        continue;
                    }
                    self.entries.push(Entry { path, name, dir });
                }
                self.entries
                    .sort_by_cached_key(|entry| (!entry.dir, entry.name.to_lowercase()));
            }
            Err(error) => self.problem = error.to_string(),
        }
    }
    fn navigate(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.problem = "That folder could not be found.".into();
            return;
        }
        self.folder = path.canonicalize().unwrap_or(path);
        self.folder_field = Field::new(self.folder.to_string_lossy());
        self.refresh();
        self.focus = 1;
    }
    fn path_for(&self, raw: &str) -> PathBuf {
        let raw = PathBuf::from(raw.trim());
        if let Ok(rest) = raw.strip_prefix("~")
            && let Some(home) = std::env::var_os("HOME")
        {
            return PathBuf::from(home).join(rest);
        }
        self.folder.join(raw)
    }
    fn chosen(&self) -> PathBuf {
        self.path_for(&self.name.value)
    }
    pub fn field(&mut self, index: usize) -> Option<&mut Field> {
        match index {
            0 => Some(&mut self.folder_field),
            1 => Some(&mut self.name),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormKind {
    Attributes,
    Turn,
    Stretch,
    Colors,
    Page,
    Print,
    Zoom,
}

impl FormKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Attributes => "Attributes",
            Self::Turn => "Flip and Rotate",
            Self::Stretch => "Stretch and Skew",
            Self::Colors => "Edit Colors",
            Self::Page => "Page Setup",
            Self::Print => "Print",
            Self::Zoom => "Custom Zoom",
        }
    }
}

pub struct Form {
    pub kind: FormKind,
    pub input: Vec<Field>,
    pub focus: usize,
    pub options: [usize; 3],
    pub problem: String,
}

#[derive(Clone, Debug)]
pub enum Pending {
    New,
    Open,
    Load(PathBuf),
    Exit,
}

pub enum AlertAction {
    Dismiss,
    Unsaved(Pending),
    Overwrite(PathBuf, Format, Box<FileBox>),
    Wallpaper(bool),
}

pub struct Alert {
    pub title: &'static str,
    pub message: String,
    pub action: AlertAction,
}

pub enum Dialog {
    File(FileBox),
    Form(Form),
    Alert(Alert),
    Help(usize),
    About,
    Preview,
}

pub struct Paint {
    pub editor: Editor,
    pub fonts: Fonts,
    pub pointer: Point,
    pub dims: Point,
    pub mods: ModifiersState,
    pub zoom: i32,
    pub preferred_zoom: i32,
    pub origin: Point,
    pub tool_box: bool,
    pub color_box: bool,
    pub status_bar: bool,
    pub text_bar: bool,
    pub grid: bool,
    pub thumbnail: bool,
    pub viewing: bool,
    pub menu: Option<usize>,
    pub zoom_menu: bool,
    pub menu_index: Option<usize>,
    pub font_menu: Option<bool>,
    pub dialog: Option<Dialog>,
    pub page: Page,
    pub recent: Vec<PathBuf>,
    pub(crate) recent_file: Option<PathBuf>,
    pub hits: Vec<(Rect, Hit)>,
    pub window_action: Option<WindowAction>,
    pub focused: bool,
    pub maximized: bool,
    pub hover_time: Instant,
    pub tooltip: bool,
    pub caret: bool,
    pub note: Option<(String, Instant)>,
    pub ime: String,
    last_click: Option<(Hit, Point, bool, Instant)>,
    button: Option<bool>,
    scroll_drag: Option<(bool, i32, i32)>,
    next_caret: Instant,
    next_spray: Instant,
}

impl Paint {
    pub fn new() -> Self {
        let now = Instant::now();
        let recent_file = files::data_dir().map(|dir| dir.join("recent"));
        Self {
            editor: Editor::new(),
            fonts: Fonts::new(),
            pointer: p(-1, -1),
            dims: p(1000, 700),
            mods: ModifiersState::empty(),
            zoom: 1,
            preferred_zoom: 4,
            origin: p(0, 0),
            tool_box: true,
            color_box: true,
            status_bar: true,
            text_bar: true,
            grid: false,
            thumbnail: false,
            viewing: false,
            menu: None,
            zoom_menu: false,
            menu_index: None,
            font_menu: None,
            dialog: None,
            page: Page::default(),
            recent: recent_file
                .as_ref()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .map(|data| {
                    data.lines()
                        .map(PathBuf::from)
                        .filter(|path| path.is_file())
                        .take(4)
                        .collect()
                })
                .unwrap_or_default(),
            recent_file,
            hits: Vec::new(),
            window_action: None,
            focused: true,
            maximized: false,
            hover_time: now,
            tooltip: false,
            caret: true,
            note: None,
            ime: String::new(),
            last_click: None,
            button: None,
            scroll_drag: None,
            next_caret: now,
            next_spray: now,
        }
    }
    pub fn title(&self) -> String {
        let name = self
            .editor
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "Untitled".into());
        format!("{name} - Paintrs")
    }
    pub fn layout(&self) -> Layout {
        Layout::new(self)
    }
    pub fn hit(&self, point: Point) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(bounds, _)| bounds.contains(point))
            .map(|(_, hit)| *hit)
    }
    pub fn pixel(&self, q: Point) -> Point {
        let view = self.layout().view;
        p(
            (q.x - view.x).div_euclid(self.zoom) + self.origin.x,
            (q.y - view.y).div_euclid(self.zoom) + self.origin.y,
        )
    }
    pub fn max_scroll(&self, view: Rect) -> Point {
        let bitmap = self.editor.bitmap();
        p(
            (bitmap.w - (view.w / self.zoom).max(1)).max(0),
            (bitmap.h - (view.h / self.zoom).max(1)).max(0),
        )
    }
    pub fn constrain_scroll(&mut self) {
        let max = self.max_scroll(self.layout().view);
        self.origin = p(self.origin.x.clamp(0, max.x), self.origin.y.clamp(0, max.y));
    }
    pub fn change_zoom(&mut self, factor: i32, pixel: Option<Point>, screen: Option<Point>) {
        let view = self.layout().view;
        let center = screen.unwrap_or(p(view.x + view.w / 2, view.y + view.h / 2));
        let anchor = pixel.unwrap_or_else(|| {
            let raw = self.pixel(center);
            let bmp = self.editor.bitmap();
            p(raw.x.min(bmp.w - 1), raw.y.min(bmp.h - 1))
        });
        self.zoom = factor.clamp(1, 10);
        self.origin = if factor == 1 {
            p(0, 0)
        } else {
            p(
                anchor.x - (center.x - view.x) / self.zoom,
                anchor.y - (center.y - view.y) / self.zoom,
            )
        };
        self.constrain_scroll();
    }
    pub fn set_note(&mut self, text: impl Into<String>) {
        self.note = Some((text.into(), Instant::now() + Duration::from_secs(4)));
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.font_menu = None;
        self.dialog = Some(Dialog::Alert(Alert {
            title: "Paintrs",
            message: message.into(),
            action: AlertAction::Dismiss,
        }));
    }
    pub fn open_initial(&mut self, path: PathBuf) {
        self.load(path);
    }
    fn remember(&mut self, path: PathBuf) {
        self.recent.retain(|old| old != &path);
        self.recent.insert(0, path);
        self.recent.truncate(4);
        if let Some(path) = &self.recent_file
            && let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_ok()
        {
            let text = self
                .recent
                .iter()
                .filter_map(|path| path.to_str())
                .filter(|path| !path.contains('\n'))
                .collect::<Vec<_>>()
                .join("\n");
            let _ = files::write_atomic(path, text.as_bytes());
        }
    }
    fn load(&mut self, path: PathBuf) {
        match files::open(&path) {
            Ok((image, format)) => {
                let path = path.canonicalize().unwrap_or(path);
                self.remember(path.clone());
                self.editor.reset(image, format, Some(path));
                self.zoom = 1;
                self.origin = p(0, 0);
                self.dialog = None;
            }
            Err(message) => self.error(format!("Could not open the picture.\n{message}")),
        }
    }
    pub fn dropped(&mut self, path: PathBuf) {
        self.ask_pending(Pending::Load(path));
    }
    fn ask_pending(&mut self, pending: Pending) {
        if self.editor.dirty() {
            self.dialog = Some(Dialog::Alert(Alert {
                title: "Paintrs",
                message: "Do you want to save changes to this picture?".into(),
                action: AlertAction::Unsaved(pending),
            }));
        } else {
            self.do_pending(pending);
        }
    }
    fn do_pending(&mut self, pending: Pending) {
        self.dialog = None;
        match pending {
            Pending::New => {
                self.editor
                    .reset(Bitmap::new(720, 480, WHITE), Format::Bitmap, None);
                self.editor.fg = BLACK;
                self.editor.bg = WHITE;
                self.zoom = 1;
                self.origin = p(0, 0);
            }
            Pending::Open => self.file_dialog(FileMode::Open, None),
            Pending::Load(path) => self.load(path),
            Pending::Exit => self.window_action = Some(WindowAction::Exit),
        }
    }
    fn file_dialog(&mut self, mode: FileMode, continuation: Option<Pending>) {
        let folder = self
            .editor
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let name = if mode.saving() {
            self.editor
                .path
                .as_ref()
                .and_then(|path| path.file_stem())
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled")
        } else {
            ""
        };
        self.dialog = Some(Dialog::File(FileBox::new(
            mode,
            folder,
            name,
            self.editor.format,
            continuation,
        )));
    }
    fn save_current(&mut self, continuation: Option<Pending>) {
        if let Some(path) = self.editor.path.clone() {
            self.save_file(path, self.editor.format, FileMode::Save, continuation);
        } else {
            self.file_dialog(FileMode::Save, continuation);
        }
    }
    fn save_file(&mut self, path: PathBuf, format: Format, mode: FileMode, next: Option<Pending>) {
        let result = match mode {
            FileMode::CopyTo => self
                .editor
                .selection
                .as_ref()
                .ok_or_else(|| "Select part of the image first.".to_string())
                .and_then(|selection| files::encode(&selection.bitmap, format)),
            FileMode::Pdf => {
                self.editor.settle(&mut self.fonts);
                files::pdf(self.editor.bitmap(), &self.page)
            }
            _ => {
                self.editor.settle(&mut self.fonts);
                files::encode(self.editor.bitmap(), format)
            }
        }
        .and_then(|data| files::write_atomic(&path, &data));
        match result {
            Ok(()) => {
                self.dialog = None;
                if mode == FileMode::Save {
                    self.editor.path = Some(path.clone());
                    self.editor.format = format;
                    self.editor.saved();
                    self.remember(path);
                }
                self.set_note("Picture saved.");
                if let Some(next) = next {
                    self.do_pending(next);
                }
            }
            Err(message) => self.error(format!("Could not save the file.\n{message}")),
        }
    }
    fn accept_file(&mut self, mut dialog: FileBox) {
        if dialog.name.value.trim().is_empty() {
            dialog.problem = "Enter a file name.".into();
            self.dialog = Some(Dialog::File(dialog));
            return;
        }
        let mut path = dialog.chosen();
        if path.is_dir() {
            dialog.navigate(path);
            self.dialog = Some(Dialog::File(dialog));
            return;
        }
        if dialog.mode.saving() {
            let mut format = dialog.format;
            if dialog.mode == FileMode::Pdf {
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
                {
                    path.set_extension("pdf");
                }
            } else if let Some(explicit) = Format::from_path(&path) {
                if explicit.extension() != format.extension() {
                    format = explicit;
                }
            } else {
                path.set_extension(format.extension());
            }
            if path.exists() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                self.dialog = Some(Dialog::Alert(Alert {
                    title: "Confirm Save As",
                    message: format!("{name} already exists.\nDo you want to replace it?"),
                    action: AlertAction::Overwrite(path, format, Box::new(dialog)),
                }));
            } else {
                self.save_file(path, format, dialog.mode, dialog.continuation);
            }
        } else if dialog.mode == FileMode::Open {
            self.load(path);
        } else {
            match files::open(&path) {
                Ok((bitmap, _)) => {
                    self.editor.paste(bitmap, self.origin, &mut self.fonts);
                }
                Err(error) => self.error(format!("Could not paste the picture.\n{error}")),
            }
        }
    }
    fn show_form(&mut self, kind: FormKind) {
        let mut options = [0; 3];
        let input = match kind {
            FormKind::Attributes => {
                self.editor.settle(&mut self.fonts);
                options[0] = usize::from(self.editor.mono());
                vec![
                    self.editor.bitmap().w.to_string(),
                    self.editor.bitmap().h.to_string(),
                ]
            }
            FormKind::Turn => Vec::new(),
            FormKind::Stretch => vec!["100".into(), "100".into(), "0".into(), "0".into()],
            FormKind::Colors => {
                let c = self.editor.palette[self.editor.palette_slot];
                vec![
                    ((c >> 16) & 255).to_string(),
                    ((c >> 8) & 255).to_string(),
                    (c & 255).to_string(),
                    format!("{c:06X}"),
                ]
            }
            FormKind::Page => {
                options = [
                    usize::from(self.page.landscape),
                    usize::from(self.page.a4),
                    usize::from(self.page.center),
                ];
                self.page.margins.iter().map(|v| v.to_string()).collect()
            }
            FormKind::Print => vec![String::new(), "1".into()],
            FormKind::Zoom => vec![(self.zoom * 100).to_string()],
        };
        self.dialog = Some(Dialog::Form(Form {
            kind,
            input: input.into_iter().map(Field::new).collect(),
            focus: 0,
            options,
            problem: String::new(),
        }));
    }
    fn accept_form(&mut self, mut form: Form) {
        let value = |n: usize| form.input[n].value.trim();
        let numbers = || -> Option<[f64; 4]> {
            let numbers: Vec<_> = (0..4)
                .map(|n| value(n).parse::<f64>().ok())
                .collect::<Option<_>>()?;
            numbers.try_into().ok()
        };
        let keep = match form.kind {
            FormKind::Attributes => {
                let factor = units_per_pixel(form.options[1]);
                let pixels =
                    |index| (value(index).parse::<f64>().unwrap_or(0.0) / factor).round() as i32;
                let w = pixels(0);
                let h = pixels(1);
                if valid_size(w, h) {
                    self.editor
                        .attributes(w, h, form.options[0] == 1, &mut self.fonts);
                    None
                } else {
                    Some("Enter valid pixel dimensions (up to 8,192 per side).")
                }
            }
            FormKind::Turn => {
                self.editor.finish_drawing(&mut self.fonts);
                self.editor.turn(
                    [
                        Turn::Horizontal,
                        Turn::Vertical,
                        Turn::Right,
                        Turn::Half,
                        Turn::Left,
                    ][form.options[0]],
                );
                None
            }
            FormKind::Stretch => {
                self.editor.finish_drawing(&mut self.fonts);
                if let Some([x, y, ax, ay]) = numbers()
                    && (1.0..=500.0).contains(&x)
                    && (1.0..=500.0).contains(&y)
                    && (-85.0..=85.0).contains(&ax)
                    && (-85.0..=85.0).contains(&ay)
                    && self.editor.stretch(x / 100.0, y / 100.0, ax, ay)
                {
                    None
                } else {
                    Some("Check the percentages, angles, and resulting size.")
                }
            }
            FormKind::Colors => {
                if let Some(color) = ui::form_color(&form) {
                    self.editor.palette[self.editor.palette_slot] = color;
                    self.editor.set_color(color, false);
                    None
                } else {
                    Some("Enter RGB values from 0 to 255 or a six-digit hex color.")
                }
            }
            FormKind::Page => {
                let page = Page {
                    landscape: form.options[0] == 1,
                    a4: form.options[1] == 1,
                    margins: numbers().unwrap_or([-1.0; 4]),
                    center: form.options[2] == 1,
                };
                let (w, h) = page.dimensions();
                let [l, t, r, b] = page.margins;
                if page.margins.iter().all(|m| (0.0..=10.0).contains(m))
                    && (l + r) * 72.0 < w
                    && (t + b) * 72.0 < h
                {
                    self.page = page;
                    None
                } else {
                    Some("Enter margins that fit inside the paper.")
                }
            }
            FormKind::Print => {
                if let Ok(copies) = value(1).parse::<u16>()
                    && (1..=999).contains(&copies)
                {
                    self.editor.settle(&mut self.fonts);
                    match files::print(self.editor.bitmap(), &self.page, value(0), copies) {
                        Ok(()) => self.set_note("Picture sent to the printer."),
                        Err(message) => {
                            self.error(format!("Could not print the picture.\n{message}"))
                        }
                    }
                    None
                } else {
                    Some("Enter a number of copies between 1 and 999.")
                }
            }
            FormKind::Zoom => match value(0).parse::<i32>() {
                Ok(factor) if (100..=1000).contains(&factor) && factor % 100 == 0 => {
                    self.change_zoom(factor / 100, None, None);
                    self.preferred_zoom = (factor / 100).max(2);
                    None
                }
                _ => Some("Enter a multiple of 100 between 100 and 1,000."),
            },
        };
        if let Some(problem) = keep {
            form.problem = problem.into();
            self.dialog = Some(Dialog::Form(form));
        }
        self.constrain_scroll();
    }
    pub fn enabled(&self, cmd: Command) -> bool {
        match cmd {
            Command::Undo => self.editor.can_undo(),
            Command::Redo => self.editor.can_redo(),
            Command::Cut | Command::Copy | Command::Delete | Command::CopyTo | Command::Crop => {
                self.editor.selection.is_some()
            }
            Command::Grid | Command::Thumbnail => self.zoom > 1,
            Command::Textbar => self.editor.tool == Tool::Text,
            _ => true,
        }
    }
    pub fn checked(&self, cmd: Command) -> bool {
        match cmd {
            Command::Toolbox => self.tool_box,
            Command::ColorBox => self.color_box,
            Command::Status => self.status_bar,
            Command::Textbar => self.text_bar,
            Command::Zoom(n) if n > 0 => self.zoom == n,
            Command::Grid => self.grid,
            Command::Thumbnail => self.thumbnail,
            Command::Opaque => self.editor.opaque,
            _ => false,
        }
    }
    pub fn invoke(&mut self, cmd: Command) {
        self.menu = None;
        self.zoom_menu = false;
        self.menu_index = None;
        self.font_menu = None;
        if !self.enabled(cmd) {
            return;
        }
        match cmd {
            Command::New => self.ask_pending(Pending::New),
            Command::Open => self.ask_pending(Pending::Open),
            Command::Recent(i) => {
                if let Some(path) = self.recent.get(i).cloned() {
                    self.ask_pending(Pending::Load(path));
                }
            }
            Command::Exit => self.ask_pending(Pending::Exit),
            Command::Save => self.save_current(None),
            Command::SaveAs => self.file_dialog(FileMode::Save, None),
            Command::CopyTo => self.file_dialog(FileMode::CopyTo, None),
            Command::PasteFrom => self.file_dialog(FileMode::PasteFrom, None),
            Command::ExportPdf => self.file_dialog(FileMode::Pdf, None),
            Command::PageSetup => self.show_form(FormKind::Page),
            Command::Preview => {
                self.editor.settle(&mut self.fonts);
                self.dialog = Some(Dialog::Preview);
            }
            Command::Print => self.show_form(FormKind::Print),
            Command::Wallpaper(tiled) => {
                self.dialog = Some(Dialog::Alert(Alert {
                    title: "Desktop Wallpaper",
                    message: "Set the current picture as your desktop wallpaper?".into(),
                    action: AlertAction::Wallpaper(tiled),
                }));
            }
            Command::Undo => self.editor.undo(),
            Command::Redo => self.editor.redo(),
            Command::Copy | Command::Cut => {
                self.editor.copy();
                if let Some(bitmap) = &self.editor.clipboard {
                    files::clipboard_put(bitmap);
                }
                if cmd == Command::Cut {
                    self.editor.delete_selection();
                }
            }
            Command::Paste => {
                let image = files::clipboard_get().or_else(|| self.editor.clipboard.clone());
                if let Some(image) = image {
                    self.editor.paste(image, self.origin, &mut self.fonts);
                } else {
                    self.set_note("There is no picture on the clipboard.");
                }
            }
            Command::Delete => self.editor.delete_selection(),
            Command::SelectAll => self.editor.select_all(&mut self.fonts),
            Command::Toolbox => self.tool_box = !self.tool_box,
            Command::ColorBox => self.color_box = !self.color_box,
            Command::Status => self.status_bar = !self.status_bar,
            Command::Textbar => self.text_bar = !self.text_bar,
            Command::Zoom(0) => self.show_form(FormKind::Zoom),
            Command::Zoom(factor) => self.change_zoom(factor, None, None),
            Command::Grid => self.grid = !self.grid,
            Command::Thumbnail => self.thumbnail = !self.thumbnail,
            Command::ViewBitmap => {
                self.editor.settle(&mut self.fonts);
                self.viewing = true;
            }
            Command::Turn => self.show_form(FormKind::Turn),
            Command::Stretch => self.show_form(FormKind::Stretch),
            Command::Attributes => self.show_form(FormKind::Attributes),
            Command::Invert => {
                self.editor.finish_drawing(&mut self.fonts);
                self.editor.invert();
            }
            Command::Clear => self.editor.clear(&mut self.fonts),
            Command::Crop => self.editor.crop(),
            Command::Opaque => self.editor.opaque = !self.editor.opaque,
            Command::Colors => self.show_form(FormKind::Colors),
            Command::Help => self.dialog = Some(Dialog::Help(0)),
            Command::About => self.dialog = Some(Dialog::About),
        }
        self.constrain_scroll();
    }
    pub fn pointer_moved(&mut self, point: Point) {
        let old = self.hit(self.pointer);
        self.pointer = point;
        let hit = self.hit(point);
        if old != hit {
            self.hover_time = Instant::now();
            self.tooltip = false;
        }
        if let Some((vertical, start, origin)) = self.scroll_drag {
            let axis = if vertical { point.y } else { point.x };
            let l = self.layout();
            let track = if vertical { l.vertical } else { l.horizontal };
            let thumb = ui::thumb(self, vertical);
            let travel = if vertical {
                track.h - thumb.h - 32
            } else {
                track.w - thumb.w - 32
            };
            let max = self.max_scroll(l.view);
            if travel > 0 {
                if vertical {
                    self.origin.y = origin + (axis - start) * max.y / travel;
                } else {
                    self.origin.x = origin + (axis - start) * max.x / travel;
                }
            }
            self.constrain_scroll();
        } else if self.editor.gesture.is_some() {
            self.editor.drag(self.pixel(point), self.mods.shift_key());
        } else if self.menu.is_some() {
            if let Some(Hit::Menu(index)) = hit {
                if self.menu != Some(index) {
                    self.menu = Some(index);
                    self.zoom_menu = false;
                    self.menu_index = None;
                }
            } else if hit == Some(Hit::MenuZoom) {
                self.zoom_menu = true;
            } else if matches!(hit, Some(Hit::Invoke(_))) {
                let submenu = self.zoom_menu && point.x >= ui::zoom_menu_rect(self).x - 2;
                self.zoom_menu = submenu;
                self.menu_index = ui::menu_items(self, self.menu.unwrap(), submenu)
                    .iter()
                    .position(|item| item.hit == hit);
            }
        }
    }
    pub fn cursor(&self) -> CursorIcon {
        if self.viewing {
            return CursorIcon::Default;
        }
        match self.hit(self.pointer) {
            Some(Hit::WindowEdge(dir)) => match dir {
                ResizeDirection::East | ResizeDirection::West => CursorIcon::EwResize,
                ResizeDirection::North | ResizeDirection::South => CursorIcon::NsResize,
                ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::NeswResize,
                _ => CursorIcon::NwseResize,
            },
            Some(Hit::SelectionHandle(dir)) => match (dir.x, dir.y) {
                (0, _) => CursorIcon::NsResize,
                (_, 0) => CursorIcon::EwResize,
                (x, y) if x == y => CursorIcon::NwseResize,
                _ => CursorIcon::NeswResize,
            },
            Some(Hit::CanvasHandle(true, true)) => CursorIcon::NwseResize,
            Some(Hit::CanvasHandle(true, false)) => CursorIcon::EwResize,
            Some(Hit::CanvasHandle(false, true)) => CursorIcon::NsResize,
            Some(Hit::Field(_)) => CursorIcon::Text,
            Some(Hit::Canvas) => {
                let pixel = self.pixel(self.pointer);
                if self
                    .editor
                    .selection
                    .as_ref()
                    .is_some_and(|sel| sel.bounds().contains(pixel))
                    && self.editor.tool.is_selection()
                {
                    CursorIcon::Move
                } else if !self.editor.bitmap().bounds().contains(pixel) {
                    CursorIcon::Default
                } else {
                    match self.editor.tool {
                        Tool::Text => CursorIcon::Text,
                        Tool::Zoom => {
                            if self.zoom == 1 {
                                CursorIcon::ZoomIn
                            } else {
                                CursorIcon::ZoomOut
                            }
                        }
                        _ => CursorIcon::Crosshair,
                    }
                }
            }
            _ => CursorIcon::Default,
        }
    }
    pub fn pointer_down(&mut self, right: bool) {
        if self.button.is_some() {
            return;
        }
        if self.viewing {
            self.viewing = false;
            return;
        }
        let Some(hit) = self.hit(self.pointer) else {
            self.menu = None;
            self.zoom_menu = false;
            return;
        };
        let now = Instant::now();
        let q = self.pointer;
        let double = self.last_click.is_some_and(|(old, a, was_right, before)| {
            old == hit
                && was_right == right
                && now.duration_since(before) < Duration::from_millis(380)
                && (a.x - q.x).abs() <= 5
                && (a.y - q.y).abs() <= 5
        });
        self.last_click = if double {
            None
        } else {
            Some((hit, q, right, now))
        };
        self.tooltip = false;
        self.hover_time = now;
        if self.dialog.is_some() {
            self.dialog_click(hit, double);
            return;
        }
        if self.menu.is_some()
            && !matches!(
                hit,
                Hit::Menu(_)
                    | Hit::MenuZoom
                    | Hit::Invoke(_)
                    | Hit::System
                    | Hit::Minimize
                    | Hit::Maximize
            )
        {
            self.menu = None;
            self.zoom_menu = false;
            return;
        }
        if !matches!(
            hit,
            Hit::FontFamily | Hit::FontSize | Hit::FontChoice(_) | Hit::SizeChoice(_)
        ) {
            self.font_menu = None;
        }
        match hit {
            Hit::Menu(index) => {
                self.menu = (self.menu != Some(index)).then_some(index);
                self.zoom_menu = false;
                self.menu_index = None;
            }
            Hit::MenuZoom => {
                self.zoom_menu = true;
                self.menu_index = None;
            }
            Hit::Invoke(cmd) => self.invoke(cmd),
            Hit::Title if !right => {
                self.window_action = Some(if double {
                    WindowAction::Maximize
                } else {
                    WindowAction::Drag
                })
            }
            Hit::Title | Hit::System => {
                self.menu = if self.menu == Some(6) { None } else { Some(6) };
                self.menu_index = None;
            }
            Hit::WindowEdge(dir) if !right => self.window_action = Some(WindowAction::Resize(dir)),
            Hit::Minimize => {
                self.menu = None;
                self.window_action = Some(WindowAction::Minimize);
            }
            Hit::Maximize => {
                self.menu = None;
                self.window_action = Some(WindowAction::Maximize);
            }
            Hit::Close => self.invoke(Command::Exit),
            Hit::Tool(tool) => self.editor.set_tool(tool, &mut self.fonts),
            Hit::Color(index) => {
                self.editor.palette_slot = index;
                if double && !right {
                    self.show_form(FormKind::Colors);
                } else {
                    self.editor.set_color(self.editor.palette[index], right);
                }
            }
            Hit::EditColor => self.show_form(FormKind::Colors),
            Hit::Width(index) => self.editor.stroke_width = index,
            Hit::Brush(index) => self.editor.brush = index,
            Hit::Eraser(index) => self.editor.eraser = index,
            Hit::Spray(index) => self.editor.spray = index,
            Hit::Fill(index) => {
                self.editor.fill_mode = [FillMode::Outline, FillMode::Both, FillMode::Solid][index]
            }
            Hit::Magnify(n) => {
                self.preferred_zoom = n.max(2);
                self.change_zoom(n, None, None);
            }
            Hit::Opaque(on) => self.editor.opaque = on,
            Hit::FontFamily => self.font_menu = (self.font_menu != Some(false)).then_some(false),
            Hit::FontSize => self.font_menu = (self.font_menu != Some(true)).then_some(true),
            Hit::FontChoice(family) => {
                self.editor.text_style.family = family;
                self.font_menu = None;
            }
            Hit::SizeChoice(size) => {
                self.editor.text_style.size = size;
                self.font_menu = None;
            }
            Hit::Bold => self.editor.text_style.bold = !self.editor.text_style.bold,
            Hit::Italic => self.editor.text_style.italic = !self.editor.text_style.italic,
            Hit::Underline => self.editor.text_style.underline = !self.editor.text_style.underline,
            Hit::ThumbnailClose => self.thumbnail = false,
            Hit::Scroll(x, y) => {
                self.origin.x += x * 12;
                self.origin.y += y * 12;
                self.constrain_scroll();
            }
            Hit::Track(vertical) => {
                let layout = self.layout();
                let rect = ui::thumb(self, vertical);
                if vertical {
                    self.origin.y +=
                        (if q.y < rect.y { -1 } else { 1 }) * (layout.view.h / self.zoom);
                } else {
                    self.origin.x +=
                        (if q.x < rect.x { -1 } else { 1 }) * (layout.view.w / self.zoom);
                }
                self.constrain_scroll();
            }
            Hit::Thumb(vertical) => {
                self.button = Some(right);
                self.scroll_drag = Some((
                    vertical,
                    if vertical { q.y } else { q.x },
                    if vertical {
                        self.origin.y
                    } else {
                        self.origin.x
                    },
                ));
            }
            Hit::SelectionHandle(edge) => {
                self.button = Some(right);
                self.editor.begin_selection_resize(self.pixel(q), edge);
            }
            Hit::CanvasHandle(w, h) => {
                self.button = Some(right);
                self.editor.begin_resize(w, h, &mut self.fonts);
            }
            Hit::Canvas => {
                let pixel = self.pixel(q);
                if self.editor.tool == Tool::Zoom {
                    if self.editor.bitmap().bounds().contains(pixel) {
                        let factor = if right || self.zoom > 1 {
                            1
                        } else {
                            self.preferred_zoom
                        };
                        self.change_zoom(factor, Some(pixel), Some(q));
                    }
                } else {
                    self.editor.start(
                        pixel,
                        right,
                        self.mods.shift_key(),
                        self.mods.control_key(),
                        double,
                        &mut self.fonts,
                    );
                    self.button = self.editor.gesture.is_some().then_some(right);
                    self.next_spray = now + Duration::from_millis(32);
                }
            }
            _ => {}
        }
    }
    pub fn pointer_up(&mut self, right: bool) {
        if self.button == Some(right) {
            self.button = None;
            self.scroll_drag = None;
            self.editor.end(&mut self.fonts);
            self.constrain_scroll();
        }
    }
    pub fn wheel(&mut self, x: i32, y: i32) {
        if let Some(Dialog::File(dialog)) = &mut self.dialog {
            dialog.offset = (dialog.offset as i32 - y * 3)
                .max(0)
                .min(dialog.entries.len().saturating_sub(ui::FILE_ROWS) as i32)
                as usize;
        } else if self.dialog.is_none() {
            if self.mods.control_key() {
                let factor = (self.zoom + y.signum()).clamp(1, 10);
                self.change_zoom(factor, Some(self.pixel(self.pointer)), Some(self.pointer));
            } else {
                let (x, y) = if self.mods.shift_key() {
                    (x + y, 0)
                } else {
                    (x, y)
                };
                self.origin.x -= x * 24;
                self.origin.y -= y * 24;
                self.constrain_scroll();
            }
        }
    }
    fn dialog_click(&mut self, hit: Hit, double: bool) {
        let Some(mut state) = self.dialog.take() else {
            return;
        };
        if matches!(hit, Hit::ModalClose | Hit::ModalCancel) {
            if let Dialog::Alert(Alert {
                action: AlertAction::Overwrite(_, _, dialog),
                ..
            }) = state
            {
                self.dialog = Some(Dialog::File(*dialog));
            }
            return;
        }
        match &mut state {
            Dialog::File(dialog) => match hit {
                Hit::Scroll(_, y) => {
                    let limit = dialog.entries.len().saturating_sub(ui::FILE_ROWS) as i32;
                    dialog.offset = (dialog.offset as i32 + y).clamp(0, limit) as usize;
                }

                Hit::Field(index) => {
                    dialog.focus = index;
                    if let Some(field) = dialog.field(index) {
                        field.select_all();
                    }
                    dialog.dropdown = false;
                }
                Hit::FileUp => {
                    if let Some(parent) = dialog.folder.parent() {
                        dialog.navigate(parent.to_path_buf());
                    }
                }
                Hit::FileHome => {
                    if let Some(home) = std::env::var_os("HOME") {
                        dialog.navigate(PathBuf::from(home));
                    }
                }
                Hit::FileEntry(index) => {
                    if let Some(entry) = dialog.entries.get(index) {
                        dialog.selected = Some(index);
                        dialog.focus = 1;
                        dialog.name = Field::new(entry.name.clone());
                        dialog.dropdown = false;
                        if double {
                            if entry.dir {
                                dialog.navigate(entry.path.clone());
                            } else if let Dialog::File(dialog) = state {
                                self.accept_file(dialog);
                                return;
                            }
                        }
                    }
                }
                Hit::FileFormat => dialog.dropdown = !dialog.dropdown,
                Hit::Format(format) => {
                    dialog.format = format;
                    dialog.dropdown = false;
                }
                Hit::ModalAccept => {
                    if let Dialog::File(dialog) = state {
                        self.accept_file(dialog);
                        return;
                    }
                }
                _ => dialog.dropdown = false,
            },
            Dialog::Form(form) => match hit {
                Hit::Field(index) => {
                    form.focus = index;
                    if let Some(f) = form.input.get_mut(index) {
                        f.select_all();
                    }
                }
                Hit::Option(index) => {
                    let (group, value) = (index / 8, index % 8);
                    if form.kind == FormKind::Attributes && group == 1 && form.options[1] != value {
                        let old = units_per_pixel(form.options[1]);
                        let new = units_per_pixel(value);
                        for field in &mut form.input {
                            if let Ok(amount) = field.value.parse::<f64>() {
                                let pixels = (amount / old).round();
                                *field = Field::new(if value == 0 {
                                    format!("{pixels:.0}")
                                } else {
                                    format!("{:.4}", pixels * new)
                                });
                            }
                        }
                    }
                    if let Some(option) = form.options.get_mut(group) {
                        *option = value;
                    }
                }
                Hit::ColorSample(color) => {
                    form.input = [(color >> 16) & 255, (color >> 8) & 255, color & 255]
                        .into_iter()
                        .map(|v| Field::new(v.to_string()))
                        .collect();
                    form.input.push(Field::new(format!("{color:06X}")));
                }
                Hit::ModalAccept => {
                    if let Dialog::Form(form) = state {
                        self.accept_form(form);
                        return;
                    }
                }
                _ => {}
            },
            Dialog::Alert(_) => {
                if matches!(hit, Hit::ModalAccept | Hit::ModalAlt) {
                    if let Dialog::Alert(alert) = state {
                        self.accept_alert(alert.action, hit == Hit::ModalAlt);
                    }
                    return;
                }
            }
            Dialog::Help(tab) => match hit {
                Hit::HelpTab(index) => *tab = index,
                Hit::ModalAccept => return,
                _ => {}
            },
            Dialog::About => {
                if hit == Hit::ModalAccept {
                    return;
                }
            }
            Dialog::Preview => match hit {
                Hit::Invoke(cmd) => {
                    self.invoke(cmd);
                    return;
                }
                Hit::ModalAccept => return,
                _ => {}
            },
        }
        self.dialog = Some(state);
    }
    fn accept_alert(&mut self, action: AlertAction, alt: bool) {
        match action {
            AlertAction::Dismiss => {}
            AlertAction::Unsaved(pending) => {
                if alt {
                    self.do_pending(pending);
                } else {
                    self.save_current(Some(pending));
                }
            }
            AlertAction::Overwrite(path, format, dialog) => {
                self.save_file(path, format, dialog.mode, dialog.continuation);
            }
            AlertAction::Wallpaper(tiled) => {
                self.editor.settle(&mut self.fonts);
                match files::wallpaper(self.editor.bitmap(), tiled) {
                    Ok(()) => self.set_note("Desktop wallpaper updated."),
                    Err(error) => {
                        self.error(format!("Could not update desktop wallpaper.\n{error}"))
                    }
                }
            }
        }
    }
    pub fn key(&mut self, key: &Key, produced: Option<&str>) {
        if self.viewing {
            self.viewing = false;
            return;
        }
        if self.font_menu.is_some() && *key == Key::Named(NamedKey::Escape) {
            self.font_menu = None;
            return;
        }
        if self.dialog.is_some() {
            self.dialog_key(key, produced);
            return;
        }
        if self.menu.is_some() && self.menu_key(key) {
            return;
        }
        let ctrl = self.mods.control_key() || self.mods.super_key();
        let shift = self.mods.shift_key();
        if let Key::Character(ch) = key {
            let ch = ch.to_ascii_lowercase();
            if self.mods.alt_key()
                && let Some(idx) = ["f", "e", "v", "i", "c", "h"].iter().position(|&s| s == ch)
            {
                self.menu = Some(idx);
                self.menu_index = None;
                return;
            }
            if ctrl {
                if let Some(text) = &mut self.editor.text {
                    match ch.as_str() {
                        "a" => {
                            text.select_all();
                            return;
                        }
                        "c" | "x" => {
                            files::clipboard_text_put(text.selected());
                            if ch == "x" {
                                text.insert("");
                            }
                            return;
                        }
                        "v" => {
                            if let Some(value) = files::clipboard_text_get() {
                                text.insert(&value.replace('\r', ""));
                            }
                            return;
                        }
                        _ => {}
                    }
                }
                let action = match ch.as_str() {
                    "n" if shift => Some(Command::Clear),
                    "n" => Some(Command::New),
                    "o" => Some(Command::Open),
                    "s" if shift => Some(Command::SaveAs),
                    "s" => Some(Command::Save),
                    "p" => Some(Command::Print),
                    "z" if shift => Some(Command::Redo),
                    "z" => Some(Command::Undo),
                    "y" => Some(Command::Redo),
                    "x" => Some(Command::Cut),
                    "c" => Some(Command::Copy),
                    "v" => Some(Command::Paste),
                    "a" => Some(Command::SelectAll),
                    "r" => Some(Command::Turn),
                    "w" => Some(Command::Stretch),
                    "i" => Some(Command::Invert),
                    "e" => Some(Command::Attributes),
                    "g" => Some(Command::Grid),
                    "t" => Some(Command::Toolbox),
                    "l" => Some(Command::ColorBox),
                    "1" | "pageup" => Some(Command::Zoom(1)),
                    "2" => Some(Command::Zoom(4)),
                    "+" | "=" => Some(Command::Zoom((self.zoom + 1).min(10))),
                    "-" => Some(Command::Zoom((self.zoom - 1).max(1))),
                    _ => None,
                };
                if let Some(action) = action {
                    self.invoke(action);
                }
                return;
            }
        }
        if !ctrl
            && !self.mods.alt_key()
            && let Some(text) = &mut self.editor.text
        {
            match key {
                Key::Named(NamedKey::Escape) => self.editor.cancel(),
                Key::Named(NamedKey::Enter) => text.insert("\n"),
                Key::Named(NamedKey::Backspace) => text.delete(true),
                Key::Named(NamedKey::Delete) => text.delete(false),
                Key::Named(NamedKey::ArrowUp) => text.movement(TextMove::Up, shift),
                Key::Named(NamedKey::ArrowDown) => text.movement(TextMove::Down, shift),
                _ if movement(key).is_some() => text.movement(movement(key).unwrap(), shift),
                _ => {
                    if let Some(value) = produced {
                        text.insert(
                            &value
                                .chars()
                                .filter(|ch| !ch.is_control())
                                .collect::<String>(),
                        );
                    }
                }
            }
            return;
        }
        if let Key::Named(name) = key {
            match name {
                NamedKey::Escape => self.editor.cancel(),
                NamedKey::Enter => self.editor.settle(&mut self.fonts),
                NamedKey::Delete => self.invoke(Command::Delete),
                NamedKey::F1 => self.invoke(Command::Help),
                NamedKey::F10 => {
                    self.menu = Some(0);
                    self.menu_index = None;
                }
                NamedKey::F11 => self.invoke(Command::ViewBitmap),
                NamedKey::F4 if self.mods.alt_key() => self.invoke(Command::Exit),
                NamedKey::Insert if ctrl => self.invoke(Command::Copy),
                NamedKey::Insert if shift => self.invoke(Command::Paste),
                NamedKey::ArrowLeft => self.origin.x -= 12,
                NamedKey::ArrowRight => self.origin.x += 12,
                NamedKey::ArrowUp => self.origin.y -= 12,
                NamedKey::ArrowDown => self.origin.y += 12,
                _ => {}
            }
            self.constrain_scroll();
        }
    }
    pub fn text_input(&mut self, text: &str) {
        if let Some(field) = self.focused_field() {
            field.insert(text);
            self.sync_color_fields();
        } else if self.dialog.is_none()
            && let Some(target) = &mut self.editor.text
        {
            target.insert(text);
        }
    }
    fn sync_color_fields(&mut self) {
        if let Some(Dialog::Form(form)) = &mut self.dialog
            && form.kind == FormKind::Colors
            && let Some(color) = ui::form_color(form)
        {
            if form.focus == 3 {
                for (n, v) in [(color >> 16) & 255, (color >> 8) & 255, color & 255]
                    .into_iter()
                    .enumerate()
                {
                    form.input[n] = Field::new(v.to_string());
                }
            } else {
                form.input[3] = Field::new(format!("{color:06X}"));
            }
        }
    }
    fn focused_field(&mut self) -> Option<&mut Field> {
        match self.dialog.as_mut()? {
            Dialog::File(state) => state.field(state.focus),
            Dialog::Form(state) => state.input.get_mut(state.focus),
            _ => None,
        }
    }
    fn dialog_key(&mut self, key: &Key, data: Option<&str>) {
        let ctrl = self.mods.control_key() || self.mods.super_key();
        let shift = self.mods.shift_key();
        match key {
            Key::Named(NamedKey::Escape) => {
                self.dialog_click(Hit::ModalCancel, false);
                return;
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(Dialog::File(file)) = &mut self.dialog
                    && file.focus == 0
                {
                    let path = file.path_for(&file.folder_field.value);
                    file.navigate(path);
                    return;
                }
                self.dialog_click(Hit::ModalAccept, false);
                return;
            }
            Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp) => {
                if let Some(Dialog::File(file)) = &mut self.dialog
                    && !file.entries.is_empty()
                {
                    let up = *key == Key::Named(NamedKey::ArrowUp);
                    let i = file.selected.map_or(0, |old| {
                        if up {
                            old.saturating_sub(1)
                        } else {
                            (old + 1).min(file.entries.len() - 1)
                        }
                    });
                    file.selected = Some(i);
                    file.offset = file
                        .offset
                        .min(i)
                        .max((i + 1).saturating_sub(ui::FILE_ROWS));
                    file.name = Field::new(file.entries[i].name.clone());
                    file.focus = 1;
                    return;
                }
            }
            Key::Named(NamedKey::Tab) => {
                match self.dialog.as_mut() {
                    Some(Dialog::File(dialog)) => {
                        dialog.focus = 1 - dialog.focus;
                    }
                    Some(Dialog::Form(form)) if !form.input.is_empty() => {
                        let len = form.input.len();
                        form.focus = (form.focus + if shift { len - 1 } else { 1 }) % len;
                    }
                    _ => {}
                }
                if let Some(field) = self.focused_field() {
                    field.select_all();
                }
                return;
            }
            _ => {}
        }
        if let Some(Dialog::Alert(Alert {
            action: AlertAction::Unsaved(_),
            ..
        })) = &self.dialog
        {
            if let Key::Character(ch) = key {
                if ch.eq_ignore_ascii_case("n") {
                    self.dialog_click(Hit::ModalAlt, false);
                } else if ch.eq_ignore_ascii_case("y") {
                    self.dialog_click(Hit::ModalAccept, false);
                }
            }
            return;
        }
        if let Some(field) = self.focused_field() {
            if ctrl {
                if let Key::Character(ch) = key {
                    match ch.to_ascii_lowercase().as_str() {
                        "a" => field.select_all(),
                        "v" => {
                            if let Some(value) = files::clipboard_text_get() {
                                field.insert(value.lines().next().unwrap_or(""));
                            }
                        }
                        "x" | "c" => {
                            files::clipboard_text_put(&field.value[field.range()]);
                            if ch.eq_ignore_ascii_case("x") {
                                field.insert("");
                            }
                        }
                        _ => {}
                    }
                }
                self.sync_color_fields();
                return;
            }
            match key {
                Key::Named(NamedKey::Delete) => field.delete(false),
                Key::Named(NamedKey::Backspace) => field.delete(true),
                _ if movement(key).is_some() => field.movement(movement(key).unwrap(), shift),
                _ => {
                    if let Some(data) = data {
                        field.insert(&data.chars().filter(|c| !c.is_control()).collect::<String>());
                    }
                }
            }
        }
        self.sync_color_fields();
    }
    fn menu_key(&mut self, key: &Key) -> bool {
        let Some(menu) = self.menu else {
            return false;
        };
        let entries = ui::menu_items(self, menu, self.zoom_menu);
        let options: Vec<_> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.enabled && entry.hit.is_some())
            .collect();
        match key {
            Key::Named(NamedKey::Escape) => {
                if self.zoom_menu {
                    self.zoom_menu = false;
                } else {
                    self.menu = None;
                }
                self.menu_index = None;
            }
            Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp) if !options.is_empty() => {
                let up = *key == Key::Named(NamedKey::ArrowUp);
                let current = options
                    .iter()
                    .position(|(idx, _)| Some(*idx) == self.menu_index)
                    .unwrap_or(if up { 0 } else { options.len() - 1 });
                let next = (current + if up { options.len() - 1 } else { 1 }) % options.len();
                self.menu_index = Some(options[next].0);
            }
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                let left = *key == Key::Named(NamedKey::ArrowLeft);
                if left && self.zoom_menu {
                    self.zoom_menu = false;
                } else if !left
                    && self
                        .menu_index
                        .and_then(|i| entries.get(i))
                        .is_some_and(|e| e.hit == Some(Hit::MenuZoom))
                {
                    self.zoom_menu = true;
                } else {
                    self.menu = Some((menu.min(5) + if left { 5 } else { 1 }) % 6);
                    self.zoom_menu = false;
                }
                self.menu_index = None;
            }
            Key::Named(NamedKey::Enter) => {
                match self
                    .menu_index
                    .and_then(|i| entries.get(i))
                    .and_then(|entry| entry.hit)
                {
                    Some(Hit::Invoke(cmd)) => self.invoke(cmd),
                    Some(Hit::MenuZoom) => {
                        self.zoom_menu = true;
                        self.menu_index = None;
                    }
                    _ => {}
                }
            }
            Key::Character(ch) if ch.chars().count() == 1 => {
                if let Some(entry) = entries.iter().find(|entry| {
                    entry.enabled
                        && entry
                            .label
                            .to_ascii_lowercase()
                            .starts_with(ch.to_ascii_lowercase().as_str())
                }) {
                    if let Some(Hit::Invoke(cmd)) = entry.hit {
                        self.invoke(cmd);
                    } else if entry.hit == Some(Hit::MenuZoom) {
                        self.zoom_menu = true;
                        self.menu_index = None;
                    }
                } else {
                    self.menu = None;
                    self.zoom_menu = false;
                    return false;
                }
            }
            _ => return false,
        }
        true
    }
    pub fn animate(&mut self, now: Instant) -> bool {
        let mut redraw = false;
        if self.focused && now >= self.next_caret {
            self.next_caret = now + Duration::from_millis(525);
            if self.dialog.is_some() || self.editor.text.is_some() {
                self.caret = !self.caret;
                redraw = true;
            }
        }
        if !self.tooltip
            && self.menu.is_none()
            && self.dialog.is_none()
            && self.editor.gesture.is_none()
            && now.duration_since(self.hover_time) >= Duration::from_millis(570)
            && matches!(self.hit(self.pointer), Some(Hit::Tool(_)))
        {
            self.tooltip = true;
            redraw = true;
        }
        if self.editor.stroke_active() && now >= self.next_spray {
            self.editor.spray_tick();
            self.next_spray = now + Duration::from_millis(32);
            redraw = true;
        }
        if self.note.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.note = None;
            redraw = true;
        }
        redraw
    }
    pub fn animation_deadline(&self) -> Option<Instant> {
        let (mut deadlines, active) = ([None; 4], self.focused);
        if active && (self.dialog.is_some() || self.editor.text.is_some()) {
            deadlines[0] = Some(self.next_caret);
        }
        if active
            && !self.tooltip
            && self.menu.is_none()
            && self.dialog.is_none()
            && self.editor.gesture.is_none()
            && matches!(self.hit(self.pointer), Some(Hit::Tool(_)))
        {
            deadlines[1] = Some(self.hover_time + Duration::from_millis(570));
        }
        if self.editor.stroke_active() {
            deadlines[2] = Some(self.next_spray);
        }
        deadlines[3] = self.note.as_ref().map(|(_, until)| *until);
        deadlines.into_iter().flatten().min()
    }
    pub fn lost_focus(&mut self) {
        if let Some(right) = self.button {
            self.pointer_up(right);
        }
        self.tooltip = false;
    }
}

fn units_per_pixel(option: usize) -> f64 {
    [1.0, 1.0 / 96.0, 2.54 / 96.0][option]
}

fn movement(key: &Key) -> Option<TextMove> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some(TextMove::Left),
        Key::Named(NamedKey::ArrowRight) => Some(TextMove::Right),
        Key::Named(NamedKey::Home) => Some(TextMove::Home),
        Key::Named(NamedKey::End) => Some(TextMove::End),
        _ => None,
    }
}
