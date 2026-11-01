//! The Signal widget: what an input's picture and sound are — codec,
//! sampling, colour space, transfer, range, HDR (HDR10, HLG, Dolby Vision,
//! HDR10+) and its metadata (ST 2086, MaxCLL / MaxFALL), its sound tracks
//! (Dolby Digital, Digital Plus, Atmos…). Laid out as items like the other
//! broadcast widgets, drawn by both renderers (`web/src/widgets/signalInfo.tsx`
//! mirrors this module: keep them in sync).

use serde::{Deserialize, Serialize};

use super::broadcast::{pad_of, rect, text, Item, DIM, WHITE};
use crate::model::TextAlign;
use crate::signal::{SourceView, VideoInfo};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SignalConfig {
    pub input: String,
    /// Codec, size, sampling.
    pub video: bool,
    /// Colour space, transfer, range.
    pub color: bool,
    /// Mastering display and light levels.
    pub hdr: bool,
    /// The sound tracks.
    pub audio: bool,
    pub background: String,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            input: String::new(),
            video: true,
            color: true,
            hdr: true,
            audio: true,
            background: "#0b0d12d9".into(),
        }
    }
}

const DASH: &str = "—";

/// A matrix's name ("bt2020nc" → "BT.2020 NCL").
pub fn matrix_name(m: &str) -> String {
    match m {
        "bt709" => "BT.709",
        "bt2020nc" => "BT.2020 NCL",
        "bt2020c" => "BT.2020 CL",
        "smpte170m" => "BT.601 (525)",
        "bt470bg" => "BT.601 (625)",
        "ictcp" => "ICtCp",
        "rgb" | "gbr" => "RGB",
        "" => DASH,
        other => other,
    }
    .to_string()
}

/// Primaries' name ("bt2020" → "BT.2020").
pub fn primaries_name(p: &str) -> String {
    match p {
        "bt709" => "BT.709",
        "bt2020" => "BT.2020",
        "smpte432" => "P3 D65",
        "smpte431" => "DCI-P3",
        "smpte170m" => "BT.601 (525)",
        "bt470bg" => "BT.601 (625)",
        "" => DASH,
        other => other,
    }
    .to_string()
}

/// A transfer's name ("smpte2084" → "PQ (ST 2084)").
pub fn transfer_name(t: &str) -> String {
    match t {
        "bt709" | "bt1361e" => "BT.709 (SDR)",
        "smpte2084" => "PQ (ST 2084)",
        "arib-std-b67" => "HLG (ARIB STD-B67)",
        "iec61966-2-1" => "sRGB",
        "smpte170m" => "BT.601 (SDR)",
        "bt2020-10" | "bt2020-12" => "BT.2020 (SDR)",
        "linear" => "Linéaire",
        "" => DASH,
        other => other,
    }
    .to_string()
}

/// The colour of a dynamic's badge, and its text's.
pub fn dynamic_colors(d: &str) -> (&'static str, &'static str) {
    match d {
        "Dolby Vision" => ("#8b5cf6", "#ffffff"),
        "HDR10+" => ("#f97316", "#1c0d00"),
        "HDR10" => ("#f59e0b", "#1c1300"),
        "HLG" => ("#22c55e", "#04130a"),
        _ => ("#4b5563", "#ffffff"),
    }
}

/// ST 2086 as ffmpeg tells it → "P3 D65 · 0.0001–1000 nits".
pub fn mastering_text(m: &str) -> String {
    if m.trim().is_empty() {
        return DASH.into();
    }
    let num = |key: &str| -> Option<f32> {
        let r = m.split(key).nth(1)?;
        r.chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .parse()
            .ok()
    };
    // The red primary tells the gamut.
    let gamut = num("r(").map(|x| {
        if (x - 0.708).abs() < 0.01 {
            "BT.2020"
        } else if (x - 0.68).abs() < 0.01 {
            "P3"
        } else if (x - 0.64).abs() < 0.01 {
            "BT.709"
        } else {
            ""
        }
    });
    let lum = match (num("min_luminance="), num("max_luminance=")) {
        (Some(lo), Some(hi)) => {
            let lo = (lo * 10_000.0).round() / 10_000.0;
            format!("{lo}–{} nits", hi.round())
        }
        _ => String::new(),
    };
    match (gamut.unwrap_or(""), lum.as_str()) {
        ("", "") => m.chars().take(40).collect(),
        (g, "") => g.to_string(),
        ("", l) => l.to_string(),
        (g, l) => format!("{g} · {l}"),
    }
}

/// The levels measured: "−2 % – 104 % · 0.4 % hors normes", amber out of
/// EBU R103 (luma −1 % to 103 %, 1 % of the pixels at most).
pub fn levels_text(v: &VideoInfo, fr: bool) -> (String, &'static str) {
    let pct = |x: f32| {
        let s = format!("{x:.0}");
        match s.strip_prefix('-') {
            Some(r) if r != "0" => format!("\u{2212}{r} %"),
            Some(_) => "0 %".to_string(),
            None => format!("{s} %"),
        }
    };
    let Some([lo, hi]) = v.luma else {
        return (DASH.into(), WHITE);
    };
    let mut s = format!("{} – {}", pct(lo), pct(hi));
    let illegal = v.illegal.unwrap_or(0.0);
    if illegal >= 0.05 {
        s.push_str(&format!(
            " · {illegal:.1} % {}",
            if fr { "hors normes" } else { "out of range" }
        ));
    }
    let bad = lo < -1.0 || hi > 103.0 || illegal > 1.0;
    (s, if bad { "#fbbf24" } else { WHITE })
}

/// A sound track in a line: "E-AC-3 (Dolby Digital Plus) · 5.1(side) · 48 kHz · fra".
pub fn audio_line(a: &crate::signal::AudioTrack) -> String {
    let mut parts = vec![a.codec.clone()];
    if !a.layout.is_empty() {
        parts.push(a.layout.clone());
    }
    if a.rate > 0 {
        parts.push(format!("{} kHz", a.rate as f32 / 1000.0));
    }
    if !a.language.is_empty() && a.language != "und" {
        parts.push(a.language.clone());
    }
    parts.join(" · ")
}

/// The lines shown: (name, value, value's colour) by section.
pub fn lines(c: &SignalConfig, v: &VideoInfo, fr: bool) -> Vec<(String, String, &'static str)> {
    let l = |f: &str, e: &str| if fr { f.to_string() } else { e.to_string() };
    let or_dash = |s: String| {
        if s.trim().is_empty() {
            DASH.to_string()
        } else {
            s
        }
    };
    let mut out = Vec::new();
    if c.video {
        let codec = match (v.codec.as_str(), v.profile.as_str()) {
            ("", _) => String::new(),
            (c, "") => c.to_string(),
            (c, p) => format!("{c} · {p}"),
        };
        out.push((l("Codec", "Codec"), or_dash(codec), WHITE));
        let mut pic = Vec::new();
        if !v.size.is_empty() {
            pic.push(v.size.replace('x', "×"));
        }
        if v.bits > 0 {
            pic.push(format!("{} bits", v.bits));
        }
        if !v.chroma.is_empty() {
            pic.push(v.chroma.clone());
        }
        out.push((l("Image", "Picture"), or_dash(pic.join(" · ")), WHITE));
    }
    if c.color {
        let space = if v.matrix.is_empty() && v.primaries.is_empty() {
            String::new()
        } else if v.matrix.is_empty() {
            primaries_name(&v.primaries)
        } else if v.primaries.is_empty() {
            matrix_name(&v.matrix)
        } else {
            format!(
                "{} · {}",
                matrix_name(&v.matrix),
                primaries_name(&v.primaries)
            )
        };
        out.push((l("Colorimétrie", "Colour space"), or_dash(space), WHITE));
        out.push((
            l("Transfert", "Transfer"),
            transfer_name(&v.transfer),
            WHITE,
        ));
        let range = match v.range.as_str() {
            "tv" => l("Limitée (TV)", "Limited (TV)"),
            "pc" => l("Complète (PC)", "Full (PC)"),
            _ => DASH.into(),
        };
        out.push((l("Plage", "Range"), range, WHITE));
        let (levels, col) = levels_text(v, fr);
        out.push((l("Niveaux (R103)", "Levels (R103)"), levels, col));
    }
    if c.hdr {
        out.push((
            "Mastering (ST 2086)".into(),
            mastering_text(&v.mastering),
            WHITE,
        ));
        let light = if v.max_cll > 0 || v.max_fall > 0 {
            format!("{} / {} nits", v.max_cll, v.max_fall)
        } else {
            DASH.into()
        };
        out.push(("MaxCLL / MaxFALL".into(), light, WHITE));
    }
    if c.audio {
        if v.audio.is_empty() {
            out.push((l("Son", "Sound"), DASH.into(), WHITE));
        }
        for (i, a) in v.audio.iter().enumerate() {
            let name = if v.audio.len() == 1 {
                l("Son", "Sound")
            } else {
                format!("{} {}", l("Son", "Sound"), i + 1)
            };
            out.push((name, audio_line(a), WHITE));
        }
    }
    out
}

/// The Signal widget of an input.
pub fn signal_info(
    c: &SignalConfig,
    w: f32,
    h: f32,
    v: Option<&SourceView>,
    lang: &str,
) -> Vec<Item> {
    let fr = lang.to_lowercase().starts_with("fr");
    let pad = pad_of(w, h);
    let mut out = vec![rect([0.0, 0.0, w, h], pad * 0.6, &c.background)];
    let info = v.map(|v| v.video.clone()).unwrap_or_default();
    let rows = lines(c, &info, fr);
    // Header: the input, its format, its dynamic.
    let n = rows.len() as f32 + 1.4;
    let rh = ((h - 2.0 * pad) / n).max(1.0);
    let fs = (rh * 0.52).min(w * 0.035).clamp(7.0, 26.0).round();
    let head = rh * 1.4;
    let live = v.is_some_and(|v| v.live);
    let name = v.map(|v| v.name.clone()).unwrap_or_default();
    let fmt = v.map(|v| v.format.clone()).unwrap_or_default();
    let dynamic = if live {
        info.dynamic().to_string()
    } else {
        String::new()
    };
    let badge_w = if dynamic.is_empty() {
        0.0
    } else {
        (fs * 0.62 * dynamic.chars().count() as f32 + fs * 1.4).min(w * 0.45)
    };
    let tw = w - 2.0 * pad - badge_w - fs * 0.5;
    out.push(text(
        [pad, pad, tw * 0.62, head],
        if name.is_empty() {
            DASH.to_string()
        } else {
            name
        },
        fs * 1.15,
        700,
        false,
        WHITE,
        TextAlign::Left,
    ));
    out.push(text(
        [pad + tw * 0.62, pad, tw * 0.38, head],
        fmt,
        fs,
        600,
        true,
        DIM,
        TextAlign::Left,
    ));
    if badge_w > 0.0 {
        let (bg, fg) = dynamic_colors(&dynamic);
        let bh = fs * 1.5;
        let bx = [w - pad - badge_w, pad + (head - bh) / 2.0, badge_w, bh];
        out.push(rect(bx, bh * 0.25, bg));
        out.push(text(
            bx,
            dynamic,
            fs * 0.9,
            800,
            false,
            fg,
            TextAlign::Center,
        ));
    }
    // A line under the header.
    out.push(rect(
        [pad, pad + head - 1.0, w - 2.0 * pad, 1.0],
        0.0,
        "#ffffff1f",
    ));
    // The lines: name on the left, value on the right of it; Dolby and Atmos badges on sound.
    let tracks: Vec<(bool, bool)> = info.audio.iter().map(|a| (a.dolby, a.atmos)).collect();
    let first_track = rows.len() - if c.audio { tracks.len().max(1) } else { 0 };
    for (i, (label, value, col)) in rows.iter().enumerate() {
        let y = pad + head + i as f32 * rh;
        let nw = (w - 2.0 * pad) * 0.36;
        out.push(text(
            [pad, y, nw, rh],
            label.clone(),
            fs * 0.85,
            600,
            false,
            DIM,
            TextAlign::Left,
        ));
        let mut vx = pad + nw;
        let mut vw = w - pad - vx;
        // The track's Dolby / Atmos marks, at the right.
        if c.audio && i >= first_track {
            if let Some((dolby, atmos)) = tracks.get(i - first_track) {
                let mut x = w - pad;
                for (on, mark, bg) in [(*atmos, "ATMOS", "#0ea5e9"), (*dolby, "DOLBY", "#e5e7eb")] {
                    if !on {
                        continue;
                    }
                    let bw = fs * 3.6;
                    let bh = rh * 0.7;
                    x -= bw;
                    out.push(rect([x, y + (rh - bh) / 2.0, bw, bh], bh * 0.2, bg));
                    out.push(text(
                        [x, y + (rh - bh) / 2.0, bw, bh],
                        mark,
                        fs * 0.7,
                        800,
                        false,
                        "#0b0d12",
                        TextAlign::Center,
                    ));
                    x -= fs * 0.3;
                }
                vw = x - vx - fs * 0.3;
            }
        }
        vx = vx.min(w - pad);
        out.push(text(
            [vx, y, vw.max(1.0), rh],
            value.clone(),
            fs,
            600,
            false,
            col,
            TextAlign::Left,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::AudioTrack;

    fn hdr10() -> VideoInfo {
        VideoInfo {
            codec: "H.265 / HEVC".into(),
            profile: "Main 10".into(),
            size: "3840x2160".into(),
            bits: 10,
            chroma: "4:2:0".into(),
            range: "tv".into(),
            matrix: "bt2020nc".into(),
            primaries: "bt2020".into(),
            transfer: "smpte2084".into(),
            mastering: "has_primaries:1 has_luminance:1 r(0.6800,0.3200) g(0.2650,0.6900) b(0.1500 0.0600) wp(0.3127, 0.3290) min_luminance=0.000100, max_luminance=1000.000000".into(),
            max_cll: 1000,
            max_fall: 400,
            audio: vec![AudioTrack {
                codec: "E-AC-3 (Dolby Digital Plus)".into(),
                layout: "5.1(side)".into(),
                rate: 48000,
                language: "fra".into(),
                dolby: true,
                atmos: true,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn hdr10_reads_as_such() {
        let v = hdr10();
        let l = lines(&SignalConfig::default(), &v, true);
        let get = |n: &str| l.iter().find(|x| x.0 == n).map(|x| x.1.clone()).unwrap();
        assert_eq!(get("Codec"), "H.265 / HEVC · Main 10");
        assert_eq!(get("Image"), "3840×2160 · 10 bits · 4:2:0");
        assert_eq!(get("Colorimétrie"), "BT.2020 NCL · BT.2020");
        assert_eq!(get("Transfert"), "PQ (ST 2084)");
        assert_eq!(get("Plage"), "Limitée (TV)");
        assert_eq!(get("Niveaux (R103)"), "—");
        let mut v2 = v.clone();
        v2.luma = Some([-2.4, 104.0]);
        v2.illegal = Some(0.42);
        assert_eq!(
            levels_text(&v2, true),
            (
                "\u{2212}2 % – 104 % · 0.4 % hors normes".to_string(),
                "#fbbf24"
            )
        );
        assert_eq!(get("Mastering (ST 2086)"), "P3 · 0.0001–1000 nits");
        assert_eq!(get("MaxCLL / MaxFALL"), "1000 / 400 nits");
        assert_eq!(
            get("Son"),
            "E-AC-3 (Dolby Digital Plus) · 5.1(side) · 48 kHz · fra"
        );
        assert_eq!(v.dynamic(), "HDR10");
    }

    #[test]
    fn lays_out_with_or_without_an_input() {
        let view = SourceView {
            name: "UHD 1".into(),
            format: "2160p50".into(),
            live: true,
            video: hdr10(),
            ..Default::default()
        };
        let items = signal_info(&SignalConfig::default(), 520.0, 360.0, Some(&view), "fr");
        let texts: Vec<String> = items
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"HDR10".to_string()));
        assert!(texts.contains(&"DOLBY".to_string()));
        assert!(texts.contains(&"ATMOS".to_string()));
        assert!(signal_info(&SignalConfig::default(), 520.0, 360.0, None, "en").len() > 5);
    }
}
