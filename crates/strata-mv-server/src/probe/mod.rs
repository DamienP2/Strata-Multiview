//! What an input carries, whatever its source: the container and its
//! programs (names, EPG now / next), every stream (codec, profile and level,
//! picture, colours and HDR, scan, audio layout, languages, subtitles,
//! closed captions, data such as SCTE-35, KLV, ID3 or ANC), its pictures
//! (GOP, I / P / B, timecode, side data), and by transport: the MPEG-TS
//! tables and TR 101 290 checks, the HLS playlists, NDI and SDI details.
//!
//! Kept up to date as the input runs (`Facts`, filled by `inputs.rs` from
//! ffmpeg's log, the relayed transport stream, the NDI and DeckLink
//! receivers), and read as a [`Report`] by the Video inputs page.

pub mod banner;
pub mod frames;
pub mod hls;
pub mod ts;

use std::time::Instant;

use serde::Serialize;

/// A program (a service of a transport stream, a variant of HLS).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub number: u16,
    pub name: String,
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<(String, String)>,
    /// The indexes of its streams.
    pub streams: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<ts::Event>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<ts::Event>,
    pub scrambled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ca_systems: Vec<String>,
}

/// A stream of an input.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u16>,
    /// video, audio, subtitle, data, attachment.
    pub kind: String,
    /// ffmpeg's name ("h264") and a readable one ("H.264 / AVC").
    pub codec: String,
    pub codec_name: String,
    /// "High", "LC", "High@4.1"…
    #[serde(skip_serializing_if = "String::is_empty")]
    pub profile: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dispositions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sar: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dar: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub scan: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pix_fmt: String,
    /// "10 bits, 4:2:2".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub depth: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub range: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub matrix: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub primaries: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub transfer: String,
    /// SDR, HDR10 / PQ, HLG.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hdr: String,
    /// Closed captions in the video (A/53).
    pub captions: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub layout: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sample_fmt: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<(String, String)>,
    /// Anything else known of it (descriptors of its PMT, side data…).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub format: String,
    pub format_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub duration: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate_kbps: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<(String, String)>,
}

/// An NDI source as received.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NdiView {
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub rate: (u32, u32),
    pub aspect: f32,
    /// Progressive, interleaved, field 0, field 1.
    pub format: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode: String,
    pub audio_rate: u32,
    pub audio_channels: usize,
    pub audio_samples: usize,
    /// The XML of the last frame that had some.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub frame_metadata: String,
    /// The source's own metadata frames (product, capabilities…).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    pub video_frames: u64,
    pub audio_frames: u64,
}

/// An ancillary packet seen in the SDI signal.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anc {
    pub did: u8,
    pub sdid: u8,
    pub line: u32,
    /// What it is (SMPTE ST 291 registry): "CEA-708 (SMPTE 334)", "AFD"…
    pub name: String,
    pub count: u64,
}

/// An SDI input of a DeckLink card.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiView {
    pub device: u32,
    pub mode: String,
    /// What the card detected: YCbCr 4:2:2 or RGB 4:4:4, its depth.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub signal: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode: String,
    /// RP 188 (ATC), VITC…
    #[serde(skip_serializing_if = "String::is_empty")]
    pub timecode_kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub colorspace: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hdr: String,
    pub audio_channels: usize,
    pub anc: Vec<Anc>,
}

/// Everything known of an input.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// Its address, NDI source or card.
    pub source: String,
    pub live: bool,
    pub status: String,
    /// Where the facts come from ("ffmpeg", "MPEG-TS", "HLS"…).
    pub how: Vec<String>,
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    pub programs: Vec<Program>,
    pub streams: Vec<Stream>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<frames::FrameStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<ts::TsView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hls: Option<hls::HlsView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ndi: Option<NdiView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdi: Option<SdiView>,
    /// A web platform's live: the page and the stream found on it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<crate::weblive::Resolved>,
}

/// What a running input has told so far.
#[derive(Default)]
pub struct Facts {
    pub banner: Option<banner::Banner>,
    /// The lines of ffmpeg's `Input #0` while it is being written.
    pub banner_lines: Option<Vec<String>>,
    pub frames: frames::Frames,
    pub ts: Option<ts::TsMonitor>,
    pub ndi: Option<NdiView>,
    pub sdi: Option<SdiView>,
    /// The playlists of an HLS input, read when asked (at most every few seconds).
    pub hls: Option<(Instant, hls::HlsView, Option<ts::TsView>)>,
    /// A web platform's live: the stream found on its page.
    pub web: Option<crate::weblive::Resolved>,
    /// The levels measured lately (`signalstats`): lowest and highest luma
    /// (code values), share of the pixels out of the broadcast range (0–1),
    /// when.
    pub levels: Option<([f32; 3], Instant)>,
}

impl Facts {
    /// Starts again (a new connection).
    pub fn reset(&mut self, relayed: bool) {
        let (hls, web) = (self.hls.take(), self.web.take());
        *self = Facts {
            ts: relayed.then(|| ts::TsMonitor::new(true)),
            hls,
            web,
            ..Default::default()
        };
    }

    /// A line of the decoder's log; true when it is taken (not for the
    /// log shown to people). A timecode it carries goes to `timecode`.
    pub fn line(&mut self, line: &str, now: Instant, timecode: impl FnOnce(String)) -> bool {
        if line.starts_with("[Parsed_metadata_") {
            // "lavfi.signalstats.YMAX=235"; the "frame:…" lines are taken too.
            if let Some((k, v)) = line
                .split_once("lavfi.signalstats.")
                .and_then(|(_, r)| r.split_once('='))
            {
                let i = match k {
                    "YMIN" => Some(0),
                    "YMAX" => Some(1),
                    "BRNG" => Some(2),
                    _ => None,
                };
                if let (Some(i), Ok(v)) = (i, v.trim().parse::<f32>()) {
                    let l = self.levels.get_or_insert(([f32::NAN; 3], now));
                    l.0[i] = v;
                    l.1 = now;
                }
            }
            return true;
        }
        if let Some(said) = self.frames.line(line, now) {
            if let frames::Said::Timecode(tc) = said {
                timecode(tc);
            }
            return true;
        }
        if line.starts_with("Input #0") {
            self.banner_lines = Some(vec![line.to_string()]);
            return false;
        }
        if let Some(lines) = self.banner_lines.as_mut() {
            if line.starts_with(' ') {
                lines.push(line.to_string());
            } else {
                self.banner = banner::Banner::parse(lines);
                self.banner_lines = None;
            }
        }
        false
    }

    /// TR 101 290 errors of the last 5 s (first, second priority).
    pub fn ts_errors(&self, now: Instant) -> (bool, bool) {
        self.ts.as_ref().map_or((false, false), |t| {
            t.recent_errors(now, std::time::Duration::from_secs(5))
        })
    }

    /// What the picture and sound are, in short (the Signal widget, the
    /// scopes' graticules): the decoded pictures first, then the banner.
    pub fn video_info(&self, now: Instant) -> strata_mv_core::signal::VideoInfo {
        use strata_mv_core::signal::{AudioTrack, VideoInfo};
        let f = &self.frames;
        let [range, matrix, primaries, transfer] = f.color().clone();
        let mut v = VideoInfo {
            pix_fmt: f.pix_fmt().to_string(),
            size: f.size().to_string(),
            range,
            matrix,
            primaries,
            transfer,
            dolby_vision: f.seen("Dolby Vision", now),
            hdr10_plus: f.seen("HDR10+ (dynamique)", now),
            ..Default::default()
        };
        let (mastering, light) = f.hdr_metadata();
        v.mastering = mastering.to_string();
        let number_after = |key: &str| -> u32 {
            light
                .split(key)
                .nth(1)
                .map(|r| {
                    r.chars()
                        .take_while(char::is_ascii_digit)
                        .collect::<String>()
                })
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        };
        v.max_cll = number_after("MaxCLL=");
        v.max_fall = number_after("MaxFALL=");
        if let Some(b) = &self.banner {
            if let Some(s) = b.streams.iter().find(|s| s.kind == "video") {
                v.codec = if s.codec_name.is_empty() {
                    codec_name(&s.codec).to_string()
                } else {
                    s.codec_name.clone()
                };
                if v.codec.is_empty() {
                    v.codec = s.codec.clone();
                }
                v.profile = s.profile.clone();
                if v.pix_fmt.is_empty() {
                    v.pix_fmt = s.pix_fmt.clone();
                }
                if v.size.is_empty() {
                    if let (Some(w), Some(h)) = (s.width, s.height) {
                        v.size = format!("{w}x{h}");
                    }
                }
                for (mine, theirs) in [
                    (&mut v.range, &s.range),
                    (&mut v.matrix, &s.matrix),
                    (&mut v.primaries, &s.primaries),
                    (&mut v.transfer, &s.transfer),
                ] {
                    if mine.is_empty() {
                        *mine = theirs.clone();
                    }
                }
                if s.details
                    .iter()
                    .any(|d| d.contains("DOVI") || d.contains("Dolby Vision"))
                {
                    v.dolby_vision = true;
                }
                v.hdr = s.hdr.clone();
            }
            for s in b.streams.iter().filter(|s| s.kind == "audio") {
                let name = if s.codec_name.is_empty() {
                    codec_name(&s.codec)
                } else {
                    &s.codec_name
                };
                let all =
                    format!("{} {} {}", s.codec, s.profile, s.details.join(" ")).to_lowercase();
                v.audio.push(AudioTrack {
                    codec: if name.is_empty() {
                        s.codec.clone()
                    } else {
                        name.to_string()
                    },
                    layout: s.layout.clone(),
                    rate: s.sample_rate.unwrap_or(0),
                    language: s.language.clone(),
                    dolby: matches!(
                        s.codec.as_str(),
                        "ac3" | "eac3" | "truehd" | "ac4" | "dolby_e"
                    ) || all.contains("dolby"),
                    atmos: all.contains("atmos") || all.contains("joc"),
                });
            }
        }
        let d = banner::depth_chroma(&v.pix_fmt);
        // "10 bits, 4:2:2"
        let mut parts = d.split(", ");
        if let (Some(b), Some(c)) = (parts.next(), parts.next()) {
            v.bits = b
                .split(' ')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            v.chroma = c.to_string();
        }
        if v.hdr.is_empty() && !v.transfer.is_empty() {
            v.hdr = banner::hdr(&v.transfer).to_string();
        }
        // The levels of the last seconds, luma in % of the nominal range
        // (0 % black, 100 % white).
        if let Some((l, at)) = self.levels {
            if now.duration_since(at).as_secs() < 3 && l.iter().all(|x| x.is_finite()) {
                let k = 2f32.powi(v.bits.max(8) as i32 - 8);
                let pct = |y: f32| (y - 16.0 * k) / (219.0 * k) * 100.0;
                v.luma = Some([pct(l[0]), pct(l[1])]);
                v.illegal = Some(l[2] * 100.0);
            }
        }
        v
    }

    /// The report: the decoder's view, completed by the transport's.
    pub fn report(&self, now: Instant) -> Report {
        let mut r = Report {
            web: self.web.clone(),
            ..Default::default()
        };
        if let Some(b) = &self.banner {
            r.how.push("ffmpeg (ouverture de l’entrée)".into());
            r.container = Some(Container {
                format: b.format.clone(),
                format_name: format_name(&b.format).to_string(),
                duration: b.duration.clone(),
                start: b.start,
                bitrate_kbps: b.bitrate_kbps,
                metadata: b.metadata.clone(),
            });
            r.programs = b.programs.clone();
            r.streams = b.streams.clone();
        }
        let frames = self.frames.view(now);
        if frames.frames > 0 {
            r.how.push("ffmpeg (chaque image décodée)".into());
            r.frames = Some(frames);
        }
        let ts = self
            .ts
            .as_ref()
            .filter(|t| t.packets() > 0)
            .map(|t| t.view(now))
            .or_else(|| self.hls.as_ref().and_then(|(_, _, t)| t.clone()));
        if let Some(t) = &ts {
            r.how.push(if t.live {
                "MPEG-TS (en continu)".into()
            } else {
                "MPEG-TS (dernier segment HLS)".into()
            });
            // HLS: ffmpeg's programs are the variants, not the segment's services.
            if t.live {
                merge_ts(&mut r, t);
            }
        }
        if let Some((_, h, _)) = &self.hls {
            r.how.push("HLS (listes de lecture)".into());
            for p in &mut r.programs {
                p.kind = "Variante HLS".into();
            }
            r.hls = Some(h.clone());
        }
        r.ts = ts;
        if let Some(n) = &self.ndi {
            r.how.push("NDI".into());
            r.ndi = Some(n.clone());
        }
        if let Some(s) = &self.sdi {
            r.how.push("DeckLink".into());
            r.sdi = Some(s.clone());
        }
        // What the pictures tell, on the stream decoded (the first video).
        if let (Some(f), Some(v)) = (&r.frames, r.streams.iter_mut().find(|s| s.kind == "video")) {
            if f.captions {
                v.captions = true;
            }
            if v.scan.is_empty() {
                v.scan = f.scan.clone();
            }
            if v.transfer.is_empty() && !f.transfer.is_empty() {
                v.matrix = f.matrix.clone();
                v.primaries = f.primaries.clone();
                v.transfer = f.transfer.clone();
                v.hdr = banner::hdr(&f.transfer).to_string();
            }
        }
        for s in &mut r.streams {
            if s.codec_name.is_empty() {
                s.codec_name = codec_name(&s.codec).to_string();
            }
            if s.kind == "video" && s.depth.is_empty() {
                s.depth = banner::depth_chroma(&s.pix_fmt);
            }
        }
        r
    }
}

/// The transport stream's tables over what ffmpeg said: names, EPG,
/// descriptors, profile and level, rates.
fn merge_ts(r: &mut Report, t: &ts::TsView) {
    for svc in &t.services {
        let at = match r.programs.iter().position(|p| p.number == svc.number) {
            Some(i) => i,
            None => {
                r.programs.push(Program {
                    number: svc.number,
                    ..Default::default()
                });
                r.programs.len() - 1
            }
        };
        let p = &mut r.programs[at];
        if !svc.name.is_empty() {
            p.name = svc.name.clone();
        }
        if !svc.provider.is_empty() {
            p.provider = svc.provider.clone();
        }
        p.kind = svc.kind.clone();
        p.now = svc.now.clone();
        p.next = svc.next.clone();
        p.scrambled = svc.scrambled;
        p.ca_systems = svc.ca_systems.clone();
        for es in &svc.streams {
            let rate = t.pids.iter().find(|x| x.pid == es.pid).map(|x| x.kbps);
            match r.streams.iter_mut().find(|s| s.pid == Some(es.pid)) {
                Some(s) => {
                    // ffmpeg's profile ("Constrained Baseline") with the SPS's level.
                    if let Some((name, level)) = es.profile.split_once('@') {
                        if s.profile.is_empty() {
                            s.profile = es.profile.clone();
                        } else if !s.profile.contains('@') {
                            let _ = name;
                            s.profile = format!("{}@{level}", s.profile);
                        }
                    }
                    if s.language.is_empty() {
                        s.language = es.language.clone();
                    }
                    for d in &es.details {
                        if !s.details.contains(d) {
                            s.details.push(d.clone());
                        }
                    }
                    // A codec ffmpeg does not name ("none", "bin_data"): the PMT's.
                    if s.codec == "bin_data" || codec_name(&s.codec).is_empty() {
                        s.codec_name = es.kind.clone();
                    }
                    if let Some(rate) = rate.filter(|r| *r > 0.0) {
                        s.bitrate_kbps = Some(rate);
                    }
                }
                None => {
                    // Not opened by ffmpeg (data it does not know): as the PMT says.
                    let index = 1000 + u32::from(es.pid);
                    if !p.streams.contains(&index) {
                        p.streams.push(index);
                    }
                    r.streams.push(Stream {
                        index,
                        pid: Some(es.pid),
                        kind: es.class.clone(),
                        codec: String::new(),
                        codec_name: es.kind.clone(),
                        profile: es.profile.clone(),
                        language: es.language.clone(),
                        bitrate_kbps: rate,
                        details: es.details.clone(),
                        ..Default::default()
                    });
                }
            }
        }
    }
}

/// A container by its ffmpeg name.
pub fn format_name(f: &str) -> &'static str {
    match f {
        "mpegts" => "MPEG-TS",
        "hls" => "HLS",
        "flv" | "live_flv" => "FLV (RTMP)",
        "rtsp" => "RTSP",
        "rtp" => "RTP",
        "mov,mp4,m4a,3gp,3g2,mj2" => "MP4 / QuickTime",
        "matroska,webm" => "Matroska / WebM",
        "mxf" => "MXF",
        "dash" => "MPEG-DASH",
        "data" => "Données brutes",
        "sdp" => "SDP",
        _ => "",
    }
}

/// A codec by its ffmpeg name.
pub fn codec_name(c: &str) -> &'static str {
    match c {
        "h264" => "H.264 / AVC",
        "hevc" => "H.265 / HEVC",
        "vvc" => "H.266 / VVC",
        "av1" => "AV1",
        "vp9" => "VP9",
        "vp8" => "VP8",
        "mpeg2video" => "MPEG-2 vidéo",
        "mpeg1video" => "MPEG-1 vidéo",
        "mpeg4" => "MPEG-4 Part 2",
        "jpeg2000" => "JPEG 2000",
        "jpegxs" => "JPEG XS",
        "prores" => "Apple ProRes",
        "dnxhd" => "Avid DNxHD / DNxHR",
        "mjpeg" => "Motion JPEG",
        "rawvideo" => "Vidéo non compressée",
        "aac" => "AAC",
        "aac_latm" => "AAC (LATM)",
        "mp2" => "MPEG-1 Layer II",
        "mp3" => "MP3",
        "ac3" => "AC-3 (Dolby Digital)",
        "eac3" => "E-AC-3 (Dolby Digital Plus)",
        "ac4" => "AC-4",
        "dts" => "DTS",
        "opus" => "Opus",
        "flac" => "FLAC",
        "s302m" => "AES3 (SMPTE 302M)",
        "pcm_s16le" | "pcm_s16be" => "PCM 16 bits",
        "pcm_s24le" | "pcm_s24be" => "PCM 24 bits",
        "pcm_s32le" | "pcm_s32be" => "PCM 32 bits",
        "dvb_teletext" => "Télétexte",
        "dvb_subtitle" => "Sous-titres DVB",
        "eia_608" => "CEA-608",
        "webvtt" => "WebVTT",
        "mov_text" => "Sous-titres MP4 (tx3g)",
        "hdmv_pgs_subtitle" => "Sous-titres PGS",
        "subrip" => "SubRip",
        "ass" => "ASS",
        "arib_caption" => "Sous-titres ARIB",
        "scte_35" => "SCTE-35",
        "epg" => "EPG (EIT)",
        "klv" => "KLV",
        "timed_id3" => "ID3 (métadonnées)",
        "smpte_2038" => "SMPTE 2038 (ANC)",
        "bin_data" => "Données",
        "ttf" | "otf" => "Police jointe",
        _ => "",
    }
}

/// The name of an ancillary packet (SMPTE ST 291 registry).
pub fn anc_name(did: u8, sdid: u8) -> &'static str {
    match (did, sdid) {
        (0x41, 0x05) => "AFD et bar data (SMPTE 2016-3)",
        (0x41, 0x06) => "Pan-scan (SMPTE 2016-4)",
        (0x41, 0x07) => "SCTE-104 (insertion publicitaire)",
        (0x41, 0x08) => "DVB/SCTE VBI (SMPTE 2031)",
        (0x43, 0x01) => "Identification de trame (ITU-R BT.1685)",
        (0x43, 0x02) => "Sous-titres OP-47 (SDP)",
        (0x43, 0x03) => "Télétexte OP-47 (multi-paquets)",
        (0x43, 0x05) => "Métadonnées de type acquisition (RDD 18)",
        (0x45, _) => "Métadonnées audio (SMPTE 2020)",
        (0x50, 0x01) => "WSS (RDD 8)",
        (0x51, 0x01) => "Métadonnées de film (RP 215)",
        (0x60, 0x60) => "Timecode ATC (SMPTE 12-2 / RP 188)",
        (0x61, 0x01) => "Sous-titres CEA-708 (SMPTE 334)",
        (0x61, 0x02) => "Sous-titres CEA-608 (SMPTE 334)",
        (0x62, 0x01) => "Programme DTV (SMPTE 334)",
        (0x62, 0x02) => "Données de diffusion VBI (SMPTE 334)",
        (0x64, 0x64) => "Timecode LTC dans le VANC (RP 196)",
        (0x64, 0x7F) => "Timecode VITC dans le VANC (RP 196)",
        (0x41, 0x0C) => "Métadonnées HDR dynamiques (SMPTE 2108)",
        (0x41, 0x01) => "Identification de charge utile (SMPTE 352)",
        (0x80, _) => "Paquet marqué à supprimer",
        (0xE0..=0xEF, _) => "Données d’utilisateur (privé)",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_an_hdr10_picture_is() {
        let mut f = Facts::default();
        let now = Instant::now();
        for l in [
            "[Parsed_showinfo_0 @ 0x55] n:   1 pts:     40 pts_time:0.04    duration:     40 fmt:yuv420p10le cl:left sar:1/1 s:1280x720 i:P iskey:0 type:B",
            "[Parsed_showinfo_0 @ 0x55]   side data - Mastering display metadata: has_primaries:1 has_luminance:1 r(0.6800,0.3200) g(0.2650,0.6900) b(0.1500 0.0600) wp(0.3127, 0.3290) min_luminance=0.000100, max_luminance=1000.000000",
            "[Parsed_showinfo_0 @ 0x55]   side data - Content light level metadata: MaxCLL=1000, MaxFALL=400",
            "[Parsed_showinfo_0 @ 0x55] color_range:tv color_space:bt2020nc color_primaries:bt2020 color_trc:smpte2084",
        ] {
            f.line(l, now, |_| {});
        }
        let v = f.video_info(now);
        assert_eq!((v.bits, v.chroma.as_str()), (10, "4:2:0"));
        assert_eq!(v.transfer, "smpte2084");
        assert!(v.wide());
        assert_eq!(v.dynamic(), "HDR10");
        assert_eq!((v.max_cll, v.max_fall), (1000, 400));
        assert!(v.mastering.contains("max_luminance=1000"));
    }

    #[test]
    fn a_report_puts_the_log_and_the_transport_together() {
        let mut f = Facts::default();
        f.reset(true);
        let now = Instant::now();
        let mut tc = None;
        for l in [
            "Input #0, mpegts, from 'pipe:0':",
            "  Duration: N/A, start: 1.4, bitrate: N/A",
            "  Program 1 ",
            "  Stream #0:0[0x100]: Video: h264 (High) ([27][0][0][0] / 0x001B), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 25 fps, 25 tbr, 90k tbn",
            "Stream mapping:",
            "[Parsed_showinfo_0 @ 0x1] n:   0 pts:0 fmt:yuv420p s:1920x1080 i:P iskey:1 type:I",
            "[Parsed_showinfo_0 @ 0x1]   side data - timecode - 10:00:00:00",
        ] {
            f.line(l, now, |t| tc = Some(t));
        }
        assert_eq!(tc.as_deref(), Some("10:00:00:00"));
        let r = f.report(now);
        assert_eq!(r.container.as_ref().unwrap().format_name, "MPEG-TS");
        assert_eq!(r.streams[0].codec_name, "H.264 / AVC");
        assert_eq!(r.streams[0].depth, "8 bits, 4:2:0");
        assert_eq!(r.frames.as_ref().unwrap().timecode, "10:00:00:00");
        assert_eq!(anc_name(0x61, 0x01), "Sous-titres CEA-708 (SMPTE 334)");
    }
}
