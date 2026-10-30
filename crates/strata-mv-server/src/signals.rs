//! What the inputs carry besides their pictures, by input id: the tally
//! and UMD the switchers send (`crate::tally`), the levels of their audio
//! (measured by `crate::inputs`). The engine reads them through
//! `Inputs::source`; the browsers get them in `signals` events (a dozen a
//! second while they change).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use strata_mv_core::signal::{Level, Tally, SILENCE};

/// The hold mark stays this long on a peak, then falls.
const HOLD: Duration = Duration::from_secs(2);
/// How fast a meter falls back, dB a second (peak meters fall slowly).
const FALL: f32 = 24.0;
/// Without a measure for this long, the input has no audio (no meters).
const GONE: Duration = Duration::from_secs(3);

struct Meter {
    shown: Vec<Level>,
    /// When each channel's hold mark was set.
    held: Vec<Instant>,
    at: Instant,
}

/// The signals of every input.
#[derive(Default)]
pub struct Signals {
    tally: RwLock<BTreeMap<String, Tally>>,
    audio: RwLock<BTreeMap<String, Meter>>,
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
}

fn is_off(t: &Tally) -> bool {
    *t == Tally::default()
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
        let mut t = self.tally.write().unwrap_or_else(|p| p.into_inner());
        if *t != all {
            *t = all;
            self.bump();
        }
    }

    pub fn tally(&self, id: &str) -> Tally {
        self.tally
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// A new measure of an input's audio: the meters rise at once, fall
    /// at [`FALL`]; the hold mark keeps the highest peak for [`HOLD`].
    pub fn measure(&self, id: &str, levels: &[Level]) {
        let now = Instant::now();
        let mut audio = self.audio.write().unwrap_or_else(|p| p.into_inner());
        let m = audio.entry(id.to_string()).or_insert_with(|| Meter {
            shown: Vec::new(),
            held: Vec::new(),
            at: now,
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
        self.bump();
    }

    /// The meters of an input (none: no audio lately).
    pub fn levels(&self, id: &str) -> Vec<Level> {
        let audio = self.audio.read().unwrap_or_else(|p| p.into_inner());
        match audio.get(id) {
            Some(m) if m.at.elapsed() < GONE => m.shown.clone(),
            _ => Vec::new(),
        }
    }

    /// Everything, for the browsers (inputs with neither tally nor audio left out).
    pub fn snapshot(&self) -> BTreeMap<String, SignalView> {
        let mut out: BTreeMap<String, SignalView> = BTreeMap::new();
        for (id, t) in self.tally.read().unwrap_or_else(|p| p.into_inner()).iter() {
            if !is_off(t) {
                out.entry(id.clone()).or_default().tally = t.clone();
            }
        }
        let round = |v: f32| (v * 10.0).round() / 10.0;
        for (id, m) in self.audio.read().unwrap_or_else(|p| p.into_inner()).iter() {
            if m.at.elapsed() < GONE && !m.shown.is_empty() {
                out.entry(id.clone()).or_default().levels = m
                    .shown
                    .iter()
                    .map(|l| [round(l.peak), round(l.rms), round(l.hold)])
                    .collect();
            }
        }
        out
    }

    /// Forgets the audio of inputs that are gone.
    pub fn forget_audio(&self, keep: impl Fn(&str) -> bool) {
        let mut audio = self.audio.write().unwrap_or_else(|p| p.into_inner());
        let before = audio.len();
        audio.retain(|id, _| keep(id));
        if audio.len() != before {
            self.bump();
        }
    }
}

/// Sends the signals to the browsers while they change (every 80 ms at most).
pub async fn broadcast(s: crate::state::Shared) {
    let mut sent = u64::MAX;
    let mut last = BTreeMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(80));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let signals = s.inputs.signals();
        let v = signals.version();
        if v == sent {
            continue;
        }
        sent = v;
        let now = signals.snapshot();
        if now != last {
            let _ = s.events.send(crate::state::Event::Signals {
                sources: now.clone(),
            });
            last = now;
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
}
