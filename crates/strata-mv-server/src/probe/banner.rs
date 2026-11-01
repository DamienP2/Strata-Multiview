//! What ffmpeg says of an input when it opens it (the `Input #0` part of its
//! log): the container, its programs and their names, every stream with its
//! codec, profile, picture, colours, scan, audio layout, language, closed
//! captions and metadata.

use super::{Program, Stream};

/// The input as ffmpeg opened it.
#[derive(Debug, Clone, Default)]
pub struct Banner {
    /// "mpegts", "hls", "flv", "rtsp"…
    pub format: String,
    pub duration: String,
    pub start: Option<f64>,
    pub bitrate_kbps: Option<f64>,
    pub metadata: Vec<(String, String)>,
    pub programs: Vec<Program>,
    pub streams: Vec<Stream>,
}

/// Where the next metadata lines go.
enum Into {
    Input,
    Program(usize),
    Stream(usize),
}

impl Banner {
    /// Reads the lines from `Input #0, …` to the end of its streams.
    pub fn parse(lines: &[String]) -> Option<Banner> {
        let mut start = lines.iter().position(|l| l.starts_with("Input #0"))?;
        // A sound source (Dante): its picture is made here (lavfi), its
        // sound is the second input.
        if lines[start].starts_with("Input #0, lavfi") {
            if let Some(s) = lines.iter().position(|l| l.starts_with("Input #1")) {
                start = s;
            }
        }
        let head = &lines[start];
        let mut b = Banner {
            format: head
                .split_once(", ")
                .map_or("", |x| x.1)
                .split(", from ")
                .next()
                .unwrap_or("")
                .to_string(),
            ..Default::default()
        };
        let mut into = Into::Input;
        let mut program: Option<usize> = None;
        let mut in_metadata = false;
        for line in &lines[start + 1..] {
            // The end of the input: what ffmpeg does with it.
            if !line.starts_with(' ') {
                break;
            }
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if let Some(rest) = t.strip_prefix("Duration: ") {
                for part in rest.split(", ") {
                    if let Some(v) = part.strip_prefix("start: ") {
                        b.start = v.trim().parse().ok();
                    } else if let Some(v) = part.strip_prefix("bitrate: ") {
                        b.bitrate_kbps = v
                            .trim_end_matches(" kb/s")
                            .trim()
                            .parse()
                            .ok()
                            .filter(|v: &f64| *v > 0.0);
                    } else if b.duration.is_empty() {
                        b.duration = part.trim().to_string();
                    }
                }
                in_metadata = false;
                continue;
            }
            if let Some(rest) = t.strip_prefix("Program ") {
                let number = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                b.programs.push(Program {
                    number,
                    ..Default::default()
                });
                program = Some(b.programs.len() - 1);
                into = Into::Program(b.programs.len() - 1);
                in_metadata = false;
                continue;
            }
            if t.starts_with("No Program") {
                program = None;
                in_metadata = false;
                continue;
            }
            if t == "Metadata:" {
                in_metadata = true;
                continue;
            }
            if t.starts_with("Side data:") {
                in_metadata = false;
                continue;
            }
            if let Some(rest) = t
                .strip_prefix("Stream #0:")
                .or_else(|| t.strip_prefix("Stream #1:"))
            {
                in_metadata = false;
                let Some(s) = stream(rest) else {
                    continue;
                };
                let at = match b.streams.iter().position(|x| x.index == s.index) {
                    Some(i) => i,
                    None => {
                        b.streams.push(s);
                        b.streams.len() - 1
                    }
                };
                if let Some(p) = program {
                    let index = b.streams[at].index;
                    if !b.programs[p].streams.contains(&index) {
                        b.programs[p].streams.push(index);
                    }
                }
                into = Into::Stream(at);
                continue;
            }
            if in_metadata {
                if let Some((k, v)) = t.split_once(':') {
                    let (k, v) = (k.trim().to_string(), v.trim().to_string());
                    match into {
                        Into::Input => b.metadata.push((k, v)),
                        Into::Program(i) => {
                            let p = &mut b.programs[i];
                            match k.as_str() {
                                "service_name" => p.name = v,
                                "service_provider" => p.provider = v,
                                _ => p.metadata.push((k, v)),
                            }
                        }
                        Into::Stream(i) => {
                            let s = &mut b.streams[i];
                            match k.as_str() {
                                "title" | "handler_name" if s.title.is_empty() => s.title = v,
                                "language" if s.language.is_empty() => s.language = v,
                                _ => s.metadata.push((k, v)),
                            }
                        }
                    }
                }
            } else if let Into::Stream(i) = into {
                // Side data of a stream ("displaymatrix: rotation…", "DOVI configuration…").
                b.streams[i].details.push(t.to_string());
            }
        }
        Some(b)
    }
}

/// Splits on ", " outside brackets and parentheses.
fn parts(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 && chars.get(i + 1) == Some(&' ') {
            out.push(std::mem::take(&mut cur).trim().to_string());
            i += 2;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// `1[0x101](fra): Audio: mp2 (…), 44100 Hz, mono, fltp, 384 kb/s (default)`.
fn stream(rest: &str) -> Option<Stream> {
    let (id, body) = rest.split_once(": ")?;
    let index: u32 = id
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    let pid = id
        .split_once("[0x")
        .and_then(|(_, r)| r.split(']').next())
        .and_then(|h| u16::from_str_radix(h, 16).ok());
    let language = id
        .rsplit_once('(')
        .map(|(_, r)| r.trim_end_matches(')').to_string())
        .unwrap_or_default();
    let (kind, desc) = body.split_once(": ")?;
    let mut s = Stream {
        index,
        pid,
        language,
        kind: match kind {
            "Video" => "video",
            "Audio" => "audio",
            "Subtitle" => "subtitle",
            "Attachment" => "attachment",
            _ => "data",
        }
        .to_string(),
        ..Default::default()
    };
    // The dispositions at the end: "(default) (hearing impaired)".
    let mut desc = desc.trim().to_string();
    while desc.ends_with(')') {
        let Some(open) = desc.rfind(" (") else {
            break;
        };
        let word = desc[open + 2..desc.len() - 1].to_string();
        if !DISPOSITIONS.contains(&word.as_str()) {
            break;
        }
        s.dispositions.push(word);
        desc.truncate(open);
    }
    s.dispositions.reverse();
    let list = parts(&desc);
    let Some(first) = list.first() else {
        return Some(s);
    };
    // "h264 (High) ([27][0][0][0] / 0x001B)": the codec, its profile, its tag.
    let codec = first.split_whitespace().next().unwrap_or("").to_string();
    for group in paren_groups(first) {
        if group.contains(" / 0x") || group.starts_with('[') {
            continue;
        }
        if s.profile.is_empty() {
            s.profile = group;
        }
    }
    s.codec = codec;
    for p in &list[1..] {
        let p = p.as_str();
        if p == "Closed Captions" {
            s.captions = true;
        } else if let Some(v) = p.strip_suffix(" kb/s") {
            s.bitrate_kbps = v.trim().parse().ok();
        } else if let Some(v) = p.strip_suffix(" fps") {
            s.fps = v.trim().parse().ok();
        } else if p.ends_with(" tbr") || p.ends_with(" tbn") || p.ends_with(" tbc") {
        } else if let Some(v) = p.strip_suffix(" Hz") {
            s.sample_rate = v.trim().parse().ok();
        } else if s.kind == "video" && s.width.is_none() && size(p).is_some() {
            let (w, h) = size(p).unwrap_or((0, 0));
            s.width = Some(w);
            s.height = Some(h);
            if let Some(sar) = p.split("SAR ").nth(1) {
                s.sar = sar.split_whitespace().next().unwrap_or("").to_string();
            }
            if let Some(dar) = p.split("DAR ").nth(1) {
                s.dar = dar
                    .trim_end_matches(']')
                    .split(']')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
        } else if s.kind == "video" && s.pix_fmt.is_empty() {
            pixel(&mut s, p);
        } else if s.kind == "audio" && s.layout.is_empty() {
            s.layout = p.to_string();
        } else if s.kind == "audio" && s.sample_fmt.is_empty() {
            s.sample_fmt = p.to_string();
        } else {
            s.details.push(p.to_string());
        }
    }
    s.hdr = hdr(&s.transfer).to_string();
    Some(s)
}

const DISPOSITIONS: &[&str] = &[
    "default",
    "dub",
    "original",
    "comment",
    "lyrics",
    "karaoke",
    "forced",
    "hearing impaired",
    "visual impaired",
    "clean effects",
    "attached pic",
    "timed thumbnails",
    "captions",
    "descriptions",
    "metadata",
    "dependent",
    "still image",
];

/// The contents of the top-level parentheses of `s`.
fn paren_groups(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut depth, mut cur) = (0, String::new());
    for c in s.chars() {
        match c {
            '(' => {
                if depth > 0 {
                    cur.push(c);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.push(c);
                }
            }
            _ if depth > 0 => cur.push(c),
            _ => {}
        }
    }
    out
}

fn size(p: &str) -> Option<(u32, u32)> {
    let word = p.split_whitespace().next()?;
    let (w, h) = word.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// "yuv420p10le(tv, bt2020nc/bt2020/smpte2084, progressive)".
fn pixel(s: &mut Stream, p: &str) {
    let (fmt, inside) = match p.split_once('(') {
        Some((f, r)) => (f.trim(), r.trim_end_matches(')')),
        None => (p.trim(), ""),
    };
    s.pix_fmt = fmt.to_string();
    for item in inside.split(", ").map(str::trim).filter(|x| !x.is_empty()) {
        match item {
            "tv" | "pc" => {
                s.range = if item == "tv" {
                    "limitée (TV)"
                } else {
                    "complète (PC)"
                }
                .to_string()
            }
            "progressive" => s.scan = "progressif".into(),
            "top first" => s.scan = "entrelacé, trame haute d’abord (TFF)".into(),
            "bottom first" => s.scan = "entrelacé, trame basse d’abord (BFF)".into(),
            "top coded first (swapped)" => s.scan = "entrelacé, TFF codé (inversé)".into(),
            "bottom coded first (swapped)" => s.scan = "entrelacé, BFF codé (inversé)".into(),
            "left" | "center" | "topleft" | "top" | "bottomleft" | "bottom" => {}
            _ => {
                let c: Vec<&str> = item.split('/').collect();
                let c: Vec<&str> = c
                    .into_iter()
                    .map(|x| if x == "unknown" { "" } else { x })
                    .collect();
                match c.as_slice() {
                    [m, p, t] => {
                        s.matrix = m.to_string();
                        s.primaries = p.to_string();
                        s.transfer = t.to_string();
                    }
                    [all] => {
                        s.matrix = all.to_string();
                        s.primaries = all.to_string();
                        s.transfer = all.to_string();
                    }
                    _ => {}
                }
            }
        }
    }
}

/// HDR from the transfer characteristics.
pub fn hdr(transfer: &str) -> &'static str {
    match transfer {
        "smpte2084" => "HDR10 / PQ",
        "arib-std-b67" => "HLG",
        "" | "unknown" => "",
        _ => "SDR",
    }
}

/// The depth and chroma of a pixel format ("10 bits, 4:2:0").
pub fn depth_chroma(fmt: &str) -> String {
    let f = fmt
        .strip_suffix("le")
        .or_else(|| fmt.strip_suffix("be"))
        .unwrap_or(fmt);
    let chroma = if f.starts_with("yuv420")
        || f.starts_with("yuvj420")
        || f == "nv12"
        || f.starts_with("p0")
    {
        "4:2:0"
    } else if f.starts_with("yuv422")
        || f.starts_with("yuvj422")
        || f.contains("422")
        || f == "v210"
        || f.starts_with("p2")
    {
        "4:2:2"
    } else if f.starts_with("yuv444") || f.starts_with("yuvj444") || f.starts_with("p4") {
        "4:4:4"
    } else if f.starts_with("gbr") || f.starts_with("rgb") || f.starts_with("bgr") {
        "RGB"
    } else if f.starts_with("gray") {
        "monochrome"
    } else {
        return String::new();
    };
    let bits = if f == "v210" {
        10
    } else if f.len() == 4 && f.starts_with('p') {
        // p010, p016, p210, p410.
        f[2..].parse().unwrap_or(8)
    } else if let Some(n) = f.strip_prefix("gray") {
        n.parse().unwrap_or(8)
    } else {
        // yuv420p10 → 10, yuv420p → 8.
        f.rsplit_once('p')
            .and_then(|(_, n)| n.parse::<u32>().ok())
            .unwrap_or(8)
    };
    format!("{bits} bits, {chroma}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn reads_an_mpegts_input() {
        let b = Banner::parse(&lines(
            "Input #0, mpegts, from 'udp://127.0.0.1:5001':
  Duration: N/A, start: 1.400000, bitrate: N/A
  Program 1
    Metadata:
      service_name    : France 2
      service_provider: France TV
  Stream #0:0[0x100]: Video: h264 (High) ([27][0][0][0] / 0x001B), yuv420p10le(tv, bt2020nc/bt2020/smpte2084, top first), 1920x1080 [SAR 1:1 DAR 16:9], Closed Captions, 25 fps, 50 tbr, 90k tbn
  Stream #0:1[0x101](fra): Audio: aac (LC) ([15][0][0][0] / 0x000F), 48000 Hz, 5.1(side), fltp, 384 kb/s (default)
  Stream #0:2[0x102](fra): Subtitle: dvb_teletext ([6][0][0][0] / 0x0006) (hearing impaired)
  Stream #0:3[0x103]: Data: scte_35
Stream mapping:",
        ))
        .unwrap();
        assert_eq!(b.format, "mpegts");
        assert_eq!(b.start, Some(1.4));
        assert_eq!(b.programs[0].name, "France 2");
        assert_eq!(b.programs[0].provider, "France TV");
        assert_eq!(b.programs[0].streams, vec![0, 1, 2, 3]);
        let v = &b.streams[0];
        assert_eq!((v.codec.as_str(), v.profile.as_str()), ("h264", "High"));
        assert_eq!(v.pid, Some(0x100));
        assert_eq!((v.width, v.height), (Some(1920), Some(1080)));
        assert_eq!(v.dar, "16:9");
        assert!(v.captions);
        assert_eq!(v.fps, Some(25.0));
        assert_eq!(v.transfer, "smpte2084");
        assert_eq!(v.hdr, "HDR10 / PQ");
        assert!(v.scan.contains("TFF"));
        assert_eq!(depth_chroma(&v.pix_fmt), "10 bits, 4:2:0");
        assert_eq!(depth_chroma("yuv422p"), "8 bits, 4:2:2");
        assert_eq!(depth_chroma("p010le"), "10 bits, 4:2:0");
        assert_eq!(depth_chroma("v210"), "10 bits, 4:2:2");
        let a = &b.streams[1];
        assert_eq!(a.language, "fra");
        assert_eq!(a.sample_rate, Some(48000));
        assert_eq!(a.layout, "5.1(side)");
        assert_eq!(a.bitrate_kbps, Some(384.0));
        assert_eq!(a.dispositions, vec!["default"]);
        assert_eq!(b.streams[2].kind, "subtitle");
        assert_eq!(b.streams[2].dispositions, vec!["hearing impaired"]);
        assert_eq!(b.streams[3].codec, "scte_35");
    }

    #[test]
    fn reads_an_hls_input_and_its_variants() {
        let b = Banner::parse(&lines(
            "Input #0, hls, from 'https://cdn/master.m3u8':
  Duration: N/A, start: 0.000000, bitrate: N/A
  Program 0
    Metadata:
      variant_bitrate : 2500000
  Stream #0:0: Video: h264 (Main) ([27][0][0][0] / 0x001B), yuv420p(tv, bt709), 1280x720 [SAR 1:1 DAR 16:9], 25 fps, 25 tbr, 90k tbn
    Metadata:
      variant_bitrate : 2500000
  Program 1
    Metadata:
      variant_bitrate : 6000000
  Stream #0:1: Video: h264 (High) ([27][0][0][0] / 0x001B), yuv420p(tv, bt709), 1920x1080 [SAR 1:1 DAR 16:9], 25 fps, 25 tbr, 90k tbn
Output #0, rawvideo, to 'pipe:1':",
        ))
        .unwrap();
        assert_eq!(b.programs.len(), 2);
        assert_eq!(b.programs[1].metadata[0].1, "6000000");
        assert_eq!(b.streams[1].height, Some(1080));
        assert_eq!(b.streams[0].hdr, "SDR");
    }
}
