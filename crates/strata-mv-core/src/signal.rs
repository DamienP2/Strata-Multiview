//! What a source carries besides its picture, as the multiview shows it:
//! its tally lamps and UMD text (from the switcher: TSL, ATEM, GPI, the
//! API), the levels and the loudness of its audio, its timecode and the
//! faults the server sees on it; and the state of the plant (PTP, NTP, the
//! alarm journal). The server keeps them up to date (strata-mv-server
//! signals.rs); the widgets draw them, in the engine and in the browser
//! (`web/src/engine/liveInputs.ts`).

use serde::{Deserialize, Serialize};

/// The level of silence, dBFS (no sample, or below the meters' scale).
pub const SILENCE: f32 = -120.0;

/// One audio channel, dBFS (0 = full scale; [`SILENCE`] when nothing).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// Highest sample of the last measure.
    pub peak: f32,
    /// Mean power of the last measure.
    pub rms: f32,
    /// Highest peak of the last seconds (the meter's hold mark).
    pub hold: f32,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            peak: SILENCE,
            rms: SILENCE,
            hold: SILENCE,
        }
    }
}

impl Level {
    /// The level of samples (-1…1): peak and mean power, dBFS.
    pub fn of(samples: impl Iterator<Item = f32>) -> Self {
        let (mut peak, mut sum, mut n) = (0f32, 0f64, 0usize);
        for s in samples {
            let a = s.abs();
            if a.is_finite() {
                peak = peak.max(a);
                sum += (s as f64) * (s as f64);
                n += 1;
            }
        }
        let rms = if n > 0 {
            (sum / n as f64).sqrt() as f32
        } else {
            0.0
        };
        let peak = db(peak);
        Self {
            peak,
            rms: db(rms),
            hold: peak,
        }
    }
}

/// A linear amplitude (1 = full scale) in dBFS, [`SILENCE`] at most.
pub fn db(a: f32) -> f32 {
    if a > 0.0 && a.is_finite() {
        (20.0 * a.log10()).max(SILENCE)
    } else {
        SILENCE
    }
}

/// The tally lamps of a source and the text of its UMD.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tally {
    /// On air (red).
    pub program: bool,
    /// Next (green).
    pub preview: bool,
    /// A third lamp (yellow): an ISO record, an aux bus…
    pub aux: bool,
    /// The name the switcher gives it ("" = none received).
    pub umd: String,
}

/// EBU R128 loudness of a source's programme.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loudness {
    /// Momentary (400 ms) and short-term (3 s) loudness, LUFS.
    pub momentary: f32,
    pub short_term: f32,
    /// Since the measure began, LUFS.
    pub integrated: f32,
    /// Loudness range, LU.
    pub range: f32,
    /// Highest true peak of the last seconds, dBTP.
    pub true_peak: f32,
}

/// The loudness target of EBU R128, LUFS, and the tolerance either side.
pub const TARGET: f32 = -23.0;
pub const TOLERANCE: f32 = 1.0;
/// The true peak not to go over, dBTP.
pub const MAX_TRUE_PEAK: f32 = -1.0;

/// A fault the server sees on a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlarmKind {
    /// No picture comes.
    Loss,
    /// The picture is black.
    Black,
    /// The picture does not change.
    Freeze,
    /// Every audio channel is silent.
    Silence,
    /// Louder than the target for a while.
    Loud,
    /// The true peak goes over the limit.
    Peak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Warning,
    Critical,
}

impl AlarmKind {
    pub fn severity(self) -> Severity {
        match self {
            AlarmKind::Loss | AlarmKind::Freeze | AlarmKind::Black => Severity::Critical,
            _ => Severity::Warning,
        }
    }

    /// The words on the tile and in the journal, in the project's language.
    pub fn text(self, lang: &str) -> &'static str {
        let fr = lang.to_lowercase().starts_with("fr");
        match (self, fr) {
            (AlarmKind::Loss, true) => "PERTE DU SIGNAL",
            (AlarmKind::Loss, false) => "SIGNAL LOSS",
            (AlarmKind::Black, true) => "NOIR",
            (AlarmKind::Black, false) => "BLACK",
            (AlarmKind::Freeze, true) => "IMAGE FIGÉE",
            (AlarmKind::Freeze, false) => "FROZEN PICTURE",
            (AlarmKind::Silence, true) => "SILENCE",
            (AlarmKind::Silence, false) => "SILENCE",
            (AlarmKind::Loud, true) => "TROP FORT",
            (AlarmKind::Loud, false) => "TOO LOUD",
            (AlarmKind::Peak, true) => "TRUE PEAK",
            (AlarmKind::Peak, false) => "TRUE PEAK",
        }
    }
}

/// A fault on now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alarm {
    pub kind: AlarmKind,
    pub severity: Severity,
    /// Since when, ms since 1970.
    pub since: i64,
}

/// Everything the Source widget shows of a live input.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceView {
    /// The input's name (Video inputs page).
    pub name: String,
    /// "1080i50" once a picture comes ("" until then).
    pub format: String,
    /// A picture came in the last two seconds.
    pub live: bool,
    pub tally: Tally,
    /// One per channel of its audio, in order (none: no audio).
    pub levels: Vec<Level>,
    /// Its programme loudness (none: no audio, or not measured).
    pub loudness: Option<Loudness>,
    /// Its short-term loudness, one a second, oldest first (two minutes at most).
    pub history: Vec<f32>,
    /// The timecode it carries, "HH:MM:SS:FF" ("" = none).
    pub timecode: String,
    /// The faults on it now, the worst first.
    pub alarms: Vec<Alarm>,
}

/// PTP, as linuxptp tells it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PtpView {
    /// ptp4l answers.
    pub running: bool,
    /// Following a grandmaster.
    pub locked: bool,
    /// SLAVE, MASTER, LISTENING…
    pub state: String,
    pub grandmaster: String,
    pub domain: u8,
    /// From the master, ns.
    pub offset_ns: Option<f64>,
}

/// NTP, as the system tells it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NtpView {
    pub synced: Option<bool>,
    pub source: String,
    /// System clock − NTP time, ms.
    pub offset_ms: Option<f64>,
}

/// A line of the alarm journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    /// When it began, ms since 1970.
    pub at: i64,
    /// The input's name.
    pub input: String,
    pub kind: AlarmKind,
    pub severity: Severity,
    /// When it ended (none: still on).
    pub ended: Option<i64>,
}

/// The state of the plant: synchronisation and the alarm journal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemView {
    pub ptp: PtpView,
    pub ntp: NtpView,
    /// The newest first (two hundred at most).
    pub journal: Vec<JournalEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_of_samples() {
        let l = Level::of([0.5f32, -0.5, 0.5, -0.5].into_iter());
        assert!((l.peak - -6.02).abs() < 0.01);
        assert!((l.rms - -6.02).abs() < 0.01);
        assert_eq!(Level::of(std::iter::empty()).peak, SILENCE);
        assert_eq!(db(0.0), SILENCE);
        assert_eq!(db(1.0), 0.0);
    }
}
