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

/// A fault the server sees on a source, or on the plant (the last ones).
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
    /// One channel silent while the others are not.
    ChannelSilent,
    /// Much quieter than the target for a while.
    Quiet,
    /// The samples reach full scale.
    Clipping,
    /// Left and right out of phase.
    Phase,
    /// Fewer (or more) pictures a second than its format.
    Fps,
    /// The input keeps reconnecting.
    Reconnecting,
    /// No timecode in the pictures.
    NoTimecode,
    /// No closed captions in the pictures.
    NoCaptions,
    /// TR 101 290 first priority errors (sync, PAT, continuity, PMT, PID).
    TsPriority1,
    /// TR 101 290 second priority errors (transport, CRC, PCR).
    TsPriority2,
    /// PTP runs but follows no grandmaster.
    PtpUnlocked,
    /// The clock is not synchronised by NTP.
    NtpUnsynced,
    /// The processors are busy.
    CpuHigh,
    /// The memory is nearly full.
    MemoryHigh,
    /// An output drops pictures.
    OutputDropped,
    /// Colour space or transfer other than the one expected.
    ColorSpace,
    /// Levels out of the broadcast range (EBU R103).
    IllegalLevels,
    /// Full range (PC) where limited (TV) is expected.
    FullRange,
    /// HDR (PQ) without its static metadata (ST 2086, MaxCLL).
    HdrMetadata,
    /// No colour information carried.
    NoColorInfo,
    /// The input lost its signal: its tiles show its backup.
    OnBackup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Warning,
    Critical,
}

/// The faults of an input, in the order of the settings.
pub const INPUT_ALARMS: [AlarmKind; 22] = [
    AlarmKind::Loss,
    AlarmKind::OnBackup,
    AlarmKind::Black,
    AlarmKind::Freeze,
    AlarmKind::Fps,
    AlarmKind::Reconnecting,
    AlarmKind::Silence,
    AlarmKind::ChannelSilent,
    AlarmKind::Loud,
    AlarmKind::Quiet,
    AlarmKind::Peak,
    AlarmKind::Clipping,
    AlarmKind::Phase,
    AlarmKind::NoTimecode,
    AlarmKind::NoCaptions,
    AlarmKind::ColorSpace,
    AlarmKind::IllegalLevels,
    AlarmKind::FullRange,
    AlarmKind::HdrMetadata,
    AlarmKind::NoColorInfo,
    AlarmKind::TsPriority1,
    AlarmKind::TsPriority2,
];

/// The faults of the plant.
pub const SYSTEM_ALARMS: [AlarmKind; 5] = [
    AlarmKind::PtpUnlocked,
    AlarmKind::NtpUnsynced,
    AlarmKind::CpuHigh,
    AlarmKind::MemoryHigh,
    AlarmKind::OutputDropped,
];

impl AlarmKind {
    /// Its severity unless the settings say otherwise.
    pub fn severity(self) -> Severity {
        match self {
            AlarmKind::Loss
            | AlarmKind::Freeze
            | AlarmKind::Black
            | AlarmKind::Silence
            | AlarmKind::PtpUnlocked => Severity::Critical,
            _ => Severity::Warning,
        }
    }

    /// Of the plant (not of an input).
    pub fn system(self) -> bool {
        SYSTEM_ALARMS.contains(&self)
    }

    /// The words on the tile and in the journal, in the project's language.
    pub fn text(self, lang: &str) -> &'static str {
        let fr = lang.to_lowercase().starts_with("fr");
        let (f, e) = match self {
            AlarmKind::Loss => ("PERTE DU SIGNAL", "SIGNAL LOSS"),
            AlarmKind::Black => ("NOIR", "BLACK"),
            AlarmKind::Freeze => ("IMAGE FIGÉE", "FROZEN PICTURE"),
            AlarmKind::Silence => ("SILENCE", "SILENCE"),
            AlarmKind::Loud => ("TROP FORT", "TOO LOUD"),
            AlarmKind::Peak => ("TRUE PEAK", "TRUE PEAK"),
            AlarmKind::ChannelSilent => ("CANAL MUET", "CHANNEL SILENT"),
            AlarmKind::Quiet => ("TROP FAIBLE", "TOO QUIET"),
            AlarmKind::Clipping => ("SATURATION", "CLIPPING"),
            AlarmKind::Phase => ("OPPOSITION DE PHASE", "OUT OF PHASE"),
            AlarmKind::Fps => ("CADENCE ANORMALE", "ABNORMAL FRAME RATE"),
            AlarmKind::Reconnecting => ("RECONNEXIONS", "RECONNECTING"),
            AlarmKind::OnBackup => ("SUR SECOURS", "ON BACKUP"),
            AlarmKind::NoTimecode => ("SANS TIMECODE", "NO TIMECODE"),
            AlarmKind::NoCaptions => ("SANS SOUS-TITRES", "NO CAPTIONS"),
            AlarmKind::TsPriority1 => ("TR 101 290 P1", "TR 101 290 P1"),
            AlarmKind::TsPriority2 => ("TR 101 290 P2", "TR 101 290 P2"),
            AlarmKind::PtpUnlocked => ("PTP NON VERROUILLÉ", "PTP UNLOCKED"),
            AlarmKind::NtpUnsynced => ("NTP NON SYNCHRONISÉ", "NTP NOT SYNCED"),
            AlarmKind::CpuHigh => ("PROCESSEURS SATURÉS", "PROCESSORS BUSY"),
            AlarmKind::MemoryHigh => ("MÉMOIRE PLEINE", "MEMORY FULL"),
            AlarmKind::OutputDropped => ("IMAGES PERDUES EN SORTIE", "OUTPUT DROPS FRAMES"),
            AlarmKind::ColorSpace => ("ESPACE COULEUR", "COLOUR SPACE"),
            AlarmKind::IllegalLevels => ("NIVEAUX HORS NORMES", "ILLEGAL LEVELS"),
            AlarmKind::FullRange => ("PLAGE COMPLÈTE", "FULL RANGE"),
            AlarmKind::HdrMetadata => ("MÉTADONNÉES HDR", "HDR METADATA"),
            AlarmKind::NoColorInfo => ("COLORIMÉTRIE NON SIGNALÉE", "NO COLOUR INFO"),
        };
        if fr {
            f
        } else {
            e
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
    /// Someone has seen it (the tile stops flashing).
    #[serde(default)]
    pub acked: bool,
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
    /// What its picture and sound are (colour, HDR, codecs).
    pub video: VideoInfo,
    /// The phase correlation of its left and right (−1 to 1) over the last
    /// 300 ms, none when silent.
    pub correlation: Option<f32>,
    /// Shown instead: its backup's name (the picture, sound and faults
    /// are the backup's; "" = itself).
    pub backup: String,
}

/// What a live input's picture and sound are: the Signal widget, the scopes'
/// graticules (by transfer and primaries).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VideoInfo {
    /// "H.264 / AVC", "HEVC"…
    pub codec: String,
    /// "Main 10", "High"…
    pub profile: String,
    /// "1920x1080".
    pub size: String,
    /// As decoded ("yuv422p10le").
    pub pix_fmt: String,
    /// Bits per component (0: unknown).
    pub bits: u8,
    /// "4:2:0", "4:2:2", "4:4:4" ("" unknown).
    pub chroma: String,
    /// "tv" (limited) or "pc" (full).
    pub range: String,
    /// ffmpeg's names: "bt709", "bt2020nc"…
    pub matrix: String,
    pub primaries: String,
    /// "bt709", "smpte2084" (PQ), "arib-std-b67" (HLG)…
    pub transfer: String,
    /// SDR, HDR10, HLG, Dolby Vision, HDR10+.
    pub hdr: String,
    pub dolby_vision: bool,
    pub hdr10_plus: bool,
    /// SMPTE ST 2086, as ffmpeg tells it ("" none).
    pub mastering: String,
    /// MaxCLL and MaxFALL, cd/m² (0: not carried).
    pub max_cll: u32,
    pub max_fall: u32,
    /// Its sound tracks.
    pub audio: Vec<AudioTrack>,
    /// Lowest and highest luma lately, % of the nominal range (0 % black,
    /// 100 % white; EBU R103 allows −1 % to 103 %).
    pub luma: Option<[f32; 2]>,
    /// Pixels out of the broadcast range lately (luma 16–235, chroma 16–240
    /// in 8 bits), %.
    pub illegal: Option<f32>,
    /// Active Format Description carried by the pictures (SMPTE ST 2016).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afd: Option<u8>,
}

/// A sound track of an input.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AudioTrack {
    /// "AAC", "E-AC-3 (Dolby Digital Plus)"…
    pub codec: String,
    /// "stereo", "5.1(side)"…
    pub layout: String,
    pub rate: u32,
    pub language: String,
    /// Dolby Digital, Digital Plus, TrueHD, E, AC-4.
    pub dolby: bool,
    /// Dolby Atmos (Digital Plus JOC, TrueHD Atmos).
    pub atmos: bool,
}

impl VideoInfo {
    /// The wide gamut of BT.2020 (else BT.709).
    pub fn wide(&self) -> bool {
        self.primaries.contains("2020")
    }

    /// SDR, HDR10, HLG… from what is known.
    pub fn dynamic(&self) -> &str {
        if self.dolby_vision {
            "Dolby Vision"
        } else if self.hdr10_plus {
            "HDR10+"
        } else if self.transfer.contains("2084") {
            "HDR10"
        } else if self.transfer.contains("b67") {
            "HLG"
        } else if !self.hdr.is_empty() {
            &self.hdr
        } else {
            "SDR"
        }
    }
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
    /// The input's name (the plant's faults: what is at fault, "PTP"…).
    pub input: String,
    /// The input's id ("" for the plant).
    #[serde(default)]
    pub input_id: String,
    pub kind: AlarmKind,
    pub severity: Severity,
    /// When it ended (none: still on).
    pub ended: Option<i64>,
    /// When it was acknowledged, and by whom.
    #[serde(default)]
    pub acked: Option<i64>,
    #[serde(default)]
    pub by: String,
}

/// The state of the plant: synchronisation and the alarm journal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemView {
    pub ptp: PtpView,
    pub ntp: NtpView,
    /// The newest first (two hundred at most).
    pub journal: Vec<JournalEntry>,
    /// The plant's measures, one a second (the Chart widget).
    pub curves: Curves,
    /// The pages sound for a new fault this grave at least (none: silence).
    pub sound: Option<Severity>,
}

/// Measures of the plant, one a second, the newest last (five minutes at
/// most): "cpu", "memory", "server", "ffmpeg" (% of the machine), "ptp"
/// (offset from the master, ns), "ntp" (offset, ms), "render:<channel>"
/// (ms to draw a picture), "fps:<input>".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Curves {
    /// The time of the newest, ms since 1970 (0: none yet).
    pub end_ms: i64,
    pub series: std::collections::BTreeMap<String, Vec<Option<f32>>>,
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
