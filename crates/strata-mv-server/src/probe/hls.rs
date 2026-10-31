//! HLS: the playlists of an input (its variants and renditions, the media
//! playlist's segments, times, keys and ad markers) and the transport stream
//! of its last segment.

use std::time::Duration;

use serde::Serialize;

use super::ts::{TsMonitor, TsView};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub bandwidth: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_bandwidth: Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub resolution: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub codecs: String,
    /// SDR, PQ, HLG.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub video_range: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub audio: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subtitles: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub closed_captions: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hdcp: String,
    pub uri: String,
}

/// An alternative rendition (EXT-X-MEDIA): audio, subtitles, captions.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendition {
    pub kind: String,
    pub group: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    pub default: bool,
    pub autoselect: bool,
    pub forced: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channels: String,
    /// CC1, SERVICE1…: captions carried in the video.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instream_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub characteristics: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uri: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub uri: String,
    pub version: u32,
    pub target_duration: f64,
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    /// Live (sliding window), EVENT or VOD.
    pub kind: String,
    pub segments: u32,
    pub total_s: f64,
    pub segment_min_s: f64,
    pub segment_max_s: f64,
    /// TS, fMP4, AAC, WebVTT…
    pub format: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub program_date_time: String,
    pub discontinuities: u32,
    /// AES-128, SAMPLE-AES, none.
    pub encryption: Vec<String>,
    /// Ad markers: CUE-OUT / CUE-IN, SCTE-35 DATERANGEs.
    pub cues: Vec<String>,
    /// Low-latency HLS (parts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_target_s: Option<f64>,
    pub byte_ranges: bool,
    pub gaps: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HlsView {
    pub url: String,
    pub master: bool,
    pub version: u32,
    pub independent_segments: bool,
    pub variants: Vec<Variant>,
    pub renditions: Vec<Rendition>,
    /// Trick-play playlists (EXT-X-I-FRAME-STREAM-INF).
    pub iframe_streams: u32,
    /// EXT-X-SESSION-DATA, EXT-X-SESSION-KEY, EXT-X-CONTENT-STEERING.
    pub session: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// The attributes of a tag: `BANDWIDTH=…,CODECS="…"`.
fn attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let Some((k, r)) = rest.split_once('=') else {
            break;
        };
        let (v, r) = if let Some(r) = r.strip_prefix('"') {
            match r.split_once('"') {
                Some((v, r)) => (v, r.strip_prefix(',').unwrap_or(r)),
                None => (r, ""),
            }
        } else {
            match r.split_once(',') {
                Some((v, r)) => (v, r),
                None => (r, ""),
            }
        };
        out.push((k.trim().to_string(), v.to_string()));
        rest = r;
    }
    out
}

fn get<'a>(a: &'a [(String, String)], k: &str) -> &'a str {
    a.iter()
        .find(|(x, _)| x == k)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

/// Resolves `uri` against the playlist at `base`.
pub fn resolve(base: &str, uri: &str) -> String {
    if uri.contains("://") {
        return uri.to_string();
    }
    let base = base.split(['?', '#']).next().unwrap_or(base);
    if let Some(path) = uri.strip_prefix('/') {
        // The scheme and host of the base.
        let host_end = base
            .find("://")
            .and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j))
            .unwrap_or(base.len());
        return format!("{}/{}", &base[..host_end], path);
    }
    match base.rfind('/') {
        Some(i) => format!("{}/{}", &base[..i], uri),
        None => uri.to_string(),
    }
}

/// A master playlist: its variants, renditions and session tags.
pub fn master(view: &mut HlsView, text: &str) {
    let mut pending: Option<Variant> = None;
    for line in text.lines().map(str::trim) {
        if let Some(r) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let a = attributes(r);
            pending = Some(Variant {
                bandwidth: get(&a, "BANDWIDTH").parse().unwrap_or(0),
                average_bandwidth: get(&a, "AVERAGE-BANDWIDTH").parse().ok(),
                resolution: get(&a, "RESOLUTION").to_string(),
                frame_rate: get(&a, "FRAME-RATE").parse().ok(),
                codecs: get(&a, "CODECS").to_string(),
                video_range: get(&a, "VIDEO-RANGE").to_string(),
                audio: get(&a, "AUDIO").to_string(),
                subtitles: get(&a, "SUBTITLES").to_string(),
                closed_captions: get(&a, "CLOSED-CAPTIONS").to_string(),
                hdcp: get(&a, "HDCP-LEVEL").to_string(),
                uri: String::new(),
            });
        } else if let Some(r) = line.strip_prefix("#EXT-X-MEDIA:") {
            let a = attributes(r);
            view.renditions.push(Rendition {
                kind: get(&a, "TYPE").to_string(),
                group: get(&a, "GROUP-ID").to_string(),
                name: get(&a, "NAME").to_string(),
                language: get(&a, "LANGUAGE").to_string(),
                default: get(&a, "DEFAULT") == "YES",
                autoselect: get(&a, "AUTOSELECT") == "YES",
                forced: get(&a, "FORCED") == "YES",
                channels: get(&a, "CHANNELS").to_string(),
                instream_id: get(&a, "INSTREAM-ID").to_string(),
                characteristics: get(&a, "CHARACTERISTICS").to_string(),
                uri: get(&a, "URI").to_string(),
            });
        } else if line.starts_with("#EXT-X-I-FRAME-STREAM-INF:") {
            view.iframe_streams += 1;
        } else if let Some(v) = line.strip_prefix("#EXT-X-VERSION:") {
            view.version = v.trim().parse().unwrap_or(0);
        } else if line == "#EXT-X-INDEPENDENT-SEGMENTS" {
            view.independent_segments = true;
        } else if let Some(r) = line.strip_prefix("#EXT-X-SESSION-DATA:") {
            let a = attributes(r);
            view.session.push(format!(
                "Données « {} » : {}",
                get(&a, "DATA-ID"),
                get(&a, "VALUE")
            ));
        } else if let Some(r) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            view.session
                .push(format!("Clé de session {}", get(&attributes(r), "METHOD")));
        } else if let Some(r) = line.strip_prefix("#EXT-X-CONTENT-STEERING:") {
            view.session.push(format!(
                "Pilotage de contenu {}",
                get(&attributes(r), "SERVER-URI")
            ));
        } else if !line.is_empty() && !line.starts_with('#') {
            if let Some(mut v) = pending.take() {
                v.uri = line.to_string();
                view.variants.push(v);
            }
        }
    }
}

/// A media playlist: its segments, times, keys and markers.
pub fn media(uri: &str, text: &str) -> Media {
    let mut m = Media {
        uri: uri.to_string(),
        kind: "Direct (fenêtre glissante)".into(),
        segment_min_s: f64::MAX,
        ..Default::default()
    };
    let mut keys = Vec::new();
    let mut last_segment = String::new();
    let mut fmp4 = false;
    for line in text.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("#EXTINF:") {
            let d: f64 = v
                .split(',')
                .next()
                .unwrap_or("0")
                .trim()
                .parse()
                .unwrap_or(0.0);
            m.segments += 1;
            m.total_s += d;
            m.segment_min_s = m.segment_min_s.min(d);
            m.segment_max_s = m.segment_max_s.max(d);
        } else if let Some(v) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            m.target_duration = v.trim().parse().unwrap_or(0.0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            m.media_sequence = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-DISCONTINUITY-SEQUENCE:") {
            m.discontinuity_sequence = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-VERSION:") {
            m.version = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("#EXT-X-PLAYLIST-TYPE:") {
            m.kind = match v.trim() {
                "VOD" => "VOD",
                "EVENT" => "Évènement (EVENT)",
                other => other,
            }
            .to_string();
        } else if line == "#EXT-X-ENDLIST" && m.kind.starts_with("Direct") {
            m.kind = "Terminé (ENDLIST)".into();
        } else if let Some(v) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            m.program_date_time = v.trim().to_string();
        } else if line == "#EXT-X-DISCONTINUITY" {
            m.discontinuities += 1;
        } else if line == "#EXT-X-GAP" {
            m.gaps += 1;
        } else if let Some(r) = line.strip_prefix("#EXT-X-KEY:") {
            let a = attributes(r);
            let method = get(&a, "METHOD").to_string();
            let kf = get(&a, "KEYFORMAT");
            let k = if kf.is_empty() || method == "NONE" {
                method
            } else {
                format!("{method} ({kf})")
            };
            if !keys.contains(&k) {
                keys.push(k);
            }
        } else if line.starts_with("#EXT-X-MAP:") {
            fmp4 = true;
        } else if line.starts_with("#EXT-X-BYTERANGE:") {
            m.byte_ranges = true;
        } else if let Some(r) = line.strip_prefix("#EXT-X-PART-INF:") {
            m.part_target_s = get(&attributes(r), "PART-TARGET").parse().ok();
        } else if line.starts_with("#EXT-X-CUE-OUT")
            || line.starts_with("#EXT-X-CUE-IN")
            || line.starts_with("#EXT-OATCLS-SCTE35")
            || line.starts_with("#EXT-X-SCTE35")
        {
            let tag = line.trim_start_matches('#');
            m.cues.push(tag.chars().take(120).collect());
        } else if let Some(r) = line.strip_prefix("#EXT-X-DATERANGE:") {
            let a = attributes(r);
            let scte = ["SCTE35-OUT", "SCTE35-IN", "SCTE35-CMD"]
                .iter()
                .find(|k| !get(&a, k).is_empty())
                .copied()
                .unwrap_or("");
            let mut text = format!("DATERANGE {} {}", get(&a, "ID"), get(&a, "START-DATE"));
            if !scte.is_empty() {
                text.push_str(&format!(" ({scte})"));
            }
            let d = get(&a, "PLANNED-DURATION");
            let d = if d.is_empty() { get(&a, "DURATION") } else { d };
            if !d.is_empty() {
                text.push_str(&format!(", {d} s"));
            }
            m.cues.push(text);
        } else if !line.is_empty() && !line.starts_with('#') {
            last_segment = line.to_string();
        }
    }
    if m.segments == 0 {
        m.segment_min_s = 0.0;
    }
    m.encryption = if keys.is_empty() {
        vec!["aucun".into()]
    } else {
        keys
    };
    m.cues.truncate(20);
    let ext = last_segment
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    m.format = if fmp4 {
        "fMP4 (CMAF)".into()
    } else {
        match ext.as_str() {
            "ts" | "m2ts" => "MPEG-TS".into(),
            "aac" | "adts" => "AAC (ADTS)".into(),
            "mp4" | "m4s" | "cmfv" | "cmfa" => "fMP4 (CMAF)".into(),
            "vtt" | "webvtt" => "WebVTT".into(),
            "" => String::new(),
            other => other.to_uppercase(),
        }
    };
    m
}

/// The last segment of a media playlist.
fn last_segment(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(5)
        .user_agent(concat!("strata-multiview/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn fetch(a: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut r = a.get(url).call().map_err(|e| format!("{url} : {e}"))?;
    r.body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("{url} : {e}"))
}

/// Reads an HLS input: its playlists, then the last segment of its first
/// variant when it is a transport stream.
pub fn analyse(url: &str) -> (HlsView, Option<TsView>) {
    let a = agent();
    let mut view = HlsView {
        url: url.to_string(),
        ..Default::default()
    };
    let text = match fetch(&a, url, 2 << 20) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            view.error = e;
            return (view, None);
        }
    };
    if !text.trim_start().starts_with("#EXTM3U") {
        view.error = "ce n’est pas une liste de lecture HLS (#EXTM3U absent)".into();
        return (view, None);
    }
    let mut media_url = url.to_string();
    let mut media_text = text.clone();
    if text.contains("#EXT-X-STREAM-INF") {
        view.master = true;
        master(&mut view, &text);
        if let Some(v) = view.variants.first() {
            media_url = resolve(url, &v.uri);
            match fetch(&a, &media_url, 2 << 20) {
                Ok(b) => media_text = String::from_utf8_lossy(&b).into_owned(),
                Err(e) => {
                    view.error = e;
                    return (view, None);
                }
            }
        }
    } else if let Some(v) = text.lines().find_map(|l| l.strip_prefix("#EXT-X-VERSION:")) {
        view.version = v.trim().parse().unwrap_or(0);
    }
    let m = media(&media_url, &media_text);
    let ts = (m.format == "MPEG-TS")
        .then(|| last_segment(&media_text))
        .flatten()
        .and_then(|seg| fetch(&a, &resolve(&media_url, &seg), 24 << 20).ok())
        .filter(|b| TsMonitor::looks_like_ts(b))
        .map(|b| {
            let mut t = TsMonitor::new(false);
            t.feed(&b, std::time::Instant::now());
            t.view(std::time::Instant::now())
        });
    view.media = Some(m);
    (view, ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_master_and_a_media_playlist() {
        let mut v = HlsView::default();
        master(
            &mut v,
            r#"#EXTM3U
#EXT-X-VERSION:6
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="aud",NAME="Français",LANGUAGE="fr",DEFAULT=YES,AUTOSELECT=YES,CHANNELS="2",URI="a/fr.m3u8"
#EXT-X-MEDIA:TYPE=CLOSED-CAPTIONS,GROUP-ID="cc",NAME="CC1",LANGUAGE="fr",INSTREAM-ID="CC1"
#EXT-X-STREAM-INF:BANDWIDTH=6000000,AVERAGE-BANDWIDTH=5500000,RESOLUTION=1920x1080,FRAME-RATE=50.000,CODECS="avc1.640028,mp4a.40.2",VIDEO-RANGE=SDR,AUDIO="aud",CLOSED-CAPTIONS="cc"
v1080/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=1500000,RESOLUTION=960x540,CODECS="avc1.4d401f,mp4a.40.2",AUDIO="aud"
v540/index.m3u8
#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=300000,URI="v1080/iframes.m3u8"
"#,
        );
        assert_eq!(v.version, 6);
        assert!(v.independent_segments);
        assert_eq!(v.variants.len(), 2);
        assert_eq!(v.variants[0].resolution, "1920x1080");
        assert_eq!(v.variants[0].codecs, "avc1.640028,mp4a.40.2");
        assert_eq!(v.variants[0].frame_rate, Some(50.0));
        assert_eq!(v.variants[1].uri, "v540/index.m3u8");
        assert_eq!(v.renditions.len(), 2);
        assert_eq!(v.renditions[1].instream_id, "CC1");
        assert_eq!(v.iframe_streams, 1);
        let m = media(
            "https://cdn/v1080/index.m3u8",
            "#EXTM3U
#EXT-X-VERSION:6
#EXT-X-TARGETDURATION:6
#EXT-X-MEDIA-SEQUENCE:1200
#EXT-X-KEY:METHOD=AES-128,URI=\"k.bin\"
#EXT-X-PROGRAM-DATE-TIME:2026-10-31T20:00:00.000Z
#EXTINF:6.000,
s1200.ts
#EXT-X-CUE-OUT:30
#EXTINF:5.960,
s1201.ts
#EXT-X-DISCONTINUITY
#EXT-X-DATERANGE:ID=\"ad1\",START-DATE=\"2026-10-31T20:00:06Z\",PLANNED-DURATION=30,SCTE35-OUT=0xFC30
#EXTINF:6.000,
s1202.ts
",
        );
        assert_eq!(m.segments, 3);
        assert_eq!(m.target_duration, 6.0);
        assert_eq!(m.media_sequence, 1200);
        assert_eq!(m.format, "MPEG-TS");
        assert_eq!(m.encryption, vec!["AES-128"]);
        assert_eq!(m.discontinuities, 1);
        assert_eq!(m.cues.len(), 2);
        assert!(m.cues[1].contains("SCTE35-OUT"));
        assert!(m.kind.starts_with("Direct"));
        assert_eq!(
            resolve("https://cdn/a/master.m3u8?t=1", "v/i.m3u8"),
            "https://cdn/a/v/i.m3u8"
        );
        assert_eq!(
            resolve("https://cdn/a/master.m3u8", "/x/i.m3u8"),
            "https://cdn/x/i.m3u8"
        );
    }
}
