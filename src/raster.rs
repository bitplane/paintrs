use fontdue::{Font, FontSettings};
use std::collections::HashMap;

pub type Color = u32;
pub const BLACK: Color = 0x000000;
pub const WHITE: Color = 0xffffff;
pub const HOLE: Color = 0xff000000;
pub const MAX_SIDE: i32 = 8192;
pub const MAX_PIXELS: i64 = 16_777_216;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
pub const fn p(x: i32, y: i32) -> Point {
    Point { x, y }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
pub const fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { x, y, w, h }
}

impl Rect {
    pub fn from_points(a: Point, b: Point) -> Self {
        r(
            a.x.min(b.x),
            a.y.min(b.y),
            (b.x - a.x).abs() + 1,
            (b.y - a.y).abs() + 1,
        )
    }
    pub fn right(self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(self) -> i32 {
        self.y + self.h
    }
    pub fn origin(self) -> Point {
        p(self.x, self.y)
    }
    pub fn contains(self, q: Point) -> bool {
        q.x >= self.x && q.y >= self.y && q.x < self.right() && q.y < self.bottom()
    }
    pub fn inset(self, n: i32) -> Self {
        r(self.x + n, self.y + n, self.w - 2 * n, self.h - 2 * n)
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        r(
            x,
            y,
            (self.right().min(other.right()) - x).max(0),
            (self.bottom().min(other.bottom()) - y).max(0),
        )
    }
}

pub fn valid_size(w: i32, h: i32) -> bool {
    (1..=MAX_SIDE).contains(&w)
        && (1..=MAX_SIDE).contains(&h)
        && i64::from(w) * i64::from(h) <= MAX_PIXELS
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    pub w: i32,
    pub h: i32,
    pub pixels: Vec<Color>,
}

impl Bitmap {
    pub fn new(w: i32, h: i32, color: Color) -> Self {
        Self {
            w,
            h,
            pixels: vec![color; w as usize * h as usize],
        }
    }
    pub fn bounds(&self) -> Rect {
        r(0, 0, self.w, self.h)
    }
    pub fn get(&self, q: Point) -> Color {
        self.pixels[(q.y * self.w + q.x) as usize]
    }
    pub fn put(&mut self, q: Point, color: Color) {
        if self.bounds().contains(q) {
            self.pixels[(q.y * self.w + q.x) as usize] = color;
        }
    }
    pub fn fill(&mut self, rect: Rect, color: Color) {
        let rect = rect.intersect(self.bounds());
        if rect.w == 0 || rect.h == 0 {
            return;
        }
        for y in rect.y..rect.bottom() {
            let start = (y * self.w + rect.x) as usize;
            self.pixels[start..start + rect.w as usize].fill(color);
        }
    }
    pub fn blit(&mut self, src: &Self, dst: Point, key: Option<Color>) {
        let rect = r(dst.x, dst.y, src.w, src.h).intersect(self.bounds());
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let color = src.get(p(x - dst.x, y - dst.y));
                if color != HOLE && Some(color) != key {
                    self.put(p(x, y), color);
                }
            }
        }
    }
    pub fn extract(&self, rect: Rect) -> Self {
        debug_assert_eq!(rect, rect.intersect(self.bounds()));
        let mut result = Self::new(rect.w, rect.h, BLACK);
        for (y, row) in result.pixels.chunks_exact_mut(rect.w as usize).enumerate() {
            let start = (rect.y as usize + y) * self.w as usize + rect.x as usize;
            row.copy_from_slice(&self.pixels[start..start + rect.w as usize]);
        }
        result
    }
    pub fn scale(&self, w: i32, h: i32) -> Self {
        let mut result = Self::new(w, h, HOLE);
        for y in 0..h {
            for x in 0..w {
                result.put(p(x, y), self.get(p(x * self.w / w, y * self.h / h)));
            }
        }
        result
    }
    pub fn resize(&self, w: i32, h: i32, color: Color) -> Self {
        let mut result = Self::new(w, h, color);
        result.blit(self, p(0, 0), None);
        result
    }
    pub fn flood(&mut self, at: Point, new: Color) {
        let old = self.get(at);
        if old == new {
            return;
        }
        let mut pending = vec![at];
        self.put(at, new);
        while let Some(q) = pending.pop() {
            for next in [
                p(q.x - 1, q.y),
                p(q.x + 1, q.y),
                p(q.x, q.y - 1),
                p(q.x, q.y + 1),
            ] {
                if self.bounds().contains(next) && self.get(next) == old {
                    self.put(next, new);
                    pending.push(next);
                }
            }
        }
    }
    pub fn dot(&mut self, at: Point, size: i32, tip: Tip, color: Color, replace: Option<Color>) {
        let start = -(size / 2);
        let end = start + size;
        if tip == Tip::Left || tip == Tip::Right {
            for n in start..end {
                self.put(
                    p(at.x + n, at.y + if tip == Tip::Left { -n } else { n }),
                    color,
                );
            }
            return;
        }
        for dy in start..end {
            for dx in start..end {
                if tip == Tip::Round {
                    let even = i32::from(size % 2 == 0);
                    if (2 * dx + even).pow(2) + (2 * dy + even).pow(2) > size.pow(2) {
                        continue;
                    }
                }
                let q = p(at.x + dx, at.y + dy);
                if replace.is_none() || (self.bounds().contains(q) && Some(self.get(q)) == replace)
                {
                    self.put(q, color);
                }
            }
        }
    }
    pub fn stroke(&mut self, a: Point, b: Point, size: i32, color: Color) {
        walk_line(a, b, |q| self.dot(q, size, Tip::Round, color, None));
    }
    pub fn shape(&mut self, shape: Shape, rect: Rect, style: ShapeStyle) {
        let outer = rect.intersect(self.bounds());
        let inner = rect.inset(style.width);
        let has_outline = style.fill != FillMode::Solid;
        for y in outer.y..outer.bottom() {
            for x in outer.x..outer.right() {
                let q = p(x, y);
                if !inside_shape(shape, rect, q) {
                    continue;
                }
                if has_outline && !inside_shape(shape, inner, q) {
                    self.put(q, style.ink);
                } else if style.fill != FillMode::Outline {
                    self.put(q, if has_outline { style.paper } else { style.ink });
                }
            }
        }
    }
    pub fn polygon(&mut self, points: &[Point], style: ShapeStyle) {
        if points.len() < 2 {
            return;
        }
        if style.fill != FillMode::Outline && points.len() >= 3 {
            let bounds = point_bounds(points).intersect(self.bounds());
            let color = if style.fill == FillMode::Solid {
                style.ink
            } else {
                style.paper
            };
            for y in bounds.y..bounds.bottom() {
                let mut intersections = scanline(points, y);
                intersections.sort_by(f64::total_cmp);
                for pair in intersections.as_chunks::<2>().0 {
                    let start = pair[0].ceil() as i32;
                    let end = pair[1].ceil() as i32;
                    self.fill(r(start, y, end - start, 1), color);
                }
            }
        }
        if style.fill != FillMode::Solid || points.len() >= 3 {
            let width = if style.fill == FillMode::Solid {
                1
            } else {
                style.width
            };
            for (&a, &b) in points.iter().zip(points.iter().cycle().skip(1)) {
                self.stroke(a, b, width, style.ink);
            }
        }
    }
    pub fn bezier(&mut self, points: [Point; 4], size: i32, color: Color) {
        let mut last = points[0];
        let steps: i32 = points
            .windows(2)
            .map(|pair| {
                (pair[1].x - pair[0].x)
                    .abs()
                    .max((pair[1].y - pair[0].y).abs())
            })
            .sum::<i32>()
            .clamp(8, 32768);
        for i in 1..=steps {
            let t = f64::from(i) / f64::from(steps);
            let s = 1.0 - t;
            let weights = [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t];
            let coordinate = |axis: fn(Point) -> i32| {
                points
                    .iter()
                    .zip(weights)
                    .map(|(&q, k)| f64::from(axis(q)) * k)
                    .sum::<f64>()
                    .round() as i32
            };
            let next = p(coordinate(|q| q.x), coordinate(|q| q.y));
            self.stroke(last, next, size, color);
            last = next;
        }
    }
    pub fn turn(&self, turn: Turn) -> Self {
        let (w, h) = if matches!(turn, Turn::Right | Turn::Left) {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        };
        let mut result = Self::new(w, h, HOLE);
        for y in 0..self.h {
            for x in 0..self.w {
                let dest = match turn {
                    Turn::Horizontal => p(self.w - 1 - x, y),
                    Turn::Vertical => p(x, self.h - 1 - y),
                    Turn::Right => p(self.h - 1 - y, x),
                    Turn::Half => p(self.w - 1 - x, self.h - 1 - y),
                    Turn::Left => p(y, self.w - 1 - x),
                };
                result.put(dest, self.get(p(x, y)));
            }
        }
        result
    }
    pub fn stretch(&self, sx: f64, sy: f64, ax: f64, ay: f64, fill: Color) -> Option<Self> {
        let (w, h) = (
            (sx * self.w as f64).round().max(1.0),
            (sy * self.h as f64).round().max(1.0),
        );
        let (kx, ky) = (ax.to_radians().tan(), ay.to_radians().tan());
        let (cross_x, cross_y) = (h * kx, w * ky);
        let (tw, th) = (
            (w + cross_x.abs()).ceil() as i32,
            (h + cross_y.abs()).ceil() as i32,
        );
        let determinant = 1.0 - kx * ky;
        if !valid_size(tw, th) || determinant.abs() < 0.0001 {
            return None;
        }
        let mut result = Self::new(tw, th, fill);
        for y in 0..th {
            for x in 0..tw {
                let dx = f64::from(x) + 0.5 + cross_x.min(0.0);
                let dy = f64::from(y) + 0.5 + cross_y.min(0.0);
                let u = (dx - kx * dy) / determinant;
                let v = (dy - ky * dx) / determinant;
                if u >= 0.0 && v >= 0.0 && u < w && v < h {
                    result.put(
                        p(x, y),
                        self.get(p(
                            (u * self.w as f64 / w) as i32,
                            (v * self.h as f64 / h) as i32,
                        )),
                    );
                }
            }
        }
        Some(result)
    }
    pub fn invert(&mut self) {
        for color in &mut self.pixels {
            if *color != HOLE {
                *color ^= WHITE;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    Round,
    Square,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Ellipse,
    Rounded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    Outline,
    Both,
    Solid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Horizontal,
    Vertical,
    Right,
    Half,
    Left,
}

#[derive(Clone, Copy)]
pub struct ShapeStyle {
    pub width: i32,
    pub ink: Color,
    pub paper: Color,
    pub fill: FillMode,
}

fn inside_shape(shape: Shape, bounds: Rect, q: Point) -> bool {
    if !bounds.contains(q) {
        return false;
    }
    match shape {
        Shape::Rectangle => true,
        Shape::Ellipse => {
            let x = (2 * (q.x - bounds.x) + 1) as f64 / bounds.w as f64 - 1.0;
            let y = (2 * (q.y - bounds.y) + 1) as f64 / bounds.h as f64 - 1.0;
            x * x + y * y <= 1.0
        }
        Shape::Rounded => {
            let rad = (bounds.w.min(bounds.h) / 4).clamp(0, 18);
            let x = q.x
                - q.x.clamp(
                    bounds.x + rad,
                    (bounds.right() - 1 - rad).max(bounds.x + rad),
                );
            let y = q.y
                - q.y.clamp(
                    bounds.y + rad,
                    (bounds.bottom() - 1 - rad).max(bounds.y + rad),
                );
            x * x + y * y <= rad * rad
        }
    }
}

pub fn walk_line(a: Point, b: Point, mut draw: impl FnMut(Point)) {
    let (mut x, mut y) = (a.x, a.y);
    let (dx, dy) = ((b.x - x).abs(), -(b.y - y).abs());
    let (sx, sy) = ((b.x - x).signum(), (b.y - y).signum());
    let mut error = dx + dy;
    loop {
        draw(p(x, y));
        if (x, y) == (b.x, b.y) {
            break;
        }
        let test = 2 * error;
        if test >= dy {
            error += dy;
            x += sx;
        }
        if test <= dx {
            error += dx;
            y += sy;
        }
    }
}

pub fn point_bounds(points: &[Point]) -> Rect {
    let (mut lo, mut hi) = (points[0], points[0]);
    for q in &points[1..] {
        lo = p(lo.x.min(q.x), lo.y.min(q.y));
        hi = p(hi.x.max(q.x), hi.y.max(q.y));
    }
    Rect::from_points(lo, hi)
}

pub fn scanline(points: &[Point], y: i32) -> Vec<f64> {
    let y = f64::from(y) + 0.5;
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .filter_map(|(a, b)| {
            let y0 = f64::from(a.y);
            let y1 = f64::from(b.y);
            if y >= y0.min(y1) && y < y0.max(y1) {
                Some(f64::from(a.x) + (y - y0) / (y1 - y0) * f64::from(b.x - a.x))
            } else {
                None
            }
        })
        .collect()
}

pub fn mix(a: Color, b: Color, amount: u32) -> Color {
    let channel = |shift: u32| {
        (((a >> shift) & 255u32) * (255 - amount) + ((b >> shift) & 255u32) * amount + 127) / 255
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Sans,
    Serif,
    Mono,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextStyle {
    pub family: Family,
    pub size: u16,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: Family::Sans,
            size: 18,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

type GlyphKey = (Family, bool, bool, u16, char);
type Glyph = (fontdue::Metrics, Vec<u8>);

pub struct Fonts {
    fonts: HashMap<(Family, bool, bool), Font>,
    glyphs: HashMap<GlyphKey, Glyph>,
}

impl Fonts {
    pub fn empty() -> Self {
        Self {
            fonts: HashMap::new(),
            glyphs: HashMap::new(),
        }
    }
    pub fn new() -> Self {
        let mut fonts = HashMap::new();
        for (family, faces) in [
            (
                Family::Sans,
                [
                    include_bytes!("../assets/LiberationSans-Regular.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSans-Bold.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSans-Italic.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSans-BoldItalic.ttf").as_slice(),
                ],
            ),
            (
                Family::Serif,
                [
                    include_bytes!("../assets/LiberationSerif-Regular.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSerif-Bold.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSerif-Italic.ttf").as_slice(),
                    include_bytes!("../assets/LiberationSerif-BoldItalic.ttf").as_slice(),
                ],
            ),
            (
                Family::Mono,
                [
                    include_bytes!("../assets/LiberationMono-Regular.ttf").as_slice(),
                    include_bytes!("../assets/LiberationMono-Bold.ttf").as_slice(),
                    include_bytes!("../assets/LiberationMono-Italic.ttf").as_slice(),
                    include_bytes!("../assets/LiberationMono-BoldItalic.ttf").as_slice(),
                ],
            ),
        ] {
            for ((bold, italic), bytes) in
                [(false, false), (true, false), (false, true), (true, true)]
                    .into_iter()
                    .zip(faces)
            {
                fonts.insert(
                    (family, bold, italic),
                    Font::from_bytes(bytes, FontSettings::default()).expect("Liberation font"),
                );
            }
        }
        Self {
            fonts,
            glyphs: HashMap::new(),
        }
    }
    fn glyph(&mut self, style: TextStyle, ch: char) -> &Glyph {
        let key = (style.family, style.bold, style.italic, style.size, ch);
        self.glyphs.entry(key).or_insert_with(|| {
            self.fonts[&(style.family, style.bold, style.italic)]
                .rasterize(ch, f32::from(style.size))
        })
    }
    pub fn measure(&mut self, text: &str, style: TextStyle) -> i32 {
        text.chars()
            .map(|ch| self.glyph(style, ch).0.advance_width)
            .sum::<f32>()
            .ceil() as i32
    }
    pub fn caret(&mut self, text: &str, bounds: Rect, style: TextStyle) -> Point {
        let line_height = self.fonts[&(style.family, style.bold, style.italic)]
            .horizontal_line_metrics(f32::from(style.size))
            .expect("horizontal metrics")
            .new_line_size
            .ceil() as i32;
        let mut x = bounds.x as f32;
        let mut y = bounds.y;
        for ch in text.chars() {
            let width = self.glyph(style, ch).0.advance_width;
            if ch == '\n' || (x > bounds.x as f32 && x + width > bounds.right() as f32) {
                x = bounds.x as f32;
                y += line_height;
            }
            if ch != '\n' {
                x += width;
            }
        }
        p(x.round() as i32, y)
    }
    pub fn draw(
        &mut self,
        target: &mut Bitmap,
        text: &str,
        bounds: Rect,
        style: TextStyle,
        color: Color,
        wrap: bool,
    ) -> Point {
        let metrics = self.fonts[&(style.family, style.bold, style.italic)]
            .horizontal_line_metrics(f32::from(style.size))
            .expect("horizontal metrics");
        let baseline = metrics.ascent.ceil() as i32;
        let line_height = metrics.new_line_size.ceil() as i32;
        let mut x = bounds.x as f32;
        let mut y = bounds.y;
        let clip = bounds.intersect(target.bounds());
        for ch in text.chars() {
            if ch == '\n' {
                x = bounds.x as f32;
                y += line_height;
                continue;
            }
            let (glyph, alpha) = self.glyph(style, ch);
            if wrap && x > bounds.x as f32 && x + glyph.advance_width > bounds.right() as f32 {
                x = bounds.x as f32;
                y += line_height;
            }
            if y >= clip.bottom() {
                break;
            }
            let x0 = x.round() as i32 + glyph.xmin;
            let y0 = y + baseline - glyph.ymin - glyph.height as i32;
            for iy in 0..glyph.height {
                for ix in 0..glyph.width {
                    let q = p(x0 + ix as i32, y0 + iy as i32);
                    let coverage = u32::from(alpha[iy * glyph.width + ix]);
                    if coverage > 0 && clip.contains(q) {
                        target.put(q, mix(target.get(q), color, coverage));
                    }
                }
            }
            if style.underline {
                let line = r(
                    x.round() as i32,
                    y + baseline + 1,
                    glyph.advance_width.ceil() as i32,
                    (i32::from(style.size) / 18).max(1),
                );
                target.fill(line.intersect(clip), color);
            }
            x += glyph.advance_width;
        }
        p(x.round() as i32, y)
    }
}
