use crate::files::{self, Format};
use crate::raster::*;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Lasso,
    Select,
    Eraser,
    Fill,
    Picker,
    Zoom,
    Pencil,
    Brush,
    Spray,
    Text,
    Line,
    Curve,
    Rectangle,
    Polygon,
    Ellipse,
    Rounded,
}

impl Tool {
    pub const ALL: [Self; 16] = [
        Self::Lasso,
        Self::Select,
        Self::Eraser,
        Self::Fill,
        Self::Picker,
        Self::Zoom,
        Self::Pencil,
        Self::Brush,
        Self::Spray,
        Self::Text,
        Self::Line,
        Self::Curve,
        Self::Rectangle,
        Self::Polygon,
        Self::Ellipse,
        Self::Rounded,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Lasso => "Free-Form Select",
            Self::Select => "Select",
            Self::Eraser => "Eraser",
            Self::Fill => "Fill With Color",
            Self::Picker => "Pick Color",
            Self::Zoom => "Magnifier",
            Self::Pencil => "Pencil",
            Self::Brush => "Brush",
            Self::Spray => "Airbrush",
            Self::Text => "Text",
            Self::Line => "Line",
            Self::Curve => "Curve",
            Self::Rectangle => "Rectangle",
            Self::Polygon => "Polygon",
            Self::Ellipse => "Ellipse",
            Self::Rounded => "Rounded Rectangle",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Lasso => "Drag around the area you want to select.",
            Self::Select => "Drag to select a rectangular area.",
            Self::Eraser => "Drag to erase. Right-drag to replace the foreground color.",
            Self::Fill => "Click an enclosed area to fill it with color.",
            Self::Picker => "Click an image pixel to pick its color.",
            Self::Zoom => "Click to magnify. Right-click to return to normal size.",
            Self::Pencil => "Drag to draw a one-pixel line.",
            Self::Brush => "Drag to paint with the selected brush.",
            Self::Spray => "Hold and drag to spray color.",
            Self::Text => "Drag a text box, then type.",
            Self::Line => "Drag to draw a line. Hold Shift to constrain its angle.",
            Self::Curve => "Drag a line, then drag twice to bend it.",
            Self::Rectangle => "Drag to draw a rectangle. Hold Shift for a square.",
            Self::Polygon => "Drag the first edge, click each corner, then double-click to finish.",
            Self::Ellipse => "Drag to draw an ellipse. Hold Shift for a circle.",
            Self::Rounded => "Drag to draw a rounded rectangle.",
        }
    }
    pub fn is_selection(self) -> bool {
        matches!(self, Self::Lasso | Self::Select)
    }
    pub fn shape(self) -> Option<Shape> {
        match self {
            Self::Rectangle => Some(Shape::Rectangle),
            Self::Ellipse => Some(Shape::Ellipse),
            Self::Rounded => Some(Shape::Rounded),
            _ => None,
        }
    }
}

pub const PALETTE: [Color; 28] = [
    0x000000, 0x7e7e7e, 0x7d1721, 0x806618, 0x176b36, 0x176c71, 0x26236e, 0x713276, 0x7e8060,
    0x225747, 0x248494, 0x345281, 0x62277f, 0x794725, 0xffffff, 0xc5c5c5, 0xf43839, 0xffdb3d,
    0x37db60, 0x36e1dc, 0x325aff, 0xf13ecf, 0xffec89, 0x76deac, 0x8bdde9, 0xa7acff, 0xe296f2,
    0xf8a060,
];
pub const WIDTHS: [i32; 5] = [1, 2, 3, 4, 5];

#[derive(Clone)]
struct Snapshot {
    bitmap: Bitmap,
    mono: bool,
    id: u64,
}

const HISTORY_BYTES: usize = 96 * 1024 * 1024;

struct Source {
    rect: Rect,
    mask: Option<Vec<bool>>,
}
pub struct Selection {
    pub at: Point,
    pub bitmap: Bitmap,
    source: Option<Source>,
    modified: bool,
}
impl Selection {
    pub fn bounds(&self) -> Rect {
        r(self.at.x, self.at.y, self.bitmap.w, self.bitmap.h)
    }
    fn changed(&self) -> bool {
        self.modified
            || self
                .source
                .as_ref()
                .is_none_or(|source| source.rect.origin() != self.at)
    }
    fn erase_source(&self, dest: &mut Bitmap, background: Color) {
        if let Some(Source { rect, mask }) = &self.source {
            if let Some(mask) = mask {
                for y in 0..rect.h {
                    for x in 0..rect.w {
                        if mask[(y * rect.w + x) as usize] {
                            dest.put(p(rect.x + x, rect.y + y), background);
                        }
                    }
                }
            } else {
                dest.fill(*rect, background);
            }
        }
    }
    fn draw(&self, dest: &mut Bitmap, background: Color, opaque: bool) {
        self.erase_source(dest, background);
        dest.blit(&self.bitmap, self.at, (!opaque).then_some(background));
    }
}

pub struct TextBox {
    pub rect: Rect,
    pub value: String,
    pub right: bool,
    cursor: usize,
    anchor: usize,
}
impl TextBox {
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }
    pub fn insert(&mut self, value: &str) {
        if self.value.len() + value.len() > 32_768 {
            return;
        }
        let range = self.range();
        self.cursor = range.start + value.len();
        self.anchor = self.cursor;
        self.value.replace_range(range, value);
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.value.len();
    }
    pub fn selected(&self) -> &str {
        &self.value[self.range()]
    }
    pub fn caret_text(&self) -> &str {
        &self.value[..self.cursor]
    }
    pub fn movement(&mut self, movement: TextMove, select: bool) {
        let before = &self.value[..self.cursor];
        let after = &self.value[self.cursor..];
        self.cursor = match movement {
            TextMove::Left => before.char_indices().next_back().map_or(0, |(i, _)| i),
            TextMove::Right => self.cursor + after.chars().next().map_or(0, char::len_utf8),
            TextMove::Home => before.rfind('\n').map_or(0, |i| i + 1),
            TextMove::End => self.cursor + after.find('\n').unwrap_or(after.len()),
            TextMove::Up | TextMove::Down => {
                let start = before.rfind('\n').map_or(0, |i| i + 1);
                let end = self.cursor + after.find('\n').unwrap_or(after.len());
                let column = self.value[start..self.cursor].chars().count();
                let adjacent = if matches!(movement, TextMove::Up) {
                    start
                        .checked_sub(1)
                        .map(|end| (self.value[..end].rfind('\n').map_or(0, |i| i + 1), end))
                } else {
                    (end < self.value.len()).then(|| {
                        let start = end + 1;
                        (
                            start,
                            start
                                + self.value[start..]
                                    .find('\n')
                                    .unwrap_or(self.value.len() - start),
                        )
                    })
                };
                adjacent.map_or(self.cursor, |(start, end)| {
                    self.value[start..end]
                        .char_indices()
                        .nth(column)
                        .map_or(end, |(i, _)| start + i)
                })
            }
        };
        if !select {
            self.anchor = self.cursor;
        }
    }
    pub fn delete(&mut self, backwards: bool) {
        if self.anchor == self.cursor {
            self.movement(
                if backwards {
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

pub enum TextMove {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

#[derive(Clone)]
enum Draft {
    Curve {
        a: Point,
        b: Point,
        first: Option<Point>,
        right: bool,
    },
    Polygon {
        points: Vec<Point>,
        right: bool,
    },
}

pub enum Gesture {
    Stroke {
        last: Point,
        right: bool,
    },
    Band {
        a: Point,
        b: Point,
        right: bool,
    },
    Lasso {
        points: Vec<Point>,
    },
    Move {
        offset: Point,
        stamp: bool,
    },
    ResizeSelection {
        original: Rect,
        offset: Point,
        edge: Point,
        bounds: Rect,
    },
    Control {
        at: Point,
    },
    Resize {
        width: bool,
        height: bool,
        size: Point,
    },
}

pub struct Editor {
    doc: Snapshot,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    next_id: u64,
    saved: u64,
    pub path: Option<PathBuf>,
    pub format: Format,
    pub tool: Tool,
    prior_tool: Tool,
    pub fg: Color,
    pub bg: Color,
    pub palette: [Color; 28],
    pub palette_slot: usize,
    pub stroke_width: usize,
    pub brush: usize,
    pub eraser: usize,
    pub spray: usize,
    pub fill_mode: FillMode,
    pub opaque: bool,
    pub text_style: TextStyle,
    pub text: Option<TextBox>,
    pub selection: Option<Selection>,
    pub clipboard: Option<Bitmap>,
    pub gesture: Option<Gesture>,
    draft: Option<Draft>,
    rng: u32,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            doc: Snapshot {
                bitmap: Bitmap::new(720, 480, WHITE),
                mono: false,
                id: 0,
            },
            undo: VecDeque::new(),
            redo: Vec::new(),
            next_id: 1,
            saved: 0,
            path: None,
            format: Format::Bitmap,
            tool: Tool::Pencil,
            prior_tool: Tool::Pencil,
            fg: BLACK,
            bg: WHITE,
            palette: PALETTE,
            palette_slot: 0,
            stroke_width: 0,
            brush: 1,
            eraser: 1,
            spray: 1,
            fill_mode: FillMode::Outline,
            opaque: true,
            text_style: TextStyle::default(),
            text: None,
            selection: None,
            clipboard: None,
            gesture: None,
            draft: None,
            rng: 0x62b82175,
        }
    }
    pub fn bitmap(&self) -> &Bitmap {
        &self.doc.bitmap
    }
    pub fn mono(&self) -> bool {
        self.doc.mono
    }
    pub fn dirty(&self) -> bool {
        self.doc.id != self.saved
            || self.selection.as_ref().is_some_and(Selection::changed)
            || self
                .text
                .as_ref()
                .is_some_and(|text| !text.value.is_empty())
            || self.draft.as_ref().is_some_and(|draft| match draft {
                Draft::Curve { .. } => true,
                Draft::Polygon { points, .. } => points.len() >= 2,
            })
    }
    pub fn saved(&mut self) {
        self.saved = self.doc.id;
    }
    pub fn reset(&mut self, bitmap: Bitmap, format: Format, path: Option<PathBuf>) {
        self.doc = Snapshot {
            bitmap,
            mono: format == Format::Monochrome,
            id: self.next_id,
        };
        self.next_id += 1;
        self.saved();
        self.undo.clear();
        self.redo.clear();
        self.selection = None;
        self.text = None;
        self.gesture = None;
        self.draft = None;
        self.path = path;
        self.format = format;
        if self.mono() {
            self.quantize();
            self.fg = files::monochrome(self.fg);
            self.bg = files::monochrome(self.bg);
        }
    }
    fn record(&mut self) {
        self.undo.push_back(self.doc.clone());
        self.redo.clear();
        self.doc.id = self.next_id;
        self.next_id += 1;
        let mut used = self.doc.bitmap.pixels.len() * 4;
        for (n, snapshot) in self.undo.iter().rev().enumerate() {
            used += snapshot.bitmap.pixels.len() * 4;
            if n > 0 && (used > HISTORY_BYTES || n >= 30) {
                let keep = n;
                self.undo.drain(..self.undo.len() - keep);
                break;
            }
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
            || self.selection.as_ref().is_some_and(Selection::changed)
            || self.text.is_some()
            || self.draft.is_some()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        let had_live_edit = self.text.take().is_some()
            || self.selection.as_ref().is_some_and(Selection::changed)
            || self.draft.is_some();
        self.selection = None;
        self.draft = None;
        self.gesture = None;
        if !had_live_edit && let Some(previous) = self.undo.pop_back() {
            self.redo.push(std::mem::replace(&mut self.doc, previous));
            self.normalize_colors();
        }
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.selection = None;
            self.text = None;
            self.draft = None;
            self.gesture = None;
            self.undo.push_back(std::mem::replace(&mut self.doc, next));
            self.normalize_colors();
        }
    }
    pub fn color(&self, raw: Color) -> Color {
        if self.mono() {
            files::monochrome(raw)
        } else {
            raw
        }
    }
    fn normalize_colors(&mut self) {
        self.fg = self.color(self.fg);
        self.bg = self.color(self.bg);
    }
    pub fn set_color(&mut self, color: Color, right: bool) {
        let color = self.color(color);
        if right {
            self.bg = color;
        } else {
            self.fg = color;
        }
    }
    fn colors(&self, right: bool) -> (Color, Color) {
        if right {
            (self.bg, self.fg)
        } else {
            (self.fg, self.bg)
        }
    }
    fn shape_style(&self, right: bool) -> ShapeStyle {
        let (ink, paper) = self.colors(right);
        ShapeStyle {
            width: WIDTHS[self.stroke_width],
            ink,
            paper,
            fill: self.fill_mode,
        }
    }
    pub fn set_tool(&mut self, tool: Tool, fonts: &mut Fonts) {
        if self.tool == tool {
            return;
        }
        if !tool.is_selection() || !self.tool.is_selection() {
            self.settle(fonts);
        }
        self.prior_tool = self.tool;
        self.tool = tool;
    }
    pub fn settle(&mut self, fonts: &mut Fonts) {
        self.commit_selection();
        self.finish_drawing(fonts);
        self.gesture = None;
    }

    pub fn finish_drawing(&mut self, fonts: &mut Fonts) {
        self.commit_text(fonts);
        if let Some(draft) = self.draft.take() {
            if matches!(&draft, Draft::Polygon { points, .. } if points.len() < 2) {
                return;
            }
            self.record();
            let style = self.shape_style(draft.right());
            render_draft(&mut self.doc.bitmap, &draft, None, style);
        }
    }
    fn draw_text(&self, target: &mut Bitmap, fonts: &mut Fonts, cursor: bool) {
        let Some(text) = &self.text else {
            return;
        };
        let (ink, paper) = self.colors(text.right);
        if self.opaque {
            target.fill(text.rect, paper);
        }
        let bounds = text.rect.inset(2);
        fonts.draw(target, &text.value, bounds, self.text_style, ink, true);
        if cursor && bounds.w > 0 && bounds.h > 0 {
            let location = fonts.caret(text.caret_text(), bounds, self.text_style);
            target.fill(
                r(
                    location.x.min(bounds.right() - 1),
                    location.y,
                    1,
                    i32::from(self.text_style.size),
                )
                .intersect(bounds),
                ink,
            );
        }
    }
    pub fn commit_text(&mut self, fonts: &mut Fonts) {
        if self
            .text
            .as_ref()
            .is_some_and(|text| !text.value.is_empty())
        {
            self.record();
            let mut bitmap = std::mem::replace(&mut self.doc.bitmap, Bitmap::new(1, 1, WHITE));
            self.draw_text(&mut bitmap, fonts, false);
            self.doc.bitmap = bitmap;
            self.quantize();
        }
        self.text = None;
    }
    fn commit_selection(&mut self) {
        if let Some(selection) = self.selection.take()
            && selection.changed()
        {
            self.record();
            selection.draw(&mut self.doc.bitmap, self.bg, self.opaque);
            self.quantize();
        }
    }
    fn quantize(&mut self) {
        if self.doc.mono {
            self.doc
                .bitmap
                .pixels
                .iter_mut()
                .for_each(|c| *c = files::monochrome(*c));
        }
    }
    pub fn cancel(&mut self) {
        self.selection = None;
        self.text = None;
        self.gesture = None;
        self.draft = None;
    }
    pub fn select_all(&mut self, fonts: &mut Fonts) {
        self.settle(fonts);
        self.tool = Tool::Select;
        self.select_rect(self.doc.bitmap.bounds(), None);
    }
    fn select_rect(&mut self, rect: Rect, mask: Option<Vec<bool>>) {
        let rect = rect.intersect(self.doc.bitmap.bounds());
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        let mut bitmap = self.doc.bitmap.extract(rect);
        if let Some(mask) = &mask {
            for (color, &selected) in bitmap.pixels.iter_mut().zip(mask) {
                if !selected {
                    *color = HOLE;
                }
            }
        }
        self.selection = Some(Selection {
            at: rect.origin(),
            bitmap,
            source: Some(Source { rect, mask }),
            modified: false,
        });
    }
    pub fn copy(&mut self) {
        if let Some(selection) = &self.selection {
            let mut bitmap = selection.bitmap.clone();
            if !self.opaque {
                for value in &mut bitmap.pixels {
                    if *value == self.bg {
                        *value = HOLE;
                    }
                }
            }
            self.clipboard = Some(bitmap);
        }
    }
    pub fn delete_selection(&mut self) {
        if let Some(selection) = self.selection.take()
            && selection.source.is_some()
        {
            self.record();
            selection.erase_source(&mut self.doc.bitmap, self.bg);
        }
    }
    pub fn paste(&mut self, mut image: Bitmap, origin: Point, fonts: &mut Fonts) {
        self.settle(fonts);
        if self.mono() {
            image
                .pixels
                .iter_mut()
                .filter(|color| **color != HOLE)
                .for_each(|color| *color = files::monochrome(*color));
        }
        self.tool = Tool::Select;
        self.selection = Some(Selection {
            at: origin,
            bitmap: image,
            source: None,
            modified: true,
        });
    }
    pub fn crop(&mut self) {
        if let Some(selection) = self.selection.take() {
            self.record();
            self.doc.bitmap =
                selection
                    .bitmap
                    .resize(selection.bitmap.w, selection.bitmap.h, self.bg);
        }
    }
    pub fn invert(&mut self) {
        if let Some(selection) = &mut self.selection {
            selection.bitmap.invert();
            selection.modified = true;
        } else {
            self.record();
            self.doc.bitmap.invert();
        }
    }
    pub fn turn(&mut self, turn: Turn) {
        if let Some(selection) = &mut self.selection {
            selection.bitmap = selection.bitmap.turn(turn);
            selection.modified = true;
        } else {
            self.record();
            self.doc.bitmap = self.doc.bitmap.turn(turn);
        }
    }
    pub fn stretch(&mut self, sx: f64, sy: f64, ax: f64, ay: f64) -> bool {
        if let Some(selection) = &mut self.selection {
            if let Some(bitmap) = selection.bitmap.stretch(sx, sy, ax, ay, HOLE) {
                selection.bitmap = bitmap;
                selection.modified = true;
                return true;
            }
        } else if let Some(bitmap) = self.doc.bitmap.stretch(sx, sy, ax, ay, self.bg) {
            self.record();
            self.doc.bitmap = bitmap;
            return true;
        }
        false
    }
    pub fn clear(&mut self, fonts: &mut Fonts) {
        self.settle(fonts);
        self.record();
        self.doc.bitmap.pixels.fill(self.bg);
    }
    pub fn attributes(&mut self, w: i32, h: i32, mono: bool, fonts: &mut Fonts) {
        self.settle(fonts);
        if (w, h, mono) == (self.doc.bitmap.w, self.doc.bitmap.h, self.doc.mono) {
            return;
        }
        self.record();
        if (w, h) != (self.doc.bitmap.w, self.doc.bitmap.h) {
            self.doc.bitmap = self.doc.bitmap.resize(w, h, self.bg);
        }
        self.doc.mono = mono;
        self.quantize();
        self.normalize_colors();
        if mono {
            self.format = Format::Monochrome;
        } else if self.format == Format::Monochrome {
            self.format = Format::Bitmap;
        }
    }
    pub fn begin_resize(&mut self, width: bool, height: bool, fonts: &mut Fonts) {
        self.settle(fonts);
        self.gesture = Some(Gesture::Resize {
            width,
            height,
            size: p(self.doc.bitmap.w, self.doc.bitmap.h),
        });
    }
    pub fn begin_selection_resize(&mut self, point: Point, edge: Point) {
        if let Some(selection) = &self.selection {
            let rect = selection.bounds();
            let anchor = p(
                if edge.x < 0 { rect.x } else { rect.right() - 1 },
                if edge.y < 0 {
                    rect.y
                } else {
                    rect.bottom() - 1
                },
            );
            self.gesture = Some(Gesture::ResizeSelection {
                original: rect,
                offset: p(point.x - anchor.x, point.y - anchor.y),
                edge,
                bounds: rect,
            });
        }
    }
    pub fn selection_bounds(&self) -> Option<Rect> {
        if let Some(Gesture::ResizeSelection { bounds, .. }) = self.gesture {
            Some(bounds)
        } else {
            self.selection.as_ref().map(Selection::bounds)
        }
    }
    pub fn stroke_active(&self) -> bool {
        self.tool == Tool::Spray && matches!(self.gesture, Some(Gesture::Stroke { .. }))
    }
    pub fn spray_tick(&mut self) {
        if let Some(Gesture::Stroke { last, right }) = self.gesture {
            let color = self.colors(right).0;
            self.spray_at(last, color);
        }
    }
    fn random(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }
    fn spray_at(&mut self, at: Point, ink: Color) {
        let radius = [4, 8, 13][self.spray];
        for _ in 0..radius * 2 {
            let diameter = (radius * 2 + 1) as u32;
            let delta = p(
                (self.random() % diameter) as i32 - radius,
                (self.random() % diameter) as i32 - radius,
            );
            if delta.x * delta.x + delta.y * delta.y <= radius * radius {
                self.doc.bitmap.put(p(at.x + delta.x, at.y + delta.y), ink);
            }
        }
    }
    fn stroke(&mut self, a: Point, b: Point, right: bool) {
        let (ink, _) = self.colors(right);
        match self.tool {
            Tool::Pencil => walk_line(a, b, |q| self.doc.bitmap.put(q, ink)),
            Tool::Eraser => {
                let size = [3, 5, 9, 13][self.eraser];
                let replace = right.then_some(self.fg);
                walk_line(a, b, |q| {
                    self.doc.bitmap.dot(q, size, Tip::Square, self.bg, replace)
                });
            }
            Tool::Brush => {
                let size = [3, 6, 10][self.brush % 3];
                let tip = [Tip::Round, Tip::Square, Tip::Left, Tip::Right][self.brush / 3];
                walk_line(a, b, |q| self.doc.bitmap.dot(q, size, tip, ink, None));
            }
            Tool::Spray => {
                let mut step = 0;
                walk_line(a, b, |q| {
                    if step % 3 == 0 {
                        self.spray_at(q, ink);
                    }
                    step += 1;
                });
            }
            _ => {}
        }
    }
    pub fn start(
        &mut self,
        at: Point,
        right: bool,
        shift: bool,
        ctrl: bool,
        double: bool,
        fonts: &mut Fonts,
    ) {
        if let Some(selection) = &self.selection {
            if selection.bounds().contains(at) && self.tool.is_selection() {
                let offset = p(at.x - selection.at.x, at.y - selection.at.y);
                if ctrl || shift {
                    let bitmap = selection.bitmap.clone();
                    let location = selection.at;
                    self.commit_selection();
                    if shift {
                        self.record();
                        self.doc
                            .bitmap
                            .blit(&bitmap, location, (!self.opaque).then_some(self.bg));
                    }
                    self.paste(bitmap, location, fonts);
                }
                self.gesture = Some(Gesture::Move {
                    offset,
                    stamp: shift,
                });
                return;
            }
            self.commit_selection();
        }
        if self
            .text
            .as_ref()
            .is_some_and(|text| text.rect.contains(at))
        {
            return;
        }
        self.commit_text(fonts);
        if !self.doc.bitmap.bounds().contains(at) {
            return;
        }

        if let Some(draft) = &mut self.draft {
            match draft {
                Draft::Curve { .. } => self.gesture = Some(Gesture::Control { at }),
                Draft::Polygon { points, .. } => {
                    if points.last() != Some(&at) {
                        points.push(at);
                    }
                    let close = points.len() >= 3
                        && ((at.x - points[0].x).abs() <= 3 && (at.y - points[0].y).abs() <= 3);
                    if double || right || close {
                        self.settle(fonts);
                    }
                }
            }
            return;
        }

        match self.tool {
            Tool::Fill => {
                let (ink, _) = self.colors(right);
                if self.doc.bitmap.get(at) != ink {
                    self.record();
                    self.doc.bitmap.flood(at, ink);
                }
            }
            Tool::Picker => {
                self.set_color(self.doc.bitmap.get(at), right);
                self.tool = if self.prior_tool == Tool::Picker {
                    Tool::Pencil
                } else {
                    self.prior_tool
                };
            }
            Tool::Pencil | Tool::Eraser | Tool::Brush | Tool::Spray => {
                self.record();
                self.stroke(at, at, right);
                self.gesture = Some(Gesture::Stroke { last: at, right });
            }
            Tool::Lasso => self.gesture = Some(Gesture::Lasso { points: vec![at] }),
            Tool::Zoom => {}
            _ => {
                self.gesture = Some(Gesture::Band {
                    a: at,
                    b: at,
                    right,
                })
            }
        }
    }
    pub fn drag(&mut self, at: Point, shift: bool) {
        let bounds = self.doc.bitmap.bounds();
        let clipped = p(at.x.clamp(0, bounds.w - 1), at.y.clamp(0, bounds.h - 1));
        match self.gesture.as_mut() {
            Some(Gesture::Band { a, b, .. }) => {
                *b = if shift {
                    constrain(self.tool, *a, clipped)
                } else {
                    clipped
                };
            }
            Some(Gesture::Stroke { last, right }) => {
                let (old, right) = (*last, *right);
                let next = p(
                    at.x.clamp(-16, bounds.w + 16),
                    at.y.clamp(-16, bounds.h + 16),
                );
                *last = next;
                if old != next {
                    self.stroke(old, next, right);
                }
            }
            Some(Gesture::Lasso { points }) => {
                if points.last() != Some(&clipped) && points.len() < 16_384 {
                    points.push(clipped);
                }
            }
            Some(Gesture::Move { offset, stamp }) => {
                if let Some(selection) = &mut self.selection {
                    let next = p(
                        (at.x - offset.x).clamp(1 - selection.bitmap.w, bounds.w - 1),
                        (at.y - offset.y).clamp(1 - selection.bitmap.h, bounds.h - 1),
                    );
                    if *stamp && selection.at != next {
                        let destination = &mut self.doc.bitmap;
                        let bitmap = &selection.bitmap;
                        let key = (!self.opaque).then_some(self.bg);
                        let spacing = (bitmap.w.min(bitmap.h) / 8).max(1);
                        let mut step = 0;
                        walk_line(selection.at, next, |q| {
                            if step % spacing == 0 || q == next {
                                destination.blit(bitmap, q, key);
                            }
                            step += 1;
                        });
                    }
                    selection.at = next;
                }
            }
            Some(Gesture::ResizeSelection {
                original,
                offset,
                edge,
                bounds,
            }) => {
                let q = p(clipped.x - offset.x, clipped.y - offset.y);
                let left = if edge.x == -1 {
                    q.x.min(original.right() - 1)
                } else {
                    original.x
                };
                let right = if edge.x == 1 {
                    (q.x + 1).max(original.x + 1)
                } else {
                    original.right()
                };
                let top = if edge.y == -1 {
                    q.y.min(original.bottom() - 1)
                } else {
                    original.y
                };
                let bottom = if edge.y == 1 {
                    (q.y + 1).max(original.y + 1)
                } else {
                    original.bottom()
                };
                if valid_size(right - left, bottom - top) {
                    *bounds = r(left, top, right - left, bottom - top);
                }
            }
            Some(Gesture::Control { at }) => *at = clipped,
            Some(Gesture::Resize {
                width,
                height,
                size,
            }) => {
                let w = if *width {
                    at.x.clamp(1, MAX_SIDE)
                } else {
                    bounds.w
                };
                let h = if *height {
                    at.y.clamp(1, MAX_SIDE)
                } else {
                    bounds.h
                };
                if valid_size(w, h) {
                    *size = p(w, h);
                }
            }
            None => {}
        }
    }
    pub fn end(&mut self, fonts: &mut Fonts) {
        match self.gesture.take() {
            Some(Gesture::Band { a, b, right }) => {
                let bounds = Rect::from_points(a, b).intersect(self.doc.bitmap.bounds());
                match self.tool {
                    Tool::Select => {
                        if a != b {
                            self.select_rect(bounds, None);
                        }
                    }
                    Tool::Text => {
                        let mut rect = bounds;
                        if rect.w < 12 {
                            rect.w = (self.doc.bitmap.w - rect.x).min(185);
                        }
                        if rect.h < 12 {
                            rect.h = (self.doc.bitmap.h - rect.y)
                                .min(i32::from(self.text_style.size) * 4);
                        }
                        self.text = Some(TextBox {
                            rect,
                            value: String::new(),
                            right,
                            cursor: 0,
                            anchor: 0,
                        });
                    }
                    Tool::Curve => {
                        self.draft = Some(Draft::Curve {
                            a,
                            b,
                            first: None,
                            right,
                        })
                    }
                    Tool::Polygon => {
                        self.draft = Some(Draft::Polygon {
                            points: if a == b { vec![a] } else { vec![a, b] },
                            right,
                        })
                    }
                    _ => {
                        self.record();
                        let style = self.shape_style(right);
                        render_band(&mut self.doc.bitmap, self.tool, a, b, style);
                    }
                }
            }
            Some(Gesture::Lasso { points }) if points.len() >= 3 => {
                let bounds = point_bounds(&points).intersect(self.doc.bitmap.bounds());
                let mut mask = vec![false; (bounds.w * bounds.h) as usize];
                for y in bounds.y..bounds.bottom() {
                    let mut edges = scanline(&points, y);
                    edges.sort_by(f64::total_cmp);
                    for edge in edges.as_chunks::<2>().0 {
                        for x in (edge[0].ceil() as i32).max(bounds.x)
                            ..(edge[1].ceil() as i32).min(bounds.right())
                        {
                            mask[((y - bounds.y) * bounds.w + x - bounds.x) as usize] = true;
                        }
                    }
                }
                for (&a, &b) in points.iter().zip(points.iter().cycle().skip(1)) {
                    walk_line(a, b, |q| {
                        if bounds.contains(q) {
                            mask[((q.y - bounds.y) * bounds.w + q.x - bounds.x) as usize] = true;
                        }
                    });
                }
                self.select_rect(bounds, Some(mask));
            }
            Some(Gesture::Control { at }) => {
                if let Some(Draft::Curve { a, b, first, right }) = &mut self.draft {
                    if let Some(first) = *first {
                        let (a, b, right) = (*a, *b, *right);
                        self.draft = None;
                        let style = self.shape_style(right);
                        self.record();
                        self.doc
                            .bitmap
                            .bezier([a, first, at, b], style.width, style.ink);
                    } else {
                        *first = Some(at);
                    }
                }
            }
            Some(Gesture::Resize { size, .. }) => {
                self.attributes(size.x, size.y, self.mono(), fonts)
            }
            Some(Gesture::ResizeSelection {
                bounds, original, ..
            }) if bounds != original => {
                if let Some(selection) = &mut self.selection {
                    selection.bitmap = selection.bitmap.scale(bounds.w, bounds.h);
                    selection.at = bounds.origin();
                    selection.modified = true;
                }
            }
            Some(Gesture::Move { stamp: true, .. }) => {
                self.selection = None;
            }
            _ => {}
        }
    }
    pub fn lasso_preview(&self) -> Option<&[Point]> {
        match &self.gesture {
            Some(Gesture::Lasso { points }) => Some(points),
            _ => None,
        }
    }
    pub fn preview(&self, hover: Option<Point>, fonts: &mut Fonts, caret: bool) -> Option<Bitmap> {
        let selection = self.selection.as_ref().filter(|s| s.changed());
        let band = match self.gesture {
            Some(Gesture::Band { a, b, right })
                if !self.tool.is_selection() && self.tool != Tool::Text =>
            {
                Some((a, b, right))
            }
            _ => None,
        };
        let resize = match self.gesture {
            Some(Gesture::ResizeSelection {
                bounds, original, ..
            }) if bounds != original => Some(bounds),
            _ => None,
        };
        if selection.is_none()
            && resize.is_none()
            && band.is_none()
            && self.draft.is_none()
            && self.text.is_none()
        {
            return None;
        }
        let mut bitmap = self.doc.bitmap.clone();
        if let Some(bounds) = resize
            && let Some(selection) = &self.selection
        {
            selection.erase_source(&mut bitmap, self.bg);
            let scaled = selection.bitmap.scale(bounds.w, bounds.h);
            bitmap.blit(&scaled, bounds.origin(), (!self.opaque).then_some(self.bg));
        } else if let Some(s) = selection {
            s.draw(&mut bitmap, self.bg, self.opaque);
        }
        if let Some((a, b, right)) = band {
            render_band(&mut bitmap, self.tool, a, b, self.shape_style(right));
        }
        if let Some(draft) = &self.draft {
            let hover = match self.gesture {
                Some(Gesture::Control { at }) => Some(at),
                _ => hover,
            };
            render_draft(&mut bitmap, draft, hover, self.shape_style(draft.right()));
        }
        self.draw_text(&mut bitmap, fonts, caret);
        Some(bitmap)
    }
    pub fn band_preview(&self) -> Option<Rect> {
        match self.gesture {
            Some(Gesture::Band { a, b, .. })
                if self.tool.is_selection() || self.tool == Tool::Text =>
            {
                Some(Rect::from_points(a, b))
            }
            Some(Gesture::Resize { size, .. }) => Some(r(0, 0, size.x, size.y)),
            _ => None,
        }
    }
}

impl Draft {
    fn right(&self) -> bool {
        match self {
            Self::Curve { right, .. } | Self::Polygon { right, .. } => *right,
        }
    }
}

fn render_band(bitmap: &mut Bitmap, tool: Tool, a: Point, b: Point, style: ShapeStyle) {
    if let Some(shape) = tool.shape() {
        bitmap.shape(shape, Rect::from_points(a, b), style);
    } else {
        bitmap.stroke(a, b, style.width, style.ink);
    }
}

fn render_draft(bitmap: &mut Bitmap, draft: &Draft, hover: Option<Point>, style: ShapeStyle) {
    match draft {
        Draft::Curve { a, b, first, .. } => {
            if let Some(first) = first.or(hover) {
                bitmap.bezier(
                    [*a, first, hover.unwrap_or(first), *b],
                    style.width,
                    style.ink,
                );
            } else {
                bitmap.stroke(*a, *b, style.width, style.ink);
            }
        }
        Draft::Polygon { points, .. } => {
            if let Some(hover) = hover {
                for pair in points.windows(2) {
                    bitmap.stroke(pair[0], pair[1], style.width, style.ink);
                }
                if let Some(last) = points.last() {
                    bitmap.stroke(*last, hover, style.width, style.ink);
                }
            } else {
                bitmap.polygon(points, style);
            }
        }
    }
}

fn constrain(tool: Tool, a: Point, b: Point) -> Point {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if tool.shape().is_some() || tool == Tool::Select {
        let size = dx.abs().min(dy.abs());
        p(a.x + dx.signum() * size, a.y + dy.signum() * size)
    } else if matches!(tool, Tool::Line | Tool::Curve) {
        if dx.abs() >= dy.abs() * 2 {
            p(b.x, a.y)
        } else if dy.abs() >= dx.abs() * 2 {
            p(a.x, b.y)
        } else {
            let size = dx.abs().min(dy.abs());
            p(a.x + dx.signum() * size, a.y + dy.signum() * size)
        }
    } else {
        b
    }
}
