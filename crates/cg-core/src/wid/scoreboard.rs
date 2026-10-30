//! Scoreboard: sport presets with their counting rules, 2+ teams, a
//! timestamp-based match clock, score animations and event banners, manual
//! scores or a data feed with a priority rule, anti-double press and Undo.
//! Drawn by `cg-render/src/wid/scoreboard.rs` and `web/src/widgets/scoreboard.tsx` (keep in sync).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

// -------------------------------------------------------------- presets --

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Sport {
    #[default]
    Football,
    Rugby,
    Basketball,
    Handball,
    Volleyball,
    Tennis,
    Generic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClockDir {
    #[default]
    Up,
    Down,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Points add up (football, rugby, basketball…).
    Points,
    /// Rally point sets (volleyball).
    Sets,
    /// 15 / 30 / 40, games, sets, tie-break.
    Tennis,
}

/// A scoring button: label, points, event banner.
pub struct Pts {
    pub label: &'static str,
    pub v: i64,
    pub ev: &'static str,
}

/// The rules of a sport.
pub struct Preset {
    pub name: &'static str,
    pub periods: &'static [&'static str],
    pub len: f64,
    pub extra: f64,
    pub dir: ClockDir,
    /// Halves continue the time (2nd half starts at 45:00).
    pub cumulative: bool,
    pub kind: Kind,
    pub pts: &'static [Pts],
    pub fields: Fields,
    pub digits: u8,
    pub cards: &'static [&'static str],
    pub timeouts: u32,
    /// Team fouls giving the bonus (basketball).
    pub bonus: u32,
    /// "Possession" or "Serve".
    pub poss: &'static str,
    /// More than 2 teams allowed.
    pub multi: bool,
}

const fn f(
    period: bool,
    clock: bool,
    fouls: bool,
    timeouts: bool,
    cards: bool,
    poss: bool,
    sets: bool,
) -> Fields {
    Fields {
        period,
        clock,
        fouls,
        timeouts,
        cards,
        poss,
        sets,
        shot: false,
        shootout: false,
        susp: false,
    }
}

pub fn preset(s: Sport) -> &'static Preset {
    static FOOTBALL: Preset = Preset {
        name: "Football",
        periods: &[
            "1st half",
            "2nd half",
            "Extra time 1",
            "Extra time 2",
            "Penalties",
        ],
        len: 45.0,
        extra: 15.0,
        dir: ClockDir::Up,
        cumulative: true,
        kind: Kind::Points,
        pts: &[Pts {
            label: "GOAL",
            v: 1,
            ev: "GOAL!",
        }],
        fields: Fields {
            shootout: true,
            ..f(true, true, false, false, true, false, false)
        },
        digits: 2,
        cards: &["yellow", "red"],
        timeouts: 0,
        bonus: 0,
        poss: "Possession",
        multi: false,
    };
    static RUGBY: Preset = Preset {
        name: "Rugby",
        periods: &["1st half", "2nd half", "Extra time"],
        len: 40.0,
        extra: 10.0,
        dir: ClockDir::Up,
        cumulative: true,
        kind: Kind::Points,
        pts: &[
            Pts {
                label: "TRY +5",
                v: 5,
                ev: "TRY!",
            },
            Pts {
                label: "CONV +2",
                v: 2,
                ev: "CONVERSION",
            },
            Pts {
                label: "PEN +3",
                v: 3,
                ev: "PENALTY GOAL",
            },
            Pts {
                label: "DROP +3",
                v: 3,
                ev: "DROP GOAL",
            },
        ],
        fields: f(true, true, false, false, true, false, false),
        digits: 2,
        cards: &["yellow", "red"],
        timeouts: 0,
        bonus: 0,
        poss: "Possession",
        multi: false,
    };
    static BASKETBALL: Preset = Preset {
        name: "Basketball",
        periods: &["Q1", "Q2", "Q3", "Q4", "OT"],
        len: 10.0,
        extra: 5.0,
        dir: ClockDir::Down,
        cumulative: false,
        kind: Kind::Points,
        pts: &[
            Pts {
                label: "+1",
                v: 1,
                ev: "",
            },
            Pts {
                label: "+2",
                v: 2,
                ev: "",
            },
            Pts {
                label: "+3",
                v: 3,
                ev: "3 POINTS!",
            },
        ],
        fields: Fields {
            shot: true,
            ..f(true, true, true, true, false, true, false)
        },
        digits: 3,
        cards: &[],
        timeouts: 5,
        bonus: 5,
        poss: "Possession",
        multi: false,
    };
    static HANDBALL: Preset = Preset {
        name: "Handball",
        periods: &["1st half", "2nd half", "Extra 1", "Extra 2"],
        len: 30.0,
        extra: 5.0,
        dir: ClockDir::Up,
        cumulative: true,
        kind: Kind::Points,
        pts: &[Pts {
            label: "GOAL",
            v: 1,
            ev: "GOAL!",
        }],
        fields: Fields {
            susp: true,
            ..f(true, true, false, true, true, false, false)
        },
        digits: 2,
        cards: &["yellow", "2 min", "red"],
        timeouts: 3,
        bonus: 0,
        poss: "Possession",
        multi: false,
    };
    static VOLLEYBALL: Preset = Preset {
        name: "Volleyball",
        periods: &["Set 1", "Set 2", "Set 3", "Set 4", "Set 5"],
        len: 0.0,
        extra: 0.0,
        dir: ClockDir::None,
        cumulative: false,
        kind: Kind::Sets,
        pts: &[Pts {
            label: "POINT",
            v: 1,
            ev: "",
        }],
        fields: f(true, false, false, true, false, true, true),
        digits: 2,
        cards: &[],
        timeouts: 2,
        bonus: 0,
        poss: "Serve",
        multi: false,
    };
    static TENNIS: Preset = Preset {
        name: "Tennis",
        periods: &["Set 1", "Set 2", "Set 3", "Set 4", "Set 5"],
        len: 0.0,
        extra: 0.0,
        dir: ClockDir::None,
        cumulative: false,
        kind: Kind::Tennis,
        pts: &[Pts {
            label: "POINT",
            v: 1,
            ev: "",
        }],
        fields: f(true, false, false, false, false, true, true),
        digits: 2,
        cards: &[],
        timeouts: 0,
        bonus: 0,
        poss: "Serve",
        multi: false,
    };
    static GENERIC: Preset = Preset {
        name: "Generic",
        periods: &["P1", "P2", "P3", "P4"],
        len: 20.0,
        extra: 5.0,
        dir: ClockDir::Up,
        cumulative: false,
        kind: Kind::Points,
        pts: &[Pts {
            label: "+1",
            v: 1,
            ev: "SCORE!",
        }],
        fields: f(true, true, false, false, false, false, false),
        digits: 3,
        cards: &[],
        timeouts: 0,
        bonus: 0,
        poss: "Possession",
        multi: true,
    };
    match s {
        Sport::Football => &FOOTBALL,
        Sport::Rugby => &RUGBY,
        Sport::Basketball => &BASKETBALL,
        Sport::Handball => &HANDBALL,
        Sport::Volleyball => &VOLLEYBALL,
        Sport::Tennis => &TENNIS,
        Sport::Generic => &GENERIC,
    }
}

/// Colour of a card.
pub fn card_color(c: &str) -> &'static str {
    match c {
        "yellow" => "#facc15",
        "red" => "#ef4444",
        _ => "#38bdf8",
    }
}

// ------------------------------------------------------------- settings --

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Crest {
    #[default]
    Shield,
    Round,
    Diamond,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Team {
    pub name: String,
    /// 3 letters (empty = made from the name).
    pub short: String,
    /// Generated badge shape (when no picture).
    pub crest: Crest,
    /// Picture of the crest (asset path; empty = generated badge).
    pub logo: String,
    pub c1: String,
    pub c2: String,
    /// Score from data (`{{key}}`); empty = manual only.
    pub score: String,
}

impl Default for Team {
    fn default() -> Self {
        Self {
            name: "Team".into(),
            short: String::new(),
            crest: Crest::Shield,
            logo: String::new(),
            c1: "#334155".into(),
            c2: "#ffffff".into(),
            score: String::new(),
        }
    }
}

/// Sample teams (also those added in the inspector).
pub fn sample_team(i: usize) -> Team {
    let t = |name: &str, short: &str, c1: &str, c2: &str, crest| Team {
        name: name.into(),
        short: short.into(),
        crest,
        logo: String::new(),
        c1: c1.into(),
        c2: c2.into(),
        score: String::new(),
    };
    match i % 4 {
        0 => t(
            "Paris Saint-Germain",
            "PSG",
            "#1e3a8a",
            "#e11d48",
            Crest::Shield,
        ),
        1 => t(
            "Olympique de Marseille",
            "OM",
            "#0ea5e9",
            "#ffffff",
            Crest::Round,
        ),
        2 => t("Lyon Dragons", "LYO", "#b91c1c", "#facc15", Crest::Diamond),
        _ => t("Nantes Canaris", "NAN", "#eab308", "#15803d", Crest::Shield),
    }
}

const STOP: [&str; 11] = [
    "de", "du", "des", "la", "le", "les", "of", "the", "fc", "and", "et",
];

/// 3-letter short name from a long name: initials of the main words, or the first letters.
pub fn auto_short(name: &str) -> String {
    let clean = name.replace(['’', '\''], " ");
    let words: Vec<&str> = clean
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| {
            !w.is_empty() && !STOP.contains(&w.to_lowercase().as_str()) && w.to_lowercase() != "d"
        })
        .collect();
    let up = |s: String| s.to_uppercase();
    let first = |w: &str, n: usize| w.chars().take(n).collect::<String>();
    match words.len() {
        0 => "???".into(),
        1 => up(first(words[0], 3)),
        2 => up(format!("{}{}", first(words[0], 1), first(words[1], 2))),
        _ => up(words.iter().take(3).map(|w| first(w, 1)).collect()),
    }
}

/// The short name shown (3 letters at most).
pub fn short_of(t: &Team) -> String {
    let s = t.short.trim();
    if s.is_empty() {
        auto_short(&t.name)
    } else {
        s.chars().take(3).collect::<String>().to_uppercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Fields {
    pub period: bool,
    pub clock: bool,
    pub fouls: bool,
    pub timeouts: bool,
    pub cards: bool,
    /// Possession or serve indicator.
    pub poss: bool,
    /// Sets won (volleyball).
    pub sets: bool,
    /// Shot clock (basketball: 24 s, reset to 24 or 14).
    pub shot: bool,
    /// Penalty shoot-out: each kick scored / missed (the « Penalties » period).
    pub shootout: bool,
    /// Timed suspensions counting down (handball 2 min, rugby yellow card 10 min).
    pub susp: bool,
}

impl Default for Fields {
    fn default() -> Self {
        preset(Sport::Football).fields
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClockCfg {
    pub dir: ClockDir,
    /// Minutes of a period / of an extra period.
    pub len: f64,
    pub extra: f64,
    /// The countdown shows tenths under 1 minute.
    pub tenths: bool,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self {
            dir: ClockDir::Up,
            len: 45.0,
            extra: 15.0,
            tenths: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    #[default]
    Horizontal,
    Stacked,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Order {
    #[default]
    ClockFirst,
    ClockMid,
    ClockLast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Version {
    Compact,
    #[default]
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScoreAnim {
    #[default]
    Flip,
    Highlight,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Anim {
    pub score: ScoreAnim,
    /// Event banners (GOAL!, TRY!…).
    pub banner: bool,
    /// Seconds a banner stays.
    pub ev_dur: f64,
}

impl Default for Anim {
    fn default() -> Self {
        Self {
            score: ScoreAnim::Flip,
            banner: true,
            ev_dur: 3.2,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Rule {
    /// A manual change wins for `overS` seconds, then the feed again.
    #[default]
    Override,
    /// After a manual change, manual stays (until "Back to feed").
    Manual,
    /// The feed always wins.
    Feed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceCfg {
    pub rule: Rule,
    pub over_s: f64,
}

impl Default for SourceCfg {
    fn default() -> Self {
        Self {
            rule: Rule::Override,
            over_s: 20.0,
        }
    }
}

/// The shot clock: its length and the shorter reset, seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShotCfg {
    pub len: f64,
    pub reset: f64,
}

impl Default for ShotCfg {
    fn default() -> Self {
        Self {
            len: 24.0,
            reset: 14.0,
        }
    }
}

/// Settings of the widget, flattened into the layer's JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub preset: Sport,
    pub teams: Vec<Team>,
    pub fields: Fields,
    /// Reserved digits of the score cell (2 or 3).
    pub digits: u8,
    pub clock: ClockCfg,
    pub layout: Layout,
    pub order: Order,
    pub version: Version,
    /// Name space of the full version, × the row height.
    pub name_w: f64,
    pub radius: f64,
    pub show_crest: bool,
    pub anim: Anim,
    /// Anti-double press, ms.
    pub debounce: f64,
    pub source: SourceCfg,
    /// Tennis: best of 3 or 5 sets.
    pub best_of: u32,
    pub shot: ShotCfg,
    /// Minutes of a suspension (0 = the sport's: handball 2, rugby 10).
    pub susp_len: f64,
    /// Background of the board.
    pub bg: String,
    /// Font family of the texts.
    pub font: String,
    /// The texts the board writes (periods, banners…), by English wording;
    /// a missing or empty one is written in the project's language.
    pub texts: BTreeMap<String, String>,
    /// The rows of the control centre the Operator gets (when the Designer
    /// exposes them): the clock field, the fouls, timeouts, cards /
    /// suspensions, shot clock and shoot-out pads, the swap of the teams.
    pub op_clock: bool,
    pub op_fouls: bool,
    pub op_timeouts: bool,
    pub op_susp: bool,
    pub op_shot: bool,
    pub op_shootout: bool,
    pub op_swap: bool,
    /// The score typed by hand, the added time.
    pub op_score: bool,
    pub op_added: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: Sport::Football,
            teams: vec![sample_team(0), sample_team(1)],
            fields: Fields::default(),
            digits: 2,
            clock: ClockCfg::default(),
            layout: Layout::Horizontal,
            order: Order::ClockFirst,
            version: Version::Full,
            name_w: 5.4,
            radius: 10.0,
            show_crest: true,
            anim: Anim::default(),
            debounce: 400.0,
            source: SourceCfg::default(),
            best_of: 3,
            shot: ShotCfg::default(),
            susp_len: 0.0,
            bg: "#05060c".into(),
            font: "Inter".into(),
            texts: BTreeMap::new(),
            op_clock: true,
            op_fouls: true,
            op_timeouts: true,
            op_susp: true,
            op_shot: true,
            op_shootout: false,
            op_swap: false,
            op_score: true,
            op_added: true,
        }
    }
}

/// French of the texts the board writes.
fn fr(en: &str) -> Option<&'static str> {
    Some(match en {
        "1st half" => "1re mi-temps",
        "2nd half" => "2e mi-temps",
        "Extra time 1" => "Prolongation 1",
        "Extra time 2" => "Prolongation 2",
        "Penalties" => "Tirs au but",
        "Extra time" => "Prolongation",
        "Extra 1" => "Prolongation 1",
        "Extra 2" => "Prolongation 2",
        "OT" => "Prol.",
        "FINAL" => "TERMINÉ",
        "MANUAL ENTRY" => "SAISIE MANUELLE",
        "DEUCE" => "ÉGALITÉ",
        "ADVANTAGE" => "AVANTAGE",
        "FOULS" => "FAUTES",
        "TO" => "TM",
        "GOAL!" => "BUT\u{a0}!",
        "TRY!" => "ESSAI\u{a0}!",
        "CONVERSION" => "TRANSFORMATION",
        "PENALTY GOAL" => "PÉNALITÉ",
        "DROP GOAL" => "DROP",
        "3 POINTS!" => "3 POINTS\u{a0}!",
        "SCORE!" => "POINT\u{a0}!",
        "YELLOW CARD" => "CARTON JAUNE",
        "RED CARD" => "CARTON ROUGE",
        "2-MIN SUSPENSION" => "EXCLUSION 2 MIN",
        "SUSPENSION" => "EXCLUSION",
        "TIMEOUT" => "TEMPS MORT",
        "{team} · {n} left" => "{team} · reste {n}",
        "GAME, SET & MATCH" => "JEU, SET ET MATCH",
        "MATCH!" => "MATCH\u{a0}!",
        "{team} wins {a}–{b}" => "{team} gagne {a}–{b}",
        _ => return None,
    })
}

/// A text the board writes: the Designer's own, else in the project's
/// language (French when `lang` starts with "fr"), else the English.
pub fn tr(c: &Config, lang: &str, en: &str) -> String {
    if let Some(own) = c.texts.get(en).filter(|s| !s.trim().is_empty()) {
        return own.clone();
    }
    if lang.starts_with("fr") {
        if let Some(f) = fr(en) {
            return f.to_string();
        }
    }
    en.to_string()
}

/// `{name}` replaced by its value.
pub fn fill(s: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// The texts of a sport the Designer may rename (in order).
pub fn texts_of(c: &Config) -> Vec<&'static str> {
    let p = c.p();
    let mut out: Vec<&'static str> = Vec::new();
    let mut add = |s: &'static str| {
        if !s.is_empty() && !out.contains(&s) {
            out.push(s);
        }
    };
    for s in p.periods {
        add(s);
    }
    for pt in p.pts {
        add(pt.ev);
    }
    match p.kind {
        Kind::Tennis => {
            for s in [
                "FINAL",
                "MANUAL ENTRY",
                "TIE-BREAK",
                "DEUCE",
                "ADVANTAGE",
                "GAME, SET & MATCH",
            ] {
                add(s);
            }
        }
        Kind::Sets => {
            add("MATCH!");
            add("{team} wins {a}–{b}");
        }
        Kind::Points => {}
    }
    if p.bonus > 0 {
        add("FOULS");
        add("BONUS");
    }
    if p.timeouts > 0 {
        add("TO");
        add("TIMEOUT");
        add("{team} · {n} left");
    }
    for card in p.cards {
        add(match *card {
            "yellow" => "YELLOW CARD",
            "red" => "RED CARD",
            _ => "2-MIN SUSPENSION",
        });
    }
    if c.fields.susp {
        add("SUSPENSION");
    }
    out
}

impl Config {
    pub fn p(&self) -> &'static Preset {
        preset(self.preset)
    }
    /// Teams shown (2, or more with the Generic preset; at least 2).
    pub fn team_count(&self) -> usize {
        let n = self.teams.len().max(2);
        if self.p().multi {
            n
        } else {
            2
        }
    }
    pub fn team(&self, i: usize) -> Team {
        self.teams.get(i).cloned().unwrap_or_else(|| sample_team(i))
    }
}

// ---------------------------------------------------------------- state --

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TeamState {
    pub score: i64,
    pub fouls: u32,
    /// Timeouts taken.
    pub to_used: u32,
    pub yellow: u32,
    pub red: u32,
    /// 2-minute suspensions.
    pub susp: u32,
    /// Points added by the operator, in order (− cancels the last one).
    pub added: Vec<i64>,
    /// Penalty shoot-out: each kick, scored or missed.
    pub pens: Vec<bool>,
    /// Timed suspensions: [match time it started, length] (ms).
    pub susps: Vec<[i64; 2]>,
}

impl TeamState {
    pub fn cards(&self, c: &str) -> u32 {
        match c {
            "yellow" => self.yellow,
            "red" => self.red,
            _ => self.susp,
        }
    }
}

/// Timestamp-based clock: `base` ms elapsed before `startAt` (Unix ms).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Clock {
    pub running: bool,
    pub start_at: i64,
    pub base: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tennis {
    pub pts: [u32; 2],
    pub games: [u32; 2],
    pub sets: Vec<[u32; 2]>,
    pub tb: bool,
    pub done: bool,
    /// Free entry: the typed points show as they are.
    pub free: bool,
    pub free_pts: [String; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Volley {
    pub sets: Vec<[i64; 2]>,
    pub won: [u32; 2],
    pub done: bool,
}

/// Score animation of a team: since `at`, from the score `from`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScoreFx {
    pub at: i64,
    pub from: String,
}

/// Event banner since `at`. `text` and `sub` are written in the project's
/// language when drawn ([`tr`]), their `{name}` filled from `vars`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Banner {
    pub text: String,
    pub sub: String,
    pub color: String,
    pub at: i64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Fx {
    pub sc: BTreeMap<usize, ScoreFx>,
    pub ev: Option<Banner>,
    pub undo_at: i64,
    /// Last double press ignored.
    pub ign_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Last {
    pub key: String,
    pub at: i64,
}

/// One Undo step: the state before an action.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hist {
    pub label: String,
    pub d: serde_json::Value,
}

/// Manual changes vs the data feed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Src {
    /// Last manual score change (Unix ms).
    pub manual_at: i64,
    /// A manual change since the feed was last in charge.
    pub dirty: bool,
}

/// Kept steps of Undo.
pub const HISTORY: usize = 80;

/// What the operator changes on air.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub teams: Vec<TeamState>,
    pub period: usize,
    pub clock: Clock,
    /// Added time, minutes.
    pub added: u32,
    /// Team in possession / serving.
    pub poss: usize,
    pub swapped: bool,
    /// Compact / full chosen by the operator (None = the template's).
    pub version: Option<Version>,
    pub tennis: Tennis,
    pub volley: Volley,
    pub fx: Fx,
    pub src: Src,
    pub last: Last,
    pub ignored: u32,
    pub hist: Vec<Hist>,
    /// Shot clock: the period's elapsed ms when it was last reset, and its length (ms; 0 = the setting).
    pub shot_base: i64,
    pub shot_len: i64,
}

impl State {
    pub fn team(&self, i: usize) -> TeamState {
        self.teams.get(i).cloned().unwrap_or_default()
    }
    fn ensure(&mut self, n: usize) {
        if self.teams.len() < n {
            self.teams.resize(n, TeamState::default());
        }
    }
    pub fn version(&self, c: &Config) -> Version {
        self.version.unwrap_or(c.version)
    }
}

// ---------------------------------------------------------------- clock --

/// Length of period `i`, ms.
pub fn period_len(c: &Config, i: usize) -> i64 {
    let name = c.p().periods.get(i).copied().unwrap_or("").to_lowercase();
    let min = if name.contains("extra") || name == "ot" {
        c.clock.extra
    } else {
        c.clock.len
    };
    (min.max(0.0) * 60000.0) as i64
}

/// Elapsed ms of the period.
pub fn elapsed(st: &State, now: i64) -> i64 {
    let c = &st.clock;
    (c.base + if c.running { now - c.start_at } else { 0 }).max(0)
}

/// The clock shows (sport has one, field on, not penalties).
pub fn clock_on(c: &Config, st: &State) -> bool {
    c.clock.dir != ClockDir::None
        && c.fields.clock
        && c.p().kind == Kind::Points
        && c.p().periods.get(st.period) != Some(&"Penalties")
}

fn offset_ms(c: &Config, st: &State) -> i64 {
    if !c.p().cumulative || c.clock.dir != ClockDir::Up {
        return 0;
    }
    (0..st.period).map(|i| period_len(c, i)).sum()
}

fn fmt_ms(ms: i64, tenths: bool) -> String {
    let ms = ms.max(0);
    if tenths && ms < 60000 {
        return format!("{}.{}", ms / 1000, (ms % 1000) / 100);
    }
    let t = ms / 1000;
    format!("{:02}:{:02}", t / 60, t % 60)
}

/// Clock text, over the period length (stopwatch), ended (countdown at 0).
pub fn clock_text(c: &Config, st: &State, now: i64) -> (String, bool, bool) {
    let e = elapsed(st, now);
    let len = period_len(c, st.period);
    if c.clock.dir == ClockDir::Down {
        return (fmt_ms(len - e, c.clock.tenths), false, e >= len);
    }
    (
        fmt_ms(offset_ms(c, st) + e, false),
        len > 0 && e > len,
        false,
    )
}

/// The shot clock shows (field on, the match clock in this period).
pub fn shot_on(c: &Config, st: &State) -> bool {
    c.fields.shot && clock_on(c, st)
}

/// Shot clock: whole seconds left (up), ended (at 0). It runs with the match clock.
pub fn shot_text(c: &Config, st: &State, now: i64) -> (String, bool) {
    let len = if st.shot_len > 0 {
        st.shot_len
    } else {
        (c.shot.len.max(1.0) * 1000.0) as i64
    };
    let left = (len - (elapsed(st, now) - st.shot_base)).max(0);
    (((left + 999) / 1000).to_string(), left == 0)
}

/// The match time (ms): the periods before, then this one (cumulative sports).
pub fn match_ms(c: &Config, st: &State, now: i64) -> i64 {
    offset_ms(c, st) + elapsed(st, now)
}

/// Minutes of a suspension in this sport.
pub fn susp_min(c: &Config) -> f64 {
    if c.susp_len > 0.0 {
        c.susp_len
    } else if c.preset == Sport::Rugby {
        10.0
    } else {
        2.0
    }
}

/// Running suspensions of a team: the time left of each (ms), soonest first.
pub fn susps_left(c: &Config, st: &State, i: usize, now: i64) -> Vec<i64> {
    let m = match_ms(c, st, now);
    let mut v: Vec<i64> = st
        .team(i)
        .susps
        .iter()
        .map(|s| s[0] + s[1] - m)
        .filter(|l| *l > 0)
        .collect();
    v.sort();
    v
}

/// `m:ss` of a time left.
pub fn fmt_left(ms: i64) -> String {
    let t = (ms.max(0) + 999) / 1000;
    format!("{}:{:02}", t / 60, t % 60)
}

/// The penalty shoot-out shows (field on, the « Penalties » period).
pub fn shootout_on(c: &Config, st: &State) -> bool {
    c.fields.shootout && c.p().periods.get(st.period) == Some(&"Penalties")
}

/// `mm:ss[.t]` → ms.
pub fn parse_clock(s: &str) -> Option<i64> {
    let s = s.trim();
    let (main, tenth) = match s.split_once('.') {
        Some((a, b)) if b.len() == 1 => (a, b.parse::<i64>().ok()?),
        Some(_) => return None,
        None => (s, 0),
    };
    let (m, sec) = match main.split_once(':') {
        Some((m, s)) => (m.parse::<i64>().ok()?, s.parse::<i64>().ok()?),
        None => (main.parse::<i64>().ok()?, 0),
    };
    if m < 0 || !(0..60).contains(&sec) || m > 999 {
        return None;
    }
    Some((m * 60 + sec) * 1000 + tenth * 100)
}

// --------------------------------------------------------------- source --

#[derive(Debug, Clone, PartialEq)]
pub enum Active {
    /// No team score wired to data.
    Manual,
    /// Wired, but the data is not a number: manual (automatic fallback).
    FeedLost,
    /// Manual change has priority (seconds left of the override, if any).
    ManualPriority(Option<u64>),
    Feed,
}

impl Active {
    pub fn is_feed(&self) -> bool {
        *self == Active::Feed
    }
    /// What the Operator reads.
    pub fn text(&self, rule: Rule) -> String {
        match self {
            Active::Manual => "MANUAL".into(),
            Active::FeedLost => "MANUAL · feed lost".into(),
            Active::ManualPriority(Some(s)) => format!("MANUAL override · feed back in {s} s"),
            Active::ManualPriority(None) => "MANUAL (manual has priority)".into(),
            Active::Feed => match rule {
                Rule::Feed => "FEED (feed has priority)".into(),
                Rule::Manual => "FEED (until a manual change)".into(),
                Rule::Override => "FEED".into(),
            },
        }
    }
}

/// Feed values of the wired teams (None = not wired; Some(None) = not a number).
pub fn feed_values(c: &Config, data: &Data) -> Vec<Option<Option<i64>>> {
    (0..c.team_count())
        .map(|i| {
            let t = c.team(i);
            if !t.score.contains("{{") {
                return None;
            }
            Some(interpolate_text(&t.score, data).trim().parse::<i64>().ok())
        })
        .collect()
}

/// Which source is on air.
pub fn active_source(c: &Config, st: &State, data: &Data, now: i64) -> Active {
    let fv = feed_values(c, data);
    if c.p().kind != Kind::Points || fv.iter().all(Option::is_none) {
        return Active::Manual;
    }
    if fv.iter().any(|v| matches!(v, Some(None))) {
        return Active::FeedLost;
    }
    match c.source.rule {
        Rule::Feed => Active::Feed,
        Rule::Manual if st.src.dirty => Active::ManualPriority(None),
        Rule::Manual => Active::Feed,
        Rule::Override => {
            let left = (c.source.over_s * 1000.0) as i64 - (now - st.src.manual_at);
            if st.src.dirty && left > 0 {
                Active::ManualPriority(Some(((left + 999) / 1000) as u64))
            } else {
                Active::Feed
            }
        }
    }
}

/// The scores on air (feed or manual).
pub fn scores(c: &Config, st: &State, data: &Data, now: i64) -> Vec<i64> {
    let feed = active_source(c, st, data, now).is_feed();
    let fv = feed_values(c, data);
    (0..c.team_count())
        .map(|i| match (feed, fv.get(i)) {
            (true, Some(Some(Some(v)))) => *v,
            _ => st.team(i).score,
        })
        .collect()
}

// ---------------------------------------------------------------- tennis --

/// Points of a player as shown (0 15 30 40 AD, tie-break numbers, or typed).
pub fn tennis_pts(t: &Tennis, i: usize) -> String {
    let (p, o) = (t.pts[i], t.pts[1 - i]);
    if t.free {
        return t.free_pts[i].clone();
    }
    if t.tb {
        return p.to_string();
    }
    if p >= 3 && o >= 3 {
        return if p > o { "AD".into() } else { "40".into() };
    }
    ["0", "15", "30", "40"][p.min(3) as usize].into()
}

pub fn tennis_tag(t: &Tennis) -> &'static str {
    if t.done {
        "FINAL"
    } else if t.free {
        "MANUAL ENTRY"
    } else if t.tb {
        "TIE-BREAK"
    } else if t.pts[0] >= 3 && t.pts[0] == t.pts[1] {
        "DEUCE"
    } else if t.pts[0] >= 3 && t.pts[1] >= 3 {
        "ADVANTAGE"
    } else {
        ""
    }
}

/// The text of the period field.
pub fn period_text(c: &Config, st: &State) -> String {
    match c.p().kind {
        Kind::Tennis => format!(
            "SET {}",
            st.tennis.sets.len() + usize::from(!st.tennis.done)
        ),
        _ => c
            .p()
            .periods
            .get(st.period)
            .copied()
            .unwrap_or("")
            .to_string(),
    }
}

// -------------------------------------------------------------- actions --

fn idx(args: &serde_json::Value, key: &str) -> Option<usize> {
    let v = args.get(key)?;
    v.as_u64()
        .map(|n| n as usize)
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn s_arg(args: &serde_json::Value, key: &str) -> String {
    match args.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn banner(
    c: &Config,
    st: &mut State,
    team: usize,
    text: &str,
    sub: String,
    color: Option<&str>,
    now: i64,
) {
    banner_v(c, st, team, text, sub, color, now, BTreeMap::new());
}

/// A banner whose text / sub hold `{name}`s (filled when drawn, after the translation).
#[allow(clippy::too_many_arguments)]
fn banner_v(
    c: &Config,
    st: &mut State,
    team: usize,
    text: &str,
    sub: String,
    color: Option<&str>,
    now: i64,
    vars: BTreeMap<String, String>,
) {
    if !c.anim.banner || text.is_empty() {
        return;
    }
    st.fx.ev = Some(Banner {
        text: text.into(),
        sub,
        color: color.map(str::to_string).unwrap_or_else(|| c.team(team).c1),
        at: now,
        vars,
    });
}

fn vars(list: &[(&str, String)]) -> BTreeMap<String, String> {
    list.iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn score_line(st: &State, n: usize) -> String {
    (0..n)
        .map(|i| st.team(i).score.to_string())
        .collect::<Vec<_>>()
        .join("–")
}

fn tennis_point(c: &Config, st: &mut State, i: usize, now: i64) -> Result<(), String> {
    let o = 1 - i;
    let t = &mut st.tennis;
    if t.done {
        return Err("Match over — Undo or Reset".into());
    }
    if t.free {
        return Err("Free entry is on: type the points".into());
    }
    t.pts[i] += 1;
    let p = t.pts;
    let mut game = false;
    if t.tb {
        if p[i] >= 7 && p[i] >= p[o] + 2 {
            game = true;
        } else if (p[0] + p[1]) % 2 == 1 {
            st.poss = 1 - st.poss.min(1);
        }
    } else if p[i] >= 4 && p[i] >= p[o] + 2 {
        game = true;
    }
    if !game {
        return Ok(());
    }
    let t = &mut st.tennis;
    let was_tb = t.tb;
    t.pts = [0, 0];
    t.games[i] += 1;
    t.tb = false;
    st.poss = 1 - st.poss.min(1);
    let g = st.tennis.games;
    if was_tb || (g[i] >= 6 && g[i] >= g[o] + 2) {
        st.tennis.sets.push(g);
        st.tennis.games = [0, 0];
        let won = st.tennis.sets.iter().filter(|x| x[i] > x[o]).count() as u32;
        if won >= c.best_of.max(1).div_ceil(2) {
            st.tennis.done = true;
            banner(c, st, i, "GAME, SET & MATCH", c.team(i).name, None, now);
        } else {
            st.period = st.tennis.sets.len().min(4);
            let sets = st
                .tennis
                .sets
                .iter()
                .map(|x| format!("{}-{}", x[0], x[1]))
                .collect::<Vec<_>>()
                .join("  ");
            banner_v(
                c,
                st,
                i,
                "SET {team}",
                sets,
                None,
                now,
                vars(&[("team", short_of(&c.team(i)))]),
            );
        }
    } else if g[0] == 6 && g[1] == 6 {
        st.tennis.tb = true;
        banner(c, st, i, "TIE-BREAK", "6–6".into(), None, now);
    }
    Ok(())
}

fn volley_check(c: &Config, st: &mut State, i: usize, now: i64) {
    let o = 1 - i;
    st.poss = i; // rally point: the side that wins the point serves
    let target = if st.period >= 4 { 15 } else { 25 };
    let (a, b) = (st.team(i).score, st.team(o).score);
    if a >= target && a - b >= 2 {
        let s = [st.team(0).score, st.team(1).score];
        st.volley.sets.push(s);
        st.volley.won[i] += 1;
        let (wi, wo) = (st.volley.won[i], st.volley.won[o]);
        if wi >= 3 {
            st.volley.done = true;
            banner_v(
                c,
                st,
                i,
                "MATCH!",
                "{team} wins {a}–{b}".into(),
                None,
                now,
                vars(&[
                    ("team", short_of(&c.team(i))),
                    ("a", wi.to_string()),
                    ("b", wo.to_string()),
                ]),
            );
        } else {
            banner_v(
                c,
                st,
                i,
                "SET {team}",
                format!(
                    "{}–{} · sets {}–{}",
                    s[0], s[1], st.volley.won[0], st.volley.won[1]
                ),
                None,
                now,
                vars(&[("team", short_of(&c.team(i)))]),
            );
            st.period = (st.period + 1).min(4);
            for t in st.teams.iter_mut() {
                t.score = 0;
                t.to_used = 0;
            }
        }
    }
}

fn set_score(st: &mut State, i: usize, v: i64, now: i64) {
    let old = st.teams[i].score;
    if old != v {
        st.fx.sc.insert(
            i,
            ScoreFx {
                at: now,
                from: old.to_string(),
            },
        );
    }
    st.teams[i].score = v;
}

/// Before a manual score change: the scores on air become the manual ones.
fn take_manual(c: &Config, st: &mut State, data: &Data, now: i64) {
    let on_air = scores(c, st, data, now);
    for (i, v) in on_air.into_iter().enumerate() {
        st.teams[i].score = v;
    }
    st.src.dirty = true;
    st.src.manual_at = now;
}

fn team_arg(c: &Config, args: &serde_json::Value) -> Result<usize, String> {
    let i = idx(args, "team").unwrap_or(0);
    if i >= c.team_count() {
        return Err(format!("no team {i}"));
    }
    Ok(i)
}

/// Runs one action (no debounce or history here).
fn run(
    c: &Config,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let p = c.p();
    let now = ctx.now_ms;
    let n = c.team_count();
    match op {
        "add" | "goal" | "point" => {
            let i = team_arg(c, args)?;
            match p.kind {
                Kind::Tennis => tennis_point(c, st, i, now),
                Kind::Sets => {
                    if st.volley.done {
                        return Err("Match over — Undo or Reset".into());
                    }
                    let v = st.teams[i].score + 1;
                    set_score(st, i, v, now);
                    volley_check(c, st, i, now);
                    Ok(())
                }
                Kind::Points => {
                    let k = idx(args, "pts").unwrap_or(0).min(p.pts.len() - 1);
                    let pt = &p.pts[k];
                    take_manual(c, st, ctx.data, now);
                    let v = st.teams[i].score + pt.v;
                    set_score(st, i, v, now);
                    let added = &mut st.teams[i].added;
                    added.push(pt.v);
                    if added.len() > 100 {
                        added.remove(0);
                    }
                    let player = s_arg(args, "player");
                    let line = score_line(st, n);
                    let sub = if player.trim().is_empty() {
                        format!("{} · {line}", short_of(&c.team(i)))
                    } else {
                        format!("{} · {} · {line}", player.trim(), short_of(&c.team(i)))
                    };
                    let ev = if op == "goal" && pt.ev.is_empty() {
                        "SCORE!"
                    } else {
                        pt.ev
                    };
                    banner(c, st, i, ev, sub, None, now);
                    Ok(())
                }
            }
        }
        "sub" => {
            let i = team_arg(c, args)?;
            if p.kind == Kind::Tennis {
                if st.tennis.pts[i] == 0 {
                    return Err("Refused: no point to remove in this game (use Undo)".into());
                }
                st.tennis.pts[i] -= 1;
                return Ok(());
            }
            // The team's last point (any type), else the smallest one.
            let by = if p.kind == Kind::Points {
                take_manual(c, st, ctx.data, now);
                st.teams[i]
                    .added
                    .last()
                    .copied()
                    .unwrap_or_else(|| p.pts.iter().map(|x| x.v).min().unwrap_or(1))
            } else {
                p.pts[0].v
            };
            let v = st.teams[i].score - by;
            if v < 0 {
                return Err("Refused: a score cannot go below 0".into());
            }
            set_score(st, i, v, now);
            st.teams[i].added.pop();
            Ok(())
        }
        "set" => {
            let list = args
                .get("scores")
                .and_then(|v| v.as_array())
                .ok_or("scores: [..] needed")?;
            let mut v = Vec::new();
            for (i, x) in list.iter().enumerate().take(n) {
                let s = x
                    .as_i64()
                    .or_else(|| x.as_f64().map(|f| f.round() as i64))
                    .or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
                    .ok_or_else(|| format!("score {} is not a number", i + 1))?;
                if s < 0 {
                    return Err("Refused: a score cannot go below 0".into());
                }
                v.push(s);
            }
            if p.kind == Kind::Points {
                take_manual(c, st, ctx.data, now);
            }
            for (i, s) in v.into_iter().enumerate() {
                set_score(st, i, s, now);
                st.teams[i].added.clear();
            }
            Ok(())
        }
        "nextPeriod" | "prevPeriod" => {
            if op == "nextPeriod" {
                if st.period + 1 >= p.periods.len() {
                    return Err("Already the last period".into());
                }
                st.period += 1;
                if p.bonus > 0 {
                    st.teams.iter_mut().for_each(|t| t.fouls = 0);
                }
                // Timeouts come back each set (volleyball), each half and overtime (basketball).
                let half = p.bonus > 0 && (st.period == p.periods.len() / 2 || st.period >= 4);
                if p.kind == Kind::Sets || half {
                    st.teams.iter_mut().for_each(|t| t.to_used = 0);
                }
            } else {
                if st.period == 0 {
                    return Err("Already the first period".into());
                }
                st.period -= 1;
            }
            st.clock = Clock::default();
            st.added = 0;
            st.shot_base = 0;
            st.shot_len = 0;
            Ok(())
        }
        "clockStart" | "clockPause" | "clockToggle" => {
            if p.kind != Kind::Points || c.clock.dir == ClockDir::None {
                return Err("No clock in this sport".into());
            }
            let start = match op {
                "clockStart" => true,
                "clockPause" => false,
                _ => !st.clock.running,
            };
            if start == st.clock.running {
                return Err(if start {
                    "The clock is running"
                } else {
                    "The clock is stopped"
                }
                .into());
            }
            if start {
                // A countdown at 0 does not start again.
                if c.clock.dir == ClockDir::Down && elapsed(st, now) >= period_len(c, st.period) {
                    return Err("The period is over: go to the next one".into());
                }
                st.clock.running = true;
                st.clock.start_at = now;
            } else {
                st.clock.base = elapsed(st, now);
                st.clock.running = false;
            }
            Ok(())
        }
        "clockReset" => {
            st.clock = Clock::default();
            st.shot_base = 0;
            Ok(())
        }
        "clockSet" => {
            let v = parse_clock(&s_arg(args, "time")).ok_or("Clock format: mm:ss")?;
            let len = period_len(c, st.period);
            let e = if c.clock.dir == ClockDir::Down {
                len - v
            } else {
                let off = offset_ms(c, st);
                if v >= off {
                    v - off
                } else {
                    v
                }
            };
            // The shot clock keeps its time.
            st.shot_base += e.max(0) - elapsed(st, now);
            st.clock.base = e.max(0);
            st.clock.start_at = now;
            Ok(())
        }
        "clockAdjust" => {
            let d = args
                .get("secs")
                .and_then(|v| v.as_f64())
                .ok_or("secs needed")?;
            let d = if c.clock.dir == ClockDir::Down { -d } else { d };
            let e = (elapsed(st, now) + (d * 1000.0) as i64).max(0);
            st.shot_base += e - elapsed(st, now);
            st.clock.base = e;
            st.clock.start_at = now;
            Ok(())
        }
        "shot" => {
            if !c.fields.shot {
                return Err("No shot clock on this board".into());
            }
            let secs = args.get("s").and_then(|v| v.as_f64()).unwrap_or(c.shot.len);
            st.shot_len = (secs.max(1.0) * 1000.0) as i64;
            st.shot_base = elapsed(st, now);
            Ok(())
        }
        "penalty" => {
            let i = team_arg(c, args)?;
            let scored = args.get("scored").and_then(|v| v.as_bool()).unwrap_or(true);
            st.teams[i].pens.push(scored);
            Ok(())
        }
        "suspension" => {
            let i = team_arg(c, args)?;
            let min = args
                .get("min")
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| susp_min(c));
            let m = match_ms(c, st, now);
            st.teams[i].susps.push([m, (min.max(0.1) * 60000.0) as i64]);
            let mins = format!("{}", (min * 10.0).round() / 10.0);
            banner_v(
                c,
                st,
                i,
                "SUSPENSION",
                "{team} · {min} min".into(),
                Some(card_color("2 min")),
                now,
                vars(&[("team", short_of(&c.team(i))), ("min", mins)]),
            );
            Ok(())
        }
        "added" => {
            st.added = args
                .get("min")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(30) as u32;
            Ok(())
        }
        "card" => {
            let i = team_arg(c, args)?;
            let color = s_arg(args, "color").to_lowercase().replace("2min", "2 min");
            let color = if color.is_empty() {
                "yellow".to_string()
            } else {
                color
            };
            if !p.cards.contains(&color.as_str()) {
                return Err(format!("No {color} card in {}", p.name));
            }
            let timed = c.fields.susp
                && (color == "2 min" || (color == "yellow" && c.preset == Sport::Rugby));
            let m = match_ms(c, st, now);
            let t = &mut st.teams[i];
            match color.as_str() {
                "yellow" => t.yellow += 1,
                "red" => t.red += 1,
                _ => t.susp += 1,
            }
            if timed {
                t.susps.push([m, (susp_min(c) * 60000.0) as i64]);
            }
            let text = if color == "2 min" {
                "2-MIN SUSPENSION".to_string()
            } else {
                format!("{} CARD", color.to_uppercase())
            };
            let player = s_arg(args, "player");
            let sub = if player.trim().is_empty() {
                short_of(&c.team(i))
            } else {
                format!("{} · {}", player.trim(), short_of(&c.team(i)))
            };
            banner(c, st, i, &text, sub, Some(card_color(&color)), now);
            Ok(())
        }
        "timeout" => {
            let i = team_arg(c, args)?;
            if p.timeouts == 0 {
                return Err("No timeouts in this sport".into());
            }
            let t = &mut st.teams[i];
            if t.to_used >= p.timeouts {
                return Err(format!(
                    "Refused: {} has no timeout left",
                    short_of(&c.team(i))
                ));
            }
            t.to_used += 1;
            let left = p.timeouts - t.to_used;
            banner_v(
                c,
                st,
                i,
                "TIMEOUT",
                "{team} · {n} left".into(),
                None,
                now,
                vars(&[("team", short_of(&c.team(i))), ("n", left.to_string())]),
            );
            Ok(())
        }
        "foul" => {
            let i = team_arg(c, args)?;
            st.teams[i].fouls += 1;
            Ok(())
        }
        "possession" | "serve" => {
            st.poss = match idx(args, "team") {
                Some(i) if i < n => i,
                Some(i) => return Err(format!("no team {i}")),
                None => (st.poss + 1) % n,
            };
            Ok(())
        }
        "swap" => {
            st.swapped = !st.swapped;
            Ok(())
        }
        "version" => {
            let v = match s_arg(args, "v").as_str() {
                "compact" => Version::Compact,
                "full" => Version::Full,
                _ => match st.version(c) {
                    Version::Full => Version::Compact,
                    Version::Compact => Version::Full,
                },
            };
            st.version = Some(v);
            Ok(())
        }
        "tennisFree" => {
            if let Some(on) = args.get("on").and_then(|v| v.as_bool()) {
                st.tennis.free = on;
            }
            if let Some(a) = args.get("pts").and_then(|v| v.as_array()) {
                for (k, v) in a.iter().take(2).enumerate() {
                    st.tennis.free_pts[k] = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                }
            }
            if let Some(a) = args.get("games").and_then(|v| v.as_array()) {
                for (k, v) in a.iter().take(2).enumerate() {
                    st.tennis.games[k] = v.as_u64().unwrap_or(0).min(7) as u32;
                }
            }
            Ok(())
        }
        "reset" => {
            let hist = std::mem::take(&mut st.hist);
            *st = State {
                hist,
                ..Default::default()
            };
            st.ensure(n);
            Ok(())
        }
        _ => Err(format!("unknown action `{op}`")),
    }
}

/// Applies an operator action. Ops (args):
/// `add {team, pts?}` (preset scoring button `pts`; tennis / volleyball: a point),
/// `goal {team, player?}`, `sub {team}`, `set {scores: [..]}`, `nextPeriod`, `prevPeriod`,
/// `clockStart`, `clockPause`, `clockToggle`, `clockReset`, `clockSet {time: "mm:ss"}`,
/// `clockAdjust {secs}`, `added {min}`, `card {team, player?, color}`, `timeout {team}`,
/// `foul {team}`, `possession {team?}` / `serve`, `swap`, `version {v?}`,
/// `tennisFree {on?, pts?, games?}`, `backToFeed`, `banner {text, sub?, team?}`, `undo`, `reset`.
///
/// The same action within `debounce` ms of the last one counts once; every
/// action that changes the match goes on the Undo history (80 steps).
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let LayerKind::Scoreboard { x: c } = &layer.kind else {
        return Err("not a scoreboard".into());
    };
    let now = ctx.now_ms;
    st.ensure(c.team_count());
    // A countdown stops at 0 (it never runs on past it).
    if c.clock.dir == ClockDir::Down && st.clock.running {
        let len = period_len(c, st.period);
        if elapsed(st, now) >= len {
            st.clock.running = false;
            st.clock.base = len;
        }
    }
    match op {
        "undo" => {
            let h = st.hist.pop().ok_or("Nothing to undo")?;
            let mut prev: State = serde_json::from_value(h.d).map_err(|e| e.to_string())?;
            prev.ensure(c.team_count());
            let before = std::mem::take(st);
            prev.hist = before.hist;
            prev.last = Last::default();
            prev.ignored = before.ignored;
            prev.fx = before.fx;
            prev.fx.undo_at = now;
            prev.fx.ev = None;
            for i in 0..prev.teams.len() {
                let old = before.teams.get(i).map(|t| t.score).unwrap_or(0);
                if old != prev.teams[i].score {
                    prev.fx.sc.insert(
                        i,
                        ScoreFx {
                            at: now,
                            from: old.to_string(),
                        },
                    );
                }
            }
            *st = prev;
            return Ok(());
        }
        "backToFeed" => {
            st.src = Src::default();
            return Ok(());
        }
        "banner" => {
            let i = idx(args, "team").unwrap_or(0).min(c.team_count() - 1);
            let text = s_arg(args, "text");
            banner(
                c,
                st,
                i,
                if text.is_empty() { "GOAL!" } else { &text },
                s_arg(args, "sub"),
                None,
                now,
            );
            return Ok(());
        }
        _ => {}
    }
    let key = format!("{op} {args}");
    if c.debounce > 0.0 && st.last.key == key && ((now - st.last.at) as f64) < c.debounce {
        st.ignored += 1;
        st.fx.ign_at = now;
        return Ok(());
    }
    let mut snap = st.clone();
    snap.hist.clear();
    snap.fx = Fx::default();
    run(c, st, op, args, ctx)?;
    st.last = Last { key, at: now };
    st.hist.push(Hist {
        label: op.to_string(),
        d: serde_json::to_value(snap).map_err(|e| e.to_string())?,
    });
    if st.hist.len() > HISTORY {
        let extra = st.hist.len() - HISTORY;
        st.hist.drain(..extra);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layer(c: Config) -> Layer {
        let mut l: Layer =
            serde_json::from_str(r#"{"id":"s","name":"Score","type":"scoreboard","props":{}}"#)
                .unwrap();
        l.kind = LayerKind::Scoreboard { x: c };
        l
    }

    #[test]
    fn control_centre_rows_have_their_defaults() {
        // An older layer: the common pads exposed, the shoot-out and the swap not.
        let l: Layer = serde_json::from_str(
            r#"{"id":"s","name":"Score","type":"scoreboard","props":{},"preset":"basketball"}"#,
        )
        .unwrap();
        let LayerKind::Scoreboard { x: c } = &l.kind else {
            panic!()
        };
        assert!(c.op_clock && c.op_fouls && c.op_timeouts && c.op_susp && c.op_shot);
        assert!(c.op_score && c.op_added);
        assert!(!c.op_shootout && !c.op_swap);
        let l: Layer = serde_json::from_str(
            r#"{"id":"s","name":"Score","type":"scoreboard","props":{},"opSwap":true,"opFouls":false}"#,
        )
        .unwrap();
        let LayerKind::Scoreboard { x: c } = &l.kind else {
            panic!()
        };
        assert!(c.op_swap && !c.op_fouls);
    }

    fn go(
        l: &Layer,
        st: &mut State,
        op: &str,
        args: serde_json::Value,
        now: i64,
    ) -> Result<(), String> {
        let d = Data::new();
        act(
            l,
            st,
            op,
            &args,
            &ActCtx {
                now_ms: now,
                age_secs: 0.0,
                fps: 50.0,
                data: &d,
            },
        )
    }

    #[test]
    fn old_json_loads() {
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"S","type":"scoreboard","props":{}}"#)
                .unwrap();
        let LayerKind::Scoreboard { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
        let st: State = serde_json::from_value(serde_json::Value::Null).unwrap_or_default();
        assert_eq!(st.team(1).score, 0);
    }

    #[test]
    fn short_names() {
        assert_eq!(
            auto_short("Olympique Gymnaste Club Nice Côte d'Azur"),
            "OGC"
        );
        assert_eq!(auto_short("Paris Saint-Germain"), "PSG");
        assert_eq!(auto_short("Lyon Dragons"), "LDR");
        assert_eq!(auto_short("Nantes"), "NAN");
        let t = Team {
            short: "abcdef".into(),
            ..Default::default()
        };
        assert_eq!(short_of(&t), "ABC");
    }

    #[test]
    fn double_press_negative_and_undo() {
        let l = layer(Config::default());
        let mut st = State::default();
        go(&l, &mut st, "add", json!({"team":0}), 1000).unwrap();
        go(&l, &mut st, "add", json!({"team":0}), 1100).unwrap();
        assert_eq!(st.team(0).score, 1);
        assert_eq!(st.ignored, 1);
        go(&l, &mut st, "add", json!({"team":0}), 1600).unwrap();
        assert_eq!(st.team(0).score, 2);
        assert!(st.fx.ev.as_ref().is_some_and(|b| b.text == "GOAL!"));
        assert!(go(&l, &mut st, "sub", json!({"team":1}), 3000).is_err());
        assert!(go(&l, &mut st, "set", json!({"scores":[1,-2]}), 3000).is_err());
        go(&l, &mut st, "undo", json!(null), 4000).unwrap();
        assert_eq!(st.team(0).score, 1);
        go(&l, &mut st, "undo", json!(null), 4100).unwrap();
        assert_eq!(st.team(0).score, 0);
        assert!(go(&l, &mut st, "undo", json!(null), 4200).is_err());
        for k in 0..120 {
            go(&l, &mut st, "foul", json!({"team":0}), 10_000 + k * 1000).unwrap();
        }
        assert_eq!(st.hist.len(), HISTORY);
    }

    #[test]
    fn rugby_and_basketball_points() {
        let l = layer(Config {
            preset: Sport::Rugby,
            ..Default::default()
        });
        let mut st = State::default();
        for (k, t) in [(0, 0), (1, 1000), (2, 2000), (3, 3000)] {
            go(&l, &mut st, "add", json!({"team":0,"pts":k}), t).unwrap();
        }
        assert_eq!(st.team(0).score, 13);
        let l = layer(Config {
            preset: Sport::Basketball,
            clock: ClockCfg {
                dir: ClockDir::Down,
                len: 10.0,
                extra: 5.0,
                tenths: true,
            },
            ..Default::default()
        });
        let mut st = State::default();
        go(&l, &mut st, "set", json!({"scores":[99, 0]}), 0).unwrap();
        go(&l, &mut st, "add", json!({"team":0,"pts":2}), 1000).unwrap();
        assert_eq!(st.team(0).score, 102);
        for k in 0..5 {
            go(&l, &mut st, "timeout", json!({"team":1}), 2000 + k * 1000).unwrap();
        }
        assert!(go(&l, &mut st, "timeout", json!({"team":1}), 9000).is_err());
    }

    #[test]
    fn clock_is_timestamp_based() {
        let c = Config::default();
        let l = layer(c.clone());
        let mut st = State::default();
        go(&l, &mut st, "clockStart", json!(null), 0).unwrap();
        assert_eq!(clock_text(&c, &st, 61_000).0, "01:01");
        go(&l, &mut st, "clockPause", json!(null), 90_000).unwrap();
        assert_eq!(clock_text(&c, &st, 500_000).0, "01:30");
        go(&l, &mut st, "nextPeriod", json!(null), 600_000).unwrap();
        // 2nd half continues the time.
        assert_eq!(clock_text(&c, &st, 0).0, "45:00");
        go(&l, &mut st, "clockSet", json!({"time":"60:00"}), 700_000).unwrap();
        assert_eq!(clock_text(&c, &st, 0).0, "60:00");
        assert_eq!(parse_clock("12:3x"), None);
        let cd = Config {
            clock: ClockCfg {
                dir: ClockDir::Down,
                len: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let st = State {
            clock: Clock {
                running: true,
                start_at: 0,
                base: 0,
            },
            ..Default::default()
        };
        assert_eq!(clock_text(&cd, &st, 2_500).0, "57.5");
        assert!(clock_text(&cd, &st, 90_000).2);
    }

    #[test]
    fn tennis_rules() {
        let l = layer(Config {
            preset: Sport::Tennis,
            ..Default::default()
        });
        let mut st = State::default();
        st.tennis.pts = [3, 3];
        assert_eq!(tennis_tag(&st.tennis), "DEUCE");
        go(&l, &mut st, "add", json!({"team":0}), 0).unwrap();
        assert_eq!(tennis_pts(&st.tennis, 0), "AD");
        assert_eq!(tennis_pts(&st.tennis, 1), "40");
        go(&l, &mut st, "add", json!({"team":1}), 1000).unwrap();
        assert_eq!(tennis_tag(&st.tennis), "DEUCE");
        go(&l, &mut st, "add", json!({"team":0}), 2000).unwrap();
        go(&l, &mut st, "add", json!({"team":0}), 3000).unwrap();
        assert_eq!(st.tennis.games, [1, 0]);
        // Tie-break at 6-6, won 7-5.
        st.tennis.games = [6, 5];
        st.tennis.pts = [0, 3];
        go(&l, &mut st, "add", json!({"team":1}), 4000).unwrap();
        assert!(st.tennis.tb);
        st.tennis.pts = [5, 6];
        go(&l, &mut st, "add", json!({"team":1}), 5000).unwrap();
        assert_eq!(st.tennis.sets, vec![[6, 7]]);
        assert!(!st.tennis.tb);
    }

    #[test]
    fn volleyball_sets() {
        let l = layer(Config {
            preset: Sport::Volleyball,
            ..Default::default()
        });
        let mut st = State::default();
        go(&l, &mut st, "set", json!({"scores":[24, 24]}), 0).unwrap();
        go(&l, &mut st, "add", json!({"team":0}), 1000).unwrap();
        assert_eq!(st.team(0).score, 25);
        go(&l, &mut st, "add", json!({"team":0}), 2000).unwrap();
        assert_eq!(st.volley.won, [1, 0]);
        assert_eq!(st.team(0).score, 0);
        assert_eq!(st.period, 1);
    }

    #[test]
    fn feed_priority() {
        let mut c = Config::default();
        c.teams[0].score = "{{home}}".into();
        c.teams[1].score = "{{away}}".into();
        let l = layer(c.clone());
        let d: Data = [
            ("home".to_string(), "2".to_string()),
            ("away".to_string(), "1".to_string()),
        ]
        .into();
        let mut st = State::default();
        st.ensure(2);
        assert_eq!(scores(&c, &st, &d, 0), vec![2, 1]);
        let ctx = ActCtx {
            now_ms: 1000,
            age_secs: 0.0,
            fps: 50.0,
            data: &d,
        };
        act(&l, &mut st, "sub", &json!({"team":0}), &ctx).unwrap();
        // Manual override for 20 s, from the score on air.
        assert_eq!(scores(&c, &st, &d, 2000), vec![1, 1]);
        assert_eq!(
            active_source(&c, &st, &d, 2000),
            Active::ManualPriority(Some(19))
        );
        assert_eq!(scores(&c, &st, &d, 30_000), vec![2, 1]);
        let lost: Data = [("home".to_string(), "".to_string())].into();
        assert_eq!(active_source(&c, &st, &lost, 30_000), Active::FeedLost);
    }

    #[test]
    fn minus_cancels_the_teams_last_point() {
        let l = layer(Config {
            preset: Sport::Rugby,
            ..Default::default()
        });
        let mut st = State::default();
        // Try (5), then penalty (3, button 2): − takes the 3 off, then the 5.
        go(&l, &mut st, "add", json!({"team":0,"pts":0}), 0).unwrap();
        go(&l, &mut st, "add", json!({"team":0,"pts":2}), 1000).unwrap();
        assert_eq!(st.team(0).score, 8);
        go(&l, &mut st, "sub", json!({"team":0}), 2000).unwrap();
        assert_eq!(st.team(0).score, 5);
        go(&l, &mut st, "sub", json!({"team":0}), 3000).unwrap();
        assert_eq!(st.team(0).score, 0);
        // Nothing left: refused below zero.
        assert!(go(&l, &mut st, "sub", json!({"team":0}), 4000).is_err());
    }

    #[test]
    fn shot_clock_suspensions_and_shootout() {
        let l = layer(Config {
            preset: Sport::Basketball,
            fields: preset(Sport::Basketball).fields,
            clock: ClockCfg {
                dir: ClockDir::Down,
                len: 10.0,
                extra: 5.0,
                tenths: true,
            },
            ..Default::default()
        });
        let LayerKind::Scoreboard { x: c } = &l.kind else {
            panic!()
        };
        let mut st = State::default();
        go(&l, &mut st, "clockStart", json!({}), 0).unwrap();
        assert_eq!(shot_text(c, &st, 10_000), ("14".to_string(), false));
        go(&l, &mut st, "shot", json!({"s": 14}), 10_000).unwrap();
        assert_eq!(shot_text(c, &st, 15_500).0, "9");
        // Paused with the match clock.
        go(&l, &mut st, "clockPause", json!({}), 16_000).unwrap();
        assert_eq!(shot_text(c, &st, 60_000).0, "8");
        // A countdown at 0 stops and does not start again.
        go(&l, &mut st, "clockStart", json!({}), 60_000).unwrap();
        go(&l, &mut st, "foul", json!({"team":0}), 60_000 + 600_000).unwrap();
        assert!(!st.clock.running);
        assert!(go(&l, &mut st, "clockStart", json!({}), 700_000).is_err());
        // Handball: a 2-minute suspension counts down with the match time.
        let h = layer(Config {
            preset: Sport::Handball,
            fields: preset(Sport::Handball).fields,
            ..Default::default()
        });
        let LayerKind::Scoreboard { x: hc } = &h.kind else {
            panic!()
        };
        let mut st = State::default();
        go(&h, &mut st, "clockStart", json!({}), 0).unwrap();
        go(
            &h,
            &mut st,
            "card",
            json!({"team":1,"color":"2 min"}),
            30_000,
        )
        .unwrap();
        assert_eq!(susps_left(hc, &st, 1, 90_000), vec![60_000]);
        assert_eq!(fmt_left(60_000), "1:00");
        assert!(susps_left(hc, &st, 1, 151_000).is_empty());
        // Football: shoot-out kicks in the « Penalties » period.
        let f = layer(Config::default());
        let LayerKind::Scoreboard { x: fc } = &f.kind else {
            panic!()
        };
        let mut st = State {
            period: 4,
            ..Default::default()
        };
        assert!(shootout_on(fc, &st));
        go(&f, &mut st, "penalty", json!({"team":0,"scored":true}), 0).unwrap();
        go(
            &f,
            &mut st,
            "penalty",
            json!({"team":0,"scored":false}),
            1000,
        )
        .unwrap();
        assert_eq!(st.team(0).pens, vec![true, false]);
    }

    #[test]
    fn texts_follow_the_language_and_the_designer() {
        let mut c = Config::default();
        assert_eq!(tr(&c, "fr", "1st half"), "1re mi-temps");
        assert_eq!(tr(&c, "en", "1st half"), "1st half");
        assert_eq!(tr(&c, "fr", "GOAL!"), "BUT\u{a0}!");
        c.texts.insert("GOAL!".into(), "GOOOAL".into());
        assert_eq!(tr(&c, "fr", "GOAL!"), "GOOOAL");
        let l = layer(Config {
            preset: Sport::Basketball,
            ..Default::default()
        });
        let mut st = State::default();
        go(&l, &mut st, "timeout", json!({"team":0}), 0).unwrap();
        let ev = st.fx.ev.clone().unwrap();
        let LayerKind::Scoreboard { x: bc } = &l.kind else {
            panic!()
        };
        assert_eq!(fill(&tr(bc, "fr", &ev.sub), &ev.vars), "PSG · reste 4");
        assert!(texts_of(bc).contains(&"TIMEOUT"));
    }
}
