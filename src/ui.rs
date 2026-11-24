use crate::app::{
    AlertAction, Command as Cmd, Dialog, Field, FileMode, Form, FormKind, Hit, Paint,
};
use crate::editor::{Tool, WIDTHS};
use crate::files::{EGA, Format};
use crate::raster::*;
use winit::window::ResizeDirection as Edge;

const FACE: Color = 0xcdcbd1;
const LIGHT: Color = 0xfdfcfc;
const MID: Color = 0xe9e7ed;
const SHADOW: Color = 0x85828e;
const DARK: Color = 0x45424e;
const DESK: Color = 0x8b8994;
const SELECT: Color = 0x2c376d;
const TEXT: Color = 0x1c1b24;
const DISABLED: Color = 0x777481;
const OUTLINE: Color = 0x272639;
const CAPTION: Color = 0x22294f;
const CAPTION_END: Color = 0x6e7ea8;

const NORMAL: TextStyle = TextStyle {
    family: Family::Sans,
    size: 13,
    bold: false,
    italic: false,
    underline: false,
};
const BOLD: TextStyle = TextStyle {
    bold: true,
    ..NORMAL
};
const SMALL: TextStyle = TextStyle { size: 12, ..NORMAL };

pub const FILE_ROWS: usize = 11;

pub struct Layout {
    pub frame: Rect,
    pub title: Rect,
    pub menu: Rect,
    pub sidebar: Rect,
    pub canvas: Rect,
    pub view: Rect,
    pub vertical: Rect,
    pub horizontal: Rect,
    pub palette: Rect,
    pub status: Rect,
    pub textbar: Rect,
}

impl Layout {
    pub fn new(app: &Paint) -> Self {
        let frame = r(0, 0, app.dims.x, app.dims.y);
        let title = r(4, 4, frame.w - 8, 24);
        let menu = r(4, 29, frame.w - 8, 20);
        let status_height = i32::from(app.status_bar) * 26;
        let palette_height = i32::from(app.color_box) * 51;
        let status = r(4, frame.h - 4 - status_height, frame.w - 8, status_height);
        let palette = r(4, status.y - palette_height, frame.w - 8, palette_height);
        let textbar = r(menu.right() - 310, menu.y + 1, 306, menu.h - 2);
        let sidebar = r(4, 50, if app.tool_box { 68 } else { 0 }, palette.y - 50);
        let canvas = r(
            sidebar.right() + 2,
            sidebar.y + 1,
            frame.w - sidebar.right() - 7,
            palette.y - sidebar.y - 4,
        );
        let view = r(canvas.x + 2, canvas.y + 2, canvas.w - 20, canvas.h - 20);
        let vertical = r(view.right(), view.y, 16, view.h);
        let horizontal = r(view.x, view.bottom(), view.w, 16);
        Self {
            frame,
            title,
            menu,
            sidebar,
            canvas,
            view,
            vertical,
            horizontal,
            palette,
            status,
            textbar,
        }
    }
    fn tool_button(&self, n: usize) -> Rect {
        r(
            self.sidebar.x + 5 + (n % 2) as i32 * 29,
            self.sidebar.y + 8 + (n / 2) as i32 * 27,
            29,
            27,
        )
    }
    fn options(&self) -> Rect {
        r(self.sidebar.x + 7, self.sidebar.y + 232, 54, 77)
    }
    fn palette_chip(&self, index: usize) -> Rect {
        r(
            self.palette.x + 53 + (index % 14) as i32 * 17,
            self.palette.y + 8 + (index / 14) as i32 * 17,
            17,
            17,
        )
    }
}

struct Scene<'a> {
    bitmap: &'a mut Bitmap,
    fonts: &'a mut Fonts,
    hits: &'a mut Vec<(Rect, Hit)>,
}

impl Scene<'_> {
    fn fill(&mut self, rect: Rect, color: Color) {
        self.bitmap.fill(rect, color);
    }
    fn pixel(&mut self, point: Point, color: Color) {
        self.bitmap.put(point, color);
    }
    fn h(&mut self, x: i32, y: i32, w: i32, color: Color) {
        self.fill(r(x, y, w, 1), color);
    }
    fn v(&mut self, x: i32, y: i32, h: i32, color: Color) {
        self.fill(r(x, y, 1, h), color);
    }
    fn line(&mut self, a: Point, b: Point, color: Color) {
        self.bitmap.stroke(a, b, 1, color);
    }
    fn edge(&mut self, rect: Rect, tl: Color, br: Color) {
        if rect.w < 1 || rect.h < 1 {
            return;
        }
        self.h(rect.x, rect.y, rect.w - 1, tl);
        self.v(rect.x, rect.y, rect.h - 1, tl);
        self.h(rect.x, rect.bottom() - 1, rect.w, br);
        self.v(rect.right() - 1, rect.y, rect.h, br);
    }
    fn raised(&mut self, rect: Rect, face: Color) {
        self.fill(rect, face);
        self.edge(rect, LIGHT, DARK);
        self.edge(rect.inset(1), MID, SHADOW);
    }
    fn pressed(&mut self, rect: Rect, face: Color) {
        self.fill(rect, face);
        self.edge(rect, DARK, LIGHT);
        self.edge(rect.inset(1), SHADOW, MID);
    }
    fn checkered(&mut self, rect: Rect, background: Color, dot: Color) {
        self.fill(rect, background);
        for y in rect.y..rect.bottom() {
            for x in (rect.x + (y - rect.y).rem_euclid(2)..rect.right()).step_by(2) {
                self.pixel(p(x, y), dot);
            }
        }
    }
    fn register(&mut self, rect: Rect, hit: Hit) {
        self.hits.push((rect, hit));
    }
    fn text(&mut self, rect: Rect, text: &str, color: Color, style: TextStyle) {
        let height = i32::from(style.size) + 2;
        self.fonts.draw(
            self.bitmap,
            text,
            r(rect.x, rect.y + (rect.h - height) / 2, rect.w, height),
            style,
            color,
            false,
        );
    }
    fn center(&mut self, rect: Rect, text: &str, color: Color, style: TextStyle) {
        let w = self.fonts.measure(text, style).min(rect.w);
        self.text(
            r(rect.x + (rect.w - w) / 2, rect.y, w, rect.h),
            text,
            color,
            style,
        );
    }
    fn wrapped(&mut self, rect: Rect, text: &str, color: Color) {
        let mut output = String::new();
        for (n, para) in text.split('\n').enumerate() {
            if n != 0 {
                output.push('\n');
            }
            let mut width = 0;
            for word in para.split_whitespace() {
                let w = self.fonts.measure(word, NORMAL) + 4;
                if width > 0 {
                    if width + w > rect.w {
                        output.push('\n');
                        width = 0;
                    } else {
                        output.push(' ');
                    }
                }
                output.push_str(word);
                width += w;
            }
        }
        self.fonts
            .draw(self.bitmap, &output, rect, NORMAL, color, true);
    }
    fn button(&mut self, rect: Rect, label: &str, hit: Hit, primary: bool) {
        if primary {
            self.edge(rect, TEXT, TEXT);
            self.raised(rect.inset(1), FACE);
        } else {
            self.raised(rect, FACE);
        }
        self.center(rect, label, TEXT, NORMAL);
        self.register(rect, hit);
    }
    fn dotted(&mut self, rect: Rect, clip: Rect) {
        for x in rect.x.max(clip.x)..rect.right().min(clip.right()) {
            for y in [rect.y, rect.bottom() - 1] {
                if clip.contains(p(x, y)) {
                    self.pixel(p(x, y), if (x - rect.x) % 7 < 4 { OUTLINE } else { WHITE });
                }
            }
        }
        for y in rect.y.max(clip.y)..rect.bottom().min(clip.bottom()) {
            for x in [rect.x, rect.right() - 1] {
                if clip.contains(p(x, y)) {
                    self.pixel(p(x, y), if (y - rect.y) % 7 < 4 { OUTLINE } else { WHITE });
                }
            }
        }
    }
    fn gradient(&mut self, rect: Rect, a: Color, b: Color) {
        for x in 0..rect.w {
            self.v(
                rect.x + x,
                rect.y,
                rect.h,
                mix(a, b, (x * 255 / rect.w) as u32),
            );
        }
    }
    fn sprite(&mut self, pos: Point, rows: &[&str]) {
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.bytes().enumerate() {
                let color = match ch {
                    b'#' => OUTLINE,
                    b'w' => WHITE,
                    b's' => 0xbfc4ce,
                    b'd' => 0x77758a,
                    b't' => 0x30ab9c,
                    b'c' => 0x96e5d7,
                    b'o' => 0xee9857,
                    b'y' => 0xffdc7c,
                    b'p' => 0xde9eba,
                    b'b' => 0x6d86cb,
                    _ => continue,
                };
                self.pixel(p(pos.x + x as i32, pos.y + y as i32), color);
            }
        }
    }
}

const LOGO: &[&str] = &[
    "......#####...",
    "....##wwsww#..",
    "..##wywwoww#..",
    ".#wwwwwwww##..",
    "#wcwww##ww#...",
    "#wttw#..##....",
    "#www#.........",
    ".#ww#...#.....",
    ".#wbw#.##.....",
    "..#ww##y#.....",
    "...##o#y#.....",
    "....##o#......",
    "....#o#.......",
    ".....#........",
];

pub fn icon() -> Vec<u8> {
    let mut bitmap = Bitmap::new(16, 16, HOLE);
    let mut fonts = Fonts::new();
    Scene {
        bitmap: &mut bitmap,
        fonts: &mut fonts,
        hits: &mut Vec::new(),
    }
    .sprite(p(1, 1), LOGO);
    bitmap
        .pixels
        .into_iter()
        .flat_map(|c| {
            [
                (c >> 16) as u8,
                (c >> 8) as u8,
                c as u8,
                if c == HOLE { 0 } else { 255 },
            ]
        })
        .collect()
}

pub fn render(app: &mut Paint, target: &mut Bitmap) {
    let layout = app.layout();
    let hover = app.hit(app.pointer);
    let pixel = app.pixel(app.pointer);
    let hover_pixel = (layout.view.contains(app.pointer)
        && app.editor.bitmap().bounds().contains(pixel))
    .then_some(pixel);
    let mut fonts = std::mem::replace(&mut app.fonts, Fonts::empty());
    let mut hits = std::mem::take(&mut app.hits);
    hits.clear();
    let image = app.editor.preview(hover_pixel, &mut fonts, app.caret);
    let image = image.as_ref().unwrap_or_else(|| app.editor.bitmap());
    let mut scene = Scene {
        bitmap: target,
        fonts: &mut fonts,
        hits: &mut hits,
    };
    if app.viewing {
        scene.fill(layout.frame, 0x141422);
        let crop = image.bounds();
        let w = image.w.min(layout.frame.w - 12);
        let h = image.h.min(layout.frame.h - 12);
        let scale = (w as f64 / crop.w as f64).min(h as f64 / crop.h as f64);
        let area = r(
            0,
            0,
            (scale * image.w as f64) as i32,
            (scale * image.h as f64) as i32,
        );
        let at = p((layout.frame.w - area.w) / 2, (layout.frame.h - area.h) / 2);
        scaled(&mut scene, image, r(at.x, at.y, area.w, area.h));
    } else {
        window(&mut scene, app, &layout, hover);
        canvas(&mut scene, app, &layout, image);
        if app.tool_box {
            tools(&mut scene, app, &layout, hover);
        }
        if app.color_box {
            palette(&mut scene, app, &layout);
        }
        if app.status_bar {
            status(&mut scene, app, &layout, hover, hover_pixel);
        }
        if app.thumbnail && app.zoom > 1 {
            thumbnail(&mut scene, &layout, image);
        }
        if app.editor.tool == Tool::Text && app.text_bar {
            textbar(&mut scene, app, layout.textbar);
            if let Some(sizes) = app.font_menu {
                font_choices(&mut scene, app, layout.textbar, sizes, hover);
            }
        }
        if let Some(menu) = app.menu {
            popup(&mut scene, app, menu, hover);
        }
        if let Some(which) = &app.dialog {
            scene.hits.clear();
            dialogs(&mut scene, app, &layout, which, image);
        } else if app.tooltip
            && app.menu.is_none()
            && let Some(Hit::Tool(tool)) = hover
        {
            let label = tool.label();
            let w = scene.fonts.measure(label, SMALL) + 13;
            let rect = r(
                (app.pointer.x + 13).min(layout.frame.w - w - 5),
                app.pointer.y + 17,
                w,
                20,
            );
            scene.fill(r(rect.x + 2, rect.y + 2, rect.w, rect.h), DARK);
            scene.fill(rect, 0xfff6cd);
            scene.edge(rect, TEXT, TEXT);
            scene.text(
                r(rect.x + 6, rect.y, rect.w - 9, rect.h),
                label,
                TEXT,
                SMALL,
            );
        }
    }
    app.fonts = fonts;
    app.hits = hits;
}

fn window(s: &mut Scene, app: &Paint, l: &Layout, hover: Option<Hit>) {
    s.fill(l.frame, FACE);
    s.edge(l.frame, LIGHT, DARK);
    s.edge(l.frame.inset(1), MID, SHADOW);
    s.edge(l.frame.inset(2), FACE, FACE);
    let title = l.title;
    let (dark, end) = if app.focused {
        (CAPTION, CAPTION_END)
    } else {
        (0x595764, 0x9b99a4)
    };
    s.gradient(title, dark, end);
    s.sprite(p(title.x + 5, title.y + 5), LOGO);
    s.text(
        r(title.x + 25, title.y, title.w - 100, title.h),
        &app.title(),
        WHITE,
        BOLD,
    );
    s.register(title, Hit::Title);
    s.register(r(title.x, title.y, 23, title.h), Hit::System);
    for (index, (symbol, hit)) in [
        ("close", Hit::Close),
        ("max", Hit::Maximize),
        ("min", Hit::Minimize),
    ]
    .into_iter()
    .enumerate()
    {
        let x = title.right() - [22, 43, 62][index];
        let button = r(x, title.y + 3, 19, 18);
        s.raised(button, FACE);
        let center = p(x + 9, button.y + 8);
        match symbol {
            "close" => {
                s.line(
                    p(center.x - 3, center.y - 3),
                    p(center.x + 3, center.y + 3),
                    TEXT,
                );
                s.line(
                    p(center.x + 3, center.y - 3),
                    p(center.x - 3, center.y + 3),
                    TEXT,
                );
                s.pixel(center, TEXT);
            }
            "max" => {
                if app.maximized {
                    s.edge(r(center.x - 1, center.y - 4, 7, 6), TEXT, TEXT);
                    s.fill(r(center.x - 4, center.y - 1, 7, 6), FACE);
                    s.edge(r(center.x - 4, center.y - 1, 7, 6), TEXT, TEXT);
                    s.h(center.x - 3, center.y, 5, TEXT);
                } else {
                    s.edge(r(center.x - 4, center.y - 4, 9, 8), TEXT, TEXT);
                    s.h(center.x - 3, center.y - 3, 7, TEXT);
                }
            }
            _ => s.fill(r(center.x - 4, center.y + 2, 7, 2), TEXT),
        }
        s.register(button, hit);
    }
    for (index, label) in ["File", "Edit", "View", "Image", "Colors", "Help"]
        .into_iter()
        .enumerate()
    {
        let rect = menu_label(l, index);
        let on = app.menu == Some(index);
        if on {
            s.fill(rect, SELECT);
        } else if hover == Some(Hit::Menu(index)) {
            s.edge(rect, LIGHT, SHADOW);
        }
        let color = if on { WHITE } else { TEXT };
        let area = r(rect.x + 9, rect.y + 1, rect.w - 14, rect.h - 2);
        s.text(area, label, color, NORMAL);
        if app.menu.is_some() || app.mods.alt_key() {
            let width = s.fonts.measure(&label[..1], NORMAL);
            s.h(area.x, area.y + area.h - 3, width, color);
        }
        s.register(rect, Hit::Menu(index));
    }
    s.h(l.menu.x, l.menu.bottom(), l.menu.w, SHADOW);
    s.h(l.menu.x, l.menu.bottom() + 1, l.menu.w, MID);
    if !app.maximized {
        let w = l.frame.w;
        let h = l.frame.h;
        for (rect, dir) in [
            (r(0, 0, w, 4), Edge::North),
            (r(0, h - 4, w, 4), Edge::South),
            (r(0, 0, 4, h), Edge::West),
            (r(w - 4, 0, 4, h), Edge::East),
            (r(0, 0, 6, 6), Edge::NorthWest),
            (r(w - 6, 0, 6, 6), Edge::NorthEast),
            (r(0, h - 6, 6, 6), Edge::SouthWest),
            (r(w - 6, h - 6, 6, 6), Edge::SouthEast),
        ] {
            s.register(rect, Hit::WindowEdge(dir));
        }
    }
}

fn tools(s: &mut Scene, app: &Paint, l: &Layout, hover: Option<Hit>) {
    let panel = l.sidebar;
    s.v(panel.right() - 1, panel.y + 2, panel.h - 3, SHADOW);
    s.v(panel.right(), panel.y + 2, panel.h - 3, MID);
    for (n, tool) in Tool::ALL.into_iter().enumerate() {
        let button = l.tool_button(n);
        let selected = app.editor.tool == tool;
        if selected {
            s.pressed(button, 0xb2b1c1);
            s.checkered(button.inset(2), 0xc7c6d4, 0xd8d7e2);
        } else {
            s.raised(
                button,
                if hover == Some(Hit::Tool(tool)) {
                    0xdbd9e1
                } else {
                    FACE
                },
            );
        }
        let pt = p(
            button.x + 7 + i32::from(selected),
            button.y + 5 + i32::from(selected),
        );
        tool_icon(s, tool, pt);
        s.register(button, Hit::Tool(tool));
    }
    let options = l.options();
    s.pressed(options, FACE);
    let inset = options.inset(3);
    let ed = &app.editor;
    match ed.tool {
        Tool::Select | Tool::Lasso | Tool::Text => {
            for (i, opaque) in [true, false].into_iter().enumerate() {
                let q = r(inset.x + 1, inset.y + 4 + i as i32 * 30, inset.w - 2, 26);
                option(s, q, ed.opaque == opaque, Hit::Opaque(opaque));
                if opaque {
                    s.fill(r(q.x + 10, q.y + 5, 25, 15), LIGHT);
                } else {
                    s.checkered(r(q.x + 10, q.y + 5, 25, 15), LIGHT, SHADOW);
                }
                s.fill(r(q.x + 14, q.y + 8, 16, 9), 0x91a8d1);
                s.edge(r(q.x + 14, q.y + 8, 16, 9), OUTLINE, OUTLINE);
                s.fill(r(q.x + 18, q.y + 10, 5, 5), 0xf7ce77);
            }
        }
        Tool::Zoom => {
            for (i, n) in [1, 2, 4, 6, 8].into_iter().enumerate() {
                let q = r(inset.x, inset.y + 1 + i as i32 * 14, inset.w, 14);
                option(s, q, app.zoom == n, Hit::Magnify(n));
                s.center(q, &format!("{n}×"), TEXT, SMALL);
            }
        }
        Tool::Line | Tool::Curve => {
            for (i, width) in WIDTHS.into_iter().enumerate() {
                let q = r(inset.x, inset.y + 1 + i as i32 * 14, inset.w, 14);
                option(s, q, ed.stroke_width == i, Hit::Width(i));
                s.fill(
                    r(q.x + 7, q.y + (q.h - width) / 2, q.w - 14, width),
                    OUTLINE,
                );
            }
        }
        Tool::Rectangle | Tool::Ellipse | Tool::Rounded | Tool::Polygon => {
            for (i, fill) in [FillMode::Outline, FillMode::Both, FillMode::Solid]
                .into_iter()
                .enumerate()
            {
                let q = r(inset.x + 1, inset.y + 1 + i as i32 * 23, inset.w - 2, 22);
                option(s, q, ed.fill_mode == fill, Hit::Fill(i));
                let sample = r(q.x + 9, q.y + 5, 28, 12);
                if i != 0 {
                    s.fill(sample, if i == 1 { LIGHT } else { OUTLINE });
                }
                if i != 2 {
                    s.edge(sample, OUTLINE, OUTLINE);
                }
            }
        }
        Tool::Eraser => {
            for (i, width) in [3, 5, 9, 13].into_iter().enumerate() {
                let q = r(inset.x, inset.y + i as i32 * 17, inset.w, 17);
                option(s, q, ed.eraser == i, Hit::Eraser(i));
                let tip = r(
                    q.x + (q.w - width) / 2,
                    q.y + (q.h - width) / 2,
                    width,
                    width,
                );
                s.fill(tip, 0xf2bacd);
                s.edge(tip, OUTLINE, OUTLINE);
            }
        }
        Tool::Brush => {
            for (i, tip) in [Tip::Round, Tip::Square, Tip::Left, Tip::Right]
                .into_iter()
                .enumerate()
            {
                for (j, n) in [3, 6, 10].into_iter().enumerate() {
                    let idx = i * 3 + j;
                    let q = r(inset.x + j as i32 * 16, inset.y + i as i32 * 17, 16, 17);
                    option(s, q, ed.brush == idx, Hit::Brush(idx));
                    s.bitmap.dot(p(q.x + 8, q.y + 8), n, tip, OUTLINE, None);
                }
            }
        }
        Tool::Spray => {
            for (i, rad) in [4i32, 7, 9].into_iter().enumerate() {
                let q = r(inset.x, inset.y + 1 + i as i32 * 23, inset.w, 23);
                option(s, q, ed.spray == i, Hit::Spray(i));
                for y in -rad..=rad {
                    for x in -rad..=rad {
                        if x * x + y * y <= rad * rad && (x * 3 + y * 7 + x * y).rem_euclid(5) == 0
                        {
                            s.pixel(p(q.x + q.w / 2 + x, q.y + 11 + y), OUTLINE);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
fn option(s: &mut Scene, rect: Rect, on: bool, hit: Hit) {
    if on {
        s.pressed(rect, 0xe1dfeb);
        s.checkered(rect.inset(2), 0xd6d5e2, 0xeae8f2);
    }
    s.register(rect, hit);
}

fn tool_icon(s: &mut Scene, tool: Tool, pos: Point) {
    let (x, y) = (pos.x, pos.y);
    let c = OUTLINE;
    let pix = |dx, dy| p(x + dx, y + dy);
    match tool {
        Tool::Lasso => s.sprite(
            pos,
            &[
                "........#.#....",
                ".....#.#...#...",
                "...#.........#.",
                "..#...........#",
                "...............",
                ".#............#",
                ".............#.",
                "#..............",
                "............#..",
                "#........#.#...",
                "....#.#.#......",
                ".#.#...........",
                ".#.............",
                "..#.#..........",
                ".....#.........",
            ],
        ),
        Tool::Select => {
            for n in (1..15).step_by(2) {
                s.pixel(pix(n, 2), c);
                s.pixel(pix(n, 13), c);
            }
            for n in (3..13).step_by(2) {
                s.pixel(pix(1, n), c);
                s.pixel(pix(13, n), c);
            }
        }
        Tool::Eraser => s.sprite(
            pos,
            &[
                "...............",
                ".........###...",
                "........#ppp#..",
                ".......#ppppp#.",
                "......#ppppppp#",
                ".....#ppppppp##",
                "....#ppppppp#d.",
                "...#wwppppp#d..",
                "..#wwwwppp#d...",
                "..#swwwwp#d....",
                "...#swww#d.....",
                "....#ss#d......",
                ".....##d.......",
                "...............",
                "...............",
            ],
        ),
        Tool::Fill => s.sprite(
            pos,
            &[
                "......####.....",
                ".....#....#....",
                ".....#.....#...",
                ".....#....#....",
                "....#..#.#.....",
                "...#cw..#......",
                "..#ccww..#.....",
                ".#ttccww..#....",
                ".#tttccww#..t..",
                "..#tttcw#..ct..",
                "...#ttc#...ctt.",
                "....#t#....ctt.",
                ".....#......t..",
                "..........tttt.",
                "...............",
            ],
        ),
        Tool::Picker => s.sprite(
            pos,
            &[
                ".........###...",
                "........#bb#...",
                ".......#bbb##..",
                "......#bbb#....",
                ".....#sbb#.....",
                "....######.....",
                ".....#ww#......",
                "....#ww#.......",
                "...#wc#........",
                "..#cc#.........",
                "..#t#..........",
                ".#t#...........",
                ".##............",
                "...............",
                "...............",
            ],
        ),
        Tool::Zoom => s.sprite(
            pos,
            &[
                ".....#####.....",
                "...##ccccc##...",
                "..#cwwwwccc#...",
                ".#cwcccwcccc#..",
                ".#cwcccccccc#..",
                ".#ccwccccccc#..",
                ".#cccccccccc#..",
                ".#cccccccccc#..",
                "..#cccccccc#...",
                "...##cccc##....",
                ".....####b#....",
                ".........#b#...",
                "..........#b#..",
                "...........#b#.",
                "............##.",
            ],
        ),
        Tool::Pencil => s.sprite(
            pos,
            &[
                "..........##...",
                ".........#pp#..",
                "........#ppp#..",
                ".......#wpp#...",
                "......#yow#....",
                ".....#yoo#.....",
                "....#yoo#......",
                "...#yoo#.......",
                "..#yoo#........",
                "..#yo#.........",
                ".#ww#..........",
                ".#w#...........",
                ".##............",
                "...............",
                "...............",
            ],
        ),
        Tool::Brush => s.sprite(
            pos,
            &[
                "...........##..",
                "..........#o#..",
                ".........#yo#..",
                "........#yo#...",
                ".......#yo#....",
                "......#yo#.....",
                ".....#oo#......",
                "....#ww#.......",
                "...#sww#.......",
                "..#ddw#........",
                ".#ddd#.........",
                ".#ttd#.........",
                "##tt#..........",
                "####...........",
                "...............",
            ],
        ),
        Tool::Spray => s.sprite(
            pos,
            &[
                ".........t...t.",
                ".......t...t...",
                "....####.t...t.",
                "....#ws#...t...",
                "...######.t..t.",
                "..#wwssdd#.....",
                "..#wccccd#.....",
                "..#wccttd#.....",
                "..#wctttd#.....",
                "..#wctttd#.....",
                "..#wctttd#.....",
                "..#wwssdd#.....",
                "..########.....",
                "...............",
                "...............",
            ],
        ),
        Tool::Text => {
            s.line(pix(3, 13), pix(7, 2), c);
            s.line(pix(8, 2), pix(12, 13), c);
            s.line(pix(9, 4), pix(13, 13), c);
            s.h(x + 5, y + 9, 6, c);
            s.h(x + 1, y + 13, 5, c);
            s.h(x + 10, y + 13, 5, c);
        }
        Tool::Line => s.line(pix(2, 13), pix(13, 2), c),
        Tool::Curve => {
            s.bitmap
                .bezier([pix(1, 13), pix(7, -7), pix(7, 20), pix(14, 3)], 1, c);
        }
        Tool::Rectangle => s.edge(r(x + 1, y + 3, 14, 10), c, c),
        Tool::Polygon => {
            for (a, b) in [
                ((1, 8), (6, 2)),
                ((6, 2), (14, 5)),
                ((14, 5), (12, 13)),
                ((12, 13), (7, 10)),
                ((7, 10), (1, 13)),
                ((1, 13), (1, 8)),
            ] {
                s.line(pix(a.0, a.1), pix(b.0, b.1), c);
            }
        }
        Tool::Ellipse | Tool::Rounded => s.bitmap.shape(
            if tool == Tool::Ellipse {
                Shape::Ellipse
            } else {
                Shape::Rounded
            },
            r(x + 1, y + 2, 14, 12),
            ShapeStyle {
                width: 1,
                ink: c,
                paper: FACE,
                fill: FillMode::Outline,
            },
        ),
    }
}

fn palette(s: &mut Scene, app: &Paint, l: &Layout) {
    let y = l.palette.y;
    s.h(l.palette.x, y + 1, l.palette.w, SHADOW);
    s.h(l.palette.x, y + 2, l.palette.w, LIGHT);
    let stack = r(l.palette.x + 7, y + 7, 39, 36);
    s.pressed(stack, 0xdcd8e0);
    s.checkered(stack.inset(2), 0xdad7e0, 0xeae7ee);
    s.raised(r(stack.x + 15, stack.y + 13, 19, 19), app.editor.bg);
    s.raised(r(stack.x + 5, stack.y + 4, 19, 19), app.editor.fg);
    s.register(stack, Hit::EditColor);
    for (index, &raw) in app.editor.palette.iter().enumerate() {
        let chip = l.palette_chip(index);
        s.pressed(chip, app.editor.color(raw));
        s.register(chip, Hit::Color(index));
    }
}

fn font_field(bar: Rect, sizes: bool) -> Rect {
    let (offset, width) = if sizes { (118, 38) } else { (0, 114) };
    r(bar.x + offset, bar.y, width, bar.h)
}

fn textbar(s: &mut Scene, app: &Paint, rect: Rect) {
    let style = app.editor.text_style;
    let family = match style.family {
        Family::Sans => "Liberation Sans",
        Family::Serif => "Liberation Serif",
        Family::Mono => "Liberation Mono",
    };
    for (sizes, value, hit) in [
        (false, family.to_string(), Hit::FontFamily),
        (true, style.size.to_string(), Hit::FontSize),
    ] {
        let area = font_field(rect, sizes);
        s.pressed(area, WHITE);
        s.text(
            r(area.x + 4, area.y, area.w - 20, area.h),
            &value,
            TEXT,
            SMALL,
        );
        let button = r(area.right() - 15, area.y + 2, 13, area.h - 4);
        s.raised(button, FACE);
        arrow(s, button, 2, TEXT);
        s.register(area, hit);
    }
    for (i, (label, hit, on, style)) in [
        ("B", Hit::Bold, style.bold, BOLD),
        (
            "I",
            Hit::Italic,
            style.italic,
            TextStyle {
                italic: true,
                ..NORMAL
            },
        ),
        (
            "U",
            Hit::Underline,
            style.underline,
            TextStyle {
                underline: true,
                ..NORMAL
            },
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let area = r(rect.x + 160 + i as i32 * 21, rect.y, 20, rect.h);
        if on {
            s.pressed(area, MID);
        } else {
            s.raised(area, FACE);
        }
        s.center(area, label, TEXT, style);
        s.register(area, hit);
    }
    let inset = r(rect.x + 227, rect.y, rect.w - 227, rect.h);
    s.button(
        inset,
        if app.editor.opaque {
            "Opaque"
        } else {
            "Transparent"
        },
        Hit::Opaque(!app.editor.opaque),
        false,
    );
}

fn font_choices(s: &mut Scene, app: &Paint, rect: Rect, sizes: bool, hover: Option<Hit>) {
    let style = app.editor.text_style;
    let choices = if sizes {
        [8, 10, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 64, 72]
            .into_iter()
            .map(|n| (n.to_string(), Hit::SizeChoice(n), style.size == n))
            .collect::<Vec<_>>()
    } else {
        [
            (Family::Sans, "Liberation Sans"),
            (Family::Serif, "Liberation Serif"),
            (Family::Mono, "Liberation Mono"),
        ]
        .into_iter()
        .map(|(family, name)| (name.into(), Hit::FontChoice(family), style.family == family))
        .collect()
    };
    let field = font_field(rect, sizes);
    let bounds = r(
        field.x,
        field.bottom(),
        field.w,
        choices.len() as i32 * 20 + 4,
    );
    s.pressed(bounds, WHITE);
    for (i, (label, hit, current)) in choices.into_iter().enumerate() {
        let row = r(bounds.x + 2, bounds.y + 2 + i as i32 * 20, bounds.w - 4, 20);
        let hot = current || hover == Some(hit);
        if hot {
            s.fill(row, SELECT);
        }
        s.text(
            r(row.x + 5, row.y, row.w - 10, row.h),
            &label,
            if hot { WHITE } else { TEXT },
            NORMAL,
        );
        s.register(row, hit);
    }
}

fn status(s: &mut Scene, app: &Paint, l: &Layout, hover: Option<Hit>, pixel: Option<Point>) {
    let row = l.status;
    let grip = 17;
    let dims_w = 112;
    let coords_w = 100;
    let main = r(
        row.x + 1,
        row.y + 3,
        row.w - grip - dims_w - coords_w - 8,
        row.h - 4,
    );
    let coords = r(main.right() + 3, main.y, coords_w, main.h);
    let dims = r(coords.right() + 3, main.y, dims_w, main.h);
    for rect in [main, coords, dims] {
        s.edge(rect, SHADOW, LIGHT);
    }
    let message = match hover {
        Some(Hit::Tool(t)) => t.hint(),
        Some(Hit::Color(_)) => "Left-click for foreground. Right-click for background.",
        Some(Hit::CanvasHandle(..)) => "Drag to resize the picture.",
        _ => app.note.as_ref().map_or(
            "For Help, click Help Topics on the Help menu.",
            |(text, _)| text.as_str(),
        ),
    };
    s.text(
        r(main.x + 6, main.y, main.w - 10, main.h),
        message,
        TEXT,
        SMALL,
    );
    if let Some(q) = pixel {
        s.text(
            r(coords.x + 6, coords.y, coords.w - 10, coords.h),
            &format!("{}, {}", q.x, q.y),
            TEXT,
            SMALL,
        );
    }
    let bitmap = app.editor.bitmap();
    let size = app
        .editor
        .band_preview()
        .or_else(|| app.editor.selection_bounds())
        .unwrap_or_else(|| bitmap.bounds());
    s.text(
        r(dims.x + 6, dims.y, dims.w - 10, dims.h),
        &format!("{} × {} px", size.w, size.h),
        TEXT,
        SMALL,
    );
    if !app.maximized {
        let pt = p(row.right() - 3, row.bottom() - 3);
        for n in [4, 8, 12] {
            s.line(p(pt.x - n, pt.y), p(pt.x, pt.y - n), SHADOW);
            s.line(p(pt.x - n + 1, pt.y), p(pt.x, pt.y - n + 1), LIGHT);
        }
        s.register(
            r(pt.x - 14, pt.y - 14, 17, 17),
            Hit::WindowEdge(Edge::SouthEast),
        );
    }
}

fn screen_rect(app: &Paint, view: Rect, raw: Rect) -> Rect {
    r(
        view.x + (raw.x - app.origin.x) * app.zoom,
        view.y + (raw.y - app.origin.y) * app.zoom,
        raw.w * app.zoom,
        raw.h * app.zoom,
    )
}

fn canvas(s: &mut Scene, app: &Paint, l: &Layout, bitmap: &Bitmap) {
    s.pressed(l.canvas, DESK);
    let view = l.view;
    let art = screen_rect(app, view, bitmap.bounds());
    let crop = art.intersect(view);
    if app.zoom == 1 && crop.w > 0 {
        for y in crop.y..crop.bottom() {
            let row = ((y - art.y) * bitmap.w + crop.x - art.x) as usize;
            let dst = (y * s.bitmap.w + crop.x) as usize;
            s.bitmap.pixels[dst..dst + crop.w as usize]
                .copy_from_slice(&bitmap.pixels[row..row + crop.w as usize]);
        }
    } else {
        for y in crop.y..crop.bottom() {
            for x in crop.x..crop.right() {
                let color = if app.grid
                    && app.zoom >= 3
                    && ((x - art.x) % app.zoom == 0 || (y - art.y) % app.zoom == 0)
                {
                    0xc2c2cf
                } else {
                    bitmap.get(p((x - art.x) / app.zoom, (y - art.y) / app.zoom))
                };
                s.pixel(p(x, y), color);
            }
        }
    }
    s.register(view, Hit::Canvas);
    if art.right() < view.right() {
        s.v(art.right(), crop.y, crop.h, DARK);
        s.v(art.right() + 1, crop.y, crop.h, SHADOW);
    }
    if art.bottom() < view.bottom() {
        s.h(crop.x, art.bottom(), crop.w, DARK);
        s.h(crop.x, art.bottom() + 1, crop.w, SHADOW);
    }
    if let Some(bounds) = app.editor.selection_bounds() {
        let rect = screen_rect(app, view, bounds);
        s.dotted(rect, view);
        let (mid_x, mid_y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
        for (point, dir) in [
            (p(rect.x, rect.y), p(-1, -1)),
            (p(mid_x, rect.y), p(0, -1)),
            (p(rect.right() - 1, rect.y), p(1, -1)),
            (p(rect.x, mid_y), p(-1, 0)),
            (p(rect.right() - 1, mid_y), p(1, 0)),
            (p(rect.x, rect.bottom() - 1), p(-1, 1)),
            (p(mid_x, rect.bottom() - 1), p(0, 1)),
            (p(rect.right() - 1, rect.bottom() - 1), p(1, 1)),
        ] {
            if rect.w < 9 || rect.h < 9 {
                continue;
            }
            let mark = r(point.x - 2, point.y - 2, 5, 5).intersect(view);
            s.fill(mark, OUTLINE);
            s.fill(r(point.x - 1, point.y - 1, 3, 3).intersect(view), LIGHT);
            s.register(mark, Hit::SelectionHandle(dir));
        }
    } else if app.editor.gesture.is_none() && app.editor.text.is_none() {
        for (at, w, h) in [
            (p(art.right() + 3, art.y + art.h / 2), true, false),
            (p(art.x + art.w / 2, art.bottom() + 3), false, true),
            (p(art.right() + 3, art.bottom() + 3), true, true),
        ] {
            let handle = r(at.x - 3, at.y - 3, 7, 7);
            if view.contains(at) {
                s.fill(r(at.x - 2, at.y - 2, 5, 5).intersect(view), OUTLINE);
                s.fill(r(at.x - 1, at.y - 1, 3, 3).intersect(view), LIGHT);
                s.register(handle.intersect(view), Hit::CanvasHandle(w, h));
            }
        }
    }
    if let Some(rect) = app.editor.band_preview() {
        s.dotted(screen_rect(app, view, rect), view);
    }
    if let Some(text) = &app.editor.text {
        let rect = screen_rect(app, view, text.rect);
        s.dotted(rect, view);
        if !app.ime.is_empty() {
            let txt = r(rect.x + 3, rect.bottom() + 3, rect.w.min(200), 21).intersect(view);
            s.fill(txt, 0xfff6cd);
            s.text(txt, &app.ime, TEXT, SMALL);
        }
    }
    if let Some(points) = app.editor.lasso_preview() {
        for pair in points.windows(2) {
            let loc = |q: Point| screen_rect(app, view, r(q.x, q.y, 1, 1)).origin();
            walk_line(loc(pair[0]), loc(pair[1]), |q| {
                if view.contains(q) {
                    s.pixel(q, if (q.x + q.y) % 6 < 3 { OUTLINE } else { WHITE });
                }
            });
        }
    }
    scroll(s, app, l.vertical, true);
    scroll(s, app, l.horizontal, false);
    s.raised(r(l.vertical.x, l.horizontal.y, 16, 16), FACE);
}

pub fn thumb(app: &Paint, vertical: bool) -> Rect {
    let l = app.layout();
    let bitmap = app.editor.bitmap();
    let max = app.max_scroll(l.view);
    let (track, span, total, at, end) = if vertical {
        (l.vertical, l.view.h, bitmap.h, app.origin.y, max.y)
    } else {
        (l.horizontal, l.view.w, bitmap.w, app.origin.x, max.x)
    };
    let length = (span - 32).max(1);
    let n = if end == 0 {
        length
    } else {
        ((length as i64 * span as i64) / (total as i64 * app.zoom as i64))
            .clamp(16.min(length) as i64, length as i64) as i32
    };
    let offset = if end > 0 { (length - n) * at / end } else { 0 };
    if vertical {
        r(track.x, track.y + 16 + offset, 16, n)
    } else {
        r(track.x + 16 + offset, track.y, n, 16)
    }
}

fn arrow(s: &mut Scene, rect: Rect, dir: usize, color: Color) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    for n in 0..4 {
        let d = 3 - n;
        match dir {
            0 => s.v(x - 2 + n, y - d, 2 * d + 1, color),
            1 => s.v(x + 1 - n, y - d, 2 * d + 1, color),
            2 => s.h(x - d, y - 2 + n, 2 * d + 1, color),
            _ => s.h(x - d, y + 1 - n, 2 * d + 1, color),
        }
    }
}
fn scroll(s: &mut Scene, app: &Paint, rect: Rect, vertical: bool) {
    s.checkered(rect, 0xd3d0db, 0xe6e3eb);
    let max = app.max_scroll(app.layout().view);
    let enabled = if vertical { max.y > 0 } else { max.x > 0 };
    for ends in [false, true] {
        let at = if vertical {
            p(rect.x, if ends { rect.bottom() - 16 } else { rect.y })
        } else {
            p(if ends { rect.right() - 16 } else { rect.x }, rect.y)
        };
        let button = r(at.x, at.y, 16, 16);
        s.raised(button, FACE);
        let dir = match (vertical, ends) {
            (false, false) => 1,
            (false, true) => 0,
            (true, false) => 3,
            (true, true) => 2,
        };
        arrow(s, button, dir, if enabled { TEXT } else { SHADOW });
        if enabled {
            let sign = if ends { 1 } else { -1 };
            s.register(
                button,
                if vertical {
                    Hit::Scroll(0, sign)
                } else {
                    Hit::Scroll(sign, 0)
                },
            );
        }
    }
    if enabled {
        let track = if vertical {
            r(rect.x, rect.y + 16, rect.w, rect.h - 32)
        } else {
            r(rect.x + 16, rect.y, rect.w - 32, rect.h)
        };
        s.register(track, Hit::Track(vertical));
        let block = thumb(app, vertical);
        s.raised(block, FACE);
        s.register(block, Hit::Thumb(vertical));
    }
}
fn scaled(s: &mut Scene, bitmap: &Bitmap, rect: Rect) {
    let crop = rect.intersect(s.bitmap.bounds());
    for y in crop.y..crop.bottom() {
        for x in crop.x..crop.right() {
            s.pixel(
                p(x, y),
                bitmap.get(p(
                    (x - rect.x) * bitmap.w / rect.w,
                    (y - rect.y) * bitmap.h / rect.h,
                )),
            );
        }
    }
}
fn thumbnail(s: &mut Scene, l: &Layout, image: &Bitmap) {
    let rect = r(l.view.right() - 184, l.view.y + 12, 171, 144);
    s.raised(rect, FACE);
    let header = r(rect.x + 3, rect.y + 3, rect.w - 6, 20);
    s.gradient(header, CAPTION, CAPTION_END);
    s.text(
        r(header.x + 5, header.y, header.w - 26, header.h),
        "Thumbnail",
        WHITE,
        BOLD,
    );
    s.button(
        r(header.right() - 18, header.y + 2, 16, 16),
        "×",
        Hit::ThumbnailClose,
        false,
    );
    let ratio = (154.0 / f64::from(image.w)).min(108.0 / f64::from(image.h));
    let (w, h) = (
        (f64::from(image.w) * ratio).max(1.0) as i32,
        (f64::from(image.h) * ratio).max(1.0) as i32,
    );
    let image_rect = r(rect.x + (rect.w - w) / 2, rect.y + 28 + (108 - h) / 2, w, h);
    scaled(s, image, image_rect);
    s.edge(
        r(image_rect.x - 1, image_rect.y - 1, w + 2, h + 2),
        SHADOW,
        LIGHT,
    );
}

pub struct MenuItem {
    pub label: String,
    pub keys: &'static str,
    pub hit: Option<Hit>,
    pub enabled: bool,
    checked: bool,
}
impl MenuItem {
    fn height(&self) -> i32 {
        if self.hit.is_some() { 22 } else { 8 }
    }
}

pub fn menu_items(app: &Paint, index: usize, sub: bool) -> Vec<MenuItem> {
    use Cmd::*;
    let item = |label: &str, keys, cmd| MenuItem {
        label: label.to_owned(),
        keys,
        hit: Some(Hit::Invoke(cmd)),
        enabled: app.enabled(cmd),
        checked: app.checked(cmd),
    };
    let special = |label: &str, hit| MenuItem {
        label: label.to_owned(),
        keys: "",
        hit: Some(hit),
        enabled: true,
        checked: false,
    };
    let sep = || MenuItem {
        label: String::new(),
        keys: "",
        hit: None,
        enabled: false,
        checked: false,
    };
    if sub {
        return vec![
            item("Normal Size", "Ctrl+1", Zoom(1)),
            item("Large Size", "Ctrl+2", Zoom(4)),
            item("Custom...", "", Zoom(0)),
            sep(),
            item("Show Grid", "Ctrl+G", Grid),
            item("Show Thumbnail", "", Thumbnail),
        ];
    }
    match index {
        0 => {
            let mut entries = vec![
                item("New", "Ctrl+N", New),
                item("Open...", "Ctrl+O", Open),
                item("Save", "Ctrl+S", Save),
                item("Save As...", "Ctrl+Shift+S", SaveAs),
                sep(),
                item("Print Preview", "", Preview),
                item("Page Setup...", "", PageSetup),
                item("Print...", "Ctrl+P", Print),
                item("Save as PDF...", "", ExportPdf),
                sep(),
                item("Set As Wallpaper (Tiled)", "", Wallpaper(true)),
                item("Set As Wallpaper (Centered)", "", Wallpaper(false)),
            ];
            if !app.recent.is_empty() {
                entries.push(sep());
                for (i, path) in app.recent.iter().enumerate() {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default();
                    entries.push(item(&format!("{} {name}", i + 1), "", Recent(i)));
                }
            }
            entries.extend([sep(), item("Exit", "Alt+F4", Exit)]);
            entries
        }
        1 => vec![
            item("Undo", "Ctrl+Z", Undo),
            item("Redo", "Ctrl+Y", Redo),
            sep(),
            item("Cut", "Ctrl+X", Cut),
            item("Copy", "Ctrl+C", Copy),
            item("Paste", "Ctrl+V", Paste),
            item("Clear Selection", "Del", Delete),
            item("Select All", "Ctrl+A", SelectAll),
            sep(),
            item("Copy To...", "", CopyTo),
            item("Paste From...", "", PasteFrom),
        ],
        2 => vec![
            item("Tool Box", "Ctrl+T", Toolbox),
            item("Color Box", "Ctrl+L", ColorBox),
            item("Status Bar", "", Status),
            item("Text Toolbar", "", Textbar),
            sep(),
            special("Zoom", Hit::MenuZoom),
            item("View Bitmap", "F11", ViewBitmap),
        ],
        3 => vec![
            item("Flip/Rotate...", "Ctrl+R", Turn),
            item("Stretch/Skew...", "Ctrl+W", Stretch),
            item("Invert Colors", "Ctrl+I", Invert),
            item("Attributes...", "Ctrl+E", Attributes),
            item("Clear Image", "Ctrl+Shift+N", Clear),
            item("Crop to Selection", "", Crop),
            sep(),
            item("Draw Opaque", "", Opaque),
        ],
        4 => vec![item("Edit Colors...", "", Colors)],
        5 => vec![
            item("Help Topics", "F1", Help),
            sep(),
            item("About Paintrs", "", About),
        ],
        _ => vec![
            special("Minimize", Hit::Minimize),
            special(
                if app.maximized { "Restore" } else { "Maximize" },
                Hit::Maximize,
            ),
            sep(),
            item("Close", "Alt+F4", Exit),
        ],
    }
}

fn menu_label(l: &Layout, index: usize) -> Rect {
    const X: [i32; 6] = [0, 39, 79, 124, 177, 231];
    const W: [i32; 6] = [39, 40, 45, 53, 54, 43];
    r(
        l.menu.x + 4 + X[index],
        l.menu.y + 1,
        W[index],
        l.menu.h - 2,
    )
}

fn menu_rect(app: &Paint, n: usize, entries: &[MenuItem]) -> Rect {
    let l = app.layout();
    let widths = [270, 214, 199, 237, 155, 163, 143];
    let x = if n == 6 {
        l.title.x
    } else {
        menu_label(&l, n).x
    };
    let y = if n == 6 {
        l.title.bottom()
    } else {
        l.menu.bottom()
    };
    let height = entries.iter().map(MenuItem::height).sum::<i32>() + 6;
    r(x.min(l.frame.w - widths[n] - 8), y, widths[n], height)
}

pub fn zoom_menu_rect(app: &Paint) -> Rect {
    let entries = menu_items(app, 2, false);
    let main = menu_rect(app, 2, &entries);
    let y = main.y
        + 3
        + entries
            .iter()
            .take_while(|e| e.hit != Some(Hit::MenuZoom))
            .map(MenuItem::height)
            .sum::<i32>();
    let w = 191;
    let h = menu_items(app, 2, true)
        .iter()
        .map(MenuItem::height)
        .sum::<i32>()
        + 6;
    r(main.right() - 3, y - 3, w, h)
}

fn popup(s: &mut Scene, app: &Paint, n: usize, hover: Option<Hit>) {
    let entries = menu_items(app, n, false);
    let bounds = menu_rect(app, n, &entries);
    menu_rows(
        s,
        bounds,
        &entries,
        if app.zoom_menu { None } else { app.menu_index },
        hover,
        app.zoom_menu,
    );
    if app.zoom_menu && n == 2 {
        menu_rows(
            s,
            zoom_menu_rect(app),
            &menu_items(app, n, true),
            app.menu_index,
            hover,
            false,
        );
    }
}

fn menu_rows(
    s: &mut Scene,
    bounds: Rect,
    entries: &[MenuItem],
    active: Option<usize>,
    hover: Option<Hit>,
    submenu: bool,
) {
    s.fill(r(bounds.x + 3, bounds.y + 3, bounds.w, bounds.h), DARK);
    s.raised(bounds, FACE);
    let mut y = bounds.y + 3;
    for (i, item) in entries.iter().enumerate() {
        let row = r(bounds.x + 3, y, bounds.w - 6, item.height());
        y += row.h;
        let Some(hit) = item.hit else {
            s.h(row.x + 3, row.y + 3, row.w - 6, SHADOW);
            s.h(row.x + 3, row.y + 4, row.w - 6, LIGHT);
            continue;
        };
        let selected = item.enabled
            && (active == Some(i) || hover == Some(hit) || (submenu && hit == Hit::MenuZoom));
        let color = if selected {
            WHITE
        } else if item.enabled {
            TEXT
        } else {
            DISABLED
        };
        if selected {
            s.fill(row, SELECT);
        }
        if item.checked {
            s.line(p(row.x + 5, row.y + 11), p(row.x + 8, row.y + 14), color);
            s.line(p(row.x + 8, row.y + 14), p(row.x + 14, row.y + 7), color);
            s.line(p(row.x + 8, row.y + 15), p(row.x + 14, row.y + 8), color);
        }
        s.text(
            r(row.x + 21, row.y, row.w - 25, row.h),
            &item.label,
            color,
            NORMAL,
        );
        if !item.keys.is_empty() {
            let w = s.fonts.measure(item.keys, SMALL);
            s.text(
                r(row.right() - 12 - w, row.y, w, row.h),
                item.keys,
                color,
                SMALL,
            );
        }
        if hit == Hit::MenuZoom {
            arrow(s, r(row.right() - 19, row.y + 3, 16, 16), 0, color);
        }
        if item.enabled {
            s.register(row, hit);
        }
    }
}

fn modal(s: &mut Scene, l: &Layout, w: i32, h: i32, title: &str) -> Rect {
    let box_r = r((l.frame.w - w) / 2, (l.frame.h - h) / 2, w, h);
    s.fill(r(box_r.x + 5, box_r.y + 5, box_r.w, box_r.h), DARK);
    s.raised(box_r, FACE);
    s.edge(box_r.inset(2), FACE, FACE);
    let head = r(box_r.x + 4, box_r.y + 4, box_r.w - 8, 23);
    s.gradient(head, CAPTION, CAPTION_END);
    s.text(
        r(head.x + 7, head.y, head.w - 38, head.h),
        title,
        WHITE,
        BOLD,
    );
    s.button(
        r(head.right() - 22, head.y + 3, 18, 17),
        "×",
        Hit::ModalClose,
        false,
    );
    box_r
}

fn dialogs(s: &mut Scene, app: &Paint, l: &Layout, dialog: &Dialog, image: &Bitmap) {
    match dialog {
        Dialog::File(file) => file_box(s, app, l, file),
        Dialog::Form(form) => form_box(s, app, l, form),
        Dialog::Alert(alert) => {
            let bounds = modal(s, l, 396, 153, alert.title);
            let circle = r(bounds.x + 19, bounds.y + 47, 32, 32);
            s.bitmap.shape(
                Shape::Ellipse,
                circle,
                ShapeStyle {
                    width: 1,
                    ink: OUTLINE,
                    paper: if matches!(alert.action, AlertAction::Dismiss) {
                        0xffd888
                    } else {
                        0x93a9d5
                    },
                    fill: FillMode::Both,
                },
            );
            let glyph = if matches!(alert.action, AlertAction::Dismiss) {
                "!"
            } else {
                "?"
            };
            s.center(circle, glyph, OUTLINE, TextStyle { size: 23, ..BOLD });
            s.wrapped(
                r(bounds.x + 64, bounds.y + 47, bounds.w - 82, 59),
                &alert.message,
                TEXT,
            );
            let y = bounds.bottom() - 38;
            match alert.action {
                AlertAction::Dismiss => {
                    s.button(r(bounds.x + 153, y, 90, 25), "OK", Hit::ModalAccept, true)
                }
                AlertAction::Unsaved(_) => {
                    s.button(r(bounds.x + 54, y, 86, 25), "Yes", Hit::ModalAccept, true);
                    s.button(r(bounds.x + 155, y, 86, 25), "No", Hit::ModalAlt, false);
                    s.button(
                        r(bounds.x + 256, y, 86, 25),
                        "Cancel",
                        Hit::ModalCancel,
                        false,
                    );
                }
                _ => {
                    s.button(r(bounds.x + 101, y, 90, 25), "Yes", Hit::ModalAccept, true);
                    s.button(r(bounds.x + 205, y, 90, 25), "No", Hit::ModalCancel, false);
                }
            }
        }
        Dialog::Help(tab) => help(s, l, *tab),
        Dialog::About => {
            let bounds = modal(s, l, 337, 193, "About Paintrs");
            s.sprite(p(bounds.x + 27, bounds.y + 54), LOGO);
            s.text(
                r(bounds.x + 52, bounds.y + 44, 253, 35),
                "Paintrs",
                OUTLINE,
                TextStyle { size: 24, ..BOLD },
            );
            s.wrapped(
                r(bounds.x + 27, bounds.y + 91, 283, 53),
                "A classic bitmap editor.\nVersion 0.1.0",
                TEXT,
            );
            s.button(
                r(bounds.x + 123, bounds.bottom() - 39, 90, 25),
                "OK",
                Hit::ModalAccept,
                true,
            );
        }
        Dialog::Preview => preview(s, app, l, image),
    }
}

fn field(s: &mut Scene, rect: Rect, value: &Field, focus: bool, caret: bool, index: usize) {
    s.pressed(rect, WHITE);
    let inner = rect.inset(3);
    let width = s.fonts.measure(&value.value[..value.cursor], NORMAL);
    let offset = if focus {
        (width - inner.w + 2).max(0)
    } else {
        0
    };
    let range = value.range();
    let a = s.fonts.measure(&value.value[..range.start], NORMAL);
    let b = s.fonts.measure(&value.value[..range.end], NORMAL);
    let start = inner.x - offset;
    // Draw into the edit field so a long path never overflows the neighboring controls.
    let mut strip = Bitmap::new(inner.w, inner.h, WHITE);
    if focus && !range.is_empty() {
        strip.fill(r(a - offset, 1, b - a, inner.h - 2), SELECT);
    }
    let baseline = (inner.h - 15) / 2;
    if focus && !range.is_empty() {
        for (x, word, color) in [
            (-offset, &value.value[..range.start], TEXT),
            (a - offset, &value.value[range.clone()], WHITE),
            (b - offset, &value.value[range.end..], TEXT),
        ] {
            s.fonts.draw(
                &mut strip,
                word,
                r(x, baseline, inner.w - x, 15),
                NORMAL,
                color,
                false,
            );
        }
    } else {
        s.fonts.draw(
            &mut strip,
            &value.value,
            r(start - inner.x, baseline, inner.w + offset, 15),
            NORMAL,
            TEXT,
            false,
        );
        if focus && caret {
            strip.fill(r(width - offset, 2, 1, inner.h - 4), TEXT);
        }
    }
    s.bitmap.blit(&strip, inner.origin(), None);
    s.register(rect, Hit::Field(index));
}

fn file_box(s: &mut Scene, app: &Paint, l: &Layout, file: &crate::app::FileBox) {
    let b = modal(s, l, 527, 385, file.mode.title());
    let (x, y) = (b.x + 13, b.y + 37);
    s.text(
        r(x, y, 78, 24),
        if file.mode.saving() {
            "Save in:"
        } else {
            "Look in:"
        },
        TEXT,
        NORMAL,
    );
    field(
        s,
        r(x + 73, y, 362, 24),
        &file.folder_field,
        file.focus == 0,
        app.caret,
        0,
    );
    s.button(r(x + 440, y, 26, 24), "↑", Hit::FileUp, false);
    s.button(r(x + 471, y, 27, 24), "⌂", Hit::FileHome, false);
    let list = r(x, y + 33, b.w - 26, 213);
    s.pressed(list, WHITE);
    for (n, entry) in file
        .entries
        .iter()
        .enumerate()
        .skip(file.offset)
        .take(FILE_ROWS)
    {
        let row = r(
            list.x + 3,
            list.y + 3 + ((n - file.offset) as i32) * 19,
            list.w - 23,
            19,
        );
        let selected = file.selected == Some(n);
        if selected {
            s.fill(row, SELECT);
        }
        let pos = p(row.x + 5, row.y + 4);
        if entry.dir {
            s.sprite(
                pos,
                &[
                    ".####.........",
                    ".#yy#######...",
                    "##yyyyyyyy#...",
                    "#oooooooooo#..",
                    "#oyyyyyyyyy#..",
                    "#yyyyyyyyyy#..",
                    "############..",
                ],
            );
        } else {
            s.sprite(
                pos,
                &[
                    ".#######......",
                    ".#wwwws##.....",
                    ".#wwwwss#.....",
                    ".#wcbbw##.....",
                    ".#wttoww#.....",
                    ".#wwwwww#.....",
                    ".########.....",
                ],
            );
        }
        s.text(
            r(row.x + 26, row.y, row.w - 30, row.h),
            &entry.name,
            if selected { WHITE } else { TEXT },
            NORMAL,
        );
        s.register(row, Hit::FileEntry(n));
    }
    if file.entries.is_empty() {
        s.text(
            r(list.x + 8, list.y + 5, list.w - 25, 20),
            "This folder is empty.",
            DISABLED,
            NORMAL,
        );
    }
    let bar = r(list.right() - 18, list.y + 2, 16, list.h - 4);
    s.checkered(bar, 0xd3d0db, 0xe6e3eb);
    for (yy, dir, hit) in [
        (bar.y, 3, Hit::Scroll(0, -1)),
        (bar.bottom() - 16, 2, Hit::Scroll(0, 1)),
    ] {
        let area = r(bar.x, yy, 16, 16);
        s.raised(area, FACE);
        arrow(
            s,
            area,
            dir,
            if file.entries.len() > FILE_ROWS {
                TEXT
            } else {
                SHADOW
            },
        );
        if file.entries.len() > FILE_ROWS {
            s.register(area, hit);
        }
    }
    if file.entries.len() > FILE_ROWS {
        let track = bar.h - 32;
        let h = (track * FILE_ROWS as i32 / file.entries.len() as i32).max(16);
        let shift = (track - h) * file.offset as i32 / (file.entries.len() - FILE_ROWS) as i32;
        s.raised(r(bar.x, bar.y + 16 + shift, 16, h), FACE);
    }
    let ny = list.bottom() + 9;
    s.text(r(x, ny, 89, 24), "File name:", TEXT, NORMAL);
    field(
        s,
        r(x + 92, ny, 310, 24),
        &file.name,
        file.focus == 1,
        app.caret,
        1,
    );
    s.button(
        r(x + 410, ny, 89, 25),
        if file.mode.saving() { "Save" } else { "Open" },
        Hit::ModalAccept,
        true,
    );
    let line = ny + 31;
    let kind = r(x + 92, line, 310, 24);
    s.text(r(x, line, 91, 24), "Files of type:", TEXT, NORMAL);
    s.pressed(kind, WHITE);
    let label = match file.mode {
        FileMode::Open | FileMode::PasteFrom => "All image files",
        FileMode::Pdf => "PDF Document (*.pdf)",
        _ => file.format.label(),
    };
    s.text(
        r(kind.x + 5, kind.y, kind.w - 27, kind.h),
        label,
        TEXT,
        NORMAL,
    );
    if matches!(file.mode, FileMode::Save | FileMode::CopyTo) {
        let button = r(kind.right() - 20, kind.y + 2, 18, 20);
        s.raised(button, FACE);
        arrow(s, button, 2, TEXT);
        s.register(kind, Hit::FileFormat);
    }
    s.button(r(x + 410, line, 89, 25), "Cancel", Hit::ModalCancel, false);
    if !file.problem.is_empty() {
        s.text(r(x, line + 28, 500, 19), &file.problem, 0x8a2333, SMALL);
    }
    if file.dropdown {
        let pop = r(
            kind.x,
            kind.y - Format::ALL.len() as i32 * 20 - 4,
            kind.w,
            Format::ALL.len() as i32 * 20 + 4,
        );
        s.raised(pop, FACE);
        for (n, format) in Format::ALL.into_iter().enumerate() {
            let row = r(pop.x + 2, pop.y + 2 + n as i32 * 20, pop.w - 4, 20);
            let on = format == file.format;
            if on {
                s.fill(row, SELECT);
            }
            s.text(
                r(row.x + 6, row.y, row.w - 12, 20),
                format.label(),
                if on { WHITE } else { TEXT },
                NORMAL,
            );
            s.register(row, Hit::Format(format));
        }
    }
}

fn group(s: &mut Scene, rect: Rect, label: &str) {
    s.edge(rect, SHADOW, LIGHT);
    s.edge(rect.inset(1), LIGHT, SHADOW);
    let width = s.fonts.measure(label, NORMAL) + 9;
    s.fill(r(rect.x + 7, rect.y - 7, width, 14), FACE);
    s.text(r(rect.x + 11, rect.y - 8, width, 16), label, TEXT, NORMAL);
}
fn radio(s: &mut Scene, rect: Rect, label: &str, index: usize, on: bool) {
    let p = r(rect.x, rect.y + (rect.h - 12) / 2, 12, 12);
    s.bitmap.shape(
        Shape::Ellipse,
        p,
        ShapeStyle {
            width: 1,
            ink: DARK,
            paper: WHITE,
            fill: FillMode::Both,
        },
    );
    if on {
        s.bitmap.dot(
            crate::raster::p(p.x + 6, p.y + 6),
            5,
            Tip::Round,
            SELECT,
            None,
        );
    }
    s.text(
        r(rect.x + 18, rect.y, rect.w - 18, rect.h),
        label,
        TEXT,
        NORMAL,
    );
    s.register(rect, Hit::Option(index));
}
fn labeled_field(
    s: &mut Scene,
    form: &Form,
    i: usize,
    rect: Rect,
    label: &str,
    field_width: i32,
    caret: bool,
) {
    s.text(
        r(rect.x, rect.y, rect.w - field_width - 5, rect.h),
        label,
        TEXT,
        NORMAL,
    );
    field(
        s,
        r(rect.right() - field_width, rect.y, field_width, rect.h),
        &form.input[i],
        form.focus == i,
        caret,
        i,
    );
}
fn form_box(s: &mut Scene, app: &Paint, l: &Layout, form: &Form) {
    let (w, h) = match form.kind {
        FormKind::Attributes => (365, 297),
        FormKind::Turn => (326, 245),
        FormKind::Stretch => (355, 276),
        FormKind::Colors => (449, 305),
        FormKind::Page => (425, 309),
        FormKind::Print => (365, 189),
        FormKind::Zoom => (324, 163),
    };
    let b = modal(s, l, w, h, form.kind.title());
    let (x, y) = (b.x + 17, b.y + 42);
    match form.kind {
        FormKind::Attributes => {
            group(s, r(x, y + 2, w - 34, 92), "Image size");
            let bmp = app.editor.bitmap();
            s.text(
                r(x + 178, y + 18, 127, 22),
                &format!("{} × {} pixels", bmp.w, bmp.h),
                TEXT,
                SMALL,
            );
            labeled_field(
                s,
                form,
                0,
                r(x + 12, y + 17, 147, 24),
                "Width:",
                77,
                app.caret,
            );
            labeled_field(
                s,
                form,
                1,
                r(x + 12, y + 49, 147, 24),
                "Height:",
                77,
                app.caret,
            );
            group(s, r(x, y + 107, w - 34, 45), "Units");
            for (i, label) in ["Pixels", "Inches", "Centimeters"].into_iter().enumerate() {
                radio(
                    s,
                    r(x + 12 + i as i32 * 104, y + 119, 103, 22),
                    label,
                    8 + i,
                    form.options[1] == i,
                );
            }
            group(s, r(x, y + 165, w - 34, 41), "Colors");
            radio(
                s,
                r(x + 12, y + 175, 103, 22),
                "Colors",
                0,
                form.options[0] == 0,
            );
            radio(
                s,
                r(x + 128, y + 175, 142, 22),
                "Black and white",
                1,
                form.options[0] == 1,
            );
        }
        FormKind::Turn => {
            group(s, r(x, y + 2, w - 34, 155), "Flip or rotate");
            for (i, label) in [
                "Flip horizontal",
                "Flip vertical",
                "Rotate 90° clockwise",
                "Rotate 180°",
                "Rotate 270° clockwise",
            ]
            .into_iter()
            .enumerate()
            {
                radio(
                    s,
                    r(x + 13, y + 14 + i as i32 * 27, w - 62, 23),
                    label,
                    i,
                    form.options[0] == i,
                );
            }
        }
        FormKind::Stretch => {
            for (i, label) in ["Stretch", "Skew"].into_iter().enumerate() {
                let top = y + i as i32 * 91;
                group(s, r(x, top + 3, w - 34, 74), label);
                for (j, axis) in ["Horizontal:", "Vertical:"].into_iter().enumerate() {
                    let rect = r(x + 14, top + 14 + j as i32 * 26, 200, 22);
                    labeled_field(s, form, i * 2 + j, rect, axis, 95, app.caret);
                    s.text(
                        r(rect.right() + 9, rect.y, 85, 22),
                        if i == 0 { "%" } else { "degrees" },
                        TEXT,
                        SMALL,
                    );
                }
            }
        }
        FormKind::Colors => {
            s.text(r(x, y, 210, 22), "Basic colors", TEXT, NORMAL);
            let mut samples = Vec::from(EGA);
            samples.extend(crate::editor::PALETTE);
            for (n, color) in samples.into_iter().enumerate() {
                let q = r(
                    x + (n % 8) as i32 * 26,
                    y + 26 + (n / 8) as i32 * 25,
                    23,
                    22,
                );
                s.pressed(q, color);
                s.register(q, Hit::ColorSample(color));
            }
            let right = x + 235;
            for (i, label) in ["Red:", "Green:", "Blue:", "Hex:"].into_iter().enumerate() {
                labeled_field(
                    s,
                    form,
                    i,
                    r(right, y + 8 + i as i32 * 31, 151, 24),
                    label,
                    88,
                    app.caret,
                );
            }
            s.pressed(r(right, y + 141, 151, 34), form_color(form).unwrap_or(FACE));
            s.text(r(right, y + 177, 151, 18), "Color preview", TEXT, SMALL);
        }
        FormKind::Page => {
            group(s, r(x, y + 1, 185, 75), "Paper");
            radio(
                s,
                r(x + 12, y + 15, 161, 22),
                "Letter (8.5 × 11 in)",
                8,
                form.options[1] == 0,
            );
            radio(
                s,
                r(x + 12, y + 41, 161, 22),
                "A4 (210 × 297 mm)",
                9,
                form.options[1] == 1,
            );
            group(s, r(x + 202, y + 1, 189, 75), "Orientation");
            radio(
                s,
                r(x + 214, y + 15, 161, 22),
                "Portrait",
                0,
                form.options[0] == 0,
            );
            radio(
                s,
                r(x + 214, y + 41, 161, 22),
                "Landscape",
                1,
                form.options[0] == 1,
            );
            group(s, r(x, y + 89, 391, 83), "Margins (inches)");
            for (i, label) in ["Left:", "Top:", "Right:", "Bottom:"]
                .into_iter()
                .enumerate()
            {
                labeled_field(
                    s,
                    form,
                    i,
                    r(
                        x + 13 + (i / 2) as i32 * 187,
                        y + 103 + (i % 2) as i32 * 28,
                        153,
                        23,
                    ),
                    label,
                    76,
                    app.caret,
                );
            }
            radio(
                s,
                r(x + 5, y + 181, 148, 23),
                "Top left",
                16,
                form.options[2] == 0,
            );
            radio(
                s,
                r(x + 166, y + 181, 190, 23),
                "Center on page",
                17,
                form.options[2] == 1,
            );
        }
        FormKind::Print => {
            labeled_field(s, form, 0, r(x, y + 4, 321, 25), "Printer:", 245, app.caret);
            s.text(
                r(x + 77, y + 32, 243, 20),
                "Leave blank for the default printer.",
                TEXT,
                SMALL,
            );
            labeled_field(s, form, 1, r(x, y + 61, 155, 24), "Copies:", 78, app.caret);
        }
        FormKind::Zoom => {
            labeled_field(
                s,
                form,
                0,
                r(x, y + 10, 233, 25),
                "Magnification (%):",
                100,
                app.caret,
            );
        }
    }
    let actions_y = b.bottom() - 37;
    if !form.problem.is_empty() {
        s.text(
            r(x, actions_y - 22, w - 34, 20),
            &form.problem,
            0x8a2333,
            SMALL,
        );
    }
    s.button(
        r(b.right() - 192, actions_y, 82, 24),
        if form.kind == FormKind::Print {
            "Print"
        } else {
            "OK"
        },
        Hit::ModalAccept,
        true,
    );
    s.button(
        r(b.right() - 100, actions_y, 83, 24),
        "Cancel",
        Hit::ModalCancel,
        false,
    );
}

pub fn form_color(form: &Form) -> Option<Color> {
    if form.focus == 3 {
        let raw = form.input[3].value.trim().trim_start_matches('#');
        if raw.len() != 6 {
            return None;
        }
        return u32::from_str_radix(raw, 16).ok();
    }
    let value = |idx: usize| {
        form.input[idx]
            .value
            .trim()
            .parse::<u8>()
            .ok()
            .map(u32::from)
    };
    Some((value(0)? << 16) | (value(1)? << 8) | value(2)?)
}

fn help(s: &mut Scene, l: &Layout, current: usize) {
    let b = modal(s, l, 482, 348, "Paintrs Help");
    let labels = ["Drawing", "Selections", "Pictures", "Keyboard"];
    let notes = [
        "Choose a tool from the toolbox. Hold the left mouse button to paint with the foreground color, or the right button to paint with the background color. Select colors from the palette.\n\nUse the panel below the tools to choose a brush, line width, or fill style. For a curve, drag the straight line first, then drag each of its two control points. Double-click the last corner of a polygon to finish it.\n\nTo type, drag a text box. Click outside it to apply the text.",
        "Drag with Select to choose a rectangle, or trace a shape with Free-Form Select. Drag inside the selection to move it. Hold Ctrl while dragging to copy it.\n\nUse Draw Opaque to control whether the background color is included. Choose Copy To or Paste From on the Edit menu to exchange part of a picture with a file.",
        "Use Attributes to change the canvas size or switch to black and white. The small handles on the canvas edge also resize it. Use Stretch/Skew to scale pixels instead.\n\nThe File menu opens and saves bitmap files and other image formats. Print Preview shows how the picture fits the current paper and margins.",
        "Ctrl+N New    Ctrl+O Open    Ctrl+S Save\nCtrl+Z Undo    Ctrl+Y Redo\nCtrl+X Cut    Ctrl+C Copy    Ctrl+V Paste\nCtrl+A Select All    Delete Clear Selection\nCtrl+E Attributes    Ctrl+R Flip/Rotate\nCtrl+1 Normal Size    Ctrl+2 Large Size\n\nPress Alt and the first letter of a menu to open it. F11 shows the picture by itself. Escape closes a dialog or cancels an unfinished selection, shape, or text box.",
    ];
    for (i, label) in labels.into_iter().enumerate() {
        let rect = r(b.x + 13 + i as i32 * 96, b.y + 39, 97, 26);
        s.button(rect, label, Hit::HelpTab(i), i == current);
    }
    let sheet = r(b.x + 13, b.y + 64, b.w - 26, 226);
    s.pressed(sheet, WHITE);
    s.wrapped(sheet.inset(13), notes[current], TEXT);
    s.button(
        r(b.right() - 102, b.bottom() - 39, 87, 25),
        "Close",
        Hit::ModalAccept,
        true,
    );
}

fn preview(s: &mut Scene, app: &Paint, l: &Layout, image: &Bitmap) {
    let h = (l.frame.h - 36).min(615);
    let b = modal(s, l, 520, h, "Print Preview");
    for (i, (text, hit)) in [
        ("Print...", Hit::Invoke(Cmd::Print)),
        ("Page Setup...", Hit::Invoke(Cmd::PageSetup)),
        ("Save as PDF...", Hit::Invoke(Cmd::ExportPdf)),
        ("Close", Hit::ModalAccept),
    ]
    .into_iter()
    .enumerate()
    {
        s.button(
            r(b.x + 12 + i as i32 * 112, b.y + 34, 106, 25),
            text,
            hit,
            false,
        );
    }
    let well = r(b.x + 11, b.y + 66, b.w - 22, b.h - 79);
    s.pressed(well, DESK);
    let (w, h) = app.page.dimensions();
    let scale = ((well.w - 38) as f64 / w).min((well.h - 28) as f64 / h);
    let (sw, sh) = ((w * scale) as i32, (h * scale) as i32);
    let paper = r(
        well.x + (well.w - sw) / 2,
        well.y + (well.h - sh) / 2,
        sw,
        sh,
    );
    s.fill(r(paper.x + 3, paper.y + 3, paper.w, paper.h), DARK);
    s.fill(paper, WHITE);
    s.edge(paper, OUTLINE, OUTLINE);
    let (x, y, w, h) = app.page.image_box(image);
    scaled(
        s,
        image,
        r(
            paper.x + (x * scale) as i32,
            paper.y + (y * scale) as i32,
            (w * scale) as i32,
            (h * scale) as i32,
        ),
    );
}
