//! What the inputs carry besides their pictures, by input id: the tally
//! and UMD the switchers send (`crate::tally`), the levels and loudness of
//! their audio and their timecode (measured by `crate::inputs`), and the
//! faults seen on them (loss, black, frozen picture, silence, loudness,
//! true peak), with the journal of those faults and the state of PTP and
//! NTP. The engine reads them through `Inputs::source` and
//! `Inputs::system`; the browsers get them in `signals` events (a dozen a
//! second while they change).

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use strata_mv_core::alarm::AlarmSettings;
use strata_mv_core::signal::{
    Alarm, AlarmKind, JournalEntry, Level, Loudness, NtpView, PtpView, Severity, SystemView, Tally,
    SILENCE, TARGET,
};

/// The hold mark stays this long on a peak, then falls.
const HOLD: Duration = Duration::from_secs(2);
/// How fast a meter falls back, dB a second (peak meters fall slowly).
const FALL: f32 = 24.0;
/// Without a measure for this long, the input has no audio (no meters).
const GONE: Duration = Duration::from_secs(3);
/// The short-term loudness kept, one a second.
const HISTORY: usize = 120;
/// A timecode not received for this long is gone.
const TC_GONE: Duration = Duration::from_secs(1);

/// A picture that moved and stopped moving (only noise left) is frozen
/// after this long…
const FREEZE_AFTER: Duration = Duration::from_secs(10);
/// …if it moved, a few moments at least, in this time before.
const MOVED_BEFORE: Duration = Duration::from_secs(30);
/// A live picture (never twice the same, by its noise) for this long that
/// becomes the same to the pixel picture after picture (a decoder or an
/// encoder repeating its last one) is frozen after…
const NOISY_BEFORE: Duration = Duration::from_secs(5);
const EXACT_AFTER: Duration = Duration::from_secs(4);
/// A sample changing by more than this is the picture moving, not noise.
const MOVES: u8 = 10;
/// An input that never had a picture is not lost while it starts.
const STARTING: Duration = Duration::from_secs(10);
/// Lines of the journal kept (and saved).
const JOURNAL: usize = 1000;

struct Meter {
    shown: Vec<Level>,
    /// When each channel's hold mark was set.
    held: Vec<Instant>,
    at: Instant,
    /// The meters as they were at each measure, the last second and a half:
    /// an output shows them as late as its pictures.
    past: VecDeque<(Instant, Vec<Level>)>,
    /// The time of the sound the meters show now.
    shown_at: Instant,
    /// The measures not sent to the browsers yet, each with its time: the
    /// measures come in bursts, each is played at its time.
    unsent: Vec<(Instant, Vec<Level>)>,
}

struct Loud {
    now: Loudness,
    at: Instant,
    history: VecDeque<f32>,
    /// When the last value went into the history.
    sampled: Instant,
    /// The history changed since the browsers had it.
    history_changed: bool,
}

/// What the picture of an input looks like, to tell black and frozen ones.
struct Look {
    /// 16 × 9 luma samples of the last picture.
    print: [u8; PRINT],
    /// Since when it has not moved (only noise, if anything).
    still_since: Instant,
    /// When it moved (twice a second at most), the last minute.
    moved: VecDeque<Instant>,
    /// Since when each picture is the same as the one before, to the pixel.
    exact_since: Option<Instant>,
    /// It was never the same twice (a live picture's noise) for a while
    /// just before it became the same to the pixel.
    noisy_before: bool,
    /// When two pictures in a row were last the same to the pixel.
    last_exact: Option<Instant>,
    /// Since it stopped moving: pictures, and how many of them were the
    /// same to the pixel as the one before (a frozen picture coded again
    /// differs at each key frame only; a live one, by its noise, always).
    still_pictures: u32,
    still_exact: u32,
    /// Since when it is black.
    black_since: Option<Instant>,
    /// Still by nature (colour bars, a plain picture): never frozen.
    still_kind: bool,
}

/// Samples of a picture's print (16 × 9).
const PRINT: usize = 144;

/// The faults on an input now (or on a part of the plant), and its name
/// (the journal's lines say it).
#[derive(Default)]
struct Active {
    kinds: BTreeMap<AlarmKind, On>,
    name: String,
}

/// The faults raised on an input now: its name, each with when it began
/// and how grave it is.
type Found = (String, BTreeMap<AlarmKind, (Instant, Severity)>);

/// A fault on now: since when (ms since 1970), how grave, seen or not.
struct On {
    since: i64,
    severity: Severity,
    acked: bool,
}

/// What is known of an input to look at its faults (`Inputs::watched`).
pub struct Watched {
    pub id: String,
    pub name: String,
    /// A picture came lately.
    pub fresh: bool,
    /// When the last one came.
    pub lost_at: Option<Instant>,
    /// Pictures a second measured, and of its format.
    pub fps: Option<f64>,
    pub nominal: Option<f64>,
    /// Times it started again in the last minute.
    pub restarts: usize,
    pub correlation: Option<f32>,
    /// Closed captions lately.
    pub captions: bool,
    /// TR 101 290 errors lately: first, second priority.
    pub ts: (bool, bool),
    /// What its picture is (colour space, levels).
    pub video: strata_mv_core::signal::VideoInfo,
    /// Its tiles show its backup.
    pub on_backup: bool,
}

/// What is known of the plant to look at its faults.
#[derive(Default)]
pub struct Plant {
    pub ptp_running: bool,
    pub ptp_locked: bool,
    pub ntp_synced: Option<bool>,
    /// % of the machine (processors over 10 s, memory).
    pub cpu: f32,
    pub memory: f32,
    /// Outputs: key, name, pictures dropped in the last 10 s.
    pub dropped: Vec<(String, String, f64)>,
}

/// A fault raised, over or acknowledged: for the actions.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlarmEvent {
    /// "raised", "ended", "acked".
    pub state: &'static str,
    pub input_id: String,
    pub input: String,
    pub kind: AlarmKind,
    pub severity: Severity,
    /// ms since 1970.
    pub at: i64,
}

impl AlarmEvent {
    fn new(
        state: &'static str,
        id: &str,
        name: &str,
        kind: AlarmKind,
        severity: Severity,
        at: i64,
    ) -> Self {
        Self {
            state,
            input_id: id.to_string(),
            input: name.to_string(),
            kind,
            severity,
            at,
        }
    }
}

/// The signals of every input.
#[derive(Default)]
pub struct Signals {
    tally: RwLock<BTreeMap<String, Tally>>,
    audio: RwLock<BTreeMap<String, Meter>>,
    loud: RwLock<BTreeMap<String, Loud>>,
    looks: RwLock<BTreeMap<String, Look>>,
    timecodes: RwLock<BTreeMap<String, (String, Instant)>>,
    active: RwLock<BTreeMap<String, Active>>,
    journal: RwLock<VecDeque<JournalEntry>>,
    ptp: RwLock<(PtpView, NtpView)>,
    /// How faults are raised (`crate::alarms`).
    settings: RwLock<AlarmSettings>,
    /// Since when each fault's condition is true (input id, fault).
    conds: std::sync::Mutex<BTreeMap<(String, AlarmKind), Instant>>,
    /// When each input was first looked at (an input starting is not lost).
    first_seen: std::sync::Mutex<BTreeMap<String, Instant>>,
    /// Bumped when the journal changes (it is saved then).
    journal_version: AtomicU64,
    /// The plant's measures, one a second (the charts).
    curves: RwLock<strata_mv_core::signal::Curves>,
    /// Bumped at each change (the broadcaster sends when it moves).
    version: AtomicU64,
}

/// One input's signals, as the browsers get them.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SignalView {
    #[serde(skip_serializing_if = "is_off")]
    pub tally: Tally,
    /// Per channel: peak, mean power, hold (dBFS, one decimal).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub levels: Vec<[f32; 3]>,
    /// The time of the sound they measure (ms since 1970): the pages show
    /// them with the pictures of that time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels_at: Option<f64>,
    /// Every measure since the last message: its time (ms since 1970) and
    /// the meters then.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub levels_seq: Vec<(f64, Vec<[f32; 3]>)>,
    /// Momentary, short-term, integrated (LUFS), range (LU), true peak (dBTP).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loudness: Option<[f32; 5]>,
    /// The short-term loudness, one a second: sent when it changed only
    /// (the browsers keep the last one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alarms: Vec<Alarm>,
    /// What its picture and sound are: sent when it changed, and every few
    /// seconds for the pages just opened (the browsers keep the last one).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<strata_mv_core::signal::VideoInfo>,
    /// The phase correlation of its left and right (−1 to 1, two decimals).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation: Option<f32>,
    /// Lost its signal: the id of the backup its tiles show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
}

fn is_off(t: &Tally) -> bool {
    *t == Tally::default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The ms since 1970 of an instant of the past.
fn ms_of(at: Instant) -> i64 {
    now_ms() - at.elapsed().as_millis() as i64
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(|p| p.into_inner())
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(|p| p.into_inner())
}

/// 8 × 8 luma samples of a picture (0…255), and whether it is black.
/// The picture is colour bars (SMPTE, EBU, HD, 75 % or 100 %): columns the
/// same from top to bottom over its upper half, yellow, cyan, green,
/// magenta, red and blue in that order from left to right.
pub fn is_bars(pm: &tiny_skia::Pixmap) -> bool {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    if w < 64 || h < 32 {
        return false;
    }
    let data = pm.data();
    let px = |x: usize, y: usize| {
        let p = (y * w + x) * 4;
        [data[p] as i32, data[p + 1] as i32, data[p + 2] as i32]
    };
    const N: usize = 96;
    let rows = [h * 12 / 100, h * 30 / 100, h * 48 / 100];
    let line: Vec<[i32; 3]> = (0..N)
        .map(|i| px((i * 2 + 1) * w / (2 * N), rows[1]))
        .collect();
    // Columns: the same colour at the three heights.
    let same = |a: [i32; 3], b: [i32; 3]| (0..3).all(|c| (a[c] - b[c]).abs() <= 28);
    let uniform = (0..N)
        .filter(|&i| {
            let x = (i * 2 + 1) * w / (2 * N);
            same(px(x, rows[0]), line[i]) && same(px(x, rows[2]), line[i])
        })
        .count();
    if uniform * 100 < N * 80 {
        return false;
    }
    // The hues, left to right: Y, C, G, M, R, B ('-' for a grey or another colour).
    let hue = |[r, g, b]: [i32; 3]| {
        let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
        if hi < 80 || hi - lo < 80 {
            return '-';
        }
        let on = |v: i32| v > lo + (hi - lo) / 2;
        match (on(r), on(g), on(b)) {
            (true, true, false) => 'Y',
            (false, true, true) => 'C',
            (false, true, false) => 'G',
            (true, false, true) => 'M',
            (true, false, false) => 'R',
            (false, false, true) => 'B',
            _ => '-',
        }
    };
    let mut seq: Vec<(char, usize)> = Vec::new();
    for c in line.iter().map(|&p| hue(p)) {
        match seq.last_mut() {
            Some((h, n)) if *h == c => *n += 1,
            _ => seq.push((c, 1)),
        }
    }
    // Each bar a few samples wide at least (not a detail of a picture).
    let bars: String = seq
        .iter()
        .filter(|(c, n)| *c != '-' && *n >= 4)
        .map(|(c, _)| *c)
        .collect();
    bars.contains("YCGMRB")
}

/// 16 × 9 luma samples of a picture, whether it is black, whether it is
/// plain (one colour, a slate's background: still by nature).
fn print_of(pm: &tiny_skia::Pixmap) -> ([u8; PRINT], bool, bool) {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data();
    let mut out = [0u8; PRINT];
    let (mut sum, mut max, mut min) = (0u32, 0u8, 255u8);
    for (i, o) in out.iter_mut().enumerate() {
        let (gx, gy) = (i % 16, i / 16);
        let x = (gx * 2 + 1) * w / 32;
        let y = (gy * 2 + 1) * h / 18;
        let p = (y.min(h.saturating_sub(1)) * w + x.min(w.saturating_sub(1))) * 4;
        let (r, g, b) = (
            u32::from(*data.get(p).unwrap_or(&0)),
            u32::from(*data.get(p + 1).unwrap_or(&0)),
            u32::from(*data.get(p + 2).unwrap_or(&0)),
        );
        let y = ((r * 54 + g * 183 + b * 19) >> 8) as u8;
        *o = y;
        sum += u32::from(y);
        max = max.max(y);
        min = min.min(y);
    }
    // Black: under 4 % on average, nothing above 12 %.
    let black = sum / (PRINT as u32) < 10 && max < 31;
    (out, black, max - min < 12)
}

impl Signals {
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    fn bump(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// The tally of every input at once (what the tally sources say now).
    pub fn set_tallies(&self, all: BTreeMap<String, Tally>) {
        let mut t = write(&self.tally);
        if *t != all {
            *t = all;
            self.bump();
        }
    }

    pub fn tally(&self, id: &str) -> Tally {
        read(&self.tally).get(id).cloned().unwrap_or_default()
    }

    /// A new measure of an input's audio: the meters rise at once, fall
    /// at [`FALL`]; the hold mark keeps the highest peak for [`HOLD`].
    pub fn measure(&self, id: &str, levels: &[Level]) {
        self.measure_at(id, levels, Instant::now());
    }

    /// The same, the sound measured being of time `when` (its own time on
    /// the wall clock: the outputs and the pages show it with the pictures
    /// of that time).
    pub fn measure_at(&self, id: &str, levels: &[Level], when: Instant) {
        let now = Instant::now();
        let mut audio = write(&self.audio);
        let m = audio.entry(id.to_string()).or_insert_with(|| Meter {
            shown: Vec::new(),
            held: Vec::new(),
            at: now,
            past: VecDeque::new(),
            shown_at: now,
            unsent: Vec::new(),
        });
        let dt = now.saturating_duration_since(m.at).as_secs_f32().min(1.0);
        m.at = now;
        m.shown.resize(levels.len(), Level::default());
        m.held.resize(levels.len(), now);
        for (i, new) in levels.iter().enumerate() {
            let s = &mut m.shown[i];
            s.peak = new.peak.max(s.peak - FALL * dt).max(SILENCE);
            s.rms = new.rms;
            if new.peak >= s.hold || now.saturating_duration_since(m.held[i]) > HOLD {
                s.hold = new.peak.max(s.hold - FALL * dt).max(SILENCE);
                if new.peak >= s.hold {
                    m.held[i] = now;
                }
            }
        }
        m.past.push_back((when, m.shown.clone()));
        m.shown_at = when;
        m.unsent.push((when, m.shown.clone()));
        if m.unsent.len() > 50 {
            m.unsent.remove(0);
        }
        while m
            .past
            .front()
            .is_some_and(|(t, _)| now.saturating_duration_since(*t) > Duration::from_millis(1500))
        {
            m.past.pop_front();
        }
        self.bump();
    }

    /// A new measure of an input's loudness.
    pub fn loudness(&self, id: &str, l: Loudness) {
        let now = Instant::now();
        let mut all = write(&self.loud);
        let e = all.entry(id.to_string()).or_insert_with(|| Loud {
            now: l,
            at: now,
            history: VecDeque::new(),
            sampled: now - Duration::from_secs(1),
            history_changed: true,
        });
        e.now = l;
        e.at = now;
        if now.saturating_duration_since(e.sampled) >= Duration::from_secs(1) {
            e.sampled = now;
            e.history.push_back((l.short_term * 10.0).round() / 10.0);
            while e.history.len() > HISTORY {
                e.history.pop_front();
            }
            e.history_changed = true;
        }
        self.bump();
    }

    /// The timecode an input carries now ("HH:MM:SS:FF").
    pub fn timecode(&self, id: &str, tc: String) {
        write(&self.timecodes).insert(id.to_string(), (tc, Instant::now()));
        self.bump();
    }

    /// A new picture of an input: is it black, has it changed?
    pub fn picture(&self, id: &str, pm: &tiny_skia::Pixmap) {
        let (print, black, plain) = print_of(pm);
        let still_kind = plain || is_bars(pm);
        let now = Instant::now();
        let mut looks = write(&self.looks);
        let l = looks.entry(id.to_string()).or_insert(Look {
            print,
            still_since: now,
            moved: VecDeque::new(),
            exact_since: None,
            noisy_before: false,
            last_exact: None,
            still_pictures: 0,
            still_exact: 0,
            black_since: None,
            still_kind,
        });
        let most = l
            .print
            .iter()
            .zip(print.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        // Moving: more than noise somewhere.
        if most > MOVES {
            l.still_since = now;
            l.still_pictures = 0;
            l.still_exact = 0;
            if l.moved
                .back()
                .is_none_or(|t| now.saturating_duration_since(*t) >= Duration::from_millis(500))
            {
                l.moved.push_back(now);
            }
        }
        while l
            .moved
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) > Duration::from_secs(60))
        {
            l.moved.pop_front();
        }
        l.still_pictures += 1;
        if most == 0 {
            l.still_exact += 1;
        }
        // The same to the pixel as the one before.
        if most == 0 {
            if l.exact_since.is_none() {
                l.noisy_before = l
                    .last_exact
                    .is_none_or(|t| now.saturating_duration_since(t) >= NOISY_BEFORE);
                l.exact_since = Some(now);
            }
            l.last_exact = Some(now);
        } else {
            l.exact_since = None;
        }
        l.print = print;
        l.still_kind = still_kind;
        if black {
            l.black_since.get_or_insert(now);
        } else {
            l.black_since = None;
        }
    }

    /// Since when an input's picture is frozen (None: it is not). Frozen:
    /// not still by nature (colour bars, a plain picture, black) and either
    /// - it was live (its noise) and is the same to the pixel picture after
    ///   picture for `EXACT_AFTER` (a decoder or an encoder repeating its
    ///   last picture), or
    /// - it moved, then stopped for `FREEZE_AFTER`, its pictures the same to
    ///   the pixel 4 times out of 5 since (a frozen picture coded again; a
    ///   calm live shot keeps its noise).
    ///
    /// A picture still from the start (a slate, a still graphic) is never
    /// frozen.
    fn frozen(l: &Look, now: Instant) -> Option<Instant> {
        if l.still_kind || l.black_since.is_some() {
            return None;
        }
        if let Some(e) = l.exact_since.filter(|_| l.noisy_before) {
            if now.saturating_duration_since(e) >= EXACT_AFTER {
                return Some(e);
            }
        }
        let moved_before = l
            .moved
            .iter()
            .filter(|t| {
                **t <= l.still_since && l.still_since.saturating_duration_since(**t) <= MOVED_BEFORE
            })
            .count();
        let repeated = l.still_pictures > 0 && l.still_exact * 5 >= l.still_pictures * 4;
        (moved_before >= 3
            && repeated
            && now.saturating_duration_since(l.still_since) >= FREEZE_AFTER)
            .then_some(l.still_since)
    }

    /// The meters of an input as they were `delay` ago (an output shows
    /// its pictures that late): the peaks falling smoothly since that
    /// measure, not by steps from one measure to the next.
    pub fn levels_at(&self, id: &str, delay: Duration) -> Vec<Level> {
        let audio = read(&self.audio);
        let Some(m) = audio.get(id).filter(|m| m.at.elapsed() < GONE) else {
            return Vec::new();
        };
        let Some(when) = Instant::now().checked_sub(delay) else {
            return m.shown.clone();
        };
        let Some((t, levels)) = m
            .past
            .iter()
            .rev()
            .find(|(t, _)| *t <= when)
            .or(m.past.front())
        else {
            return m.shown.clone();
        };
        let fall = FALL * when.saturating_duration_since(*t).as_secs_f32().min(1.0);
        levels
            .iter()
            .map(|l| Level {
                peak: (l.peak - fall).max(SILENCE),
                rms: l.rms,
                hold: l.hold,
            })
            .collect()
    }

    /// The meters of an input (none: no audio lately).
    pub fn levels(&self, id: &str) -> Vec<Level> {
        match read(&self.audio).get(id) {
            Some(m) if m.at.elapsed() < GONE => m.shown.clone(),
            _ => Vec::new(),
        }
    }

    /// Its loudness and short-term history (none: no measure lately).
    pub fn loudness_of(&self, id: &str) -> (Option<Loudness>, Vec<f32>) {
        match read(&self.loud).get(id) {
            Some(l) if l.at.elapsed() < GONE => (Some(l.now), l.history.iter().copied().collect()),
            _ => (None, Vec::new()),
        }
    }

    pub fn timecode_of(&self, id: &str) -> String {
        match read(&self.timecodes).get(id) {
            Some((tc, at)) if at.elapsed() < TC_GONE => tc.clone(),
            _ => String::new(),
        }
    }

    /// The faults on an input now, the worst first.
    pub fn alarms(&self, id: &str) -> Vec<Alarm> {
        let mut out: Vec<Alarm> = read(&self.active)
            .get(id)
            .map(|a| {
                a.kinds
                    .iter()
                    .map(|(k, on)| Alarm {
                        kind: *k,
                        severity: on.severity,
                        since: on.since,
                        acked: on.acked,
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.kind.cmp(&b.kind)));
        out
    }

    /// Looks at the faults of every input and of the plant again, by the
    /// alarm settings (`crate::alarms`): new ones and the ones over go to
    /// the journal, and are returned for the actions (calls to other
    /// systems).
    pub fn evaluate(&self, inputs: &[Watched], plant: &Plant) -> Vec<AlarmEvent> {
        let settings = read(&self.settings).clone();
        let now = Instant::now();
        // The conditions true now, each with when it began if known.
        let mut seen: Vec<(String, String, AlarmKind, Option<Instant>)> = Vec::new();
        let starting: std::collections::BTreeSet<String> = {
            let mut first = self.first_seen.lock().unwrap_or_else(|p| p.into_inner());
            first.retain(|id, _| inputs.iter().any(|w| &w.id == id));
            inputs
                .iter()
                .filter(|w| {
                    let at = *first.entry(w.id.clone()).or_insert(now);
                    now.saturating_duration_since(at) < STARTING
                })
                .map(|w| w.id.clone())
                .collect()
        };
        {
            let looks = read(&self.looks);
            let audio = read(&self.audio);
            let loud = read(&self.loud);
            let tcs = read(&self.timecodes);
            for w in inputs {
                let mut on = |k: AlarmKind, since: Option<Instant>| {
                    seen.push((w.id.clone(), w.name.clone(), k, since));
                };
                let rule = |k: AlarmKind| settings.rule(&w.id, k);
                if w.on_backup {
                    on(AlarmKind::OnBackup, None);
                }
                if !w.fresh {
                    // No picture: the other faults of the picture do not count;
                    // none yet while it starts (it connects).
                    if w.lost_at.is_some() || !starting.contains(&w.id) {
                        on(AlarmKind::Loss, Some(w.lost_at.unwrap_or(now)));
                    }
                } else {
                    if let Some(l) = looks.get(&w.id) {
                        if let Some(s) = l.black_since {
                            on(AlarmKind::Black, Some(s));
                        } else if let Some(s) = Self::frozen(l, now) {
                            on(AlarmKind::Freeze, Some(s));
                        }
                    }
                    if let (Some(fps), Some(nominal)) = (w.fps, w.nominal.filter(|n| *n > 1.0)) {
                        // Off by more than the threshold (fields counted
                        // as pictures are not off).
                        let off = ((fps - nominal).abs().min((fps - 2.0 * nominal).abs()))
                            / nominal
                            * 100.0;
                        if off > rule(AlarmKind::Fps).threshold as f64 {
                            on(AlarmKind::Fps, None);
                        }
                    }
                    if !tcs
                        .get(&w.id)
                        .is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(2))
                    {
                        on(AlarmKind::NoTimecode, None);
                    }
                    if !w.captions {
                        on(AlarmKind::NoCaptions, None);
                    }
                    // Its colours.
                    let v = &w.video;
                    if !strata_mv_core::alarm::color_space_ok(
                        &rule(AlarmKind::ColorSpace).expect,
                        v,
                    ) {
                        on(AlarmKind::ColorSpace, None);
                    }
                    if v.range != "pc"
                        && v.illegal
                            .is_some_and(|x| x > rule(AlarmKind::IllegalLevels).threshold)
                    {
                        on(AlarmKind::IllegalLevels, None);
                    }
                    if v.range == "pc" {
                        on(AlarmKind::FullRange, None);
                    }
                    if v.transfer.contains("2084") && v.max_cll == 0 && v.mastering.is_empty() {
                        on(AlarmKind::HdrMetadata, None);
                    }
                    if !v.pix_fmt.is_empty()
                        && v.matrix.is_empty()
                        && v.primaries.is_empty()
                        && v.transfer.is_empty()
                    {
                        on(AlarmKind::NoColorInfo, None);
                    }
                }
                if w.restarts as f32 >= rule(AlarmKind::Reconnecting).threshold.max(1.0) {
                    on(AlarmKind::Reconnecting, None);
                }
                if w.ts.0 {
                    on(AlarmKind::TsPriority1, None);
                }
                if w.ts.1 {
                    on(AlarmKind::TsPriority2, None);
                }
                let mut silent = false;
                if let Some(m) = audio.get(&w.id).filter(|m| m.at.elapsed() < GONE) {
                    let quiet = rule(AlarmKind::Silence).threshold;
                    if !m.shown.is_empty() && m.shown.iter().all(|l| l.peak < quiet) {
                        silent = true;
                        on(AlarmKind::Silence, None);
                    }
                    // One channel silent, another not (the LFE of 5.1 aside).
                    let mute = rule(AlarmKind::ChannelSilent).threshold;
                    let lfe = (m.shown.len() == 6).then_some(3);
                    let heard: Vec<&Level> = m
                        .shown
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| Some(*i) != lfe)
                        .map(|(_, l)| l)
                        .collect();
                    if heard.len() >= 2
                        && heard.iter().any(|l| l.peak < mute)
                        && heard.iter().any(|l| l.peak >= mute + 20.0)
                    {
                        on(AlarmKind::ChannelSilent, None);
                    }
                    // The peak hold (two seconds): a clip is seen a moment.
                    let clip = rule(AlarmKind::Clipping).threshold;
                    if m.shown.iter().any(|l| l.hold >= clip) {
                        on(AlarmKind::Clipping, None);
                    }
                }
                if let Some(l) = loud.get(&w.id).filter(|l| l.at.elapsed() < GONE) {
                    let st = l.now.short_term;
                    if st > TARGET + rule(AlarmKind::Loud).threshold {
                        on(AlarmKind::Loud, None);
                    }
                    if !silent && st > -70.0 && st < TARGET - rule(AlarmKind::Quiet).threshold {
                        on(AlarmKind::Quiet, None);
                    }
                    // The highest true peak of the last seconds.
                    if l.now.true_peak > rule(AlarmKind::Peak).threshold {
                        on(AlarmKind::Peak, None);
                    }
                }
                if let Some(c) = w.correlation {
                    if c < rule(AlarmKind::Phase).threshold {
                        on(AlarmKind::Phase, None);
                    }
                }
            }
            // The plant's, under names of their own.
            let mut on = |id: &str, name: &str, k: AlarmKind| {
                seen.push((id.to_string(), name.to_string(), k, None));
            };
            if plant.ptp_running && !plant.ptp_locked {
                on("~ptp", "PTP", AlarmKind::PtpUnlocked);
            }
            if plant.ntp_synced == Some(false) {
                on("~ntp", "NTP", AlarmKind::NtpUnsynced);
            }
            if plant.cpu > settings.rule("", AlarmKind::CpuHigh).threshold {
                on("~cpu", "CPU", AlarmKind::CpuHigh);
            }
            if plant.memory > settings.rule("", AlarmKind::MemoryHigh).threshold {
                on("~memory", "RAM", AlarmKind::MemoryHigh);
            }
            let drops = settings
                .rule("", AlarmKind::OutputDropped)
                .threshold
                .max(1.0);
            for (key, name, n) in &plant.dropped {
                if *n as f32 >= drops {
                    on(&format!("~output:{key}"), name, AlarmKind::OutputDropped);
                }
            }
        }

        // When each condition began; raised once it has lasted long enough.
        let mut found: BTreeMap<String, Found> = BTreeMap::new();
        {
            let mut conds = self.conds.lock().unwrap_or_else(|p| p.into_inner());
            let mut still: BTreeMap<(String, AlarmKind), Instant> = BTreeMap::new();
            for (id, name, k, since) in &seen {
                let began = conds
                    .get(&(id.clone(), *k))
                    .copied()
                    .or(*since)
                    .unwrap_or(now);
                let began = since.map_or(began, |s| s.min(began));
                still.insert((id.clone(), *k), began);
                let r = settings.rule(if k.system() { "" } else { id }, *k);
                if !r.enabled {
                    continue;
                }
                if now.saturating_duration_since(began).as_secs_f32() >= r.after.max(0.0) {
                    found
                        .entry(id.clone())
                        .or_insert_with(|| (name.clone(), BTreeMap::new()))
                        .1
                        .insert(*k, (began, r.severity));
                }
            }
            *conds = still;
        }
        // Inputs (and the plant's parts) watched now, their faults or not.
        for w in inputs {
            found
                .entry(w.id.clone())
                .or_insert_with(|| (w.name.clone(), BTreeMap::new()));
        }

        let t = now_ms();
        let mut events = Vec::new();
        let mut changed = false;
        let mut active = write(&self.active);
        let mut journal = write(&self.journal);
        // Gone (an input removed, a part of the plant fine again): over.
        let gone: Vec<String> = active
            .keys()
            .filter(|id| !found.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(a) = active.remove(&id) {
                for (k, on) in &a.kinds {
                    end(&mut journal, &id, *k, t);
                    events.push(AlarmEvent::new("ended", &id, &a.name, *k, on.severity, t));
                }
                changed = true;
            }
        }
        for (id, (name, kinds)) in found {
            let a = active.entry(id.clone()).or_default();
            a.name = name.clone();
            let over: Vec<AlarmKind> = a
                .kinds
                .keys()
                .filter(|k| !kinds.contains_key(k))
                .copied()
                .collect();
            for k in over {
                if let Some(on) = a.kinds.remove(&k) {
                    end(&mut journal, &id, k, t);
                    events.push(AlarmEvent::new("ended", &id, &name, k, on.severity, t));
                    changed = true;
                }
            }
            for (k, (since, severity)) in kinds {
                if let Some(on) = a.kinds.get_mut(&k) {
                    // Its severity set otherwise meanwhile.
                    if on.severity != severity {
                        on.severity = severity;
                        changed = true;
                    }
                    continue;
                }
                let at = ms_of(since);
                a.kinds.insert(
                    k,
                    On {
                        since: at,
                        severity,
                        acked: false,
                    },
                );
                journal.push_front(JournalEntry {
                    at,
                    input: name.clone(),
                    input_id: id.clone(),
                    kind: k,
                    severity,
                    ended: None,
                    acked: None,
                    by: String::new(),
                });
                journal.truncate(JOURNAL);
                events.push(AlarmEvent::new("raised", &id, &name, k, severity, at));
                changed = true;
            }
        }
        if changed {
            self.journal_version.fetch_add(1, Ordering::Relaxed);
            self.bump();
        }
        events
    }

    /// Someone has seen the faults on now (of an input, of a kind, or
    /// all): they stop flashing, the journal says who and when.
    pub fn ack(&self, input: Option<&str>, kind: Option<AlarmKind>, by: &str) -> Vec<AlarmEvent> {
        let t = now_ms();
        let mut events = Vec::new();
        let mut active = write(&self.active);
        let mut journal = write(&self.journal);
        for (id, a) in active.iter_mut() {
            if input.is_some_and(|i| i != id) {
                continue;
            }
            for (k, on) in a.kinds.iter_mut() {
                if on.acked || kind.is_some_and(|x| x != *k) {
                    continue;
                }
                on.acked = true;
                if let Some(e) = journal
                    .iter_mut()
                    .find(|e| e.input_id == *id && e.kind == *k && e.ended.is_none())
                {
                    e.acked = Some(t);
                    e.by = by.to_string();
                }
                events.push(AlarmEvent::new("acked", id, &a.name, *k, on.severity, t));
            }
        }
        if !events.is_empty() {
            self.journal_version.fetch_add(1, Ordering::Relaxed);
            self.bump();
        }
        events
    }

    /// The alarm settings to follow from now on.
    pub fn set_settings(&self, s: AlarmSettings) {
        *write(&self.settings) = s;
    }

    /// The journal kept from before (the server starts).
    pub fn load_journal(&self, mut lines: Vec<JournalEntry>) {
        let t = now_ms();
        // Faults still on when the server stopped: over then.
        for e in lines.iter_mut().filter(|e| e.ended.is_none()) {
            e.ended = Some(t);
        }
        lines.truncate(JOURNAL);
        *write(&self.journal) = lines.into();
    }

    /// The journal, and a number changing with it (to save it).
    pub fn journal(&self) -> (u64, Vec<JournalEntry>) {
        (
            self.journal_version.load(Ordering::Relaxed),
            read(&self.journal).iter().cloned().collect(),
        )
    }

    pub fn journal_version(&self) -> u64 {
        self.journal_version.load(Ordering::Relaxed)
    }

    /// What PTP and NTP say now.
    pub fn set_sync(&self, ptp: PtpView, ntp: NtpView) {
        let mut s = write(&self.ptp);
        if s.0 != ptp || s.1 != ntp {
            *s = (ptp, ntp);
            self.bump();
        }
    }

    /// The plant's measures of the last minutes (once a second).
    pub fn set_curves(&self, c: strata_mv_core::signal::Curves) {
        let mut s = write(&self.curves);
        if s.end_ms != c.end_ms {
            *s = c;
            self.bump();
        }
    }

    /// PTP, NTP, the journal and the plant's measures.
    pub fn system(&self) -> SystemView {
        let (ptp, ntp) = read(&self.ptp).clone();
        SystemView {
            ptp,
            ntp,
            journal: read(&self.journal).iter().cloned().collect(),
            curves: read(&self.curves).clone(),
            sound: read(&self.settings).sound,
        }
    }

    /// Everything, for the browsers (inputs with nothing to say left out).
    pub fn snapshot(&self) -> BTreeMap<String, SignalView> {
        let mut out: BTreeMap<String, SignalView> = BTreeMap::new();
        for (id, t) in read(&self.tally).iter() {
            if !is_off(t) {
                out.entry(id.clone()).or_default().tally = t.clone();
            }
        }
        let round = |v: f32| (v * 10.0).round() / 10.0;
        for (id, m) in write(&self.audio).iter_mut() {
            let unsent = std::mem::take(&mut m.unsent);
            if m.at.elapsed() < GONE && !m.shown.is_empty() {
                let v = out.entry(id.clone()).or_default();
                v.levels_seq = unsent
                    .into_iter()
                    .map(|(t, l)| {
                        (
                            epoch_ms(t).round(),
                            l.iter()
                                .map(|l| [round(l.peak), round(l.rms), round(l.hold)])
                                .collect(),
                        )
                    })
                    .collect();
                v.levels = m
                    .shown
                    .iter()
                    .map(|l| [round(l.peak), round(l.rms), round(l.hold)])
                    .collect();
                v.levels_at = Some(epoch_ms(m.shown_at).round());
            }
        }
        for (id, l) in write(&self.loud).iter_mut() {
            if l.at.elapsed() < GONE {
                let n = l.now;
                let v = out.entry(id.clone()).or_default();
                v.loudness = Some([
                    round(n.momentary),
                    round(n.short_term),
                    round(n.integrated),
                    round(n.range),
                    round(n.true_peak),
                ]);
                if std::mem::take(&mut l.history_changed) {
                    v.history = Some(l.history.iter().copied().collect());
                }
            }
        }
        for (id, (tc, at)) in read(&self.timecodes).iter() {
            if at.elapsed() < TC_GONE {
                out.entry(id.clone()).or_default().timecode = tc.clone();
            }
        }
        for id in read(&self.active).keys() {
            let alarms = self.alarms(id);
            if !alarms.is_empty() {
                out.entry(id.clone()).or_default().alarms = alarms;
            }
        }
        out
    }

    /// Forgets what is known of inputs that are gone.
    pub fn forget_audio(&self, keep: impl Fn(&str) -> bool) {
        let mut changed = false;
        {
            let mut audio = write(&self.audio);
            let before = audio.len();
            audio.retain(|id, _| keep(id));
            changed |= audio.len() != before;
        }
        write(&self.loud).retain(|id, _| keep(id));
        write(&self.looks).retain(|id, _| keep(id));
        write(&self.timecodes).retain(|id, _| keep(id));
        if changed {
            self.bump();
        }
    }
}

/// An instant as ms since 1970.
fn epoch_ms(t: Instant) -> f64 {
    let now = Instant::now();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    if t >= now {
        ms + (t - now).as_secs_f64() * 1000.0
    } else {
        ms - (now - t).as_secs_f64() * 1000.0
    }
}

/// Marks the journal's line of a fault of input `id` as over at `t`.
fn end(journal: &mut VecDeque<JournalEntry>, id: &str, kind: AlarmKind, t: i64) {
    if let Some(e) = journal
        .iter_mut()
        .find(|e| e.input_id == id && e.kind == kind && e.ended.is_none())
    {
        e.ended = Some(t);
    }
}

/// Sends the signals to the browsers while they change (every 40 ms at
/// most, the meters' own rate), with when (they play them as late as the
/// pictures); looks at the faults, and PTP and NTP every two seconds.
pub async fn broadcast(s: crate::state::Shared) {
    let mut sent = u64::MAX;
    let mut last: BTreeMap<String, SignalView> = BTreeMap::new();
    let mut last_system = SystemView::default();
    let mut tick = tokio::time::interval(Duration::from_millis(40));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut n = 0u64;
    let mut videos: BTreeMap<String, strata_mv_core::signal::VideoInfo> = BTreeMap::new();
    let mut curves_sent = 0i64;
    let mut system_at = Instant::now();
    loop {
        tick.tick().await;
        n += 1;
        let signals = s.inputs.signals();
        // What the inputs are: once a second, sent when changed (all of it
        // every 5 s, for the pages just opened).
        let mut new_video = BTreeMap::new();
        if n % 25 == 2 {
            let all = n % 125 == 2;
            for (id, v) in s.inputs.videos() {
                if all || videos.get(&id) != Some(&v) {
                    new_video.insert(id.clone(), v.clone());
                }
                videos.insert(id, v);
            }
        }
        if n % 25 == 1 {
            signals.set_curves(s.perf.curves());
            let st = s.perf.sync.get();
            signals.set_sync(
                PtpView {
                    running: st.ptp.running,
                    locked: st.ptp.locked,
                    state: st.ptp.port_state.clone().unwrap_or_default(),
                    grandmaster: st.ptp.grandmaster.clone().unwrap_or_default(),
                    domain: st.ptp.domain,
                    offset_ns: st.ptp.offset_ns,
                },
                NtpView {
                    synced: st.ntp.synced,
                    source: st.ntp.source.clone().unwrap_or_default(),
                    offset_ms: st.ntp.offset_ms,
                },
            );
        }
        if n.is_multiple_of(6) {
            let sampled = s.perf.sampled();
            let fps: std::collections::HashMap<String, f64> = sampled
                .inputs
                .iter()
                .map(|(id, r)| (id.clone(), r.fps))
                .collect();
            let st = s.perf.sync.get();
            let m = &sampled.machine;
            let plant = Plant {
                ptp_running: st.ptp.running,
                ptp_locked: st.ptp.locked,
                ntp_synced: st.ntp.synced,
                cpu: sampled.cpu10,
                memory: if m.mem_total > 0 {
                    m.mem_used as f32 / m.mem_total as f32 * 100.0
                } else {
                    0.0
                },
                dropped: sampled
                    .outputs
                    .iter()
                    .map(|(k, r)| {
                        (
                            k.clone(),
                            k.rsplit('/').next().unwrap_or(k).to_string(),
                            r.count10,
                        )
                    })
                    .collect(),
            };
            let watched = s.inputs.watched(&fps);
            let events = signals.evaluate(&watched, &plant);
            s.alarms.dispatch(events);
        }
        // The journal saved now and then.
        if n % 125 == 7 {
            let s2 = s.clone();
            tokio::task::spawn_blocking(move || s2.alarms.save_journal(s2.inputs.signals()));
        }
        let v = signals.version();
        if v == sent && new_video.is_empty() {
            continue;
        }
        sent = v;
        // The curves go once a second, when they change, not with every
        // meter (the pages keep the last ones).
        let mut system = signals.system();
        let curves = std::mem::take(&mut system.curves);
        let new_curves = curves.end_ms != curves_sent;
        let mut now = signals.snapshot();
        for (id, vi) in new_video {
            now.entry(id).or_default().video = Some(vi);
        }
        // The inputs on their backup: their tiles show it.
        for (id, b) in s.inputs.failovers() {
            now.entry(id).or_default().backup = Some(b);
        }
        // With the meters: the inputs heard.
        for (id, c) in s.inputs.correlations() {
            if let Some(v) = now.get_mut(&id).filter(|v| !v.levels.is_empty()) {
                v.correlation = Some((c * 100.0).round() / 100.0);
            }
        }
        // The same as sent, a history aside (sent when it changes only).
        let new_history = now
            .values()
            .any(|v| v.history.is_some() || v.video.is_some());
        let mut bare = now.clone();
        for v in bare.values_mut() {
            v.history = None;
            v.video = None;
        }
        let system_changed = system != last_system || system_at.elapsed() > Duration::from_secs(5);
        if new_history || bare != last || system_changed || new_curves {
            if system_changed {
                system_at = Instant::now();
            }
            let out = (system_changed || new_curves).then(|| {
                let mut out = system.clone();
                if new_curves {
                    curves_sent = curves.end_ms;
                    out.curves = curves;
                }
                out
            });
            let _ = s.events.send(crate::state::Event::Signals {
                sources: now,
                system: out,
                at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |d| d.as_secs_f64() * 1000.0),
            });
            last = bare;
            last_system = system;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(db: f32) -> Level {
        Level {
            peak: db,
            rms: db,
            hold: db,
        }
    }

    /// A picture `w` wide of vertical bars of these colours, the lower third grey.
    fn bars(w: u32, colours: &[[u8; 3]]) -> tiny_skia::Pixmap {
        let h = w * 9 / 16;
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        let d = pm.data_mut();
        for y in 0..h as usize {
            for x in 0..w as usize {
                let c = if y > (h as usize) * 2 / 3 {
                    [40, 40, 40]
                } else {
                    colours[x * colours.len() / w as usize]
                };
                let p = (y * w as usize + x) * 4;
                d[p..p + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        pm
    }

    #[test]
    fn colour_bars_are_told_apart() {
        // SMPTE 75 %: white, yellow, cyan, green, magenta, red, blue.
        let smpte = [
            [191, 191, 191],
            [191, 191, 0],
            [0, 191, 191],
            [0, 191, 0],
            [191, 0, 191],
            [191, 0, 0],
            [0, 0, 191],
        ];
        assert!(is_bars(&bars(1280, &smpte)));
        // HD bars: grey sides.
        let mut hd = vec![[104, 104, 104]];
        hd.extend_from_slice(&smpte);
        hd.push([104, 104, 104]);
        assert!(is_bars(&bars(1920, &hd)));
        // A picture with some of those colours, not in that order: not bars.
        assert!(!is_bars(&bars(
            1280,
            &[[0, 0, 191], [191, 0, 0], [0, 191, 0], [191, 191, 0]]
        )));
        // Black: not bars.
        assert!(!is_bars(&tiny_skia::Pixmap::new(640, 360).unwrap()));
    }

    /// ffmpeg's own test patterns (skipped without ffmpeg).
    #[test]
    fn ffmpeg_bars_are_bars_and_other_patterns_not() {
        let frame = |src: &str| {
            let out = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    src,
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "rgba",
                    "-f",
                    "rawvideo",
                    "-",
                ])
                .output()
                .ok()
                .filter(|o| o.status.success())?;
            tiny_skia::Pixmap::from_vec(out.stdout, tiny_skia::IntSize::from_wh(640, 360)?)
        };
        for (src, want) in [
            ("smptebars=size=640x360", true),
            ("smptehdbars=size=640x360", true),
            ("pal75bars=size=640x360", true),
            ("pal100bars=size=640x360", true),
            ("testsrc2=size=640x360", false),
            ("mandelbrot=size=640x360", false),
            ("color=c=0x2040a0:size=640x360", false),
        ] {
            let Some(pm) = frame(src) else { return };
            assert_eq!(is_bars(&pm), want, "{src}");
        }
    }

    /// A look of a picture still since `still_since`, which moved at `moved`.
    fn look(still_since: Instant, moved: &[Instant]) -> Look {
        Look {
            print: [0; PRINT],
            still_since,
            moved: moved.iter().copied().collect(),
            exact_since: None,
            noisy_before: false,
            last_exact: None,
            still_pictures: 250,
            still_exact: 240,
            black_since: None,
            still_kind: false,
        }
    }

    #[test]
    fn a_picture_still_from_the_start_is_not_frozen() {
        // A slate, a still graphic: never moved.
        let t0 = Instant::now();
        let l = look(t0, &[]);
        assert_eq!(Signals::frozen(&l, t0 + Duration::from_secs(120)), None);
    }

    #[test]
    fn a_picture_that_moved_then_stopped_is_frozen() {
        let t0 = Instant::now();
        let moved: Vec<Instant> = (0..6)
            .map(|i| t0 + Duration::from_millis(500 * i))
            .collect();
        let stop = t0 + Duration::from_millis(2500);
        let l = look(stop, &moved);
        // Not yet after 5 s; frozen after 10 s, since it stopped.
        assert_eq!(Signals::frozen(&l, stop + Duration::from_secs(5)), None);
        assert_eq!(
            Signals::frozen(&l, stop + Duration::from_secs(11)),
            Some(stop)
        );
        // The same on colour bars or a plain picture: never.
        let bars = Look {
            still_kind: true,
            ..look(stop, &moved)
        };
        assert_eq!(Signals::frozen(&bars, stop + Duration::from_secs(60)), None);
        // Black is its own fault.
        let black = Look {
            black_since: Some(stop),
            ..look(stop, &moved)
        };
        assert_eq!(
            Signals::frozen(&black, stop + Duration::from_secs(60)),
            None
        );
        // It moved once only, long before: a still shot now, not frozen.
        let once = look(stop, &[t0]);
        assert_eq!(Signals::frozen(&once, stop + Duration::from_secs(60)), None);
        // A calm live shot (an empty set): no motion, but its noise; not frozen.
        let calm = Look {
            still_exact: 10,
            ..look(stop, &moved)
        };
        assert_eq!(Signals::frozen(&calm, stop + Duration::from_secs(60)), None);
    }

    #[test]
    fn a_live_picture_repeated_to_the_pixel_is_frozen_sooner() {
        // A still shot of a camera (noise, no motion), then the same picture to the pixel.
        let t0 = Instant::now();
        let exact = t0 + Duration::from_secs(20);
        let l = Look {
            exact_since: Some(exact),
            noisy_before: true,
            ..look(t0, &[])
        };
        assert_eq!(Signals::frozen(&l, exact + Duration::from_secs(2)), None);
        assert_eq!(
            Signals::frozen(&l, exact + Duration::from_secs(5)),
            Some(exact)
        );
        // A computer picture is always the same to the pixel: no noise before, not frozen.
        let still = Look {
            noisy_before: false,
            ..l
        };
        assert_eq!(
            Signals::frozen(&still, exact + Duration::from_secs(60)),
            None
        );
    }

    #[test]
    fn colour_bars_never_raise_an_alarm() {
        let s = Signals::default();
        let smpte = [
            [191, 191, 191],
            [191, 191, 0],
            [0, 191, 191],
            [0, 191, 0],
            [191, 0, 191],
            [191, 0, 0],
            [0, 0, 191],
        ];
        let pm = bars(640, &smpte);
        for _ in 0..3 {
            s.picture("a", &pm);
        }
        {
            // As if they had moved before and stood for a minute.
            let mut looks = write(&s.looks);
            let l = looks.get_mut("a").unwrap();
            let past = Instant::now() - Duration::from_secs(60);
            l.still_since = past;
            l.moved = (0..6).map(|i| past - Duration::from_secs(i)).collect();
            assert!(l.still_kind);
        }
        s.evaluate(&[w("a", "A", true, None)], &Plant::default());
        assert!(s.alarms("a").is_empty(), "{:?}", s.alarms("a"));
    }

    #[test]
    fn meters_rise_at_once_and_hold_their_peak() {
        let s = Signals::default();
        s.measure("a", &[level(-6.0), level(-40.0)]);
        let l = s.levels("a");
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].peak, -6.0);
        s.measure("a", &[level(-30.0), level(-40.0)]);
        let l = s.levels("a");
        // Fallen a little only, the hold still at the peak.
        assert!(l[0].peak > -7.0, "{l:?}");
        assert_eq!(l[0].hold, -6.0);
        assert!(s.levels("b").is_empty());
        let snap = s.snapshot();
        assert_eq!(snap["a"].levels.len(), 2);
    }

    #[test]
    fn the_tally_is_sent_when_it_changes() {
        let s = Signals::default();
        let v = s.version();
        let mut all = BTreeMap::new();
        all.insert(
            "cam1".to_string(),
            Tally {
                program: true,
                ..Default::default()
            },
        );
        s.set_tallies(all.clone());
        assert_ne!(s.version(), v);
        let v = s.version();
        s.set_tallies(all);
        assert_eq!(s.version(), v);
        assert!(s.tally("cam1").program);
        assert!(s.snapshot().contains_key("cam1"));
    }

    fn w(id: &str, name: &str, fresh: bool, lost_at: Option<Instant>) -> Watched {
        Watched {
            id: id.into(),
            name: name.into(),
            fresh,
            lost_at,
            fps: None,
            nominal: None,
            restarts: 0,
            correlation: None,
            captions: false,
            ts: (false, false),
            video: Default::default(),
            on_backup: false,
        }
    }

    #[test]
    fn a_profile_says_which_faults_count_after_how_long_and_how_grave() {
        use strata_mv_core::alarm::{default_rule, Profile, Rule};
        let s = Signals::default();
        let mut set = AlarmSettings::default();
        let mut p = Profile {
            id: "feed".into(),
            name: "Feed".into(),
            ..Default::default()
        };
        p.rules.insert(
            AlarmKind::Loss,
            Rule {
                after: 60.0,
                ..default_rule(AlarmKind::Loss)
            },
        );
        p.rules.insert(
            AlarmKind::Fps,
            Rule {
                severity: Severity::Critical,
                after: 0.0,
                ..default_rule(AlarmKind::Fps)
            },
        );
        set.profiles.push(p);
        set.inputs.insert("feed".into(), "feed".into());
        s.set_settings(set);
        let long_ago = Instant::now() - Duration::from_secs(30);
        let mut slow = w("cam", "CAM", true, None);
        slow.fps = Some(12.5);
        slow.nominal = Some(25.0);
        let mut feed_slow = w("feed2", "F2", true, None);
        feed_slow.fps = Some(25.0);
        feed_slow.nominal = Some(25.0);
        let events = s.evaluate(
            &[w("feed", "FEED", false, Some(long_ago)), slow, feed_slow],
            &Plant::default(),
        );
        // Lost for 30 s, raised after 60 s only: nothing yet.
        assert!(s.alarms("feed").is_empty());
        // Half its rate, the default profile: raised after 5 s (not yet).
        assert!(s.alarms("cam").is_empty());
        assert!(events.is_empty());
        // Its condition began 6 s ago.
        s.conds.lock().unwrap().insert(
            ("cam".into(), AlarmKind::Fps),
            Instant::now() - Duration::from_secs(6),
        );
        let mut slow = w("cam", "CAM", true, None);
        slow.fps = Some(12.5);
        slow.nominal = Some(25.0);
        let events = s.evaluate(&[slow], &Plant::default());
        assert_eq!(s.alarms("cam")[0].kind, AlarmKind::Fps);
        assert_eq!(s.alarms("cam")[0].severity, Severity::Warning);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, "raised");
    }

    #[test]
    fn colours_out_of_the_norms() {
        use strata_mv_core::signal::VideoInfo;
        let s = Signals::default();
        let mut hdr = w("uhd", "UHD", true, None);
        hdr.video = VideoInfo {
            pix_fmt: "yuv420p10le".into(),
            primaries: "bt2020".into(),
            transfer: "smpte2084".into(),
            range: "tv".into(),
            illegal: Some(4.0),
            ..Default::default()
        };
        s.evaluate(&[hdr], &Plant::default());
        let on: Vec<AlarmKind> = s
            .conds
            .lock()
            .unwrap()
            .keys()
            .filter(|(id, _)| id == "uhd")
            .map(|(_, k)| *k)
            .collect();
        // Not BT.709 SDR (the default expected), levels out of range, no
        // HDR metadata; limited range and colours carried: fine.
        for k in [
            AlarmKind::ColorSpace,
            AlarmKind::IllegalLevels,
            AlarmKind::HdrMetadata,
        ] {
            assert!(on.contains(&k), "{k:?} {on:?}");
        }
        for k in [AlarmKind::FullRange, AlarmKind::NoColorInfo] {
            assert!(!on.contains(&k), "{k:?}");
        }
    }

    #[test]
    fn acknowledged_faults_say_who_saw_them() {
        let s = Signals::default();
        let long_ago = Instant::now() - Duration::from_secs(30);
        s.evaluate(
            &[w("cam", "CAM 1", false, Some(long_ago))],
            &Plant::default(),
        );
        assert!(!s.alarms("cam")[0].acked);
        let ev = s.ack(Some("cam"), None, "Régie");
        assert_eq!(ev.len(), 1);
        assert!(s.alarms("cam")[0].acked);
        let j = s.system().journal;
        assert_eq!(j[0].by, "Régie");
        assert!(j[0].acked.is_some());
        // Once only.
        assert!(s.ack(None, None, "x").is_empty());
    }

    #[test]
    fn the_plant_has_faults_too() {
        let s = Signals::default();
        let plant = Plant {
            ptp_running: true,
            ptp_locked: false,
            cpu: 97.0,
            dropped: vec![("main/pgm/ndi1".into(), "ndi1".into(), 12.0)],
            ..Default::default()
        };
        s.evaluate(&[], &plant);
        // PTP after 5 s, the processors after 10 s: as if begun long ago.
        for (id, k) in [
            ("~ptp", AlarmKind::PtpUnlocked),
            ("~cpu", AlarmKind::CpuHigh),
        ] {
            s.conds
                .lock()
                .unwrap()
                .insert((id.into(), k), Instant::now() - Duration::from_secs(20));
        }
        s.evaluate(&[], &plant);
        assert_eq!(s.alarms("~ptp")[0].kind, AlarmKind::PtpUnlocked);
        assert_eq!(s.alarms("~ptp")[0].severity, Severity::Critical);
        assert_eq!(s.alarms("~cpu")[0].kind, AlarmKind::CpuHigh);
        assert_eq!(
            s.alarms("~output:main/pgm/ndi1")[0].kind,
            AlarmKind::OutputDropped
        );
        // Fine again: over.
        s.evaluate(&[], &Plant::default());
        assert!(s.alarms("~ptp").is_empty());
        assert!(s.system().journal.iter().all(|e| e.ended.is_some()));
    }

    #[test]
    fn faults_go_to_the_journal_and_end_there() {
        let s = Signals::default();
        let long_ago = Instant::now() - Duration::from_secs(30);
        // No picture: a loss.
        s.evaluate(
            &[w("cam", "CAM 1", false, Some(long_ago))],
            &Plant::default(),
        );
        let a = s.alarms("cam");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].kind, AlarmKind::Loss);
        let j = s.system().journal;
        assert_eq!(
            (j.len(), j[0].input.as_str(), j[0].ended),
            (1, "CAM 1", None)
        );
        // The picture is back: over.
        s.evaluate(&[w("cam", "CAM 1", true, None)], &Plant::default());
        assert!(s.alarms("cam").is_empty());
        assert!(s.system().journal[0].ended.is_some());
    }

    #[test]
    fn black_and_frozen_pictures_are_told() {
        let black = tiny_skia::Pixmap::new(64, 36).unwrap();
        let (_, is_black, _) = print_of(&black);
        assert!(is_black);
        let mut grey = tiny_skia::Pixmap::new(64, 36).unwrap();
        grey.fill(tiny_skia::Color::from_rgba8(128, 128, 128, 255));
        let (p, is_black, plain) = print_of(&grey);
        assert!(!is_black && p[0] > 100);
        // One colour: still by nature.
        assert!(plain);
    }

    #[test]
    fn loudness_keeps_one_value_a_second() {
        let s = Signals::default();
        let l = Loudness {
            momentary: -20.0,
            short_term: -22.0,
            integrated: -23.0,
            range: 5.0,
            true_peak: -3.0,
        };
        s.loudness("a", l);
        s.loudness("a", l);
        let (now, history) = s.loudness_of("a");
        assert_eq!(now, Some(l));
        assert_eq!(history, vec![-22.0]);
        let snap = s.snapshot();
        assert_eq!(snap["a"].loudness, Some([-20.0, -22.0, -23.0, 5.0, -3.0]));
        assert_eq!(snap["a"].history.as_deref(), Some(&[-22.0][..]));
        // Sent once.
        assert!(s.snapshot()["a"].history.is_none());
    }
}
