//! The alarm settings: for each fault, whether it is raised, after how
//! long, how grave it is and from what threshold — gathered in profiles
//! that inputs share (a camera, a playout server, a feed with colour bars
//! between programmes…); the faults of the plant; and what happens when a
//! fault is raised (a sound on the pages, calls to other systems: an
//! automation, a GPO box, a messenger). Kept by the server in
//! `<data>/alarms.json`, edited on the Alarms page.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::signal::{AlarmKind, Severity, INPUT_ALARMS, SYSTEM_ALARMS};

/// How a fault is raised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rule {
    pub enabled: bool,
    /// Seconds the fault lasts before it is raised.
    pub after: f32,
    pub severity: Severity,
    /// Its threshold, in the fault's unit (see `threshold_unit`).
    pub threshold: f32,
    /// What is expected (the colour space: see `COLOR_SPACES`).
    pub expect: String,
}

/// The colour spaces an input may be expected in: id, name.
pub const COLOR_SPACES: [(&str, &str); 6] = [
    ("bt709", "SDR BT.709 (BT.601 en SD)"),
    ("bt2020", "SDR BT.2020"),
    ("pq", "HDR10 PQ BT.2020"),
    ("hlg", "HLG BT.2020"),
    ("hdr", "HDR (PQ ou HLG)"),
    ("any", "Toutes"),
];

/// The colour space is the one expected (`expect`, an id of
/// `COLOR_SPACES`); unknown (not carried) is never wrong here.
pub fn color_space_ok(expect: &str, v: &crate::signal::VideoInfo) -> bool {
    if v.primaries.is_empty() && v.transfer.is_empty() {
        return true;
    }
    let wide = v.primaries.contains("2020");
    let pq = v.transfer.contains("2084");
    let hlg = v.transfer.contains("b67");
    let sd = v
        .size
        .split_once('x')
        .and_then(|(_, h)| h.parse::<u32>().ok())
        .is_some_and(|h| h < 720);
    let sdr709 = !wide
        && !pq
        && !hlg
        && (v.primaries.is_empty()
            || v.primaries == "bt709"
            || (sd && (v.primaries == "bt470bg" || v.primaries == "smpte170m")));
    match expect {
        "bt709" => sdr709,
        "bt2020" => wide && !pq && !hlg,
        "pq" => pq && (wide || v.primaries.is_empty()),
        "hlg" => hlg && (wide || v.primaries.is_empty()),
        "hdr" => pq || hlg,
        _ => true,
    }
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            enabled: true,
            after: 0.0,
            severity: Severity::Warning,
            threshold: 0.0,
            expect: String::new(),
        }
    }
}

/// The rule of a fault unless set otherwise.
pub fn default_rule(k: AlarmKind) -> Rule {
    let r = |enabled: bool, after: f32, threshold: f32| Rule {
        enabled,
        after,
        severity: k.severity(),
        threshold,
        expect: String::new(),
    };
    match k {
        AlarmKind::Loss => r(true, 0.0, 0.0),
        AlarmKind::Black => r(true, 3.0, 0.0),
        AlarmKind::Freeze => r(true, 10.0, 0.0),
        // % away from the format's rate.
        AlarmKind::Fps => r(true, 5.0, 10.0),
        // Reconnections within a minute.
        AlarmKind::Reconnecting => r(true, 0.0, 3.0),
        // dBFS under which a channel is silent.
        AlarmKind::Silence => r(true, 5.0, -60.0),
        AlarmKind::ChannelSilent => r(true, 10.0, -60.0),
        // LU over the target (short-term).
        AlarmKind::Loud => r(true, 10.0, 3.0),
        // LU under the target (short-term).
        AlarmKind::Quiet => r(true, 30.0, 10.0),
        // dBTP.
        AlarmKind::Peak => r(true, 0.0, -1.0),
        // dBFS (sample peak).
        AlarmKind::Clipping => r(true, 0.0, -0.1),
        // Correlation under which it is out of phase.
        AlarmKind::Phase => r(true, 3.0, -0.3),
        AlarmKind::NoTimecode => r(false, 5.0, 0.0),
        AlarmKind::NoCaptions => r(false, 10.0, 0.0),
        AlarmKind::TsPriority1 => r(true, 0.0, 0.0),
        // Off unless asked: many IP contributions (ffmpeg, OBS) space their
        // PCR more than TR 101 290's 40 ms.
        AlarmKind::TsPriority2 => r(false, 0.0, 0.0),
        AlarmKind::PtpUnlocked => r(true, 5.0, 0.0),
        AlarmKind::NtpUnsynced => r(true, 60.0, 0.0),
        // % of the machine.
        AlarmKind::CpuHigh => r(true, 10.0, 90.0),
        AlarmKind::MemoryHigh => r(true, 10.0, 90.0),
        // Pictures dropped in 10 s.
        AlarmKind::OutputDropped => r(true, 0.0, 5.0),
        AlarmKind::ColorSpace => Rule {
            expect: "bt709".into(),
            ..r(true, 5.0, 0.0)
        },
        // % of the pixels out of the broadcast range (EBU R103: 1 %).
        AlarmKind::IllegalLevels => r(true, 5.0, 1.0),
        AlarmKind::FullRange => r(false, 5.0, 0.0),
        AlarmKind::HdrMetadata => r(true, 10.0, 0.0),
        AlarmKind::NoColorInfo => r(false, 10.0, 0.0),
    }
}

/// The unit of a fault's threshold ("" when it has none).
pub fn threshold_unit(k: AlarmKind) -> &'static str {
    match k {
        AlarmKind::Fps | AlarmKind::CpuHigh | AlarmKind::MemoryHigh => "%",
        AlarmKind::Reconnecting => "/min",
        AlarmKind::Silence | AlarmKind::ChannelSilent | AlarmKind::Clipping => "dBFS",
        AlarmKind::Loud | AlarmKind::Quiet => "LU",
        AlarmKind::Peak => "dBTP",
        AlarmKind::Phase => "corr.",
        AlarmKind::OutputDropped => "/10 s",
        AlarmKind::IllegalLevels => "% px",
        _ => "",
    }
}

/// Faults set for some inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// The faults set apart from their default.
    pub rules: BTreeMap<AlarmKind, Rule>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: DEFAULT_PROFILE.into(),
            name: "Standard".into(),
            rules: BTreeMap::new(),
        }
    }
}

pub const DEFAULT_PROFILE: &str = "default";

/// A call to another system when a fault is raised, ends or is acknowledged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Webhook {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// GET (a GPO box, a simple automation) or POST (JSON body).
    pub method: String,
    /// `{input}`, `{kind}`, `{severity}`, `{state}` (raised, ended, acked)
    /// and `{text}` are replaced.
    pub url: String,
    /// The faults this grave at least.
    pub min_severity: Severity,
    /// Called when the fault ends too.
    pub on_end: bool,
}

impl Default for Webhook {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            method: "POST".into(),
            url: String::new(),
            min_severity: Severity::Warning,
            on_end: true,
        }
    }
}

/// Every alarm setting of the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AlarmSettings {
    /// The first is the default one ("default").
    pub profiles: Vec<Profile>,
    /// Input id → profile id (absent: the default profile).
    pub inputs: BTreeMap<String, String>,
    /// The faults of the plant.
    pub system: BTreeMap<AlarmKind, Rule>,
    /// A sound on the pages (outputs, Alarms page) for a new fault this
    /// grave at least; none: silence.
    pub sound: Option<Severity>,
    pub webhooks: Vec<Webhook>,
}

impl Default for AlarmSettings {
    fn default() -> Self {
        Self {
            profiles: vec![Profile::default()],
            inputs: BTreeMap::new(),
            system: BTreeMap::new(),
            sound: Some(Severity::Critical),
            webhooks: Vec::new(),
        }
    }
}

impl AlarmSettings {
    /// The profile of an input.
    pub fn profile_of(&self, input: &str) -> Option<&Profile> {
        let id = self
            .inputs
            .get(input)
            .map(String::as_str)
            .unwrap_or(DEFAULT_PROFILE);
        self.profiles
            .iter()
            .find(|p| p.id == id)
            .or_else(|| self.profiles.first())
    }

    /// How fault `k` of `input` is raised ("" for the plant's).
    pub fn rule(&self, input: &str, k: AlarmKind) -> Rule {
        if k.system() {
            return self
                .system
                .get(&k)
                .cloned()
                .unwrap_or_else(|| default_rule(k));
        }
        self.profile_of(input)
            .and_then(|p| p.rules.get(&k).cloned())
            .unwrap_or_else(|| default_rule(k))
    }

    /// A default profile first, unique ids, known faults only.
    pub fn normalize(&mut self) {
        if !self.profiles.iter().any(|p| p.id == DEFAULT_PROFILE) {
            self.profiles.insert(0, Profile::default());
        }
        let mut seen = std::collections::BTreeSet::new();
        self.profiles
            .retain(|p| !p.id.is_empty() && seen.insert(p.id.clone()));
        for p in &mut self.profiles {
            p.rules.retain(|k, _| INPUT_ALARMS.contains(k));
        }
        self.system.retain(|k, _| SYSTEM_ALARMS.contains(k));
        let ids: Vec<String> = self.profiles.iter().map(|p| p.id.clone()).collect();
        self.inputs.retain(|_, p| ids.contains(p));
        for (i, w) in self.webhooks.iter_mut().enumerate() {
            if w.id.is_empty() {
                w.id = format!("hook{}", i + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_input_follows_its_profile() {
        let mut s = AlarmSettings::default();
        let mut bars = Profile {
            id: "bars".into(),
            name: "Mire entre programmes".into(),
            ..Default::default()
        };
        bars.rules.insert(
            AlarmKind::Freeze,
            Rule {
                enabled: false,
                ..default_rule(AlarmKind::Freeze)
            },
        );
        s.profiles.push(bars);
        s.inputs.insert("cam2".into(), "bars".into());
        assert!(s.rule("cam1", AlarmKind::Freeze).enabled);
        assert!(!s.rule("cam2", AlarmKind::Freeze).enabled);
        assert_eq!(
            s.rule("cam2", AlarmKind::Black),
            default_rule(AlarmKind::Black)
        );
        // A profile gone: back to the default one.
        s.profiles.retain(|p| p.id != "bars");
        s.normalize();
        assert!(s.rule("cam2", AlarmKind::Freeze).enabled);
        assert!(s.inputs.is_empty());
    }

    #[test]
    fn colour_spaces_as_expected() {
        use crate::signal::VideoInfo;
        let v = |p: &str, t: &str, size: &str| VideoInfo {
            primaries: p.into(),
            transfer: t.into(),
            size: size.into(),
            ..Default::default()
        };
        let hd709 = v("bt709", "bt709", "1920x1080");
        let sd601 = v("bt470bg", "bt470bg", "720x576");
        let hdr10 = v("bt2020", "smpte2084", "3840x2160");
        let hlg = v("bt2020", "arib-std-b67", "3840x2160");
        let unknown = v("", "", "1920x1080");
        assert!(color_space_ok("bt709", &hd709));
        assert!(color_space_ok("bt709", &sd601));
        assert!(!color_space_ok("bt709", &hdr10));
        assert!(color_space_ok("pq", &hdr10));
        assert!(!color_space_ok("pq", &hlg));
        assert!(color_space_ok("hdr", &hlg));
        assert!(!color_space_ok("hdr", &hd709));
        assert!(color_space_ok("bt709", &unknown));
        assert!(color_space_ok("any", &hdr10));
        // BT.601 in HD is not BT.709.
        assert!(!color_space_ok(
            "bt709",
            &v("smpte170m", "bt709", "1920x1080")
        ));
    }

    #[test]
    fn settings_read_back() {
        let mut s = AlarmSettings::default();
        s.system.insert(
            AlarmKind::CpuHigh,
            Rule {
                threshold: 80.0,
                ..default_rule(AlarmKind::CpuHigh)
            },
        );
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"cpuHigh\""));
        let back: AlarmSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.rule("", AlarmKind::CpuHigh).threshold, 80.0);
    }
}
