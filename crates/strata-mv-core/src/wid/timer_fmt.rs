//! Timer formatting: format tokens, wall-clock / duration segments, day and
//! month names, time zones. Mirrored by `web/src/widgets/timer-fmt.ts`.

use jiff::civil::DateTime;
use jiff::tz::{Offset, TimeZone};
use jiff::Timestamp;

/// One piece of the shown text.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub k: SegKind,
    pub t: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    /// Digits.
    Num,
    /// A day / month name or AM / PM.
    Name,
    /// Literal text.
    Lit,
    /// The separator (the `:` of the format, shown as the chosen character).
    Sep,
}

impl Seg {
    pub fn lit(t: &str) -> Self {
        Seg {
            k: SegKind::Lit,
            t: t.into(),
        }
    }
}

/// The text of segments, the separator replacing `:`.
pub fn seg_text(segs: &[Seg], sep: &str) -> String {
    segs.iter()
        .map(|g| if g.k == SegKind::Sep { sep } else { &g.t })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Literal text; `raw` = from `[brackets]` (no separator inside).
    Lit(String, bool),
    T(&'static str),
}

/// Longest first. New tokens (YYYY MMMM dddd DD HH mm ss S SS FF A) and the
/// tokens of the older clocks (yyyy dd d hh h m s f M) together.
const TOKENS: &[&str] = &[
    "YYYY", "yyyy", "MMMM", "dddd", "MMM", "ddd", "YY", "yy", "MM", "DD", "dd", "HH", "hh", "mm",
    "ss", "SS", "FF", "M", "D", "d", "H", "h", "m", "s", "S", "f", "A",
];

pub fn tokenize(fmt: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut rest = fmt;
    let flush = |lit: &mut String, out: &mut Vec<Tok>| {
        if !lit.is_empty() {
            out.push(Tok::Lit(std::mem::take(lit), false));
        }
    };
    'outer: while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('[') {
            if let Some(end) = r.find(']') {
                flush(&mut lit, &mut out);
                out.push(Tok::Lit(r[..end].to_string(), true));
                rest = &r[end + 1..];
                continue;
            }
        }
        for t in TOKENS {
            if let Some(r) = rest.strip_prefix(t) {
                flush(&mut lit, &mut out);
                out.push(Tok::T(t));
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap();
        lit.push(c);
        rest = &rest[c.len_utf8()..];
    }
    flush(&mut lit, &mut out);
    out
}

pub fn has_token(fmt: &str) -> bool {
    tokenize(fmt).iter().any(|t| matches!(t, Tok::T(_)))
}

fn push_lit(segs: &mut Vec<Seg>, t: &str, raw: bool) {
    if raw {
        segs.push(Seg::lit(t));
        return;
    }
    let mut cur = String::new();
    for c in t.chars() {
        if c == ':' {
            if !cur.is_empty() {
                segs.push(Seg::lit(&std::mem::take(&mut cur)));
            }
            segs.push(Seg {
                k: SegKind::Sep,
                t: ":".into(),
            });
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        segs.push(Seg::lit(&cur));
    }
}

/// Leading zeros: shown (`00:01:05`) or hidden (`1:05`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DigitOpts {
    pub trim: bool,
    pub hours_auto: bool,
}

/// How a duration is rounded to the last unit of the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    Up,
    Trunc,
    Near,
}

fn is_day(t: &str) -> bool {
    matches!(t, "DD" | "D" | "dd" | "d")
}
fn is_hour(t: &str) -> bool {
    matches!(t, "HH" | "H" | "hh" | "h")
}

/// A duration (ms, sign ignored) → segments. Units missing from the format
/// carry into the next one (no hours: 90:00).
pub fn dur_segs(ms: f64, fmt: &str, fps: f64, round: Rounding, o: DigitOpts) -> Vec<Seg> {
    let toks = tokenize(fmt);
    let has = |f: fn(&str) -> bool| toks.iter().any(|t| matches!(t, Tok::T(x) if f(x)));
    let h_d = has(is_day);
    let h_h = has(is_hour);
    let h_m = has(|t| t == "mm" || t == "m");
    let h_s = has(|t| t == "ss" || t == "s");
    let frame = 1000.0 / fps.max(1.0);
    let p = if has(|t| t == "FF") {
        frame
    } else if has(|t| t == "SS") {
        10.0
    } else if has(|t| t == "S" || t == "f") {
        100.0
    } else if h_s {
        1000.0
    } else if h_m {
        60000.0
    } else if h_h {
        3_600_000.0
    } else {
        86_400_000.0
    };
    let q = ms.abs() / p;
    let q = match round {
        Rounding::Up => (q - 1e-7).ceil(),
        Rounding::Near => q.round(),
        Rounding::Trunc => (q + 1e-7).floor(),
    };
    let a = (q * p).max(0.0);
    let mut rem = a;
    let dd = if h_d {
        (rem / 86_400_000.0).floor()
    } else {
        0.0
    };
    rem -= dd * 86_400_000.0;
    let hh = if h_h {
        (rem / 3_600_000.0).floor()
    } else {
        0.0
    };
    rem -= hh * 3_600_000.0;
    let mm = if h_m { (rem / 60000.0).floor() } else { 0.0 };
    rem -= mm * 60000.0;
    let ss = if h_s { (rem / 1000.0).floor() } else { 0.0 };
    let f = (a % 1000.0).round().min(999.0);
    let val = |t: &str| -> u64 {
        (match t {
            "DD" | "D" | "dd" | "d" => dd,
            "HH" | "H" | "hh" | "h" => hh,
            "mm" | "m" => mm,
            "ss" | "s" => ss,
            "S" | "f" => (f / 100.0).floor(),
            "SS" => (f / 10.0).floor(),
            "FF" => ((f + 0.01) / frame).floor(),
            _ => 0.0,
        }) as u64
    };
    let mut parts: Vec<(Tok, u64)> = toks
        .into_iter()
        .map(|t| match t {
            Tok::T(x) => (Tok::T(x), val(x)),
            l => (l, 0),
        })
        .collect();
    // Hide the leading zero units (days, hours), never minutes:seconds.
    if o.trim || o.hours_auto {
        while let Some(i) = parts.iter().position(|p| matches!(p.0, Tok::T(_))) {
            let Tok::T(x) = parts[i].0 else { break };
            if !(is_day(x) || is_hour(x)) || parts[i].1 != 0 {
                break;
            }
            let mut j = i + 1;
            while j < parts.len() && !matches!(parts[j].0, Tok::T(_)) {
                j += 1;
            }
            if j >= parts.len() {
                break;
            }
            let Tok::T(y) = parts[j].0 else { break };
            if !(is_hour(y) || matches!(y, "mm" | "m" | "ss" | "s")) {
                break;
            }
            parts.drain(i..j);
        }
    }
    let mut segs = Vec::new();
    let mut first = true;
    for (t, n) in parts {
        match t {
            Tok::Lit(s, raw) => push_lit(&mut segs, &s, raw),
            Tok::T(x) => {
                let two = matches!(x, "DD" | "dd" | "HH" | "hh" | "mm" | "ss" | "SS" | "FF");
                let mut txt = if two {
                    format!("{n:02}")
                } else {
                    n.to_string()
                };
                if first && o.trim && matches!(x, "DD" | "dd" | "HH" | "hh" | "mm") {
                    txt = n.to_string();
                }
                first = false;
                segs.push(Seg {
                    k: SegKind::Num,
                    t: txt,
                });
            }
        }
    }
    segs
}

/// Calendar fields of an instant in a zone. `wd`: 0 = Sunday.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Comp {
    pub y: i32,
    pub mo: u32,
    pub d: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
    pub ms: u32,
    pub wd: u32,
}

/// Where a wall clock is read.
#[derive(Debug, Clone, PartialEq)]
pub enum ZoneSel {
    /// The server's zone.
    Machine,
    Named(String),
    /// Hours from UTC.
    Offset(f64),
}

impl ZoneSel {
    /// "" = machine, else a named zone.
    pub fn named(name: &str) -> Self {
        if name.trim().is_empty() || name.trim() == "machine" {
            ZoneSel::Machine
        } else {
            ZoneSel::Named(name.trim().into())
        }
    }
    pub fn tz(&self, default_tz: &str) -> TimeZone {
        let get = |n: &str| {
            if n.trim().is_empty() {
                TimeZone::system()
            } else {
                TimeZone::get(n.trim()).unwrap_or_else(|_| TimeZone::system())
            }
        };
        match self {
            ZoneSel::Machine => get(default_tz),
            ZoneSel::Named(n) => get(n),
            ZoneSel::Offset(h) => Offset::from_seconds((h * 3600.0).round() as i32)
                .map(TimeZone::fixed)
                .unwrap_or(TimeZone::UTC),
        }
    }
}

pub fn comp_at(ms: i64, tz: &TimeZone) -> Comp {
    let Ok(ts) = Timestamp::from_millisecond(ms) else {
        return Comp::default();
    };
    let z = ts.to_zoned(tz.clone());
    Comp {
        y: z.year() as i32,
        mo: z.month() as u32,
        d: z.day() as u32,
        h: z.hour() as u32,
        mi: z.minute() as u32,
        s: z.second() as u32,
        ms: ms.rem_euclid(1000) as u32,
        wd: z.weekday().to_sunday_zero_offset() as u32,
    }
}

/// A wall-clock date and time in a zone → Unix ms (summer / winter time safe).
pub fn zoned_ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, tz: &TimeZone) -> Option<i64> {
    let dt = DateTime::new(y as i16, mo as i8, d as i8, h as i8, mi as i8, s as i8, 0).ok()?;
    Some(dt.to_zoned(tz.clone()).ok()?.timestamp().as_millisecond())
}

// ------------------------------------------------------------- names --

const MONTHS: [(&str, [&str; 12], [&str; 12]); 7] = [
    (
        "en",
        [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ],
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
    ),
    (
        "fr",
        [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ],
        [
            "janv", "févr", "mars", "avr", "mai", "juin", "juil", "août", "sept", "oct", "nov",
            "déc",
        ],
    ),
    (
        "de",
        [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ],
        [
            "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
        ],
    ),
    (
        "es",
        [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ],
        [
            "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic",
        ],
    ),
    (
        "it",
        [
            "gennaio",
            "febbraio",
            "marzo",
            "aprile",
            "maggio",
            "giugno",
            "luglio",
            "agosto",
            "settembre",
            "ottobre",
            "novembre",
            "dicembre",
        ],
        [
            "gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic",
        ],
    ),
    (
        "pt",
        [
            "janeiro",
            "fevereiro",
            "março",
            "abril",
            "maio",
            "junho",
            "julho",
            "agosto",
            "setembro",
            "outubro",
            "novembro",
            "dezembro",
        ],
        [
            "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
        ],
    ),
    (
        "nl",
        [
            "januari",
            "februari",
            "maart",
            "april",
            "mei",
            "juni",
            "juli",
            "augustus",
            "september",
            "oktober",
            "november",
            "december",
        ],
        [
            "jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec",
        ],
    ),
];

const DAYS: [(&str, [&str; 7], [&str; 7]); 7] = [
    (
        "en",
        [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ],
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    ),
    (
        "fr",
        [
            "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
        ],
        ["dim", "lun", "mar", "mer", "jeu", "ven", "sam"],
    ),
    (
        "de",
        [
            "Sonntag",
            "Montag",
            "Dienstag",
            "Mittwoch",
            "Donnerstag",
            "Freitag",
            "Samstag",
        ],
        ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
    ),
    (
        "es",
        [
            "domingo",
            "lunes",
            "martes",
            "miércoles",
            "jueves",
            "viernes",
            "sábado",
        ],
        ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    ),
    (
        "it",
        [
            "domenica",
            "lunedì",
            "martedì",
            "mercoledì",
            "giovedì",
            "venerdì",
            "sabato",
        ],
        ["dom", "lun", "mar", "mer", "gio", "ven", "sab"],
    ),
    (
        "pt",
        [
            "domingo",
            "segunda-feira",
            "terça-feira",
            "quarta-feira",
            "quinta-feira",
            "sexta-feira",
            "sábado",
        ],
        ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"],
    ),
    (
        "nl",
        [
            "zondag",
            "maandag",
            "dinsdag",
            "woensdag",
            "donderdag",
            "vrijdag",
            "zaterdag",
        ],
        ["zo", "ma", "di", "wo", "do", "vr", "za"],
    ),
];

/// The languages of the day and month names (same tables in the web renderer:
/// the browser's locale is never used, so every output shows the same names).
pub const NAME_LANGS: [&str; 7] = ["fr", "en", "de", "es", "it", "pt", "nl"];

fn known(lang: &str) -> Option<&'static str> {
    let l = lang
        .trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    NAME_LANGS.iter().copied().find(|k| *k == l)
}

/// The names' language of a clock: its own (`fr-FR`, `de`…), else the
/// project's (`""` = the project's language), else French.
pub fn name_lang(lang: &str, project: &str) -> &'static str {
    known(lang).or_else(|| known(project)).unwrap_or("fr")
}

fn lang_idx(lang: &str) -> usize {
    let l = name_lang(lang, "");
    MONTHS.iter().position(|m| m.0 == l).unwrap_or(0)
}

pub fn month_name(lang: &str, mo: u32, long: bool) -> &'static str {
    let m = &MONTHS[lang_idx(lang)];
    let i = (mo.clamp(1, 12) - 1) as usize;
    if long {
        m.1[i]
    } else {
        m.2[i]
    }
}

pub fn day_name(lang: &str, wd: u32, long: bool) -> &'static str {
    let d = &DAYS[lang_idx(lang)];
    let i = (wd % 7) as usize;
    if long {
        d.1[i]
    } else {
        d.2[i]
    }
}

/// Case of the date names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    None,
    First,
    Words,
    Upper,
    Lower,
}

fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn apply_case(segs: Vec<Seg>, mode: CaseMode) -> Vec<Seg> {
    match mode {
        CaseMode::None => segs,
        CaseMode::Upper => segs
            .into_iter()
            .map(|g| Seg {
                t: g.t.to_uppercase(),
                ..g
            })
            .collect(),
        CaseMode::Lower => segs
            .into_iter()
            .map(|g| Seg {
                t: g.t.to_lowercase(),
                ..g
            })
            .collect(),
        CaseMode::Words => segs
            .into_iter()
            .map(|g| {
                let mut out = String::new();
                let mut start = true;
                for c in g.t.chars() {
                    if start && c.is_alphabetic() {
                        out.extend(c.to_uppercase());
                    } else {
                        out.push(c);
                    }
                    start = c.is_whitespace() || c == '-';
                }
                Seg { t: out, ..g }
            })
            .collect(),
        CaseMode::First => {
            let mut done = false;
            segs.into_iter()
                .map(|g| {
                    if done || !g.t.chars().any(char::is_alphabetic) {
                        return g;
                    }
                    done = true;
                    let i = g.t.find(char::is_alphabetic).unwrap();
                    Seg {
                        t: format!("{}{}", &g.t[..i], cap_first(&g.t[i..])),
                        ..g
                    }
                })
                .collect()
        }
    }
}

/// Options of a wall clock.
#[derive(Debug, Clone)]
pub struct WallOpts<'a> {
    pub lang: &'a str,
    pub h12: bool,
    pub am: &'a str,
    pub pm: &'a str,
    pub trim: bool,
    pub fps: f64,
}

/// Wall-clock fields → segments.
pub fn wall_segs(c: &Comp, fmt: &str, o: &WallOpts) -> Vec<Seg> {
    let mut toks = tokenize(fmt);
    let has = |toks: &[Tok], f: &dyn Fn(&str) -> bool| {
        toks.iter().any(|t| matches!(t, Tok::T(x) if f(x)))
    };
    if o.h12 && has(&toks, &|t| t == "HH" || t == "H") && !has(&toks, &|t| t == "A") {
        toks.push(Tok::Lit(" ".into(), true));
        toks.push(Tok::T("A"));
    }
    let h12 = (c.h + 11) % 12 + 1;
    let hh = if o.h12 { h12 } else { c.h };
    let frame = 1000.0 / o.fps.max(1.0);
    let mut segs = Vec::new();
    let mut first = true;
    for t in toks {
        let x = match t {
            Tok::Lit(s, raw) => {
                push_lit(&mut segs, &s, raw);
                continue;
            }
            Tok::T(x) => x,
        };
        let t: String = match x {
            "YYYY" | "yyyy" => format!("{:04}", c.y),
            "YY" | "yy" => format!("{:02}", c.y.rem_euclid(100)),
            "MMMM" => month_name(o.lang, c.mo, true).into(),
            "MMM" => month_name(o.lang, c.mo, false).into(),
            "MM" => format!("{:02}", c.mo),
            "M" => c.mo.to_string(),
            "DD" | "dd" => format!("{:02}", c.d),
            "D" | "d" => c.d.to_string(),
            "dddd" => day_name(o.lang, c.wd, true).into(),
            "ddd" => day_name(o.lang, c.wd, false).into(),
            "HH" if first && o.trim => hh.to_string(),
            "HH" => format!("{hh:02}"),
            "H" => hh.to_string(),
            "hh" => format!("{h12:02}"),
            "h" => h12.to_string(),
            "mm" => format!("{:02}", c.mi),
            "m" => c.mi.to_string(),
            "ss" => format!("{:02}", c.s),
            "s" => c.s.to_string(),
            "S" | "f" => (c.ms / 100).to_string(),
            "SS" => format!("{:02}", c.ms / 10),
            "FF" => format!("{:02}", ((c.ms as f64 + 0.01) / frame).floor() as u32),
            "A" => (if c.h < 12 { o.am } else { o.pm }).into(),
            _ => String::new(),
        };
        first = false;
        let k = if matches!(x, "MMMM" | "MMM" | "dddd" | "ddd" | "A") {
            SegKind::Name
        } else {
            SegKind::Num
        };
        segs.push(Seg { k, t });
    }
    segs
}

// ------------------------------------------------------------- parsing --

fn digits(s: &str, min: usize, max: usize) -> Option<u32> {
    if s.len() < min || s.len() > max || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// "HH:mm" or "HH:mm:ss" (a time of day).
pub fn parse_hms(s: &str) -> Option<(u32, u32, u32)> {
    let p: Vec<&str> = s.trim().split(':').collect();
    if p.len() < 2 || p.len() > 3 {
        return None;
    }
    let h = digits(p[0], 1, 2)?;
    let m = digits(p[1], 2, 2)?;
    let sec = if p.len() == 3 { digits(p[2], 2, 2)? } else { 0 };
    (h <= 23 && m <= 59 && sec <= 59).then_some((h, m, sec))
}

/// A duration typed by the operator: `ss`, `mm:ss`, `h:mm:ss`, optional `.mmm`.
pub fn parse_dur(s: &str) -> Result<i64, String> {
    let x = s.trim();
    if x.is_empty() {
        return Err("Empty value".into());
    }
    if x.starts_with('-') {
        return Err("Negative values are refused".into());
    }
    let bad = || format!("“{x}” is not a time: use ss, mm:ss or h:mm:ss");
    let (main, frac) = match x.find(['.', ',']) {
        Some(i) => (&x[..i], Some(&x[i + 1..])),
        None => (x, None),
    };
    let parts: Vec<&str> = main.split(':').collect();
    if parts.len() > 3 {
        return Err(bad());
    }
    let mut total: i64 = 0;
    for (i, p) in parts.iter().enumerate() {
        let n = if i == 0 {
            digits(p, 1, 9)
        } else {
            digits(p, 1, 2)
        }
        .ok_or_else(bad)?;
        if i > 0 && n > 59 {
            return Err(format!("“{x}”: minutes and seconds go up to 59"));
        }
        total = total * 60 + n as i64;
    }
    let ms = match frac {
        Some(f) => {
            let n = digits(f, 1, 3).ok_or_else(bad)?;
            (n as f64 * 10f64.powi(3 - f.len() as i32)).round() as i64
        }
        None => 0,
    };
    Ok(total * 1000 + ms)
}

/// A target: "20:00", "20:00:00", "2026-10-18 20:00" or "2026-10-18T20:00:00".
pub type Target = (Option<(i32, u32, u32)>, (u32, u32, u32));

pub fn parse_target(s: &str) -> Result<Target, String> {
    let x = s.trim();
    if x.is_empty() {
        return Err("The target time is empty".into());
    }
    let bad = || format!("“{x}” is not a target: use 20:00, 20:00:00 or 2026-10-18 20:00");
    let (date, time) = match x.find([' ', 'T']) {
        Some(i) => (Some(&x[..i]), x[i + 1..].trim()),
        None => (None, x),
    };
    let t = parse_hms(time).ok_or_else(bad)?;
    let d = match date {
        Some(d) => {
            let p: Vec<&str> = d.split('-').collect();
            if p.len() != 3 {
                return Err(bad());
            }
            let (y, m, dd) = (
                digits(p[0], 4, 4).ok_or_else(bad)?,
                digits(p[1], 1, 2).ok_or_else(bad)?,
                digits(p[2], 1, 2).ok_or_else(bad)?,
            );
            if jiff::civil::Date::new(y as i16, m as i8, dd as i8).is_err() {
                return Err(format!("“{d}” is not a date"));
            }
            Some((y as i32, m, dd))
        }
        None => None,
    };
    Ok((d, t))
}

/// The instant of a target in a zone (no date = today in that zone).
pub fn target_ms(t: &Target, tz: &TimeZone, now: i64) -> Option<i64> {
    let (y, mo, d) = match t.0 {
        Some(d) => d,
        None => {
            let c = comp_at(now, tz);
            (c.y, c.mo, c.d)
        }
    };
    zoned_ms(y, mo, d, t.1 .0, t.1 .1, t.1 .2, tz)
}

/// "1:05:00", "05:00", "05:00.5" (for messages).
pub fn plain_dur(ms: i64) -> String {
    let ms = ms.max(0);
    let sec = ms / 1000;
    let f = ms % 1000;
    let h = sec / 3600;
    let t = if h > 0 {
        format!("{h}:{:02}:{:02}", (sec / 60) % 60, sec % 60)
    } else {
        format!("{:02}:{:02}", sec / 60, sec % 60)
    };
    if f > 0 {
        format!("{t}.{}", format!("{f:03}").trim_end_matches('0'))
    } else {
        t
    }
}
