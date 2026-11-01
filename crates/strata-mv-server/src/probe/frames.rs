//! The pictures of a stream as ffmpeg decodes them (its `showinfo` filter,
//! one line per picture and one per side data): their type (I, P, B), the
//! GOP, the scan, the colours, and what rides along (timecode, closed
//! captions, HDR metadata, AFD, SEI).

use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use serde::Serialize;

/// The last pictures counted.
const WINDOW: usize = 250;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideData {
    pub kind: String,
    pub count: u64,
    /// Its last value ("10:00:12:04", "MaxCLL=1000, MaxFALL=400"…).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last: String,
    /// Seconds since it was last seen.
    pub ago_s: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStats {
    /// Pictures decoded since the input started.
    pub frames: u64,
    /// Over the last `window` pictures.
    pub window: u32,
    pub i: u32,
    pub p: u32,
    pub b: u32,
    pub other: u32,
    /// Pictures between key frames (the GOP).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gop: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gop_min: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gop_max: Option<u32>,
    /// Progressive, top field first, bottom field first, mixed.
    pub scan: String,
    pub progressive: u32,
    pub tff: u32,
    pub bff: u32,
    /// As decoded.
    pub pix_fmt: String,
    pub size: String,
    pub sar: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub range: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub matrix: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub primaries: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub transfer: String,
    /// The last timecode carried by the pictures, and how.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode_kind: String,
    /// A/53 closed captions in the last seconds.
    pub captions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afd: Option<u8>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mastering: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub light: String,
    pub side: Vec<SideData>,
}

/// Counts the pictures as their lines come.
#[derive(Default)]
pub struct Frames {
    frames: u64,
    recent: VecDeque<(char, char)>,
    since_key: Option<u32>,
    gops: VecDeque<u32>,
    pix_fmt: String,
    size: String,
    sar: String,
    color: [String; 4],
    timecode: Option<(String, &'static str, Instant)>,
    side: BTreeMap<String, (u64, String, Instant)>,
    afd: Option<u8>,
    mastering: String,
    light: String,
    captions_at: Option<Instant>,
}

/// What a line said worth passing on at once.
pub enum Said {
    Nothing,
    /// A timecode ("10:00:00:00").
    Timecode(String),
}

impl Frames {
    /// Reads a line of ffmpeg's log; None: not one of `showinfo`.
    pub fn line(&mut self, line: &str, now: Instant) -> Option<Said> {
        let rest = line.strip_prefix("[Parsed_showinfo_")?;
        let body = rest.split_once("] ").map(|(_, b)| b).unwrap_or("");
        if body.trim_start().starts_with("n:") {
            self.picture(body);
            return Some(Said::Nothing);
        }
        let t = body.trim_start();
        if let Some(sd) = t.strip_prefix("side data - ") {
            return Some(self.side_data(sd, now));
        }
        if t.starts_with("color_range:") {
            for kv in t.split_whitespace() {
                let Some((k, v)) = kv.split_once(':') else {
                    continue;
                };
                let i = match k {
                    "color_range" => 0,
                    "color_space" => 1,
                    "color_primaries" => 2,
                    "color_trc" => 3,
                    _ => continue,
                };
                if v != "unknown" {
                    self.color[i] = v.to_string();
                }
            }
        }
        Some(Said::Nothing)
    }

    fn picture(&mut self, body: &str) {
        self.frames += 1;
        let (mut kind, mut scan, mut key) = ('?', '?', false);
        for kv in body.split_whitespace() {
            let Some((k, v)) = kv.split_once(':') else {
                continue;
            };
            match k {
                "type" => kind = v.chars().next().unwrap_or('?'),
                "i" => scan = v.chars().next().unwrap_or('?'),
                "iskey" => key = v == "1",
                "color_range" if v != "unknown" => self.color[0] = v.to_string(),
                // Copied when they change only.
                "fmt" if self.pix_fmt != v => self.pix_fmt = v.to_string(),
                "s" if self.size != v => self.size = v.to_string(),
                "sar" if self.sar != v => self.sar = v.to_string(),
                _ => {}
            }
        }
        self.recent.push_back((kind, scan));
        if self.recent.len() > WINDOW {
            self.recent.pop_front();
        }
        if key {
            if let Some(n) = self.since_key {
                self.gops.push_back(n);
                if self.gops.len() > 20 {
                    self.gops.pop_front();
                }
            }
            self.since_key = Some(1);
        } else if let Some(n) = self.since_key.as_mut() {
            *n += 1;
        }
    }

    fn side_data(&mut self, sd: &str, now: Instant) -> Said {
        // "kind: value", "timecode - 10:00:00:00", "A/53 closed captions (12 bytes)".
        let (kind, value) = if let Some((k, v)) = sd.split_once(" - ") {
            (k.trim(), v.trim())
        } else if let Some((k, v)) = sd.split_once(": ") {
            (k.trim(), v.trim())
        } else {
            (sd.split(" (").next().unwrap_or(sd).trim(), "")
        };
        let mut said = Said::Nothing;
        let name = match kind {
            "timecode" | "SMPTE 12-1 timecode" => {
                let tc = value.split(", ").next().unwrap_or("").to_string();
                self.timecode = Some((tc.clone(), "SMPTE 12-1 (SEI)", now));
                said = Said::Timecode(tc);
                "Timecode SMPTE 12-1"
            }
            "GOP timecode" => {
                if self
                    .timecode
                    .as_ref()
                    .is_none_or(|t| t.1 == "GOP" || now.duration_since(t.2).as_secs() > 2)
                {
                    self.timecode = Some((value.to_string(), "GOP", now));
                    said = Said::Timecode(value.to_string());
                }
                "Timecode de GOP"
            }
            k if k.starts_with("A/53 closed captions") => {
                self.captions_at = Some(now);
                "Sous-titres codés A/53 (CEA-608/708)"
            }
            "afd" | "active format description" => {
                self.afd = value.trim_start_matches("value of ").trim().parse().ok();
                "AFD"
            }
            "mastering display" => {
                self.mastering = value.to_string();
                "Mastering display (SMPTE 2086)"
            }
            k if k.starts_with("Content Light Level") || k.starts_with("content light level") => {
                self.light = value.to_string();
                "Content light level (MaxCLL/MaxFALL)"
            }
            k if k.contains("User Data Unregistered") => "SEI user data (non enregistré)",
            k if k.starts_with("HDR10+") => "HDR10+ (dynamique)",
            k if k.starts_with("HDR Vivid") => "HDR Vivid",
            k if k.starts_with("Dolby Vision") => "Dolby Vision",
            k if k.starts_with("displaymatrix") => "Matrice d’affichage (rotation)",
            k if k.starts_with("stereoscopic") => "Stéréoscopie 3D",
            k if k.starts_with("spherical") => "Vidéo 360°",
            k if k.starts_with("film grain") => "Grain de film",
            k if k.starts_with("pan/scan") || k.starts_with("AVPanScan") => "Pan & scan",
            k if k.starts_with("bar data") => "Bar data",
            k => k,
        };
        let value = match name {
            "SEI user data (non enregistré)" => {
                value.split("UUID=").nth(1).unwrap_or("").to_string()
            }
            _ => value.to_string(),
        };
        let e = self
            .side
            .entry(name.to_string())
            .or_insert((0, String::new(), now));
        e.0 += 1;
        e.1 = value.chars().take(160).collect();
        e.2 = now;
        said
    }

    /// Side data `name` came with a picture in the last 10 s.
    pub fn seen(&self, name: &str, now: Instant) -> bool {
        self.side
            .get(name)
            .is_some_and(|(_, _, at)| now.duration_since(*at).as_secs() < 10)
    }

    /// Range, matrix, primaries and transfer as decoded ("" unknown).
    pub fn color(&self) -> &[String; 4] {
        &self.color
    }

    pub fn pix_fmt(&self) -> &str {
        &self.pix_fmt
    }

    pub fn size(&self) -> &str {
        &self.size
    }

    /// SMPTE 2086 and content light level as ffmpeg tells them.
    pub fn hdr_metadata(&self) -> (&str, &str) {
        (&self.mastering, &self.light)
    }

    pub fn view(&self, now: Instant) -> FrameStats {
        let mut s = FrameStats {
            frames: self.frames,
            window: self.recent.len() as u32,
            pix_fmt: self.pix_fmt.clone(),
            size: self.size.clone(),
            sar: self.sar.clone(),
            range: match self.color[0].as_str() {
                "tv" => "limitée (TV)".into(),
                "pc" => "complète (PC)".into(),
                other => other.to_string(),
            },
            matrix: self.color[1].clone(),
            primaries: self.color[2].clone(),
            transfer: self.color[3].clone(),
            afd: self.afd,
            mastering: self.mastering.clone(),
            light: self.light.clone(),
            captions: self
                .captions_at
                .is_some_and(|t| now.duration_since(t).as_secs() < 5),
            ..Default::default()
        };
        for (kind, scan) in &self.recent {
            match kind {
                'I' => s.i += 1,
                'P' => s.p += 1,
                'B' => s.b += 1,
                _ => s.other += 1,
            }
            match scan {
                'P' => s.progressive += 1,
                'T' => s.tff += 1,
                'B' => s.bff += 1,
                _ => {}
            }
        }
        s.scan = match (s.progressive > 0, s.tff > 0, s.bff > 0) {
            (true, false, false) => "progressif",
            (false, true, false) => "entrelacé, trame haute d’abord (TFF)",
            (false, false, true) => "entrelacé, trame basse d’abord (BFF)",
            (false, false, false) => "",
            _ => "mixte (progressif et entrelacé)",
        }
        .to_string();
        if !self.gops.is_empty() {
            let n = self.gops.len() as f64;
            s.gop = Some(self.gops.iter().map(|g| *g as f64).sum::<f64>() / n);
            s.gop_min = self.gops.iter().min().copied();
            s.gop_max = self.gops.iter().max().copied();
        }
        if let Some((tc, kind, at)) = &self.timecode {
            if now.duration_since(*at).as_secs() < 5 {
                s.timecode = tc.clone();
                s.timecode_kind = kind.to_string();
            }
        }
        s.side = self
            .side
            .iter()
            .map(|(k, (count, last, at))| SideData {
                kind: k.clone(),
                count: *count,
                last: last.clone(),
                ago_s: now.duration_since(*at).as_secs_f64(),
            })
            .collect();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_pictures_gops_and_side_data() {
        let mut f = Frames::default();
        let now = Instant::now();
        let pic = |n: u32, t: char, key: bool| {
            format!(
                "[Parsed_showinfo_0 @ 0x55] n:{n:4} pts:{n} pts_time:0 duration:3600 fmt:yuv420p10le cl:left sar:1/1 s:1920x1080 i:T iskey:{} type:{t} ",
                u8::from(key)
            )
        };
        for g in 0..3 {
            for i in 0..12 {
                let t = if i == 0 {
                    'I'
                } else if i % 3 == 0 {
                    'P'
                } else {
                    'B'
                };
                f.line(&pic(g * 12 + i, t, i == 0), now);
            }
        }
        f.line(
            "[Parsed_showinfo_0 @ 0x55]   side data - A/53 closed captions (12 bytes)",
            now,
        );
        let said = f.line(
            "[Parsed_showinfo_0 @ 0x55]   side data - timecode - 10:00:01:12",
            now,
        );
        assert!(matches!(said, Some(Said::Timecode(ref t)) if t == "10:00:01:12"));
        f.line(
            "[Parsed_showinfo_0 @ 0x55]   side data - afd: value of 10",
            now,
        );
        f.line("[Parsed_showinfo_0 @ 0x55] color_range:tv color_space:bt2020nc color_primaries:bt2020 color_trc:arib-std-b67", now);
        assert!(f.line("frame=  10 fps=25", now).is_none());
        let v = f.view(now);
        assert_eq!(v.frames, 36);
        assert_eq!((v.i, v.p, v.b), (3, 9, 24));
        assert_eq!(v.gop, Some(12.0));
        assert_eq!(v.tff, 36);
        assert!(v.scan.contains("TFF"));
        assert!(v.captions);
        assert_eq!(v.timecode, "10:00:01:12");
        assert_eq!(v.afd, Some(10));
        assert_eq!(v.transfer, "arib-std-b67");
        assert_eq!(v.size, "1920x1080");
    }
}
