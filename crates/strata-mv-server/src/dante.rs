//! Dante audio sources, received in two open ways:
//!
//! - **AES67** (Dante devices with AES67 mode on, Dante Domain Manager,
//!   other AES67 senders): RTP multicast, L24 or L16 at 48 kHz, announced by
//!   SAP (RFC 2974) on 239.255.255.255:9875. The announcements heard are
//!   listed here; an input gets its SDP and ffmpeg receives the stream.
//! - **Sound card** (Dante Virtual Soundcard, Dante Via, or any audio
//!   interface): captured by ffmpeg (ALSA or PulseAudio on Linux, CoreAudio
//!   via AVFoundation on macOS, DirectShow on Windows).
//!
//! Both are audio only: their tiles show meters, loudness and faults.

use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use serde::Serialize;

use crate::auth::CurrentUser;

/// An AES67 stream heard announced.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Announced {
    /// The session's name (`s=`), as Dante Controller shows it.
    pub name: String,
    /// Who announced it.
    pub origin: String,
    /// "239.69.1.10:5004".
    pub address: String,
    /// "L24/48000/8".
    pub format: String,
    pub channels: u32,
    pub sdp: String,
    /// Seconds since last heard.
    pub age: u64,
}

/// SDP announcements heard, by (origin, message hash): when, and the SDP.
type Heard = HashMap<(String, u16), (Instant, String)>;

static HEARD: LazyLock<Mutex<Heard>> = LazyLock::new(Default::default);

/// The SDP of a SAP packet (none: a deletion, or not SDP), with its key.
pub fn sap_payload(p: &[u8]) -> Option<((String, u16), Option<String>)> {
    let flags = *p.first()?;
    if flags >> 5 != 1 {
        return None; // SAP version 1
    }
    let ipv6 = flags & 0x10 != 0;
    let deletion = flags & 0x04 != 0;
    let encrypted = flags & 0x02 != 0;
    let compressed = flags & 0x01 != 0;
    let auth = *p.get(1)? as usize * 4;
    let hash = u16::from_be_bytes([*p.get(2)?, *p.get(3)?]);
    let alen = if ipv6 { 16 } else { 4 };
    let origin = p.get(4..4 + alen)?;
    let origin = if ipv6 {
        std::net::Ipv6Addr::from(<[u8; 16]>::try_from(origin).ok()?).to_string()
    } else {
        Ipv4Addr::from(<[u8; 4]>::try_from(origin).ok()?).to_string()
    };
    let key = (origin, hash);
    if deletion {
        return Some((key, None));
    }
    if encrypted || compressed {
        return None;
    }
    let mut body = p.get(4 + alen + auth..)?;
    // The payload type, unless the SDP starts at once.
    if !body.starts_with(b"v=0") {
        let end = body.iter().position(|b| *b == 0)?;
        let ty = std::str::from_utf8(&body[..end]).ok()?;
        if ty != "application/sdp" {
            return None;
        }
        body = &body[end + 1..];
    }
    Some((key, Some(String::from_utf8_lossy(body).into_owned())))
}

/// What an SDP says of its stream: name, address, format, channels.
pub fn describe(sdp: &str) -> (String, String, String, u32) {
    let mut name = String::new();
    let mut group = String::new();
    let mut port = String::new();
    let mut format = String::new();
    for l in sdp.lines().map(str::trim) {
        if let Some(v) = l.strip_prefix("s=") {
            name = v.to_string();
        } else if let Some(v) = l.strip_prefix("c=IN IP4 ") {
            if group.is_empty() {
                group = v.split('/').next().unwrap_or("").to_string();
            }
        } else if let Some(v) = l.strip_prefix("m=audio ") {
            port = v.split_whitespace().next().unwrap_or("").to_string();
        } else if let Some(v) = l.strip_prefix("a=rtpmap:") {
            if format.is_empty() {
                format = v.split_whitespace().nth(1).unwrap_or("").to_string();
            }
        }
    }
    let channels = format
        .split('/')
        .nth(2)
        .and_then(|c| c.parse().ok())
        .unwrap_or(if format.is_empty() { 0 } else { 1 });
    (name, format!("{group}:{port}"), format, channels)
}

/// Listens to the SAP announcements, for as long as the server runs.
pub fn spawn_sap() {
    let _ = std::thread::Builder::new().name("mv-sap".into()).spawn(|| {
        let sock = match UdpSocket::bind("0.0.0.0:9875") {
            Ok(s) => s,
            Err(e) => {
                tracing::info!("AES67 / Dante discovery (SAP, port 9875) off: {e}");
                return;
            }
        };
        for group in [
            Ipv4Addr::new(239, 255, 255, 255),
            Ipv4Addr::new(224, 2, 127, 254),
        ] {
            let _ = sock.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED);
        }
        let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = vec![0u8; 4096];
        loop {
            if let Ok((n, _)) = sock.recv_from(&mut buf) {
                if let Some((key, sdp)) = sap_payload(&buf[..n]) {
                    let mut h = HEARD.lock().unwrap_or_else(|p| p.into_inner());
                    match sdp {
                        Some(s) if s.contains("m=audio") => {
                            h.insert(key, (Instant::now(), s));
                        }
                        Some(_) => {}
                        None => {
                            h.remove(&key);
                        }
                    }
                }
            }
            // Not announced for 5 minutes (SAP repeats every 30 s or so): gone.
            HEARD
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|_, (t, _)| t.elapsed() < Duration::from_secs(300));
        }
    });
}

/// `GET /api/inputs/aes67-sources`: the AES67 streams heard announced.
pub async fn sources(CurrentUser(_me): CurrentUser) -> Json<Vec<Announced>> {
    let h = HEARD.lock().unwrap_or_else(|p| p.into_inner());
    let mut v: Vec<Announced> = h
        .iter()
        .map(|((origin, _), (t, sdp))| {
            let (name, address, format, channels) = describe(sdp);
            Announced {
                name,
                origin: origin.clone(),
                address,
                format,
                channels,
                sdp: sdp.clone(),
                age: t.elapsed().as_secs(),
            }
        })
        .collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    Json(v)
}

/// An SDP for an AES67 stream given by hand ("239.69.1.10:5004").
pub fn sdp_for(address: &str, channels: u32, encoding: &str) -> Result<String, String> {
    let a = address.trim().trim_start_matches("rtp://");
    let (ip, port) = a
        .rsplit_once(':')
        .ok_or("address: group:port (239.69.1.10:5004)")?;
    let ip: Ipv4Addr = ip
        .parse()
        .map_err(|_| format!("not an IPv4 address: {ip}"))?;
    let port: u16 = port.parse().map_err(|_| format!("not a port: {port}"))?;
    let enc = if encoding.eq_ignore_ascii_case("L16") {
        "L16"
    } else {
        "L24"
    };
    let ch = channels.clamp(1, 64);
    Ok(format!(
        "v=0\r\no=- 0 0 IN IP4 {ip}\r\ns=Strata AES67\r\nc=IN IP4 {ip}/32\r\nt=0 0\r\nm=audio {port} RTP/AVP 96\r\na=rtpmap:96 {enc}/48000/{ch}\r\na=ptime:1\r\na=recvonly\r\n"
    ))
}

/// A sound card of this machine, for the inputs page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// What the input's address holds: "alsa:hw:2", "dshow:Dante Virtual Soundcard (x64)".
    pub id: String,
    pub name: String,
    /// It looks like Dante's.
    pub dante: bool,
}

/// The ffmpeg input of a sound card address ("alsa:hw:2").
pub fn device_input(spec: &str, channels: u32) -> Result<Vec<String>, String> {
    let (fmt, dev) = spec.split_once(':').ok_or("choose a sound card")?;
    let ch = channels.clamp(1, 64).to_string();
    let mut a: Vec<String> = Vec::new();
    match fmt {
        "alsa" | "pulse" => {
            a.extend([
                "-f".into(),
                fmt.into(),
                "-channels".into(),
                ch,
                "-sample_rate".into(),
                "48000".into(),
            ]);
            a.extend(["-i".into(), dev.into()]);
        }
        "avfoundation" => {
            a.extend([
                "-f".into(),
                "avfoundation".into(),
                "-i".into(),
                format!(":{dev}"),
            ]);
        }
        "dshow" => {
            a.extend([
                "-f".into(),
                "dshow".into(),
                "-audio_buffer_size".into(),
                "20".into(),
                "-channels".into(),
                ch,
                "-sample_rate".into(),
                "48000".into(),
            ]);
            a.extend(["-i".into(), format!("audio={dev}")]);
        }
        "wasapi" => {
            a.extend(["-f".into(), "wasapi".into(), "-i".into(), dev.into()]);
        }
        other => return Err(format!("unknown sound card system `{other}`")),
    }
    Ok(a)
}

fn looks_dante(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("dante") || n.contains("dvs")
}

/// The sound cards ffmpeg can capture here.
pub fn devices() -> Vec<Device> {
    let mut out = Vec::new();
    #[cfg(target_os = "linux")]
    {
        if let Ok(cards) = std::fs::read_to_string("/proc/asound/cards") {
            for l in cards.lines() {
                // " 2 [DVS            ]: USB-Audio - Dante Virtual Soundcard"
                let t = l.trim_start();
                let Some((num, rest)) = t.split_once(' ') else {
                    continue;
                };
                if num.parse::<u32>().is_err() {
                    continue;
                }
                let name = rest
                    .split_once(": ")
                    .map_or(rest, |x| x.1)
                    .trim()
                    .to_string();
                out.push(Device {
                    id: format!("alsa:hw:{num}"),
                    dante: looks_dante(&name) || looks_dante(rest),
                    name,
                });
            }
        }
        out.push(Device {
            id: "pulse:default".into(),
            name: "PulseAudio / PipeWire (default)".into(),
            dante: false,
        });
    }
    #[cfg(target_os = "macos")]
    {
        // [AVFoundation indev @ …] AVFoundation audio devices:
        // [AVFoundation indev @ …] [0] Dante Virtual Soundcard
        let o = strata_mv_output::ffmpeg::command()
            .args([
                "-hide_banner",
                "-f",
                "avfoundation",
                "-list_devices",
                "true",
                "-i",
                "",
            ])
            .output();
        if let Ok(o) = o {
            let text = String::from_utf8_lossy(&o.stderr);
            let mut audio = false;
            for l in text.lines() {
                if l.contains("audio devices") {
                    audio = true;
                    continue;
                }
                if !audio {
                    continue;
                }
                if let Some(rest) = l.split("] [").nth(1) {
                    if let Some((num, name)) = rest.split_once("] ") {
                        out.push(Device {
                            id: format!("avfoundation:{num}"),
                            dante: looks_dante(name),
                            name: name.trim().into(),
                        });
                    }
                }
            }
        }
    }
    #[cfg(windows)]
    {
        // [dshow @ …] "Dante Virtual Soundcard (x64)" (audio)
        let o = strata_mv_output::ffmpeg::command()
            .args([
                "-hide_banner",
                "-list_devices",
                "true",
                "-f",
                "dshow",
                "-i",
                "dummy",
            ])
            .output();
        if let Ok(o) = o {
            let text = String::from_utf8_lossy(&o.stderr);
            for l in text.lines().filter(|l| l.contains("(audio)")) {
                if let Some(name) = l.split('"').nth(1) {
                    out.push(Device {
                        id: format!("dshow:{name}"),
                        dante: looks_dante(name),
                        name: name.into(),
                    });
                }
            }
        }
    }
    out.sort_by_key(|d| !d.dante);
    out
}

/// `GET /api/inputs/audio-devices`.
pub async fn list_devices(CurrentUser(_me): CurrentUser) -> Json<Vec<Device>> {
    Json(
        tokio::task::spawn_blocking(devices)
            .await
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sap_announcement_is_read() {
        let sdp = "v=0\r\no=- 1 1 IN IP4 192.168.1.20\r\ns=AVIO-USB : 2 ch\r\nc=IN IP4 239.69.83.67/32\r\nt=0 0\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/2\r\na=ptime:1\r\n";
        let mut p = vec![0x20, 0, 0x12, 0x34, 192, 168, 1, 20];
        p.extend_from_slice(b"application/sdp\0");
        p.extend_from_slice(sdp.as_bytes());
        let (key, got) = sap_payload(&p).unwrap();
        assert_eq!(key, ("192.168.1.20".to_string(), 0x1234));
        assert_eq!(got.as_deref(), Some(sdp));
        assert_eq!(
            describe(sdp),
            (
                "AVIO-USB : 2 ch".into(),
                "239.69.83.67:5004".into(),
                "L24/48000/2".into(),
                2
            )
        );
        // A deletion.
        p[0] = 0x24;
        assert_eq!(sap_payload(&p).unwrap().1, None);
        // The SDP straight away, no payload type.
        let mut q = vec![0x20, 0, 0, 1, 10, 0, 0, 1];
        q.extend_from_slice(sdp.as_bytes());
        assert_eq!(sap_payload(&q).unwrap().1.as_deref(), Some(sdp));
    }

    #[test]
    fn sdp_and_devices() {
        let s = sdp_for("239.69.1.10:5004", 8, "L24").unwrap();
        assert!(s.contains("m=audio 5004 RTP/AVP 96") && s.contains("a=rtpmap:96 L24/48000/8"));
        assert!(sdp_for("nope", 2, "L24").is_err());
        assert_eq!(
            device_input("alsa:hw:2", 16).unwrap(),
            [
                "-f",
                "alsa",
                "-channels",
                "16",
                "-sample_rate",
                "48000",
                "-i",
                "hw:2"
            ]
        );
        assert_eq!(
            device_input("dshow:Dante Virtual Soundcard (x64)", 2)
                .unwrap()
                .last()
                .unwrap(),
            "audio=Dante Virtual Soundcard (x64)"
        );
    }
}
