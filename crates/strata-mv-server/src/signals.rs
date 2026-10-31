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
use strata_mv_core::signal::{
    Alarm, AlarmKind, JournalEntry, Level, Loudness, NtpView, PtpView, SystemView, Tally,
    MAX_TRUE_PEAK, SILENCE, TARGET,
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

/// How long a fault lasts before it is one.
const BLACK_AFTER: Duration = Duration::from_secs(3);
const FREEZE_AFTER: Duration = Duration::from_secs(4);
const SILENCE_AFTER: Duration = Duration::from_secs(5);
const LOUD_AFTER: Duration = Duration::from_secs(10);
/// Louder than the target by this much, LU, for [`LOUD_AFTER`].
const LOUD_BY: f32 = 3.0;
/// A true peak over the limit stays a fault this long.
const PEAK_FOR: Duration = Duration::from_secs(3);
/// Silent: every channel under this, dBFS.
const SILENT: f32 = -60.0;
/// Lines of the journal kept.
const JOURNAL: usize = 200;

struct Meter {
    shown: Vec<Level>,
    /// When each channel's hold mark was set.
    held: Vec<Instant>,
    at: Instant,
    /// Since when every channel is silent.
    silent_since: Option<Instant>,
}

struct Loud {
    now: Loudness,
    at: Instant,
    history: VecDeque<f32>,
    /// When the last value went into the history.
    sampled: Instant,
    /// Since when it is too loud.
    loud_since: Option<Instant>,
    /// When the true peak last went over the limit.
    peak_at: Option<Instant>,
    /// The history changed since the browsers had it.
    history_changed: bool,
}

/// What the picture of an input looks like, to tell black and frozen ones.
struct Look {
    /// 8 × 8 luma samples of the last picture.
    print: [u8; 64],
    /// Since when it has not changed.
    same_since: Instant,
    /// Since when it is black.
    black_since: Option<Instant>,
}

/// The faults on an input now, since when (ms since 1970), and its name
/// (the journal's lines say it).
#[derive(Default)]
struct Active {
    kinds: BTreeMap<AlarmKind, i64>,
    name: String,
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
fn print_of(pm: &tiny_skia::Pixmap) -> ([u8; 64], bool) {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let data = pm.data();
    let mut out = [0u8; 64];
    let (mut sum, mut max) = (0u32, 0u8);
    for (i, o) in out.iter_mut().enumerate() {
        let (gx, gy) = (i % 8, i / 8);
        let x = (gx * 2 + 1) * w / 16;
        let y = (gy * 2 + 1) * h / 16;
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
    }
    // Under 4 % on average, nothing above 12 %.
    (out, sum / 64 < 10 && max < 31)
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
        let now = Instant::now();
        let mut audio = write(&self.audio);
        let m = audio.entry(id.to_string()).or_insert_with(|| Meter {
            shown: Vec::new(),
            held: Vec::new(),
            at: now,
            silent_since: None,
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
        if levels.iter().all(|l| l.peak < SILENT) {
            m.silent_since.get_or_insert(now);
        } else {
            m.silent_since = None;
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
            loud_since: None,
            peak_at: None,
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
        if l.short_term > TARGET + LOUD_BY {
            e.loud_since.get_or_insert(now);
        } else {
            e.loud_since = None;
        }
        if l.true_peak > MAX_TRUE_PEAK {
            e.peak_at = Some(now);
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
        let (print, black) = print_of(pm);
        let now = Instant::now();
        let mut looks = write(&self.looks);
        let l = looks.entry(id.to_string()).or_insert(Look {
            print,
            same_since: now,
            black_since: None,
        });
        // Changed: more than a slight difference somewhere (noise of a
        // still picture coded again stays under it).
        if l.print
            .iter()
            .zip(print.iter())
            .any(|(a, b)| a.abs_diff(*b) > 2)
        {
            l.same_since = now;
        }
        l.print = print;
        if black {
            l.black_since.get_or_insert(now);
        } else {
            l.black_since = None;
        }
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
                    .map(|(k, since)| Alarm {
                        kind: *k,
                        severity: k.severity(),
                        since: *since,
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.kind.cmp(&b.kind)));
        out
    }

    /// Looks at the faults of every input again (`inputs`: id, name, a
    /// picture came lately, since when none did); new ones and the ones
    /// over go to the journal.
    pub fn evaluate(&self, inputs: &[(String, String, bool, Option<Instant>)]) {
        let now = Instant::now();
        let over = |since: Option<Instant>, d: Duration| {
            since.filter(|s| now.saturating_duration_since(*s) >= d)
        };
        let mut found: BTreeMap<String, BTreeMap<AlarmKind, Instant>> = BTreeMap::new();
        {
            let looks = read(&self.looks);
            let audio = read(&self.audio);
            let loud = read(&self.loud);
            for (id, _, fresh, lost_at) in inputs {
                let f = found.entry(id.clone()).or_default();
                if !fresh {
                    // No picture: the other faults of the picture do not count.
                    f.insert(AlarmKind::Loss, lost_at.unwrap_or(now));
                } else if let Some(l) = looks.get(id) {
                    if let Some(s) = over(l.black_since, BLACK_AFTER) {
                        f.insert(AlarmKind::Black, s);
                    } else if let Some(s) = over(Some(l.same_since), FREEZE_AFTER) {
                        f.insert(AlarmKind::Freeze, s);
                    }
                }
                if let Some(m) = audio.get(id).filter(|m| m.at.elapsed() < GONE) {
                    if let Some(s) = over(m.silent_since, SILENCE_AFTER) {
                        f.insert(AlarmKind::Silence, s);
                    }
                }
                if let Some(l) = loud.get(id).filter(|l| l.at.elapsed() < GONE) {
                    if let Some(s) = over(l.loud_since, LOUD_AFTER) {
                        f.insert(AlarmKind::Loud, s);
                    }
                    if let Some(p) = l
                        .peak_at
                        .filter(|p| now.saturating_duration_since(*p) < PEAK_FOR)
                    {
                        f.insert(AlarmKind::Peak, p);
                    }
                }
            }
        }
        let names: BTreeMap<&str, &str> = inputs
            .iter()
            .map(|(id, n, _, _)| (id.as_str(), n.as_str()))
            .collect();
        let t = now_ms();
        let mut changed = false;
        let mut active = write(&self.active);
        let mut journal = write(&self.journal);
        // Inputs gone: their faults end.
        let gone: Vec<String> = active
            .keys()
            .filter(|id| !found.contains_key(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(a) = active.remove(&id) {
                for k in a.kinds.keys() {
                    end(&mut journal, &a.name, *k, t);
                }
                changed = true;
            }
        }
        for (id, kinds) in found {
            let a = active.entry(id.clone()).or_default();
            let name = names.get(id.as_str()).copied().unwrap_or("");
            a.name = name.to_string();
            let over: Vec<AlarmKind> = a
                .kinds
                .keys()
                .filter(|k| !kinds.contains_key(k))
                .copied()
                .collect();
            for k in over {
                a.kinds.remove(&k);
                end(&mut journal, name, k, t);
                changed = true;
            }
            for (k, since) in kinds {
                if a.kinds.contains_key(&k) {
                    continue;
                }
                let at = ms_of(since);
                a.kinds.insert(k, at);
                journal.push_front(JournalEntry {
                    at,
                    input: name.to_string(),
                    kind: k,
                    severity: k.severity(),
                    ended: None,
                });
                journal.truncate(JOURNAL);
                changed = true;
            }
        }
        if changed {
            self.bump();
        }
    }

    /// What PTP and NTP say now.
    pub fn set_sync(&self, ptp: PtpView, ntp: NtpView) {
        let mut s = write(&self.ptp);
        if s.0 != ptp || s.1 != ntp {
            *s = (ptp, ntp);
            self.bump();
        }
    }

    /// PTP, NTP and the journal.
    pub fn system(&self) -> SystemView {
        let (ptp, ntp) = read(&self.ptp).clone();
        SystemView {
            ptp,
            ntp,
            journal: read(&self.journal).iter().cloned().collect(),
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
        for (id, m) in read(&self.audio).iter() {
            if m.at.elapsed() < GONE && !m.shown.is_empty() {
                out.entry(id.clone()).or_default().levels = m
                    .shown
                    .iter()
                    .map(|l| [round(l.peak), round(l.rms), round(l.hold)])
                    .collect();
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

/// Marks the journal's line of a fault of `input` as over at `t`.
fn end(journal: &mut VecDeque<JournalEntry>, input: &str, kind: AlarmKind, t: i64) {
    if let Some(e) = journal
        .iter_mut()
        .find(|e| e.input == input && e.kind == kind && e.ended.is_none())
    {
        e.ended = Some(t);
    }
}

/// Sends the signals to the browsers while they change (every 80 ms at
/// most); looks at the faults, and PTP and NTP every two seconds.
pub async fn broadcast(s: crate::state::Shared) {
    let mut sent = u64::MAX;
    let mut last: BTreeMap<String, SignalView> = BTreeMap::new();
    let mut last_system = SystemView::default();
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut n = 0u64;
    loop {
        tick.tick().await;
        n += 1;
        let signals = s.inputs.signals();
        if n % 25 == 1 {
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
        if n.is_multiple_of(3) {
            signals.evaluate(&s.inputs.watched());
        }
        let v = signals.version();
        if v == sent {
            continue;
        }
        sent = v;
        let now = signals.snapshot();
        let system = signals.system();
        // The same as sent, a history aside (sent when it changes only).
        let new_history = now.values().any(|v| v.history.is_some());
        let mut bare = now.clone();
        for v in bare.values_mut() {
            v.history = None;
        }
        if new_history || bare != last || system != last_system {
            let _ = s.events.send(crate::state::Event::Signals {
                sources: now,
                system: system.clone(),
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

    #[test]
    fn faults_go_to_the_journal_and_end_there() {
        let s = Signals::default();
        let long_ago = Instant::now() - Duration::from_secs(30);
        // No picture: a loss.
        s.evaluate(&[("cam".into(), "CAM 1".into(), false, Some(long_ago))]);
        let a = s.alarms("cam");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].kind, AlarmKind::Loss);
        let j = s.system().journal;
        assert_eq!(
            (j.len(), j[0].input.as_str(), j[0].ended),
            (1, "CAM 1", None)
        );
        // The picture is back: over.
        s.evaluate(&[("cam".into(), "CAM 1".into(), true, None)]);
        assert!(s.alarms("cam").is_empty());
        assert!(s.system().journal[0].ended.is_some());
    }

    #[test]
    fn black_and_frozen_pictures_are_told() {
        let black = tiny_skia::Pixmap::new(64, 36).unwrap();
        let (_, is_black) = print_of(&black);
        assert!(is_black);
        let mut grey = tiny_skia::Pixmap::new(64, 36).unwrap();
        grey.fill(tiny_skia::Color::from_rgba8(128, 128, 128, 255));
        let (p, is_black) = print_of(&grey);
        assert!(!is_black && p[0] > 100);
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
