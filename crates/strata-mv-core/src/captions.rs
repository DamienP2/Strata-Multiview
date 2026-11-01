//! Closed captions: CEA-608 (CC1 to CC4) and CEA-708 (services 1 to 6),
//! as carried in the pictures of a stream (ATSC A/53 `cc_data`: SEI of
//! H.264 and HEVC, user data of MPEG-2), read from MPEG-TS.
//!
//! [`Extractor`] takes the bytes of a transport stream and gives the
//! `cc_data` of each picture in the order they are shown (by PTS: pictures
//! are sent out of order with B frames); [`Decoder`] keeps what each
//! channel shows; [`CaptionScreen`] is what a tile draws, the same way in
//! both renderers ([`caption_boxes`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ─── What is shown ──────────────────────────────────────────────────────────

/// What a caption channel shows now.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptionScreen {
    /// Columns of its grid: 32 (CEA-608, a 4:3 area in the middle of the
    /// picture), or 42 / 32 (CEA-708 on a 16:9 / 4:3 picture, the whole
    /// safe area).
    pub cols: u8,
    /// The CEA-608 grid (in the middle 4:3 of a wide picture).
    pub narrow: bool,
    pub lines: Vec<CaptionLine>,
}

impl CaptionScreen {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// A run of characters on one row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptionLine {
    /// Its row (0 to 14, of 15) and its first column (of `cols`).
    pub row: u8,
    pub col: u8,
    pub spans: Vec<CaptionSpan>,
}

/// Characters of the same look.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CaptionSpan {
    pub text: String,
    /// `#rrggbb`.
    pub color: String,
    pub italic: bool,
    pub underline: bool,
}

impl CaptionLine {
    /// Its characters.
    pub fn len(&self) -> usize {
        self.spans.iter().map(|s| s.text.chars().count()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The caption channels, in the order "auto" picks them.
pub const CHANNELS: [&str; 10] = [
    "s1", "cc1", "s2", "cc3", "cc2", "cc4", "s3", "s4", "s5", "s6",
];

/// The channel a tile shows: `want` ("cc1"…"cc4", "s1"…"s6", "auto": the
/// first one carrying something; "" none) among those of `screens`.
pub fn pick<'a>(
    screens: &'a BTreeMap<String, CaptionScreen>,
    want: &str,
) -> Option<&'a CaptionScreen> {
    match want {
        "" => None,
        "auto" => CHANNELS
            .iter()
            .filter_map(|c| screens.get(*c))
            .find(|s| !s.is_empty()),
        c => screens.get(c),
    }
}

/// A caption drawn: its black box, its text (JetBrains Mono, `size` px,
/// one character every 0.6 × `size`) and the line under it.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionBox {
    /// The box: x, y, width, height.
    pub a: [f32; 4],
    /// The text's place in it (left aligned, centred vertically).
    pub t: [f32; 4],
    pub text: String,
    pub color: String,
    pub size: f32,
    pub italic: bool,
    pub underline: Option<[f32; 4]>,
    /// The cells of its ♪ (left as spaces in `text`: no font of the
    /// renderers has it), drawn by [`note`].
    pub notes: Vec<[f32; 4]>,
}

/// A ♪ in cell `c` (x, y, width, height) of a caption `size` px: its head
/// (centre x, y, radii x, y), its stem (a box) and its flag (a triangle).
pub fn note(c: [f32; 4], size: f32) -> ([f32; 4], [f32; 4], [[f32; 2]; 3]) {
    let [x, y, w, h] = c;
    let (rx, ry) = (size * 0.19, size * 0.14);
    let (cx, cy) = (x + w * 0.42, y + h * 0.68);
    let sw = (size * 0.07).max(1.0);
    let sx = cx + rx - sw;
    let top = y + h * 0.18;
    let stem = [sx, top, sw, cy - top];
    let flag = [
        [sx + sw, top],
        [sx + sw + size * 0.2, top + size * 0.3],
        [sx + sw, top + size * 0.24],
    ];
    ([cx, cy, rx, ry], stem, flag)
}

/// JetBrains Mono's advance, per pixel of size.
pub const ADVANCE: f32 = 0.6;

/// Where `s` is drawn over picture `p` (x, y, width, height), `scale`
/// times the usual size: 15 rows in the middle 80 % of the picture's
/// height, the columns over 80 % of its width (of its middle 4:3 for
/// CEA-608 on a wide picture).
pub fn caption_boxes(s: &CaptionScreen, p: [f32; 4], scale: f32) -> Vec<CaptionBox> {
    let [px, py, pw, ph] = p;
    if pw <= 0.0 || ph <= 0.0 || s.lines.is_empty() {
        return Vec::new();
    }
    let cols = s.cols.max(1) as f32;
    let ah = ph * 0.8;
    let aw = if s.narrow {
        (ph * 4.0 / 3.0).min(pw) * 0.8
    } else {
        pw * 0.8
    };
    let (ax, ay) = (px + (pw - aw) / 2.0, py + (ph - ah) / 2.0);
    let row_h = ah / 15.0;
    let col_w = aw / cols;
    let scale = scale.clamp(0.25, 4.0);
    // The rows' height, and characters no wider than the columns.
    let size = (row_h * 0.82).min(col_w / ADVANCE) * scale;
    let adv = size * ADVANCE;
    let bh = size * 1.2;
    let pad = size * 0.25;
    let mut out = Vec::new();
    for l in &s.lines {
        let y = ay + (l.row as f32 + 0.5) * row_h - bh / 2.0;
        // Bigger than the grid: kept in the picture.
        let width = l.len() as f32 * adv;
        let mut x = ax + l.col as f32 * col_w;
        x = x.min(px + pw - width - pad).max(px + pad);
        for sp in &l.spans {
            let n = sp.text.chars().count() as f32;
            if n == 0.0 {
                continue;
            }
            let w = n * adv;
            let notes = sp
                .text
                .chars()
                .enumerate()
                .filter(|(_, c)| *c == '♪')
                .map(|(i, _)| [x + i as f32 * adv, y, adv, bh])
                .collect();
            let first = std::ptr::eq(sp, &l.spans[0]);
            let last = std::ptr::eq(sp, l.spans.last().unwrap_or(sp));
            let (l_pad, r_pad) = (if first { pad } else { 0.0 }, if last { pad } else { 0.0 });
            out.push(CaptionBox {
                a: [x - l_pad, y, w + l_pad + r_pad, bh],
                t: [x, y, w, bh],
                text: sp.text.replace('♪', " "),
                color: sp.color.clone(),
                size,
                italic: sp.italic,
                underline: sp
                    .underline
                    .then(|| [x, y + bh - size * 0.18, w, (size * 0.07).max(1.0)]),
                notes,
            });
            x += w;
        }
    }
    out
}

// ─── A grid of characters ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pen {
    color: [u8; 3],
    italic: bool,
    underline: bool,
}

const WHITE: [u8; 3] = [255, 255, 255];

impl Default for Pen {
    fn default() -> Self {
        Pen {
            color: WHITE,
            italic: false,
            underline: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Cell {
    ch: char,
    pen: Pen,
}

/// Rows of cells (`None`: nothing written there).
#[derive(Debug, Clone, PartialEq)]
struct Grid {
    rows: Vec<Vec<Option<Cell>>>,
}

impl Grid {
    fn new(rows: usize, cols: usize) -> Self {
        Grid {
            rows: vec![vec![None; cols]; rows],
        }
    }
    fn cols(&self) -> usize {
        self.rows.first().map_or(0, |r| r.len())
    }
    fn clear(&mut self) {
        for r in &mut self.rows {
            r.iter_mut().for_each(|c| *c = None);
        }
    }
    fn clear_row(&mut self, row: usize) {
        if let Some(r) = self.rows.get_mut(row) {
            r.iter_mut().for_each(|c| *c = None);
        }
    }
    fn put(&mut self, row: usize, col: usize, cell: Option<Cell>) {
        if let Some(c) = self.rows.get_mut(row).and_then(|r| r.get_mut(col)) {
            *c = cell;
        }
    }
    fn is_empty(&self) -> bool {
        self.rows.iter().all(|r| r.iter().all(|c| c.is_none()))
    }
    fn resize(&mut self, rows: usize, cols: usize) {
        self.rows.resize(rows, vec![None; cols]);
        for r in &mut self.rows {
            r.resize(cols, None);
        }
    }
    /// The runs of written cells, row `row0 + r`, column `col0 + c`.
    fn lines(&self, row0: i32, col0: i32, out: &mut Vec<CaptionLine>) {
        for (r, cells) in self.rows.iter().enumerate() {
            let row = row0 + r as i32;
            if !(0..15).contains(&row) {
                continue;
            }
            let mut c = 0;
            while c < cells.len() {
                if cells[c].is_none() {
                    c += 1;
                    continue;
                }
                let start = c;
                let mut spans: Vec<CaptionSpan> = Vec::new();
                let mut last: Option<Pen> = None;
                while let Some(cell) = cells.get(c).copied().flatten() {
                    if last == Some(cell.pen) {
                        if let Some(s) = spans.last_mut() {
                            s.text.push(cell.ch);
                        }
                    } else {
                        let [r, g, b] = cell.pen.color;
                        spans.push(CaptionSpan {
                            text: cell.ch.to_string(),
                            color: format!("#{r:02x}{g:02x}{b:02x}"),
                            italic: cell.pen.italic,
                            underline: cell.pen.underline,
                        });
                        last = Some(cell.pen);
                    }
                    c += 1;
                }
                // Spaces only: nothing to see.
                if spans.iter().all(|s| s.text.trim().is_empty()) {
                    continue;
                }
                out.push(CaptionLine {
                    row: row as u8,
                    col: (col0 + start as i32).max(0) as u8,
                    spans,
                });
            }
        }
    }
}

// ─── CEA-608 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    PopOn,
    RollUp(usize),
    PaintOn,
    /// Text mode (TR, RTD): not captions.
    Text,
}

/// One CEA-608 channel (CC1…CC4).
#[derive(Debug, Clone)]
struct Channel608 {
    mode: Mode,
    shown: Grid,
    hidden: Grid,
    row: usize,
    col: usize,
    pen: Pen,
    /// Something was received on it.
    used: bool,
}

impl Default for Channel608 {
    fn default() -> Self {
        Channel608 {
            mode: Mode::PopOn,
            shown: Grid::new(15, 32),
            hidden: Grid::new(15, 32),
            row: 14,
            col: 0,
            pen: Pen::default(),
            used: false,
        }
    }
}

const COLORS_608: [[u8; 3]; 7] = [
    WHITE,
    [0, 255, 0],
    [0, 0, 255],
    [0, 255, 255],
    [255, 0, 0],
    [255, 255, 0],
    [255, 0, 255],
];

impl Channel608 {
    fn target(&mut self) -> Option<&mut Grid> {
        match self.mode {
            Mode::PopOn => Some(&mut self.hidden),
            Mode::RollUp(_) | Mode::PaintOn => Some(&mut self.shown),
            Mode::Text => None,
        }
    }

    fn write(&mut self, ch: char) {
        self.used = true;
        let (row, col, pen) = (self.row, self.col, self.pen);
        if let Some(g) = self.target() {
            g.put(row, col, Some(Cell { ch, pen }));
            self.col = (col + 1).min(31);
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        }
        let (row, col) = (self.row, self.col);
        if let Some(g) = self.target() {
            g.put(row, col, None);
        }
    }

    /// A preamble address: row (0-based), indent or colour, underline.
    fn pac(&mut self, row: usize, attr: u8) {
        self.used = true;
        if let Mode::RollUp(n) = self.mode {
            // The window of rows moves to the new base row.
            if row != self.row {
                let n = n.min(row + 1);
                let from = self.row as i32 - n as i32 + 1;
                let to = row as i32 - n as i32 + 1;
                let keep: Vec<Vec<Option<Cell>>> = (0..n)
                    .map(|i| {
                        let r = from + i as i32;
                        if (0..15).contains(&r) {
                            self.shown.rows[r as usize].clone()
                        } else {
                            vec![None; 32]
                        }
                    })
                    .collect();
                self.shown.clear();
                for (i, r) in keep.into_iter().enumerate() {
                    let t = to + i as i32;
                    if (0..15).contains(&t) {
                        self.shown.rows[t as usize] = r;
                    }
                }
            }
        }
        self.row = row;
        let code = (attr >> 1) & 0x0F;
        let underline = attr & 1 == 1;
        if code < 7 {
            self.pen = Pen {
                color: COLORS_608[code as usize],
                italic: false,
                underline,
            };
            self.col = 0;
        } else if code == 7 {
            self.pen = Pen {
                color: WHITE,
                italic: true,
                underline,
            };
            self.col = 0;
        } else {
            self.pen = Pen {
                color: WHITE,
                italic: false,
                underline,
            };
            self.col = ((code - 8) as usize * 4).min(31);
        }
    }

    /// A mid-row code: a space, and the look from there.
    fn mid_row(&mut self, b2: u8) {
        let code = (b2 >> 1) & 0x07;
        let underline = b2 & 1 == 1;
        if code < 7 {
            self.pen = Pen {
                color: COLORS_608[code as usize],
                italic: false,
                underline,
            };
        } else {
            self.pen.italic = true;
            self.pen.underline = underline;
        }
        self.write(' ');
    }

    fn command(&mut self, b2: u8) {
        self.used = true;
        match b2 {
            // RCL: pop-on.
            0x20 => self.mode = Mode::PopOn,
            0x21 => self.backspace(),
            // DER: to the end of the row.
            0x24 => {
                let (row, col) = (self.row, self.col);
                if let Some(g) = self.target() {
                    for c in col..32 {
                        g.put(row, c, None);
                    }
                }
            }
            // RU2, RU3, RU4.
            0x25..=0x27 => {
                let n = (b2 - 0x23) as usize;
                if !matches!(self.mode, Mode::RollUp(_)) {
                    self.shown.clear();
                    self.hidden.clear();
                    self.row = 14;
                }
                self.mode = Mode::RollUp(n);
                self.col = 0;
                // Rows above the window: gone.
                let top = self.row as i32 - n as i32 + 1;
                for r in 0..top.max(0) as usize {
                    self.shown.clear_row(r);
                }
            }
            // RDC: paint-on.
            0x29 => self.mode = Mode::PaintOn,
            // TR, RTD: text mode.
            0x2A | 0x2B => self.mode = Mode::Text,
            // EDM.
            0x2C => self.shown.clear(),
            // CR.
            0x2D => {
                if let Mode::RollUp(n) = self.mode {
                    let n = n.min(self.row + 1);
                    let top = self.row + 1 - n;
                    for r in top..self.row {
                        self.shown.rows[r] = self.shown.rows[r + 1].clone();
                    }
                    self.shown.clear_row(self.row);
                    if top > 0 {
                        self.shown.clear_row(top - 1);
                    }
                    self.col = 0;
                } else if self.mode == Mode::PaintOn {
                    self.row = (self.row + 1).min(14);
                    self.col = 0;
                }
            }
            // ENM.
            0x2E => self.hidden.clear(),
            // EOC: the hidden memory shown.
            0x2F => {
                std::mem::swap(&mut self.shown, &mut self.hidden);
                self.mode = Mode::PopOn;
            }
            // AOF, AON, FON: nothing to do.
            _ => {}
        }
    }

    fn screen(&self) -> CaptionScreen {
        let mut lines = Vec::new();
        self.shown.lines(0, 0, &mut lines);
        CaptionScreen {
            cols: 32,
            narrow: true,
            lines,
        }
    }
}

/// The two channels of a field.
#[derive(Debug, Clone, Default)]
struct Field608 {
    channels: [Channel608; 2],
    current: usize,
    /// The last control code (sent twice: the second is ignored).
    last: Option<(u8, u8)>,
    /// Extended data (XDS) going on: its bytes are not captions.
    xds: bool,
}

fn odd_parity(b: u8) -> bool {
    b.count_ones() % 2 == 1
}

/// A basic character of CEA-608.
fn char_608(b: u8) -> char {
    match b {
        0x27 => '’',
        0x2A => 'á',
        0x5C => 'é',
        0x5E => 'í',
        0x5F => 'ó',
        0x60 => 'ú',
        0x7B => 'ç',
        0x7C => '÷',
        0x7D => 'Ñ',
        0x7E => 'ñ',
        0x7F => '■',
        b => b as char,
    }
}

const SPECIAL_608: [char; 16] = [
    '®', '°', '½', '¿', '™', '¢', '£', '♪', 'à', ' ', 'è', 'â', 'ê', 'î', 'ô', 'û',
];
const EXTENDED_608_12: [char; 32] = [
    'Á', 'É', 'Ó', 'Ú', 'Ü', 'ü', '‘', '¡', '*', '’', '─', '©', '℠', '•', '“', '”', 'À', 'Â', 'Ç',
    'È', 'Ê', 'Ë', 'ë', 'Î', 'Ï', 'ï', 'Ô', 'Ù', 'ù', 'Û', '«', '»',
];
const EXTENDED_608_13: [char; 32] = [
    'Ã', 'ã', 'Í', 'Ì', 'ì', 'Ò', 'ò', 'Õ', 'õ', '{', '}', '\\', '^', '_', '|', '~', 'Ä', 'ä', 'Ö',
    'ö', 'ß', '¥', '¤', '│', 'Å', 'å', 'Ø', 'ø', '┌', '┐', '└', '┘',
];

/// The row of a preamble address code.
fn pac_row(b1: u8, b2: u8) -> Option<usize> {
    let second = b2 >= 0x60;
    let row = match b1 {
        0x11 => 1,
        0x12 => 3,
        0x15 => 5,
        0x16 => 7,
        0x17 => 9,
        0x10 if !second => return Some(10),
        0x13 => 12,
        0x14 => 14,
        _ => return None,
    };
    Some(row - 1 + second as usize)
}

impl Field608 {
    fn pair(&mut self, d1: u8, d2: u8) {
        // A byte with the wrong parity: as if not sent.
        if !odd_parity(d1) || !odd_parity(d2) {
            return;
        }
        let (b1, b2) = (d1 & 0x7F, d2 & 0x7F);
        if b1 == 0 && b2 == 0 {
            return;
        }
        if (0x01..=0x0F).contains(&b1) {
            // XDS starts (0x01–0x0E) or ends (0x0F).
            self.xds = b1 != 0x0F;
            self.last = None;
            return;
        }
        if (0x10..=0x1F).contains(&b1) {
            self.xds = false;
            if self.last == Some((b1, b2)) {
                self.last = None;
                return;
            }
            self.last = Some((b1, b2));
            if b2 < 0x20 {
                return;
            }
            self.current = ((b1 & 0x08) != 0) as usize;
            let b1 = b1 & 0xF7;
            let ch = &mut self.channels[self.current];
            match (b1, b2) {
                (_, 0x40..=0x7F) => {
                    if let Some(row) = pac_row(b1, b2) {
                        ch.pac(row, b2 & 0x1F);
                    }
                }
                (0x11, 0x20..=0x2F) => ch.mid_row(b2),
                (0x11, 0x30..=0x3F) => ch.write(SPECIAL_608[(b2 - 0x30) as usize]),
                (0x12, 0x20..=0x3F) => {
                    ch.backspace();
                    ch.write(EXTENDED_608_12[(b2 - 0x20) as usize]);
                }
                (0x13, 0x20..=0x3F) => {
                    ch.backspace();
                    ch.write(EXTENDED_608_13[(b2 - 0x20) as usize]);
                }
                (0x14 | 0x15, 0x20..=0x2F) => ch.command(b2),
                (0x17, 0x21..=0x23) => {
                    ch.used = true;
                    ch.col = (ch.col + (b2 - 0x20) as usize).min(31);
                }
                _ => {}
            }
            return;
        }
        self.last = None;
        if self.xds {
            return;
        }
        let ch = &mut self.channels[self.current];
        for b in [b1, b2] {
            if b >= 0x20 {
                ch.write(char_608(b));
            }
        }
    }
}

// ─── CEA-708 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Window {
    visible: bool,
    priority: u8,
    relative: bool,
    anchor_v: u8,
    anchor_h: u8,
    anchor_point: u8,
    grid: Grid,
    row: usize,
    col: usize,
    pen: Pen,
}

impl Window {
    fn new() -> Self {
        Window {
            visible: false,
            priority: 0,
            relative: false,
            anchor_v: 0,
            anchor_h: 0,
            anchor_point: 0,
            grid: Grid::new(1, 1),
            row: 0,
            col: 0,
            pen: Pen::default(),
        }
    }

    fn write(&mut self, ch: char) {
        let cols = self.grid.cols();
        if self.col >= cols {
            return;
        }
        let pen = self.pen;
        self.grid.put(self.row, self.col, Some(Cell { ch, pen }));
        self.col += 1;
    }

    fn carriage_return(&mut self) {
        self.col = 0;
        if self.row + 1 < self.grid.rows.len() {
            self.row += 1;
        } else {
            // The rows go up.
            self.grid.rows.remove(0);
            let cols = self.grid.cols().max(1);
            self.grid.rows.push(vec![None; cols]);
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Service708 {
    windows: [Option<Window>; 8],
    current: Option<usize>,
    used: bool,
}

/// The colour of a CEA-708 pen: 2 bits each.
fn color_708(b: u8) -> [u8; 3] {
    let c = |v: u8| [0u8, 0x55, 0xAA, 0xFF][(v & 3) as usize];
    [c(b >> 4), c(b >> 2), c(b)]
}

/// G2 characters (after EXT1).
fn g2(b: u8) -> Option<char> {
    Some(match b {
        0x20 | 0x21 => ' ',
        0x25 => '…',
        0x2A => 'Š',
        0x2C => 'Œ',
        0x30 => '█',
        0x31 => '‘',
        0x32 => '’',
        0x33 => '“',
        0x34 => '”',
        0x35 => '•',
        0x39 => '™',
        0x3A => 'š',
        0x3C => 'œ',
        0x3D => '℠',
        0x3F => 'Ÿ',
        0x76 => '⅛',
        0x77 => '⅜',
        0x78 => '⅝',
        0x79 => '⅞',
        0x7A => '│',
        0x7B => '┐',
        0x7C => '└',
        0x7D => '─',
        0x7E => '┘',
        0x7F => '┌',
        _ => return None,
    })
}

impl Service708 {
    fn window(&mut self) -> Option<&mut Window> {
        let i = self.current?;
        self.windows[i].as_mut()
    }

    fn write(&mut self, ch: char) {
        self.used = true;
        if let Some(w) = self.window() {
            w.write(ch);
        }
    }

    /// The windows of bitmap `bits`.
    fn each(&mut self, bits: u8, f: impl Fn(&mut Option<Window>)) {
        for (i, w) in self.windows.iter_mut().enumerate() {
            if bits & (1 << i) != 0 {
                f(w);
            }
        }
    }

    /// A service block's bytes.
    fn block(&mut self, b: &[u8]) {
        let mut i = 0;
        // The bytes after the command (none when the block is short).
        let arg = |i: usize, n: usize| -> Option<&[u8]> { b.get(i + 1..i + 1 + n) };
        while i < b.len() {
            let c = b[i];
            match c {
                // C0.
                0x00 | 0x03 => i += 1,
                0x08 => {
                    if let Some(w) = self.window() {
                        if w.col > 0 {
                            w.col -= 1;
                        }
                        let (r, cl) = (w.row, w.col);
                        w.grid.put(r, cl, None);
                    }
                    i += 1;
                }
                0x0C => {
                    if let Some(w) = self.window() {
                        w.grid.clear();
                        w.row = 0;
                        w.col = 0;
                    }
                    i += 1;
                }
                0x0D => {
                    if let Some(w) = self.window() {
                        w.carriage_return();
                    }
                    i += 1;
                }
                0x0E => {
                    if let Some(w) = self.window() {
                        let r = w.row;
                        w.grid.clear_row(r);
                        w.col = 0;
                    }
                    i += 1;
                }
                // EXT1: the extended sets.
                0x10 => {
                    let Some(&e) = b.get(i + 1) else { break };
                    i += 2;
                    match e {
                        0x00..=0x07 => {}
                        0x08..=0x0F => i += 1,
                        0x10..=0x17 => i += 2,
                        0x18..=0x1F => i += 3,
                        0x20..=0x7F => {
                            if let Some(ch) = g2(e) {
                                self.write(ch);
                            }
                        }
                        0x80..=0x87 => i += 4,
                        0x88..=0x8F => i += 5,
                        0x90..=0x9F => {
                            let n = b.get(i).map_or(0, |v| (v & 0x3F) as usize);
                            i += 1 + n;
                        }
                        // G3: the [CC] sign.
                        _ => {}
                    }
                }
                0x01..=0x0F => i += 1,
                0x11..=0x17 => i += 2,
                0x18..=0x1F => i += 3,
                // G0.
                0x20..=0x7E => {
                    self.write(c as char);
                    i += 1;
                }
                0x7F => {
                    self.write('♪');
                    i += 1;
                }
                // C1.
                0x80..=0x87 => {
                    self.used = true;
                    let n = (c - 0x80) as usize;
                    if self.windows[n].is_some() {
                        self.current = Some(n);
                    }
                    i += 1;
                }
                0x88..=0x8C => {
                    let Some(&bits) = arg(i, 1).and_then(|a| a.first()) else {
                        break;
                    };
                    self.used = true;
                    match c {
                        0x88 => self.each(bits, |w| {
                            if let Some(w) = w {
                                w.grid.clear();
                            }
                        }),
                        0x89 => self.each(bits, |w| {
                            if let Some(w) = w {
                                w.visible = true;
                            }
                        }),
                        0x8A => self.each(bits, |w| {
                            if let Some(w) = w {
                                w.visible = false;
                            }
                        }),
                        0x8B => self.each(bits, |w| {
                            if let Some(w) = w {
                                w.visible = !w.visible;
                            }
                        }),
                        _ => {
                            self.each(bits, |w| *w = None);
                            if self.current.is_some_and(|n| bits & (1 << n) != 0) {
                                self.current = None;
                            }
                        }
                    }
                    i += 2;
                }
                // DLY (its tenths of a second are not waited), DLC.
                0x8D => i += 2,
                0x8E => i += 1,
                // RST.
                0x8F => {
                    self.windows = Default::default();
                    self.current = None;
                    i += 1;
                }
                // SPA: italics, underline.
                0x90 => {
                    let Some(a) = arg(i, 2) else { break };
                    if let Some(w) = self.window() {
                        w.pen.italic = a[1] & 0x80 != 0;
                        w.pen.underline = a[1] & 0x40 != 0;
                    }
                    i += 3;
                }
                // SPC: the text's colour.
                0x91 => {
                    let Some(a) = arg(i, 3) else { break };
                    if let Some(w) = self.window() {
                        w.pen.color = color_708(a[0]);
                    }
                    i += 4;
                }
                // SPL.
                0x92 => {
                    let Some(a) = arg(i, 2) else { break };
                    if let Some(w) = self.window() {
                        w.row = ((a[0] & 0x0F) as usize).min(w.grid.rows.len().saturating_sub(1));
                        w.col = ((a[1] & 0x3F) as usize).min(w.grid.cols());
                    }
                    i += 3;
                }
                0x93..=0x96 => i += 1,
                // SWA.
                0x97 => i += 5,
                // DF0–DF7.
                0x98..=0x9F => {
                    let Some(a) = arg(i, 6) else { break };
                    self.used = true;
                    let n = (c - 0x98) as usize;
                    let rows = (a[3] & 0x0F) as usize + 1;
                    let cols = (a[4] & 0x3F) as usize + 1;
                    let w = self.windows[n].get_or_insert_with(|| {
                        let mut w = Window::new();
                        w.grid = Grid::new(rows, cols);
                        w
                    });
                    w.visible = a[0] & 0x20 != 0;
                    w.priority = a[0] & 0x07;
                    w.relative = a[1] & 0x80 != 0;
                    w.anchor_v = a[1] & 0x7F;
                    w.anchor_h = a[2];
                    w.anchor_point = (a[3] >> 4).min(8);
                    w.grid.resize(rows, cols);
                    w.row = w.row.min(rows - 1);
                    w.col = w.col.min(cols);
                    self.current = Some(n);
                    i += 7;
                }
                // G1: Latin-1.
                0xA0..=0xFF => {
                    self.write(char::from(c));
                    i += 1;
                }
            }
        }
    }

    /// What it shows on a `wide` (16:9) or 4:3 picture.
    fn screen(&self, wide: bool) -> CaptionScreen {
        let cols = if wide { 42 } else { 32 };
        let mut shown: Vec<&Window> = self
            .windows
            .iter()
            .flatten()
            .filter(|w| w.visible)
            .collect();
        shown.sort_by_key(|w| std::cmp::Reverse(w.priority));
        let mut lines = Vec::new();
        for w in shown {
            if w.grid.is_empty() {
                continue;
            }
            // Its anchor, as a share of the safe area.
            let (fv, fh) = if w.relative {
                (w.anchor_v as f32 / 100.0, w.anchor_h as f32 / 100.0)
            } else {
                (
                    w.anchor_v as f32 / 75.0,
                    w.anchor_h as f32 / if wide { 210.0 } else { 160.0 },
                )
            };
            let (rows, wc) = (w.grid.rows.len() as f32, w.grid.cols() as f32);
            let (av, ah) = ((w.anchor_point / 3) as f32, (w.anchor_point % 3) as f32);
            let top = (fv * 15.0 - av * rows / 2.0).round();
            let left = (fh * cols as f32 - ah * wc / 2.0).round();
            let top = top.clamp(0.0, (15.0 - rows).max(0.0)) as i32;
            let left = left.clamp(0.0, (cols as f32 - wc).max(0.0)) as i32;
            w.grid.lines(top, left, &mut lines);
        }
        CaptionScreen {
            cols,
            narrow: false,
            lines,
        }
    }
}

// ─── Both ───────────────────────────────────────────────────────────────────

/// What every caption channel of a stream shows.
#[derive(Debug, Clone, Default)]
pub struct Decoder {
    fields: [Field608; 2],
    services: [Service708; 6],
    /// The DTVCC packet being received.
    packet: Vec<u8>,
    size: usize,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// One picture's `cc_data`: (cc_valid, cc_type, byte 1, byte 2).
    pub fn feed(&mut self, triples: &[(bool, u8, u8, u8)]) {
        for &(valid, kind, d1, d2) in triples {
            match kind {
                0 | 1 if valid => self.fields[kind as usize].pair(d1, d2),
                3 => {
                    self.packet_end();
                    if valid {
                        let n = (d1 & 0x3F) as usize;
                        self.size = if n == 0 { 128 } else { n * 2 };
                        self.packet = vec![d1, d2];
                        if self.packet.len() >= self.size {
                            self.packet_end();
                        }
                    }
                }
                2 if valid && self.size > 0 => {
                    self.packet.extend([d1, d2]);
                    if self.packet.len() >= self.size {
                        self.packet_end();
                    }
                }
                _ => {}
            }
        }
    }

    /// A DTVCC packet received: its service blocks.
    fn packet_end(&mut self) {
        if self.size == 0 || self.packet.is_empty() {
            self.size = 0;
            return;
        }
        let p = std::mem::take(&mut self.packet);
        let end = self.size.min(p.len());
        self.size = 0;
        let mut i = 1;
        while i < end {
            let mut service = (p[i] >> 5) as usize;
            let len = (p[i] & 0x1F) as usize;
            i += 1;
            if service == 7 {
                let Some(&e) = p.get(i) else { break };
                service = (e & 0x3F) as usize;
                i += 1;
            }
            if service == 0 || len == 0 {
                break;
            }
            let block = &p[i.min(end)..(i + len).min(end)];
            if (1..=6).contains(&service) {
                self.services[service - 1].block(block);
            }
            i += len;
        }
    }

    /// The channels carrying something, and what they show (on a `wide` picture).
    pub fn screens(&self, wide: bool) -> BTreeMap<String, CaptionScreen> {
        let mut out = BTreeMap::new();
        for (f, field) in self.fields.iter().enumerate() {
            for (c, ch) in field.channels.iter().enumerate() {
                if ch.used {
                    out.insert(format!("cc{}", f * 2 + c + 1), ch.screen());
                }
            }
        }
        for (i, s) in self.services.iter().enumerate() {
            if s.used {
                out.insert(format!("s{}", i + 1), s.screen(wide));
            }
        }
        out
    }
}

// ─── From MPEG-TS ───────────────────────────────────────────────────────────

/// One picture's `cc_data` triples: (cc_valid, cc_type, byte 1, byte 2).
pub type CcData = Vec<(bool, u8, u8, u8)>;

/// The `cc_data` of the pictures of a transport stream, in the order they
/// are shown.
#[derive(Debug, Default)]
pub struct Extractor {
    pmt: Option<u16>,
    video: Option<(u16, u8)>,
    /// The PES being received.
    pes: Vec<u8>,
    /// Bytes not yet a whole TS packet.
    rest: Vec<u8>,
    /// Pictures waiting for their turn: (PTS, cc_data).
    waiting: Vec<(i64, CcData)>,
    /// Pictures whose data was read (with or without captions).
    pub pictures: u64,
    last_pts: Option<i64>,
}

/// Pictures kept to be put back in order (B frames).
const REORDER: usize = 6;

impl Extractor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes of the stream; the pictures' data now in order, each with its
    /// PTS (90 kHz).
    pub fn feed(&mut self, data: &[u8]) -> Vec<(i64, CcData)> {
        let mut out = Vec::new();
        let mut buf = std::mem::take(&mut self.rest);
        buf.extend_from_slice(data);
        let mut i = 0;
        while i + 188 <= buf.len() {
            if buf[i] != 0x47 {
                // Lost: to the next sync byte.
                i += 1;
                continue;
            }
            self.packet(&buf[i..i + 188], &mut out);
            i += 188;
        }
        self.rest = buf[i..].to_vec();
        out
    }

    fn packet(&mut self, p: &[u8], out: &mut Vec<(i64, CcData)>) {
        let start = p[1] & 0x40 != 0;
        let pid = (((p[1] & 0x1F) as u16) << 8) | p[2] as u16;
        let afc = (p[3] >> 4) & 3;
        if afc & 1 == 0 {
            return;
        }
        let mut off = 4;
        if afc == 3 {
            off += 1 + p[4] as usize;
        }
        if off >= 188 {
            return;
        }
        let payload = &p[off..];
        if pid == 0 {
            if start {
                self.pat(payload);
            }
        } else if Some(pid) == self.pmt {
            if start {
                self.pmt_table(payload);
            }
        } else if self.video.is_some_and(|(v, _)| v == pid) {
            if start {
                self.pes_end(out);
                self.pes.clear();
            }
            if start || !self.pes.is_empty() {
                self.pes.extend_from_slice(payload);
            }
        }
    }

    /// A PSI section (after the pointer field).
    fn section(payload: &[u8]) -> Option<&[u8]> {
        let ptr = *payload.first()? as usize;
        let s = payload.get(1 + ptr..)?;
        let len = (((*s.get(1)? & 0x0F) as usize) << 8) | *s.get(2)? as usize;
        // Without its CRC.
        s.get(..(3 + len).saturating_sub(4))
    }

    fn pat(&mut self, payload: &[u8]) {
        let Some(s) = Self::section(payload) else {
            return;
        };
        let mut i = 8;
        while i + 4 <= s.len() {
            let program = u16::from_be_bytes([s[i], s[i + 1]]);
            let pid = (((s[i + 2] & 0x1F) as u16) << 8) | s[i + 3] as u16;
            if program != 0 {
                self.pmt = Some(pid);
                return;
            }
            i += 4;
        }
    }

    fn pmt_table(&mut self, payload: &[u8]) {
        let Some(s) = Self::section(payload) else {
            return;
        };
        if s.len() < 12 {
            return;
        }
        let info = (((s[10] & 0x0F) as usize) << 8) | s[11] as usize;
        let mut i = 12 + info;
        while i + 5 <= s.len() {
            let kind = s[i];
            let pid = (((s[i + 1] & 0x1F) as u16) << 8) | s[i + 2] as u16;
            let es = (((s[i + 3] & 0x0F) as usize) << 8) | s[i + 4] as usize;
            // MPEG-2, H.264, HEVC.
            if matches!(kind, 0x01 | 0x02 | 0x1B | 0x24) {
                if self.video.map(|v| v.0) != Some(pid) {
                    self.pes.clear();
                }
                self.video = Some((pid, kind));
                return;
            }
            i += 5 + es;
        }
    }

    /// A whole PES of the video: its pictures' data.
    fn pes_end(&mut self, out: &mut Vec<(i64, CcData)>) {
        let p = &self.pes;
        if p.len() < 9 || p[0] != 0 || p[1] != 0 || p[2] != 1 {
            return;
        }
        let header = 9 + p[8] as usize;
        let pts = if p[7] & 0x80 != 0 && p.len() >= 14 {
            let b = &p[9..14];
            Some(
                (((b[0] >> 1) & 0x07) as i64) << 30
                    | (b[1] as i64) << 22
                    | ((b[2] >> 1) as i64) << 15
                    | (b[3] as i64) << 7
                    | (b[4] >> 1) as i64,
            )
        } else {
            None
        };
        let Some(es) = p.get(header..) else { return };
        let kind = self.video.map_or(0x1B, |v| v.1);
        let cc = cc_of(es, kind);
        self.pictures += 1;
        // Without a PTS: right after the last one.
        let pts = pts.or(self.last_pts.map(|t| t + 1)).unwrap_or(0);
        // The times jumped (a loop, a new stream): those waiting go first.
        if let Some(last) = self.last_pts {
            if (pts - last).abs() > 90_000 * 2 {
                self.flush(out);
            }
        }
        self.last_pts = Some(pts);
        self.waiting.push((pts, cc));
        if self.waiting.len() > REORDER {
            self.waiting.sort_by_key(|w| w.0);
            out.push(self.waiting.remove(0));
        }
    }

    /// Every picture waiting, in order.
    pub fn flush(&mut self, out: &mut Vec<(i64, CcData)>) {
        self.waiting.sort_by_key(|w| w.0);
        out.append(&mut self.waiting);
    }
}

/// The NAL units (or MPEG-2 start codes) of an elementary stream: what
/// follows each 00 00 01.
fn units(es: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= es.len() {
        if es[i] == 0 && es[i + 1] == 0 && es[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let mut e = starts.get(k + 1).map_or(es.len(), |n| n - 3);
            // The zero before a 4-byte start code.
            while e > s && es[e - 1] == 0 {
                e -= 1;
            }
            &es[s..e]
        })
        .collect()
}

/// Without its emulation prevention bytes (00 00 03).
fn rbsp(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut zeros = 0;
    for &v in b {
        if zeros >= 2 && v == 3 {
            zeros = 0;
            continue;
        }
        zeros = if v == 0 { zeros + 1 } else { 0 };
        out.push(v);
    }
    out
}

/// The `cc_data` of an elementary stream's piece (`kind`: its stream type).
pub fn cc_of(es: &[u8], kind: u8) -> Vec<(bool, u8, u8, u8)> {
    let mut out = Vec::new();
    for u in units(es) {
        let Some(&h) = u.first() else { continue };
        match kind {
            // MPEG-2: user data.
            0x01 | 0x02 if h == 0xB2 => {
                if u.get(1..6) == Some(b"GA94\x03") {
                    cc_data(&u[6..], &mut out);
                }
            }
            0x1B if h & 0x1F == 6 => sei(&rbsp(&u[1..]), &mut out),
            0x24 if matches!((h >> 1) & 0x3F, 39 | 40) && u.len() > 2 => {
                sei(&rbsp(&u[2..]), &mut out)
            }
            _ => {}
        }
    }
    out
}

/// The SEI messages: the ATSC ones (registered ITU-T T.35, "GA94", 3).
fn sei(b: &[u8], out: &mut Vec<(bool, u8, u8, u8)>) {
    let mut i = 0;
    while i < b.len() && b[i] != 0x80 {
        let mut kind = 0usize;
        while i < b.len() && b[i] == 0xFF {
            kind += 255;
            i += 1;
        }
        let Some(&k) = b.get(i) else { return };
        kind += k as usize;
        i += 1;
        let mut size = 0usize;
        while i < b.len() && b[i] == 0xFF {
            size += 255;
            i += 1;
        }
        let Some(&s) = b.get(i) else { return };
        size += s as usize;
        i += 1;
        let Some(m) = b.get(i..i + size) else { return };
        if kind == 4 && m.len() > 8 {
            // Country (one more byte after 0xFF), provider, "GA94", 3.
            let off = if m[0] == 0xFF { 2 } else { 1 };
            if m.get(off..off + 2) == Some(&[0x00, 0x31])
                && m.get(off + 2..off + 7) == Some(b"GA94\x03")
            {
                cc_data(&m[off + 7..], out);
            }
        }
        i += size;
    }
}

/// `cc_data()`: flags and count, em_data, the triples.
fn cc_data(b: &[u8], out: &mut Vec<(bool, u8, u8, u8)>) {
    let Some(&flags) = b.first() else { return };
    if flags & 0x40 == 0 {
        return;
    }
    let n = (flags & 0x1F) as usize;
    for k in 0..n {
        let Some(t) = b.get(2 + 3 * k..5 + 3 * k) else {
            return;
        };
        out.push((t[0] & 0x04 != 0, t[0] & 0x03, t[1], t[2]));
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// With odd parity.
    pub fn par(b: u8) -> u8 {
        if (b & 0x7F).count_ones().is_multiple_of(2) {
            b | 0x80
        } else {
            b & 0x7F
        }
    }

    /// A CEA-608 pair for field 1.
    fn pair(a: u8, b: u8) -> (bool, u8, u8, u8) {
        (true, 0, par(a), par(b))
    }

    fn text_pairs(s: &str) -> Vec<(bool, u8, u8, u8)> {
        let b: Vec<u8> = s.bytes().collect();
        b.chunks(2)
            .map(|c| pair(c[0], *c.get(1).unwrap_or(&0)))
            .collect()
    }

    fn row_text(s: &CaptionScreen) -> Vec<(u8, u8, String)> {
        s.lines
            .iter()
            .map(|l| {
                (
                    l.row,
                    l.col,
                    l.spans.iter().map(|s| s.text.as_str()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn pop_on_shows_at_the_end_of_caption() {
        let mut d = Decoder::new();
        // RCL twice (the second ignored), PAC row 15 indent 4, text, EOC.
        let mut t = vec![pair(0x14, 0x20), pair(0x14, 0x20), pair(0x14, 0x72)];
        t.extend(text_pairs("Bonjour"));
        d.feed(&t);
        assert!(d.screens(true)["cc1"].lines.is_empty(), "not shown yet");
        d.feed(&[pair(0x14, 0x2F), pair(0x14, 0x2F)]);
        let s = &d.screens(true)["cc1"];
        assert_eq!(row_text(s), vec![(14, 4, "Bonjour".to_string())]);
        assert!(s.narrow);
        // EDM: gone.
        d.feed(&[pair(0x14, 0x2C)]);
        assert!(d.screens(true)["cc1"].lines.is_empty());
    }

    #[test]
    fn roll_up_moves_the_lines_up() {
        let mut d = Decoder::new();
        let mut t = vec![pair(0x14, 0x25), pair(0x14, 0x70)];
        t.extend(text_pairs("ONE"));
        t.push(pair(0x14, 0x2D));
        t.extend(text_pairs("TWO"));
        t.push(pair(0x14, 0x2D));
        t.extend(text_pairs("THREE"));
        d.feed(&t);
        // Two rows: ONE went out.
        assert_eq!(
            row_text(&d.screens(true)["cc1"]),
            vec![(13, 0, "TWO".to_string()), (14, 0, "THREE".to_string())]
        );
    }

    #[test]
    fn colours_specials_extended_and_channel_two() {
        let mut d = Decoder::new();
        // RDC (paint-on), PAC row 1 yellow (code 5: 0x4A), "ca", ç…
        let mut t = vec![pair(0x14, 0x29), pair(0x11, 0x4A)];
        t.extend(text_pairs("Gar"));
        // 'r' then ç by the basic set (0x7B).
        t.push(pair(0x7B, 0x6F));
        t.push(pair(0x6E, 0x00));
        // Mid-row white italics, then ♪, then É (extended, replacing a space).
        t.push(pair(0x11, 0x2E));
        t.push(pair(0x11, 0x37));
        t.push(pair(0x20, 0x00));
        t.push(pair(0x12, 0x21));
        d.feed(&t);
        let s = &d.screens(true)["cc1"];
        assert_eq!(row_text(s), vec![(0, 0, "Garçon ♪É".to_string())]);
        assert_eq!(s.lines[0].spans[0].color, "#ffff00");
        assert!(s.lines[0].spans[1].italic);
        // CC2: control codes with 0x1C.
        d.feed(&[pair(0x1C, 0x29), pair(0x19, 0x40)]);
        d.feed(&text_pairs("B"));
        let all = d.screens(true);
        assert_eq!(row_text(&all["cc2"]), vec![(0, 0, "B".to_string())]);
        assert_eq!(row_text(&all["cc1"]), vec![(0, 0, "Garçon ♪É".to_string())]);
    }

    /// A DTVCC packet as cc_data triples (service 1).
    pub fn dtvcc(seq: u8, block: &[u8]) -> Vec<(bool, u8, u8, u8)> {
        let mut p = vec![(1u8 << 5) | block.len() as u8];
        p.extend_from_slice(block);
        // Header + data, to an even size.
        let mut all = vec![0u8];
        all.extend(p);
        if all.len() % 2 == 1 {
            all.push(0);
        }
        all[0] = (seq << 6) | (all.len() / 2) as u8;
        all.chunks(2)
            .enumerate()
            .map(|(i, c)| (true, if i == 0 { 3 } else { 2 }, c[0], c[1]))
            .collect()
    }

    #[test]
    fn cea708_window_and_text() {
        let mut d = Decoder::new();
        // DF0: visible, anchor bottom centre (point 7) at v 74 / h 105 of
        // 210, 2 rows, 20 columns; then yellow pen, text, CR, text.
        let mut b = vec![0x98, 0x20, 74, 105, (7 << 4) | 1, 19, 0x00];
        b.extend([0x91, 0b00_11_11_00, 0x00, 0x00]);
        b.extend(b"Hello");
        b.push(0x0D);
        b.extend(b"world \x7f");
        d.feed(&dtvcc(0, &b));
        let s = &d.screens(true)["s1"];
        assert_eq!(s.cols, 42);
        assert!(!s.narrow);
        let rows = row_text(s);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].2, "Hello");
        assert_eq!(rows[1].2, "world ♪");
        // The bottom centre of the safe area: last rows, centred.
        assert_eq!((rows[0].0, rows[1].0), (13, 14));
        assert_eq!(rows[0].1, 11);
        assert_eq!(s.lines[0].spans[0].color, "#ffff00");
        // HDW 0: hidden.
        d.feed(&dtvcc(1, &[0x8A, 0x01]));
        assert!(d.screens(true)["s1"].lines.is_empty());
    }

    /// A TS packet.
    fn ts(pid: u16, start: bool, cc: u8, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![0x47, ((start as u8) << 6) | (pid >> 8) as u8, pid as u8];
        let room = 184;
        if payload.len() >= room {
            p.push(0x10 | (cc & 0x0F));
            p.extend_from_slice(&payload[..room]);
        } else {
            // Stuffing in the adaptation field.
            let fill = room - payload.len();
            p.push(0x30 | (cc & 0x0F));
            p.push((fill - 1) as u8);
            if fill > 1 {
                p.push(0);
                p.extend(std::iter::repeat_n(0xFF, fill - 2));
            }
            p.extend_from_slice(payload);
        }
        p
    }

    fn psi(table: u8, body: &[u8]) -> Vec<u8> {
        let len = body.len() + 5 + 4;
        let mut s = vec![
            0,
            table,
            0xB0 | (len >> 8) as u8,
            len as u8,
            0,
            1,
            0xC1,
            0,
            0,
        ];
        s.extend_from_slice(body);
        let crc = crate::captions::tests::crc(&s[1..]);
        s.extend(crc.to_be_bytes());
        s
    }

    pub fn crc(b: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &x in b {
            c ^= (x as u32) << 24;
            for _ in 0..8 {
                c = if c & 0x8000_0000 != 0 {
                    (c << 1) ^ 0x04C1_1DB7
                } else {
                    c << 1
                };
            }
        }
        c
    }

    /// An SEI NAL (H.264) carrying `triples`.
    pub fn sei_nal(triples: &[(bool, u8, u8, u8)]) -> Vec<u8> {
        let mut m = vec![0xB5, 0x00, 0x31, b'G', b'A', b'9', b'4', 0x03];
        m.push(0x40 | triples.len() as u8);
        m.push(0xFF);
        for &(v, k, a, b) in triples {
            m.extend([0xF8 | ((v as u8) << 2) | k, a, b]);
        }
        m.push(0xFF);
        let mut nal = vec![0, 0, 0, 1, 0x06, 4, m.len() as u8];
        nal.extend(m);
        nal.push(0x80);
        nal
    }

    fn pes(pts: i64, es: &[u8]) -> Vec<u8> {
        let mut p = vec![0, 0, 1, 0xE0, 0, 0, 0x80, 0x80, 5];
        let b = [
            0x21 | (((pts >> 30) & 7) as u8) << 1,
            (pts >> 22) as u8,
            0x01 | ((pts >> 15) as u8) << 1,
            (pts >> 7) as u8,
            0x01 | (pts as u8) << 1,
        ];
        p.extend(b);
        p.extend_from_slice(es);
        p
    }

    #[test]
    fn transport_stream_pictures_back_in_order() {
        // PAT → PMT 0x100 → H.264 on 0x101.
        let mut stream = ts(0, true, 0, &psi(0, &[0, 1, 0xE1, 0x00]));
        stream.extend(ts(
            0x100,
            true,
            0,
            &psi(2, &[0xE1, 0x01, 0xF0, 0, 0x1B, 0xE1, 0x01, 0xF0, 0]),
        ));
        // Sent I P B B (decode order), shown I B B P: the letters of each
        // picture spell the words in the order shown.
        let frames = [
            (0, "AB"),
            (3, "GH"),
            (1, "CD"),
            (2, "EF"),
            (6, "MN"),
            (4, "IJ"),
            (5, "KL"),
        ];
        let mut cc = 0u8;
        for (k, (n, s)) in frames.iter().enumerate() {
            let b = s.as_bytes();
            let mut t = Vec::new();
            if k == 0 {
                t.push(pair(0x14, 0x29));
                t.push(pair(0x14, 0x70));
            }
            t.push(pair(b[0], b[1]));
            let es = [vec![0, 0, 0, 1, 0x09, 0xF0], sei_nal(&t)].concat();
            let p = pes(90_000 + n * 3600, &es);
            for (j, chunk) in p.chunks(184).enumerate() {
                stream.extend(ts(0x101, j == 0, cc, chunk));
                cc = cc.wrapping_add(1);
            }
        }
        // A last PES starting ends the one before.
        stream.extend(ts(
            0x101,
            true,
            cc,
            &pes(90_000 + 7 * 3600, &[0, 0, 1, 0x09, 0xF0]),
        ));
        let mut x = Extractor::new();
        let mut d = Decoder::new();
        let mut got = Vec::new();
        // In pieces of any size.
        for piece in stream.chunks(100) {
            got.extend(x.feed(piece));
        }
        x.flush(&mut got);
        let order: Vec<i64> = got.iter().map(|g| (g.0 - 90_000) / 3600).collect();
        // The last PES is still being received (the next one ends it).
        assert_eq!(order, vec![0, 1, 2, 3, 4, 5, 6]);
        for (_, t) in &got {
            d.feed(t);
        }
        assert_eq!(
            row_text(&d.screens(true)["cc1"]),
            vec![(14, 0, "ABCDEFGHIJKLMN".to_string())]
        );
        assert_eq!(x.pictures, 7);
    }

    #[test]
    fn boxes_on_the_picture() {
        let s = CaptionScreen {
            cols: 32,
            narrow: true,
            lines: vec![CaptionLine {
                row: 14,
                col: 4,
                spans: vec![
                    CaptionSpan {
                        text: "Hi ".into(),
                        color: "#ffffff".into(),
                        ..Default::default()
                    },
                    CaptionSpan {
                        text: "you".into(),
                        color: "#ffff00".into(),
                        underline: true,
                        ..Default::default()
                    },
                ],
            }],
        };
        let b = caption_boxes(&s, [0.0, 0.0, 1920.0, 1080.0], 1.0);
        assert_eq!(b.len(), 2);
        // 4:3 area of 1080 × 0.8 = 1152 wide, from 384; column 4 of 32.
        let size = (864.0f32 / 15.0 * 0.82).min(1152.0 / 32.0 / ADVANCE);
        assert!((b[0].size - size).abs() < 1e-3);
        assert!((b[0].a[0] - (384.0 + 4.0 * 36.0 - size * 0.25)).abs() < 1e-3);
        assert!((b[1].a[0] - (384.0 + 4.0 * 36.0 + 3.0 * size * ADVANCE)).abs() < 1e-3);
        assert!(b[1].underline.is_some() && b[0].underline.is_none());
        let mut n = s.clone();
        n.lines[0].spans[0].text = "♪ a".into();
        let b = caption_boxes(&n, [0.0, 0.0, 1920.0, 1080.0], 1.0);
        assert_eq!(b[0].text, "  a");
        assert_eq!(b[0].notes.len(), 1);
        assert!((b[0].notes[0][0] - b[0].t[0]).abs() < 1e-3);
        assert_eq!(pick(&BTreeMap::new(), "auto"), None);
    }
}
