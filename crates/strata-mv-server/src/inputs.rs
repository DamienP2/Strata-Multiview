//! Live video inputs, shown by the Video widget.
//!
//! - Streams (SRT, UDP, RTMP, HLS, RTSP, any address ffmpeg reads) are
//!   decoded by ffmpeg into raw pictures; SRT and RTMP can also wait for the
//!   sender to connect (listener).
//! - NDI sources are received through the NDI runtime (the one the NDI
//!   outputs use).
//! - SDI inputs need DeckLink support, which this server is built without.
//!
//! Streams that come as bytes (SRT, UDP, TCP) are relayed: a first ffmpeg
//! receives them, Strata reads their transport stream on the way (tables,
//! EPG, SCTE-35, TR 101 290: `probe::ts`) and hands it to the ffmpeg that
//! decodes. What every input carries is kept in `probe::Facts`.
//!
//! Pictures off a network come unevenly (bursts, gaps). Each one is given a
//! time to be shown, `buffer` frames after it came and at least one frame
//! after the previous one: the renderers, drawing at their own pace, get an
//! even motion back, at the cost of that much delay. The previews
//! (`/api/inputs/<id>/live.mjpeg`) send the newest picture as JPEG.
//! Inputs keep running across engine restarts; saving the settings only
//! restarts the ones that changed.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, Read, Write};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tiny_skia::{IntSize, Pixmap};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputKind {
    #[default]
    Srt,
    Udp,
    Rtmp,
    Hls,
    Rtsp,
    /// Any address or file ffmpeg reads.
    Url,
    /// The live of a web platform (YouTube, TikTok, Twitch…): its page's
    /// address, the stream found by yt-dlp (`weblive`).
    Web,
    Ndi,
    Sdi,
    /// An ST 2110 flow of the Hardware settings (not received yet).
    St2110,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputConfig {
    #[serde(default)]
    pub id: String,
    /// Off: kept in the settings, not started.
    #[serde(default = "strata_mv_output::on")]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub kind: InputKind,
    /// `srt://host:port`, `udp://@239.1.1.1:5000`, `rtmp://server/app/key`,
    /// `https://…/index.m3u8`, `rtsp://…`.
    #[serde(default)]
    pub url: String,
    /// SRT / RTMP: wait for the sender to connect instead of calling it.
    #[serde(default)]
    pub listen: bool,
    /// NDI: the source name ("HOST (name)").
    #[serde(default)]
    pub ndi_source: String,
    /// SDI: DeckLink device index (filled from `hardware`).
    #[serde(default)]
    pub device: u32,
    /// SDI / ST 2110: the input of the Hardware settings ("" = none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hardware: String,
    /// Streams are decoded at this size (the picture fitted inside) and rate.
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// SRT listener: the port Strata waits on (0: the port of `url`).
    #[serde(default)]
    pub port: u16,
    /// SRT: time allowed to recover lost packets, ms.
    #[serde(default = "default_latency")]
    pub latency: u32,
    /// Smoothing, in frames of delay (0: shown as soon as they come).
    #[serde(default = "default_buffer")]
    pub buffer: u32,
    /// Streams: decode on the graphics card when ffmpeg can (it falls back
    /// to the processor).
    #[serde(default)]
    pub hw_decode: bool,
}

fn default_buffer() -> u32 {
    2
}
fn default_latency() -> u32 {
    120
}

fn default_width() -> u32 {
    1920
}
fn default_height() -> u32 {
    1080
}
fn default_fps() -> u32 {
    25
}

/// The levels of the audio, measured by ffmpeg every 40 ms (at 48 kHz) and
/// written to its log (read by [`MeterLog`]).
const METER_FILTER: &str = "ebur128=metadata=1:peak=true,asetnsamples=n=1920:p=0,astats=metadata=1:reset=1:measure_overall=none:measure_perchannel=Peak_level+RMS_level,ametadata=mode=print";

/// Where a stream is read from, once looked up (`prepare`).
#[derive(Debug, Default, Clone)]
pub struct Pull {
    /// The address read instead of the configured one (a web page's stream).
    pub url: Option<String>,
    /// Options of the input (its headers).
    pub input: Vec<String>,
    /// The sound read apart (a master playlist's audio rendition).
    pub audio: Option<String>,
    /// Where ffmpeg sends the sound to be listened to (a local port).
    pub pcm: Option<u16>,
    /// This ffmpeg knows `-fps_mode` (5.1 and later).
    pub fps_mode: bool,
}

/// The sound of an input as listened to: 16-bit stereo at 48 kHz.
pub const PCM_RATE: u32 = 48_000;

/// The ffmpeg commands of a stream input.
#[derive(Debug, Default)]
pub struct StreamArgs {
    /// Decodes to raw RGBA on stdout (from stdin when relayed).
    pub decode: Vec<String>,
    /// Receives the source and writes its bytes as they come on stdout.
    pub relay: Option<Vec<String>>,
}

impl InputConfig {
    /// The stream comes as bytes of a transport stream (SRT, UDP, TCP):
    /// relayed through Strata, which reads it on the way.
    pub fn relayed(&self) -> bool {
        let u = self.url.trim().to_ascii_lowercase();
        match self.kind {
            InputKind::Srt => true,
            InputKind::Udp => u.starts_with("udp://"),
            InputKind::Url => ["srt://", "udp://", "tcp://"]
                .iter()
                .any(|s| u.starts_with(s)),
            _ => false,
        }
    }

    /// The ffmpeg arguments that decode this stream to raw RGBA on stdout,
    /// measure its first audio track (`audio`) and tell each picture in the
    /// log (`info`: the `showinfo` filter).
    pub fn ffmpeg_args(&self, audio: bool, info: bool) -> Result<StreamArgs, String> {
        self.ffmpeg_args_from(&Pull::default(), audio, info)
    }

    /// `ffmpeg_args`, reading where `pull` says.
    pub fn ffmpeg_args_from(
        &self,
        pull: &Pull,
        audio: bool,
        info: bool,
    ) -> Result<StreamArgs, String> {
        // An SRT listener needs only its port.
        let own;
        let url = if let Some(u) = &pull.url {
            u.as_str()
        } else if self.kind == InputKind::Srt && self.listen && self.port > 0 {
            own = format!("srt://0.0.0.0:{}", self.port);
            own.as_str()
        } else {
            self.url.trim()
        };
        if url.is_empty() {
            return Err("no address".into());
        }
        let need = |schemes: &[&str]| -> Result<(), String> {
            if schemes
                .iter()
                .any(|s| url.to_ascii_lowercase().starts_with(s))
            {
                Ok(())
            } else {
                Err(format!(
                    "the address must start with {}",
                    schemes.join(" or ")
                ))
            }
        };
        // Low latency, and a short look at the stream before the pictures
        // start; the decoded picture's size and rate are read from the log.
        let mut a: Vec<String> = [
            "-loglevel",
            "info",
            "-nostats",
            "-fflags",
            "nobuffer",
            "-flags",
            "low_delay",
            "-probesize",
            "500000",
            "-analyzeduration",
            "1000000",
        ]
        .map(String::from)
        .to_vec();
        let common = a.len();
        let mut url = url.to_string();
        match self.kind {
            InputKind::Srt => {
                need(&["srt://"])?;
                let add = |url: &mut String, kv: String| {
                    url.push(if url.contains('?') { '&' } else { '?' });
                    url.push_str(&kv);
                };
                if self.listen && !url.contains("mode=") {
                    add(&mut url, "mode=listener".into());
                }
                if !url.contains("latency=") {
                    // ffmpeg takes it in microseconds.
                    let us = self.latency.clamp(20, 8000) * 1000;
                    add(&mut url, format!("latency={us}"));
                }
            }
            InputKind::Udp => need(&["udp://", "rtp://"])?,
            InputKind::Rtmp => {
                need(&["rtmp://", "rtmps://"])?;
                if self.listen {
                    a.extend(["-listen", "1"].map(String::from));
                }
            }
            InputKind::Hls => need(&["http://", "https://"])?,
            InputKind::Rtsp => {
                need(&["rtsp://", "rtsps://"])?;
                a.extend(["-rtsp_transport", "tcp"].map(String::from));
            }
            InputKind::Url => {}
            InputKind::Web => need(&["http://", "https://"])?,
            InputKind::Ndi | InputKind::Sdi | InputKind::St2110 => {
                return Err("not a stream".into())
            }
        }
        let relay = self.relayed().then(|| {
            // The bytes as they come (the data demuxer and muxer copy them
            // whole, tables included), each packet passed on at once.
            let mut r: Vec<String> = ["-loglevel", "error", "-nostats"]
                .map(String::from)
                .to_vec();
            r.extend(["-f".into(), "data".into(), "-i".into(), url.clone()]);
            r.extend(
                [
                    "-map",
                    "0",
                    "-c",
                    "copy",
                    "-flush_packets",
                    "1",
                    "-f",
                    "data",
                    "pipe:1",
                ]
                .map(String::from),
            );
            r
        });
        if relay.is_some() {
            // Its options went to the relay: the decoder reads a pipe.
            a.truncate(common);
        }
        if self.hw_decode {
            a.extend(["-hwaccel", "auto"].map(String::from));
        }
        // The source's own size and rate.
        if relay.is_some() {
            a.extend(["-f", "mpegts", "-i", "pipe:0"].map(String::from));
        } else {
            a.extend(pull.input.iter().cloned());
            a.extend(["-i".into(), url]);
            // The sound apart: a second input.
            if let Some(au) = &pull.audio {
                a.extend(pull.input.iter().cloned());
                a.extend(["-i".into(), au.clone()]);
            }
        }
        let from = if pull.audio.is_some() { "1" } else { "0" };
        a.extend(["-map".into(), "0:v:0".into()]);
        if info {
            a.extend(["-vf", "showinfo=checksum=0"].map(String::from));
        }
        // Every picture once, none added nor dropped: each is the one
        // `showinfo` told of (its own time).
        if pull.fps_mode {
            a.extend(["-fps_mode", "passthrough"].map(String::from));
        }
        a.extend(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"].map(String::from));
        if audio {
            a.extend(["-map".into(), format!("{from}:a:0")]);
            a.extend(["-af", METER_FILTER, "-f", "null", "-"].map(String::from));
            // The sound, to be listened to (Strata reads it on that port).
            if let Some(port) = pull.pcm {
                a.extend(["-map".into(), format!("{from}:a:0")]);
                // Each packet written at once (not 170 ms at a time); a frame
                // lost on the way leaves silence in its place, so the count of
                // samples stays the sound's own time.
                a.extend(
                    [
                        "-ac",
                        "2",
                        "-af",
                        "aresample=48000:async=1",
                        "-flush_packets",
                        "1",
                        "-f",
                        "s16le",
                    ]
                    .map(String::from),
                );
                a.push(format!("tcp://127.0.0.1:{port}"));
            }
        }
        Ok(StreamArgs { decode: a, relay })
    }
}

/// How an input is doing, for the Video streams page.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputStatus {
    pub id: String,
    /// A picture came in the last two seconds.
    pub live: bool,
    pub message: String,
    pub width: u32,
    pub height: u32,
    /// Pictures received since the input started.
    pub frames: u64,
    /// Pictures a second of the source (streams: as detected; an
    /// interlaced source counts its fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// The source is interlaced (its fields come as pictures of their own).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub interlaced: bool,
}

/// Pictures waiting for their time, and the one on show.
#[derive(Default)]
struct Paced {
    /// Each with when it shows and how long it waited for that.
    queue: VecDeque<(Instant, Arc<Pixmap>, Duration)>,
    shown: Option<Arc<Pixmap>>,
    /// How long the picture on show waited: its sound and its meters wait
    /// as long, to go with it.
    shown_wait: Duration,
    last_due: Option<Instant>,
    /// When each picture was due (a burst spread out a frame apart): the
    /// web pages play them out on it, the outputs show them a smoothing
    /// buffer later, the sound goes with them.
    clock: RateClock,
    /// The time given to the last picture: the next one comes 0.9 frame
    /// after it at least (a burst late on its time catches up in a second).
    last_smooth: Option<Instant>,
}

/// One running input.
struct Live {
    config: InputConfig,
    /// The newest picture (previews, status).
    picture: RwLock<Option<Arc<Pixmap>>>,
    paced: Mutex<Paced>,
    /// Picture buffers done with, used again.
    pool: Mutex<Vec<Vec<u8>>>,
    received: AtomicU64,
    /// Thrown away: too many waiting.
    dropped: AtomicU64,
    last_at: Mutex<Option<std::time::Instant>>,
    /// Still there until then although no picture came: an NDI sender
    /// connected, which sends only when its picture changes.
    held_until: Mutex<Option<Instant>>,
    message: Mutex<String>,
    stop: AtomicBool,
    /// The ffmpeg decoding it (killed to stop a blocked read).
    child: Mutex<Option<Child>>,
    /// Pictures a second of the source, once known (fields when interlaced).
    rate: Mutex<Option<f64>>,
    interlaced: AtomicBool,
    /// Where its audio levels go.
    signals: Arc<crate::signals::Signals>,
    /// The stream has no audio: ffmpeg is started without the meters.
    no_audio: AtomicBool,
    /// This ffmpeg has no `showinfo` (or not its options): without it.
    no_info: AtomicBool,
    /// This ffmpeg has no `-fps_mode` (before 5.1): without it.
    no_fps_mode: AtomicBool,
    /// The ffmpeg receiving a relayed stream.
    relay: Mutex<Option<Child>>,
    /// What it carries (`probe`).
    facts: Arc<Mutex<crate::probe::Facts>>,
    /// The number of the newest picture and when it came (ms since 1970):
    /// the web streams wake on it.
    frames: tokio::sync::watch::Sender<(u64, u64)>,
    /// The newest pictures as JPEG, by width: the web pages showing this
    /// input at the same size share them.
    jpegs: Mutex<VecDeque<(u64, u32, axum::body::Bytes)>>,
    /// The last pictures, each with its number and its smoothed time (ms
    /// since 1970): a web page gets every one, at its time.
    recent: Mutex<VecDeque<(u64, u64, Arc<Pixmap>)>>,
    /// Its sound as it comes (`PCM_RATE`, stereo, 16-bit), timed: to the
    /// people listening to it and to the outputs sending it.
    audio: tokio::sync::broadcast::Sender<Pcm>,
    /// Its own times on the wall clock (streams: ffmpeg tells them).
    media: Arc<Mutex<MediaClock>>,
}

/// Sound of an input as it came: 16-bit stereo at `PCM_RATE`, and when its
/// first sample came (smoothed: the samples a constant rate apart).
#[derive(Clone, Debug)]
pub struct Pcm {
    pub at: Instant,
    /// The same, ms since 1970.
    pub at_ms: f64,
    pub data: axum::body::Bytes,
}

/// Times what comes at a constant rate in bursts (sound, pictures): the
/// time of item k is `base + k × step`, `base` the earliest arrival seen
/// (each item came at that time or later: a burst comes late, never early),
/// rising 2 ms a second to follow the clocks drifting apart. Set again when
/// the items come more than a second late (a pause, a restart).
#[derive(Default)]
struct RateClock {
    base: Option<Instant>,
    count: f64,
    seen: Option<Instant>,
}

impl RateClock {
    /// When the first of `n` items that come now (`step` apart) was due:
    /// what comes is done when it comes (`end`: sound, the last sample just
    /// came) or starts then (a picture).
    fn time(&mut self, n: f64, step: Duration) -> Instant {
        self.time_as(n, step, true)
    }

    /// When a picture that comes now (`step` apart) was due.
    fn picture(&mut self, step: Duration) -> Instant {
        self.time_as(1.0, step, false)
    }

    fn time_as(&mut self, n: f64, step: Duration, end: bool) -> Instant {
        let now = Instant::now();
        let step_s = step.as_secs_f64();
        let back = if end { self.count + n } else { self.count };
        // Item 0's time if these came right now.
        let implied = now
            .checked_sub(Duration::from_secs_f64(back * step_s))
            .unwrap_or(now);
        let base = match (self.base, self.seen) {
            (Some(b), Some(seen)) => {
                let drift = now.saturating_duration_since(seen).mul_f64(0.002);
                let b = b + drift;
                if implied > b + Duration::from_secs(1) {
                    None
                } else {
                    Some(b.min(implied))
                }
            }
            _ => None,
        };
        self.seen = Some(now);
        let base = match base {
            Some(b) => b,
            None => {
                // From now again.
                self.count = 0.0;
                let back = if end { n * step_s } else { 0.0 };
                now.checked_sub(Duration::from_secs_f64(back))
                    .unwrap_or(now)
            }
        };
        self.base = Some(base);
        let at = base + Duration::from_secs_f64(self.count * step_s);
        self.count += n;
        at
    }
}

/// An input's own times (the PTS of its pictures and of its sound) put on
/// the wall clock, the same way for both: so that its sound, its meters
/// and its pictures stay together whatever the decoder's delay between
/// them. Each stream's offset (wall time − PTS) is the smallest seen (a
/// burst comes late, never early), rising 2 ms a second; both use the
/// larger of the two (each is there by then).
#[derive(Default)]
pub struct MediaClock {
    video: Option<(f64, Instant)>,
    audio: Option<(f64, Instant)>,
    /// The PTS of the first sample of the sound (the sound listened to and
    /// the meters start there).
    audio_start: Option<f64>,
    /// The PTS of the pictures ffmpeg told of (`showinfo`), by their
    /// number, not read yet.
    pts: VecDeque<(u64, f64)>,
    /// How late its sound comes after its time, at most lately (s; falling
    /// 10 ms a second): the pictures wait as long, to be shown with it.
    late: f64,
    late_seen: Option<Instant>,
}

/// The instant the media times are counted from.
fn anchor() -> Instant {
    static A: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *A.get_or_init(Instant::now)
}

impl MediaClock {
    fn see(slot: &mut Option<(f64, Instant)>, pts: f64) {
        let now = Instant::now();
        let implied = now.saturating_duration_since(anchor()).as_secs_f64() - pts;
        *slot = Some(match *slot {
            Some((off, seen)) => {
                let off = off + now.saturating_duration_since(seen).as_secs_f64() * 0.002;
                // A jump of the times (a restart, a loop): from here again.
                if implied > off + 1.0 || implied < off - 3.0 {
                    (implied, now)
                } else {
                    (off.min(implied), now)
                }
            }
            None => (implied, now),
        });
    }
    /// A picture of time `pts` comes now.
    fn saw_picture(&mut self, pts: f64) {
        Self::see(&mut self.video, pts);
    }
    /// Sound up to time `pts` comes now.
    fn saw_sound(&mut self, pts: f64) {
        Self::see(&mut self.audio, pts);
        let now = Instant::now();
        if let Some(t) = self.at(pts) {
            let late = now.saturating_duration_since(t).as_secs_f64();
            let was = self.late
                - self.late_seen.map_or(0.0, |s| {
                    now.saturating_duration_since(s).as_secs_f64() * 0.01
                });
            self.late = late.max(was).max(0.0);
            self.late_seen = Some(now);
        }
    }

    /// How long the pictures wait at least to go with their sound.
    fn sound_late(&self) -> Duration {
        Duration::from_secs_f64(self.late.min(1.0))
    }
    /// When media time `pts` is, on the wall clock.
    fn at(&self, pts: f64) -> Option<Instant> {
        let off = match (self.video, self.audio) {
            (Some((v, _)), Some((a, _))) => v.max(a),
            (Some((v, _)), None) => v,
            (None, Some((a, _))) => a,
            (None, None) => return None,
        };
        let w = pts + off;
        Some(if w >= 0.0 {
            anchor() + Duration::from_secs_f64(w)
        } else {
            anchor()
                .checked_sub(Duration::from_secs_f64(-w))
                .unwrap_or_else(anchor)
        })
    }
}

/// `pts_time:` of an ffmpeg log line.
fn pts_time(line: &str) -> Option<f64> {
    line.split("pts_time:")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
}

/// Times the sound of an input, chunk by chunk (its samples at `PCM_RATE`).
#[derive(Default)]
struct SampleClock(RateClock);

impl SampleClock {
    /// When the first of `samples` samples that came now was due.
    fn time(&mut self, samples: usize) -> Instant {
        self.0.time(
            samples as f64,
            Duration::from_secs_f64(1.0 / PCM_RATE as f64),
        )
    }
}

/// An instant as ms since 1970.
fn epoch_ms(t: Instant) -> f64 {
    let now = Instant::now();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    if t >= now {
        ms + (t - now).as_secs_f64() * 1000.0
    } else {
        ms - (now - t).as_secs_f64() * 1000.0
    }
}

impl Live {
    fn say(&self, m: impl Into<String>) {
        *self.message.lock().unwrap_or_else(|p| p.into_inner()) = m.into();
    }
    /// How long its pictures wait before they show: its smoothing buffer,
    /// and as long as its sound comes late (with 20 ms to spare), so that
    /// they go with it.
    fn delay(&self) -> Duration {
        if self.config.kind == InputKind::Sdi {
            return Duration::ZERO;
        }
        let buffer = self.period() * self.config.buffer.min(10);
        let late = self
            .media
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .sound_late();
        if late.is_zero() {
            buffer
        } else {
            buffer.max(late + Duration::from_millis(20))
        }
    }

    /// How long the picture on show waited (see `Paced::shown_wait`).
    fn shown_wait(&self) -> Duration {
        let p = self.paced.lock().unwrap_or_else(|p| p.into_inner());
        if p.shown.is_some() {
            p.shown_wait
        } else {
            drop(p);
            self.delay()
        }
    }

    /// A picture came lately, or the source is known to be there still.
    fn fresh(&self) -> bool {
        let now = Instant::now();
        self.last_at
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(2))
            || self
                .held_until
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_some_and(|t| t > now)
    }
    /// Still there for a moment (see `held_until`), or not.
    fn hold(&self, on: bool) {
        *self.held_until.lock().unwrap_or_else(|p| p.into_inner()) =
            on.then(|| Instant::now() + Duration::from_secs(2));
    }
    fn period(&self) -> Duration {
        let fps = (*self.rate.lock().unwrap_or_else(|p| p.into_inner()))
            .unwrap_or(self.config.fps as f64)
            .clamp(1.0, 120.0);
        Duration::from_secs_f64(1.0 / fps)
    }

    fn put(&self, pm: Pixmap) {
        self.put_at(pm, None);
    }

    /// A picture that was due `when` (its PTS on the wall clock; None: as
    /// it comes, bursts spread out).
    fn put_at(&self, pm: Pixmap, when: Option<Instant>) {
        let now = Instant::now();
        // Black, frozen? (a few samples of it)
        self.signals.picture(&self.config.id, &pm);
        let pm = Arc::new(pm);
        *self.picture.write().unwrap_or_else(|p| p.into_inner()) = Some(pm.clone());
        let n = self.received.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(now);
        // SDI comes off a clocked card, evenly: shown at once, no smoothing.
        let sdi = self.config.kind == InputKind::Sdi;
        let (period, frames) = (
            self.period(),
            if sdi { 0 } else { self.config.buffer.min(10) },
        );
        let delay = period * frames;
        // Its wait (the smoothing buffer, longer while its sound comes late).
        let wait = if frames > 0 {
            self.delay()
        } else {
            Duration::ZERO
        };
        let mut p = self.paced.lock().unwrap_or_else(|p| p.into_inner());
        // When it was due: its own time, else see RateClock; never less
        // than 0.9 frame after the one before.
        let t = when.unwrap_or_else(|| p.clock.picture(period));
        let gap = period.mul_f64(0.9);
        let smooth = match p.last_smooth {
            Some(l) if t < l + gap && t + Duration::from_secs(1) > l => l + gap,
            _ => t,
        };
        p.last_smooth = Some(smooth);
        let ms = epoch_ms(smooth).max(0.0) as u64;
        {
            let mut recent = self.recent.lock().unwrap_or_else(|p| p.into_inner());
            recent.push_back((n, ms, pm.clone()));
            while recent.len() > 6 {
                recent.pop_front();
            }
        }
        self.frames.send_replace((n, ms));
        // Time goes on even when nothing draws this input.
        self.advance(&mut p, now);
        let mut due = now + delay;
        if sdi {
            // The second field of a frame, handed over with the first: a field later.
            if let Some(d) = p.last_due.filter(|d| *d + period / 2 > now) {
                due = d + period;
            }
        } else if frames > 0 {
            // A smoothing buffer after it was due (longer while its sound
            // comes late): a frame apart, whatever the bursts, and on the
            // same clock as its sound.
            due = smooth + wait;
        }
        p.last_due = Some(due);
        p.queue.push_back((due, pm, wait));
        // As many waiting as the delay holds, and 3 more.
        let held = (wait.as_secs_f64() / period.as_secs_f64()).ceil() as usize;
        while p.queue.len() > held.max(frames as usize) + 3 {
            if let Some((_, old, _)) = p.queue.pop_front() {
                self.recycle(old);
            }
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// The picture to show now.
    fn current(&self) -> Option<Arc<Pixmap>> {
        let mut p = self.paced.lock().unwrap_or_else(|p| p.into_inner());
        self.advance(&mut p, Instant::now());
        p.shown.clone()
    }

    /// Shows the pictures whose time has come.
    fn advance(&self, p: &mut Paced, now: Instant) {
        while p.queue.front().is_some_and(|(due, ..)| *due <= now) {
            let Some((_, pm, wait)) = p.queue.pop_front() else {
                break;
            };
            p.shown_wait = wait;
            if let Some(old) = p.shown.replace(pm) {
                self.recycle(old);
            }
        }
    }

    /// Keeps the buffer of a picture nobody holds any more.
    fn recycle(&self, pm: Arc<Pixmap>) {
        if let Ok(pm) = Arc::try_unwrap(pm) {
            let mut pool = self.pool.lock().unwrap_or_else(|p| p.into_inner());
            if pool.len() < 4 {
                pool.push(pm.take());
            }
        }
    }

    /// A buffer for the next picture.
    fn buffer(&self, size: usize) -> Vec<u8> {
        let mut pool = self.pool.lock().unwrap_or_else(|p| p.into_inner());
        while let Some(b) = pool.pop() {
            if b.len() == size {
                return b;
            }
        }
        vec![0u8; size]
    }
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    /// Sleeps `d` unless stopped meanwhile (then false).
    fn pause(&self, d: Duration) -> bool {
        let until = std::time::Instant::now() + d;
        while std::time::Instant::now() < until {
            if self.stopped() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        !self.stopped()
    }
    fn halt(&self) {
        self.stop.store(true, Ordering::Relaxed);
        for child in [&self.child, &self.relay] {
            if let Some(c) = child.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
                let _ = c.kill();
            }
        }
    }
    fn facts(&self) -> std::sync::MutexGuard<'_, crate::probe::Facts> {
        self.facts.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The live inputs of the server.
#[derive(Default)]
pub struct Inputs {
    live: RwLock<BTreeMap<String, Arc<Live>>>,
    /// Their tally, UMD and audio levels.
    signals: Arc<crate::signals::Signals>,
}

impl Inputs {
    /// Runs the configured inputs: new and changed ones start, removed ones stop.
    pub fn apply(&self, configs: &[InputConfig]) {
        let mut map = self.live.write().unwrap_or_else(|p| p.into_inner());
        // Switched off: stopped like removed ones.
        let configs: Vec<&InputConfig> = configs.iter().filter(|c| c.enabled).collect();
        map.retain(|id, l| {
            let keep = configs.iter().any(|c| &c.id == id && **c == l.config);
            if !keep {
                l.halt();
            }
            keep
        });
        self.signals.forget_audio(|id| map.contains_key(id));
        for c in configs {
            if map.contains_key(&c.id) || c.id.is_empty() {
                continue;
            }
            let live = Arc::new(Live {
                config: c.clone(),
                picture: RwLock::new(None),
                paced: Mutex::new(Paced::default()),
                pool: Mutex::new(Vec::new()),
                received: AtomicU64::new(0),
                dropped: AtomicU64::new(0),
                last_at: Mutex::new(None),
                held_until: Mutex::new(None),
                message: Mutex::new("Starting…".into()),
                stop: AtomicBool::new(false),
                child: Mutex::new(None),
                rate: Mutex::new(None),
                interlaced: AtomicBool::new(false),
                signals: self.signals.clone(),
                no_audio: AtomicBool::new(false),
                no_info: AtomicBool::new(false),
                no_fps_mode: AtomicBool::new(false),
                relay: Mutex::new(None),
                facts: Arc::new(Mutex::new(crate::probe::Facts::default())),
                frames: tokio::sync::watch::channel((0, 0)).0,
                jpegs: Mutex::new(VecDeque::new()),
                recent: Mutex::new(VecDeque::new()),
                audio: tokio::sync::broadcast::channel(256).0,
                media: Default::default(),
            });
            let l = live.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("mv-input-{}", c.id))
                .spawn(move || match l.config.kind {
                    InputKind::Ndi => run_ndi(&l),
                    InputKind::Sdi => run_sdi(&l),
                    InputKind::St2110 => l.say("Receiving ST 2110 is not available yet"),
                    _ => run_stream(&l),
                });
            if let Err(e) = spawned {
                live.say(format!("cannot start: {e}"));
            }
            map.insert(c.id.clone(), live);
        }
    }

    pub fn status(&self) -> Vec<InputStatus> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.values()
            .map(|l| {
                let pic = l.picture.read().unwrap_or_else(|p| p.into_inner()).clone();
                let fresh = l.fresh();
                InputStatus {
                    id: l.config.id.clone(),
                    live: fresh,
                    message: l.message.lock().unwrap_or_else(|p| p.into_inner()).clone(),
                    width: pic.as_ref().map_or(0, |p| p.width()),
                    height: pic.as_ref().map_or(0, |p| p.height()),
                    frames: l.received.load(Ordering::Relaxed),
                    fps: *l.rate.lock().unwrap_or_else(|p| p.into_inner()),
                    interlaced: l.interlaced.load(Ordering::Relaxed),
                }
            })
            .collect()
    }

    /// What the load sampler counts.
    pub fn counts(&self) -> Vec<crate::perf::InputCounts> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.values()
            .map(|l| crate::perf::InputCounts {
                id: l.config.id.clone(),
                received: l.received.load(Ordering::Relaxed),
                dropped: l.dropped.load(Ordering::Relaxed),
                pid: l
                    .child
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                    .map(|c| c.id()),
            })
            .collect()
    }

    /// The newest picture and its number (previews: not delayed by the smoothing).
    /// Pictures received so far (changes when a new one comes).
    #[cfg(test)]
    pub fn received(&self, id: &str) -> Option<u64> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.get(id).map(|l| l.received.load(Ordering::Relaxed))
    }

    /// How long the picture of an input on show waited (its smoothing
    /// buffer, longer while its sound comes late): its sound and levels wait
    /// as long to go with it.
    pub fn delay_of(&self, id: &str) -> Duration {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.get(id).map_or(Duration::ZERO, |l| l.shown_wait())
    }

    /// The sound of input `id` for an output whose pictures are rendered
    /// `ahead` before they go: each call, the sound that goes with the
    /// picture going now (16-bit stereo at 48 kHz) — what came as long ago as
    /// that picture's, the input's smoothing and `ahead`. Followed again when
    /// the input restarts.
    pub fn sound_of(
        inputs: Arc<Inputs>,
        id: String,
        ahead: Duration,
    ) -> strata_mv_output::ndi::AudioSource {
        let mut rx: Option<tokio::sync::broadcast::Receiver<Pcm>> = None;
        let mut waiting: VecDeque<Pcm> = VecDeque::new();
        Box::new(move || {
            use tokio::sync::broadcast::error::TryRecvError;
            let mut out: Vec<i16> = Vec::new();
            if rx.is_none() {
                rx = inputs.audio(&id);
            }
            if let Some(r) = rx.as_mut() {
                loop {
                    match r.try_recv() {
                        Ok(c) => waiting.push_back(c),
                        Err(TryRecvError::Lagged(_)) => continue,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Closed) => {
                            rx = None;
                            break;
                        }
                    }
                }
            }
            let wait = inputs.delay_of(&id) + ahead;
            let Some(due) = Instant::now().checked_sub(wait) else {
                return out;
            };
            // A chunk goes when its middle is due (its samples, on average, with
            // their picture).
            let mid = |c: &Pcm| {
                c.at + Duration::from_secs_f64(c.data.len() as f64 / 4.0 / PCM_RATE as f64 / 2.0)
            };
            while waiting.front().is_some_and(|c| mid(c) <= due) {
                if let Some(c) = waiting.pop_front() {
                    out.extend(
                        c.data
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| i16::from_le_bytes(*b)),
                    );
                }
            }
            // Never more than a second waiting (an output stopped a while).
            while waiting.len() > 100 {
                waiting.pop_front();
            }
            out
        })
    }

    /// The sound of an input as it comes (see `Live::audio`).
    pub fn audio(&self, id: &str) -> Option<tokio::sync::broadcast::Receiver<Pcm>> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        Some(map.get(id)?.audio.subscribe())
    }

    /// Wakes when an input has a new picture: (its number, its time).
    pub fn watch(&self, id: &str) -> Option<tokio::sync::watch::Receiver<(u64, u64)>> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.get(id).map(|l| l.frames.subscribe())
    }

    /// The pictures of an input after number `after` (the last few), each
    /// with its number and smoothed time (ms since 1970); the newest one
    /// when `after` is None.
    pub fn pictures_after(&self, id: &str, after: Option<u64>) -> Vec<(u64, u64)> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        let Some(l) = map.get(id) else {
            return Vec::new();
        };
        let recent = l.recent.lock().unwrap_or_else(|p| p.into_inner());
        match after {
            Some(a) => recent
                .iter()
                .filter(|(n, ..)| *n > a)
                .map(|(n, ms, _)| (*n, *ms))
                .collect(),
            None => recent
                .back()
                .map(|(n, ms, _)| (*n, *ms))
                .into_iter()
                .collect(),
        }
    }

    /// Picture number `n` (the newest when it is gone) as JPEG at most
    /// `width` wide, made once for every page asking for this width.
    pub fn jpeg(&self, id: &str, width: u32, n: Option<u64>) -> Option<(u64, axum::body::Bytes)> {
        let l = self
            .live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)?
            .clone();
        let (n, pic) = {
            let recent = l.recent.lock().unwrap_or_else(|p| p.into_inner());
            let found = n.and_then(|n| recent.iter().find(|(m, ..)| *m == n));
            match found.or(recent.back()) {
                Some((m, _, p)) => (*m, p.clone()),
                None => (
                    l.received.load(Ordering::Relaxed),
                    l.picture
                        .read()
                        .unwrap_or_else(|p| p.into_inner())
                        .clone()?,
                ),
            }
        };
        {
            let made = l.jpegs.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((_, _, b)) = made.iter().find(|(m, w, _)| *m == n && *w == width) {
                return Some((n, b.clone()));
            }
        }
        let b = axum::body::Bytes::from(strata_mv_render::jpeg(&pic, width, 78)?);
        let mut made = l.jpegs.lock().unwrap_or_else(|p| p.into_inner());
        made.retain(|(m, _, _)| *m + 6 >= n);
        made.push_back((n, width, b.clone()));
        while made.len() > 16 {
            made.pop_front();
        }
        Some((n, b))
    }

    pub fn latest(&self, id: &str) -> Option<(u64, Arc<Pixmap>)> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        let l = map.get(id)?;
        let pic = l
            .picture
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()?;
        Some((l.received.load(Ordering::Relaxed), pic))
    }

    /// Everything known of what an input carries. An HLS input's playlists
    /// are read again when older than half a minute, or a few seconds when
    /// `refresh` (blocking: over the network).
    pub fn analysis(&self, id: &str, refresh: bool) -> Option<crate::probe::Report> {
        let l = self
            .live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)?
            .clone();
        let c = &l.config;
        let url = c.url.trim();
        let hls = c.kind == InputKind::Hls || (c.kind == InputKind::Url && url.contains(".m3u8"));
        if hls {
            let age = l.facts().hls.as_ref().map(|(at, ..)| at.elapsed());
            let stale = age.is_none_or(|a| a > Duration::from_secs(if refresh { 3 } else { 30 }));
            if stale {
                let (view, ts) = crate::probe::hls::analyse(url);
                l.facts().hls = Some((Instant::now(), view, ts));
            }
        }
        let mut r = l.facts().report(Instant::now());
        r.id = c.id.clone();
        r.name = c.name.clone();
        r.kind = match c.kind {
            InputKind::Srt if c.listen => "SRT (en écoute)",
            InputKind::Srt => "SRT (appel)",
            InputKind::Udp if url.starts_with("rtp") => "RTP",
            InputKind::Udp => "UDP",
            InputKind::Rtmp if c.listen => "RTMP (en écoute)",
            InputKind::Rtmp => "RTMP",
            InputKind::Hls => "HLS",
            InputKind::Rtsp => "RTSP",
            InputKind::Url => "Adresse",
            InputKind::Web => "Direct web",
            InputKind::Ndi => "NDI",
            InputKind::Sdi => "SDI (DeckLink)",
            InputKind::St2110 => "SMPTE ST 2110",
        }
        .to_string();
        r.source = match c.kind {
            InputKind::Ndi => c.ndi_source.clone(),
            InputKind::Sdi => format!("DeckLink {}", c.device + 1),
            InputKind::Srt if c.listen && c.port > 0 => format!("srt://0.0.0.0:{}", c.port),
            _ => url.to_string(),
        };
        r.live = l.fresh();
        r.status = l.message.lock().unwrap_or_else(|p| p.into_inner()).clone();
        // What cannot be known by this way in.
        match c.kind {
            InputKind::Rtmp | InputKind::Rtsp => r.notes.push(
                "Pas de flux de transport (MPEG-TS) dans ce protocole : ni tables, ni EPG, ni SCTE-35 en clair.".into(),
            ),
            InputKind::Udp if !c.relayed() => r.notes.push(
                "RTP : le flux de transport est lu par le décodeur seulement (pas de tables ni d’EPG).".into(),
            ),
            InputKind::Hls => r.notes.push(
                "HLS : les tables MPEG-TS viennent du dernier segment de la première variante, relu à la demande.".into(),
            ),
            InputKind::Sdi if !strata_mv_output::decklink::BUILT => {
                r.notes.push("Ce serveur est construit sans DeckLink.".into())
            }
            _ => {}
        }
        if l.no_info.load(Ordering::Relaxed) {
            r.notes.push(
                "Cet ffmpeg n’a pas le filtre showinfo : pas d’analyse image par image.".into(),
            );
        }
        Some(r)
    }

    /// The tally, UMD and audio levels of the inputs.
    pub fn signals(&self) -> &crate::signals::Signals {
        &self.signals
    }

    pub fn exists(&self, id: &str) -> bool {
        self.live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(id)
    }
}

impl strata_mv_render::LiveInputs for Inputs {
    fn picture(&self, id: &str) -> Option<Arc<Pixmap>> {
        let live = self
            .live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)?
            .clone();
        // A source that stopped sending: no picture (its tile shows the
        // bars), not its last one frozen.
        if !live.fresh() {
            return None;
        }
        live.current()
    }

    fn source(&self, id: &str) -> Option<strata_mv_core::signal::SourceView> {
        let l = self
            .live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)?
            .clone();
        let fresh = l.fresh();
        let height = l
            .picture
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map_or(0, |p| p.height());
        let fields = l.interlaced.load(Ordering::Relaxed);
        let rate = *l.rate.lock().unwrap_or_else(|p| p.into_inner());
        let format = match rate {
            Some(r) if fresh && height > 0 => {
                format_name(height, if fields { r / 2.0 } else { r }, fields).unwrap_or_default()
            }
            _ => String::new(),
        };
        let (loudness, history) = self.signals.loudness_of(id);
        Some(strata_mv_core::signal::SourceView {
            name: l.config.name.clone(),
            format,
            live: fresh,
            tally: self.signals.tally(id),
            // As late as the picture shown with them.
            levels: self.signals.levels_at(id, l.shown_wait()),
            loudness,
            history,
            timecode: self.signals.timecode_of(id),
            alarms: self.signals.alarms(id),
        })
    }

    fn system(&self) -> strata_mv_core::signal::SystemView {
        self.signals.system()
    }
}

impl Inputs {
    /// Every input, to look at its faults: id, name, a picture came in the
    /// last two seconds, when the last one came.
    pub fn watched(&self) -> Vec<(String, String, bool, Option<Instant>)> {
        self.live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|l| {
                let last = *l.last_at.lock().unwrap_or_else(|p| p.into_inner());
                let fresh = l.fresh();
                (l.config.id.clone(), l.config.name.clone(), fresh, last)
            })
            .collect()
    }
}

impl Inputs {
    /// Stops every input (the server stops).
    pub fn stop(&self) {
        for l in self.live.read().unwrap_or_else(|p| p.into_inner()).values() {
            l.halt();
        }
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        self.stop();
    }
}

// ------------------------------------------------------------------ API --

use axum::body::Body;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::api::{not_found, require, ApiResult};
use crate::auth::{CurrentUser, MaybeUser};
use crate::state::Shared;

/// An input as the Video widget and the Video streams page list it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputInfo {
    /// `id` comes with the status.
    pub name: String,
    pub kind: InputKind,
    #[serde(flatten)]
    pub status: InputStatus,
}

/// `GET /api/inputs`: every input with how it is doing (public like the
/// HTML5 outputs when they are, since their Video layers need it).
pub async fn list(
    State(s): State<Shared>,
    MaybeUser(me): MaybeUser,
) -> ApiResult<Json<Vec<InputInfo>>> {
    require(me.is_some() || s.config.public_outputs)?;
    let status = s.inputs.status();
    Ok(Json(
        s.config
            .inputs
            .iter()
            .map(|c| InputInfo {
                name: c.name.clone(),
                kind: c.kind,
                status: status
                    .iter()
                    .find(|x| x.id == c.id)
                    .cloned()
                    .unwrap_or_else(|| InputStatus {
                        id: c.id.clone(),
                        message: if c.enabled {
                            "Not started (save the settings)".into()
                        } else {
                            "Off".into()
                        },
                        ..Default::default()
                    }),
            })
            .collect(),
    ))
}

/// `GET /api/inputs/{id}/audio`: the sound of an input as it comes, to
/// listen to it, endless: chunks of a 4-byte length (little-endian), when
/// their first sample came (f64, ms since 1970, little-endian), then their
/// 16-bit stereo PCM at 48 kHz (little-endian).
pub async fn audio(
    State(s): State<Shared>,
    MaybeUser(me): MaybeUser,
    UrlPath(id): UrlPath<String>,
) -> Response {
    if me.is_none() && !s.config.public_outputs {
        return (StatusCode::UNAUTHORIZED, "sign in").into_response();
    }
    let Some(rx) = s.inputs.audio(&id) else {
        return not_found("input").into_response();
    };
    let closing = s.closing.subscribe();
    let stream = futures_util::stream::unfold((rx, closing), |(mut rx, closing)| async move {
        loop {
            if *closing.borrow() {
                return None;
            }
            match rx.recv().await {
                // Each chunk: its length and when its first sample came
                // (ms since 1970), then its samples.
                Ok(c) => {
                    let mut b = Vec::with_capacity(12 + c.data.len());
                    b.extend_from_slice(&(c.data.len() as u32).to_le_bytes());
                    b.extend_from_slice(&c.at_ms.to_le_bytes());
                    b.extend_from_slice(&c.data);
                    return Some((
                        Ok::<_, std::io::Error>(axum::body::Bytes::from(b)),
                        (rx, closing),
                    ));
                }
                // Too slow a listener: on from the newest.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                // The input went away (restarted): the page asks again.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct WebCheckQuery {
    url: String,
    #[serde(default)]
    height: Option<u32>,
}

/// `GET /api/inputs/web-check?url=…`: looks for the stream of a live's page
/// now, and says how it went.
pub async fn web_check(
    CurrentUser(me): CurrentUser,
    Query(q): Query<WebCheckQuery>,
) -> ApiResult<Json<crate::weblive::Check>> {
    require(me.is_admin())?;
    let height = q.height.unwrap_or(1080);
    tokio::task::spawn_blocking(move || crate::weblive::check(q.url.trim(), height))
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct PlaylistQuery {
    url: String,
}

/// `GET /api/inputs/playlist?url=…`: the channels of a list (IPTV M3U),
/// empty for a stream's own playlist.
pub async fn playlist_channels(
    CurrentUser(me): CurrentUser,
    Query(q): Query<PlaylistQuery>,
) -> ApiResult<Json<Vec<crate::weblive::Channel>>> {
    require(me.is_admin())?;
    let url = q.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err((
            StatusCode::BAD_REQUEST,
            "the address must start with http:// or https://".into(),
        ));
    }
    let read = tokio::task::spawn_blocking(move || crate::weblive::playlist(&url, 1080))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match read {
        Ok(crate::weblive::Playlist::Channels(c)) => Ok(Json(c)),
        Ok(_) => Ok(Json(Vec::new())),
        Err(e) => Err((StatusCode::BAD_GATEWAY, e)),
    }
}

/// `GET /api/inputs/ndi-sources`: the NDI sources seen on the network.
#[derive(Deserialize)]
pub struct AnalysisQuery {
    #[serde(default)]
    refresh: bool,
}

/// What an input carries (Video inputs page).
pub async fn analysis(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<AnalysisQuery>,
) -> ApiResult<Json<crate::probe::Report>> {
    let st = s.clone();
    tokio::task::spawn_blocking(move || st.inputs.analysis(&id, q.refresh))
        .await
        .map_err(crate::api::internal)?
        .map(Json)
        .ok_or_else(|| not_found("input"))
}

pub async fn ndi_sources(CurrentUser(me): CurrentUser) -> ApiResult<Json<Vec<String>>> {
    require(me.is_admin())?;
    tokio::task::spawn_blocking(|| strata_mv_output::ndi::sources(6000))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    /// Largest width of the pictures sent.
    #[serde(default)]
    w: Option<u32>,
}

#[derive(Deserialize)]
pub struct SnapshotQuery {
    #[serde(default)]
    w: Option<u32>,
    /// The number of the picture the page has: 204 while there is no newer one.
    #[serde(default)]
    after: Option<u64>,
}

/// `GET /api/inputs/<id>/snapshot.jpg?w=480&after=<n>`: the newest picture
/// as JPEG (`X-Frame`: its number), 204 when there is none or nothing newer.
/// The previews ask a few times a second: short requests, unlike a stream,
/// get through any proxy and never hold one of the browser's connections.
pub async fn snapshot(
    State(s): State<Shared>,
    MaybeUser(me): MaybeUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<SnapshotQuery>,
) -> Response {
    if me.is_none() && !s.config.public_outputs {
        return (StatusCode::UNAUTHORIZED, "sign in").into_response();
    }
    if !s.inputs.exists(&id) {
        return not_found("input").into_response();
    }
    let none = |n: Option<u64>| {
        let mut r = (
            StatusCode::NO_CONTENT,
            [(header::CACHE_CONTROL, "no-store")],
        )
            .into_response();
        if let Some(n) = n.and_then(|n| n.to_string().parse().ok()) {
            r.headers_mut().insert("x-frame", n);
        }
        r
    };
    let Some((n, pic)) = s.inputs.latest(&id) else {
        return none(None);
    };
    if q.after == Some(n) {
        return none(Some(n));
    }
    let width = q.w.unwrap_or(480).clamp(64, 1920);
    let jpeg = tokio::task::spawn_blocking(move || strata_mv_render::jpeg(&pic, width, 75))
        .await
        .ok()
        .flatten();
    let Some(jpeg) = jpeg else {
        return none(None);
    };
    let mut r = (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        jpeg,
    )
        .into_response();
    if let Ok(v) = n.to_string().parse() {
        r.headers_mut().insert("x-frame", v);
    }
    r
}

#[derive(Deserialize)]
pub struct NdiPreviewQuery {
    source: String,
    #[serde(default)]
    w: Option<u32>,
}

/// One picture of an NDI source and its format.
struct NdiShot {
    jpeg: Vec<u8>,
    width: u32,
    height: u32,
    fps: f64,
    interlaced: bool,
}

/// Connects to an NDI source just long enough for one picture (4 s at most).
fn ndi_shot(source: &str, width: u32) -> Result<Option<NdiShot>, String> {
    let mut rx = strata_mv_output::ndi::NdiReceiver::open(source)?;
    let until = Instant::now() + Duration::from_secs(4);
    while Instant::now() < until {
        let mut got = None;
        rx.capture(
            500,
            |f| {
                let fps = if f.rate.1 > 0 {
                    f.rate.0 as f64 / f.rate.1 as f64
                } else {
                    0.0
                };
                got = premultiplied(&f).map(|pm| (pm, fps, f.interlaced));
            },
            |_| {},
        )?;
        if let Some((pm, fps, interlaced)) = got {
            let (w, h) = (pm.width(), pm.height());
            return Ok(strata_mv_render::jpeg(&pm, width, 75).map(|jpeg| NdiShot {
                jpeg,
                width: w,
                height: h,
                fps,
                interlaced,
            }));
        }
    }
    Ok(None)
}

/// `GET /api/inputs/ndi-preview?source=HOST%20(name)`: a picture of an NDI
/// source (JPEG), its size and rate in `x-width`, `x-height`, `x-fps`,
/// `x-interlaced`; 204 when nothing came.
pub async fn ndi_preview(
    CurrentUser(me): CurrentUser,
    Query(q): Query<NdiPreviewQuery>,
) -> Response {
    if !me.is_admin() {
        return (StatusCode::FORBIDDEN, "admin only").into_response();
    }
    let width = q.w.unwrap_or(400).clamp(64, 960);
    let shot = tokio::task::spawn_blocking(move || ndi_shot(&q.source, width)).await;
    match shot {
        Ok(Ok(Some(s))) => {
            let mut r = (
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                s.jpeg,
            )
                .into_response();
            let h = r.headers_mut();
            for (k, v) in [
                ("x-width", s.width.to_string()),
                ("x-height", s.height.to_string()),
                ("x-fps", format!("{:.3}", s.fps)),
                ("x-interlaced", s.interlaced.to_string()),
            ] {
                if let Ok(v) = v.parse() {
                    h.insert(k, v);
                }
            }
            r
        }
        Ok(Ok(None)) => (
            StatusCode::NO_CONTENT,
            [(header::CACHE_CONTROL, "no-store")],
        )
            .into_response(),
        Ok(Err(e)) => (StatusCode::SERVICE_UNAVAILABLE, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// `GET /api/inputs/<id>/live.mjpeg`: the input as Motion JPEG, for the
/// previews and the HTML5 outputs (public like them when they are).
pub async fn mjpeg(
    State(s): State<Shared>,
    MaybeUser(me): MaybeUser,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<PreviewQuery>,
) -> Response {
    if me.is_none() && !s.config.public_outputs {
        return (StatusCode::UNAUTHORIZED, "sign in").into_response();
    }
    if !s.inputs.exists(&id) {
        return not_found("input").into_response();
    }
    let width = q.w.unwrap_or(1280).clamp(64, 3840);
    let hub = s.inputs.clone();
    let Some(frames) = hub.watch(&id) else {
        return not_found("input").into_response();
    };
    let mut closing = s.closing.subscribe();
    // Every picture, in order, as soon as it comes (woken by it, not
    // polling), with its smoothed time (X-Time, ms since 1970): the page
    // plays them out a frame apart. One at least every 2 s, so the page
    // knows the stream is alive.
    let part = |n: Option<u64>, at: Option<u64>, jpeg: &[u8]| {
        let mut head = format!(
            "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n",
            jpeg.len()
        );
        if let (Some(n), Some(at)) = (n, at) {
            head.push_str(&format!("X-Frame: {n}\r\nX-Time: {at}\r\n"));
        }
        head.push_str("\r\n");
        let mut b = head.into_bytes();
        b.extend_from_slice(jpeg);
        b.extend_from_slice(b"\r\n");
        b
    };
    let stream = futures_util::stream::unfold(
        (hub, id, None::<u64>, frames),
        move |(hub, id, mut last, mut frames)| {
            let stop = *closing.borrow_and_update();
            async move {
                if stop {
                    return None;
                }
                loop {
                    frames.borrow_and_update();
                    let list = hub.pictures_after(&id, last);
                    if !list.is_empty() {
                        let (h2, id2) = (hub.clone(), id.clone());
                        let made = tokio::task::spawn_blocking(move || {
                            list.iter()
                                .filter_map(|(n, at)| {
                                    h2.jpeg(&id2, width, Some(*n)).map(|(m, j)| (m, *at, j))
                                })
                                .collect::<Vec<_>>()
                        })
                        .await
                        .unwrap_or_default();
                        if let Some((n, ..)) = made.last() {
                            last = Some(*n);
                            let mut out = Vec::new();
                            for (n, at, j) in &made {
                                out.extend(part(Some(*n), Some(*at), j));
                            }
                            return Some((
                                Ok::<_, std::io::Error>(axum::body::Bytes::from(out)),
                                (hub, id, last, frames),
                            ));
                        }
                    }
                    match tokio::time::timeout(std::time::Duration::from_secs(2), frames.changed())
                        .await
                    {
                        // The input went away: the stream ends.
                        Ok(Err(_)) => return None,
                        Ok(Ok(())) => {}
                        // Nothing new for 2 s: the same picture again.
                        Err(_) if last.is_some() => {
                            let (h2, id2) = (hub.clone(), id.clone());
                            if let Some((_, jpeg)) =
                                tokio::task::spawn_blocking(move || h2.jpeg(&id2, width, None))
                                    .await
                                    .ok()
                                    .flatten()
                            {
                                return Some((
                                    Ok(axum::body::Bytes::from(part(None, None, &jpeg))),
                                    (hub, id, last, frames),
                                ));
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
        },
    );
    (
        [
            (
                header::CONTENT_TYPE,
                "multipart/x-mixed-replace; boundary=frame",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

/// NDI timecode (100 ns units: the sender's, or its clock's time of day,
/// UTC) as the server's local time of day, "HH:MM:SS:FF" at `fps`.
fn ndi_timecode(tc: i64, fps: f64) -> Option<String> {
    if tc <= 0 || tc == i64::MAX || fps <= 0.0 {
        return None;
    }
    let t = jiff::Timestamp::from_nanosecond(i128::from(tc) * 100).ok()?;
    let z = t.to_zoned(jiff::tz::TimeZone::system());
    let frame = (f64::from(z.subsec_nanosecond()) / 1e9 * fps).floor() as i64;
    Some(format!(
        "{:02}:{:02}:{:02}:{:02}",
        z.hour(),
        z.minute(),
        z.second(),
        frame
    ))
}

/// EBU R128 loudness of NDI audio (ffmpeg measures the streams'), and the
/// highest true peak of the last three seconds.
#[derive(Default)]
struct R128 {
    meter: Option<(ebur128::EbuR128, usize, u32)>,
    peaks: std::collections::VecDeque<(Instant, f32)>,
}

impl R128 {
    fn add(
        &mut self,
        a: &strata_mv_output::ndi::ReceivedAudio,
    ) -> Option<strata_mv_core::signal::Loudness> {
        use ebur128::{EbuR128, Mode};
        let (ch, rate) = (a.channels, a.sample_rate);
        if ch == 0 || rate == 0 || a.samples == 0 {
            return None;
        }
        if !matches!(&self.meter, Some((_, c, r)) if *c == ch && *r == rate) {
            let m = EbuR128::new(
                ch as u32,
                rate,
                // Histogram: the integrated loudness and the range cost the same
                // after hours as after a second.
                Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK | Mode::HISTOGRAM,
            )
            .ok()?;
            self.meter = Some((m, ch, rate));
        }
        let (m, _, _) = self.meter.as_mut()?;
        let planes: Vec<&[f32]> = (0..ch).map(|c| a.channel(c)).collect();
        m.add_frames_planar_f32(&planes).ok()?;
        let now = Instant::now();
        let tp = (0..ch as u32)
            .filter_map(|c| m.prev_true_peak(c).ok())
            .fold(0.0f64, f64::max) as f32;
        self.peaks.push_back((now, tp));
        while self
            .peaks
            .front()
            .is_some_and(|(t, _)| t.elapsed() > Duration::from_secs(3))
        {
            self.peaks.pop_front();
        }
        let peak = self.peaks.iter().map(|(_, p)| *p).fold(0.0, f32::max);
        let lufs = |r: Result<f64, ebur128::Error>| {
            r.ok()
                .filter(|v| v.is_finite())
                .map_or(-70.0, |v| (v as f32).max(-70.0))
        };
        Some(strata_mv_core::signal::Loudness {
            momentary: lufs(m.loudness_momentary()),
            short_term: lufs(m.loudness_shortterm()),
            integrated: lufs(m.loudness_global()),
            range: m
                .loudness_range()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(0.0) as f32,
            true_peak: strata_mv_core::signal::db(peak),
        })
    }
}

/// A stream through ffmpeg, started again 2 s after it stops.
/// Where a stream is read from: a web page's stream (looked for again at
/// each start: it expires), the variant of a master playlist.
fn prepare(l: &Live) -> Result<Pull, String> {
    use crate::weblive::{self, Playlist};
    let c = &l.config;
    let url = c.url.trim();
    let mut pull = Pull::default();
    let lower = url.to_ascii_lowercase();
    let listed = match c.kind {
        InputKind::Web => {
            l.say("Looking for the stream on the page…");
            let r = weblive::resolve(url, c.height)?;
            pull.url = Some(r.url.clone());
            pull.input = weblive::header_args(&r.headers);
            l.facts().web = Some(r);
            false
        }
        InputKind::Hls => true,
        InputKind::Url => {
            lower.contains(".m3u8") || lower.split('?').next().is_some_and(|p| p.ends_with(".m3u"))
        }
        _ => false,
    };
    if listed && (lower.starts_with("http://") || lower.starts_with("https://")) {
        match weblive::playlist(url, c.height) {
            // Its variant read directly (the others are never fetched), and
            // its sound apart when it is.
            Ok(Playlist::Master { video, audio, .. }) => {
                pull.url = Some(video);
                pull.audio = audio;
            }
            Ok(Playlist::Channels(list)) => {
                return Err(format!(
                    "This address is a list of {} channels: choose one",
                    list.len()
                ))
            }
            // Unreadable here: ffmpeg tells why.
            Ok(Playlist::Media) | Err(_) => {}
        }
    }
    Ok(pull)
}

fn run_stream(l: &Live) {
    let c = &l.config;
    if let Err(e) = c.ffmpeg_args(false, false) {
        return l.say(e);
    }
    // A stream that stops at once, again and again: asked less and less
    // often (a web page is not read every two seconds).
    let (mut started, mut tries) = (None::<Instant>, 0u64);
    while !l.stopped() {
        if started.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            tries += 1;
            let wait = if c.kind == InputKind::Web {
                (5 * tries).min(120)
            } else {
                (tries - 1).min(8)
            };
            if wait > 0 && !l.pause(Duration::from_secs(wait)) {
                return;
            }
        } else {
            tries = 0;
        }
        started = Some(Instant::now());
        let mut pull = match prepare(l) {
            Ok(p) => p,
            Err(e) => {
                l.say(e);
                if !l.pause(Duration::from_secs(10)) {
                    return;
                }
                continue;
            }
        };
        let audio = !l.no_audio.load(Ordering::Relaxed);
        let info = !l.no_info.load(Ordering::Relaxed);
        // The sound to be listened to: ffmpeg sends it to this port.
        let pcm = audio
            .then(|| std::net::TcpListener::bind("127.0.0.1:0").ok())
            .flatten();
        pull.pcm = pcm
            .as_ref()
            .and_then(|t| t.local_addr().ok())
            .map(|a| a.port());
        pull.fps_mode = !l.no_fps_mode.load(Ordering::Relaxed);
        let args = c.ffmpeg_args_from(&pull, audio, info).unwrap_or_default();
        let log = Arc::new(Mutex::new(String::new()));
        // The relay first: the decoder reads what it writes.
        let mut relay = None;
        if let Some(ra) = &args.relay {
            match strata_mv_output::ffmpeg::command()
                .args(ra)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(mut child) => {
                    if let Some(err) = child.stderr.take() {
                        let log = log.clone();
                        std::thread::spawn(move || {
                            for line in log_lines(err) {
                                keep_log(&log, &line);
                            }
                        });
                    }
                    relay = Some(child);
                }
                Err(e) => {
                    l.say(format!("ffmpeg is needed for stream inputs ({e})"));
                    if !l.pause(Duration::from_secs(5)) {
                        return;
                    }
                    continue;
                }
            }
        }
        let spawned = strata_mv_output::ffmpeg::command()
            .args(&args.decode)
            .stdin(if relay.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                if let Some(mut r) = relay {
                    let _ = r.kill();
                    let _ = r.wait();
                }
                l.say(format!("ffmpeg is needed for stream inputs ({e})"));
                if !l.pause(Duration::from_secs(5)) {
                    return;
                }
                continue;
            }
        };
        l.say(if c.listen {
            "Waiting for the sender…"
        } else {
            "Connecting…"
        });
        l.facts().reset(relay.is_some());
        *l.media.lock().unwrap_or_else(|p| p.into_inner()) = MediaClock::default();
        let mut out = child.stdout.take();
        let into_decoder = child.stdin.take();
        let from_relay = relay.as_mut().and_then(|r| r.stdout.take());
        // The picture ffmpeg sends: told in its log before the first one.
        let (format_tx, format_rx) = std::sync::mpsc::channel::<(u32, u32, Option<f64>)>();
        if let Some(err) = child.stderr.take() {
            let log = log.clone();
            let (signals, id, facts) = (l.signals.clone(), c.id.clone(), l.facts.clone());
            let media = l.media.clone();
            std::thread::spawn(move || {
                let mut sent = false;
                let mut in_output = false;
                let mut meters = MeterLog::default();
                let lock = || media.lock().unwrap_or_else(|p| p.into_inner());
                for line in log_lines(err) {
                    // The audio levels: to the meters, not to the log; timed
                    // by the sound's own time (with the pictures).
                    if let Some(levels) = meters.line(&line) {
                        if let Some(pts) = meters.pts {
                            let mut m = lock();
                            m.audio_start.get_or_insert(pts);
                        }
                        if let Some(levels) = levels {
                            let at = meters.done_pts.and_then(|pts| {
                                let mut m = lock();
                                m.saw_sound(pts + 0.04);
                                m.at(pts)
                            });
                            signals.measure_at(&id, &levels, at.unwrap_or_else(Instant::now));
                            if let Some(l) = meters.loudness.take() {
                                signals.loudness(&id, l);
                            }
                        }
                        continue;
                    }
                    // A picture's own time (`showinfo`), for when it is read.
                    if line.starts_with("[Parsed_showinfo") && line.contains(" n:") {
                        let n = line
                            .split(" n:")
                            .nth(1)
                            .and_then(|r| r.split_whitespace().next())
                            .and_then(|v| v.parse::<u64>().ok());
                        if let (Some(n), Some(t)) = (n, pts_time(&line)) {
                            let mut m = lock();
                            m.pts.push_back((n, t));
                            while m.pts.len() > 120 {
                                m.pts.pop_front();
                            }
                        }
                    }
                    // Each picture and the opening of the input: what it carries.
                    let taken = facts.lock().unwrap_or_else(|p| p.into_inner()).line(
                        &line,
                        Instant::now(),
                        |tc| signals.timecode(&id, tc),
                    );
                    if taken {
                        continue;
                    }
                    if line.starts_with("Output #") {
                        in_output = true;
                    }
                    if in_output && !sent {
                        if let Some(f) = decoded_format(&line) {
                            let _ = format_tx.send(f);
                            sent = true;
                        }
                    }
                    keep_log(&log, &line);
                }
            });
        }
        *l.child.lock().unwrap_or_else(|p| p.into_inner()) = Some(child);
        *l.relay.lock().unwrap_or_else(|p| p.into_inner()) = relay;
        std::thread::scope(|scope| {
            if let Some(listener) = pcm {
                scope.spawn(move || read_pcm(l, listener));
            }
            // The relayed bytes: read, then on to the decoder.
            if let (Some(mut from), Some(mut to)) = (from_relay, into_decoder) {
                scope.spawn(move || {
                    let mut buf = vec![0u8; 64 * 1024];
                    while let Ok(n) = from.read(&mut buf) {
                        if n == 0 || l.stopped() {
                            break;
                        }
                        if let Some(ts) = l.facts().ts.as_mut() {
                            ts.feed(&buf[..n], Instant::now());
                        }
                        if to.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                });
            }
            // Until the sender comes (a listener can wait long), or ffmpeg stops.
            let format = loop {
                match format_rx.recv_timeout(Duration::from_millis(300)) {
                    Ok(f) => break Some(f),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) if !l.stopped() => {}
                    Err(_) => break None,
                }
            };
            if let (Some((w, h, fps)), Some(out)) = (format, out.as_mut()) {
                *l.rate.lock().unwrap_or_else(|p| p.into_inner()) = fps;
                let size = (w as usize) * (h as usize) * 4;
                let mut first = true;
                // Pictures read so far (their number in ffmpeg's `showinfo`).
                let mut count = 0u64;
                loop {
                    let mut buf = l.buffer(size);
                    if out.read_exact(&mut buf).is_err() || l.stopped() {
                        break;
                    }
                    // Its own time, as ffmpeg told it (`showinfo`, by the
                    // picture's number: told before the picture is written,
                    // its line may still be on its way here).
                    let when = info.then(|| {
                        let start = Instant::now();
                        loop {
                            let mut m = l.media.lock().unwrap_or_else(|p| p.into_inner());
                            while m.pts.front().is_some_and(|(n, _)| *n < count) {
                                m.pts.pop_front();
                            }
                            if let Some(&(n, pts)) = m.pts.front() {
                                if n == count {
                                    m.pts.pop_front();
                                    m.saw_picture(pts);
                                    break m.at(pts);
                                }
                            }
                            drop(m);
                            if start.elapsed() > Duration::from_millis(40) {
                                break None;
                            }
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    });
                    let when = when.flatten();
                    count += 1;
                    // Opaque or transparent black: already premultiplied.
                    if let Some(pm) = IntSize::from_wh(w, h).and_then(|s| Pixmap::from_vec(buf, s))
                    {
                        l.put_at(pm, when);
                    }
                    if first {
                        first = false;
                        l.say(format!("Receiving {w}×{h}"));
                    }
                }
            }
            // Both stopped: the relay's reader sees the end.
            for child in [&l.child, &l.relay] {
                if let Some(mut c) = child.lock().unwrap_or_else(|p| p.into_inner()).take() {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        });
        if l.stopped() {
            return;
        }
        let said = log.lock().unwrap_or_else(|p| p.into_inner()).clone();
        // No audio track: again at once, without the meters.
        if audio && said.contains("matches no streams") {
            l.no_audio.store(true, Ordering::Relaxed);
            continue;
        }
        // An ffmpeg without -fps_mode (before 5.1): again, without it.
        if pull.fps_mode && said.contains("fps_mode") {
            l.no_fps_mode.store(true, Ordering::Relaxed);
            continue;
        }
        // An ffmpeg without showinfo (or its options): again, without it.
        if info && (said.contains("showinfo") || said.contains("'checksum'")) {
            l.no_info.store(true, Ordering::Relaxed);
            continue;
        }
        let why = strata_mv_output::ffmpeg::tail(&said);
        l.say(format!("Stopped: {why} — trying again"));
        if !l.pause(Duration::from_secs(2)) {
            return;
        }
    }
}

/// The sound ffmpeg sends to `listener`, to the people listening to it,
/// until the decoder stops.
fn read_pcm(l: &Live, listener: std::net::TcpListener) {
    let _ = listener.set_nonblocking(true);
    let gone = || l.stopped() || l.child.lock().unwrap_or_else(|p| p.into_inner()).is_none();
    let mut conn = loop {
        match listener.accept() {
            Ok((c, _)) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && !gone() => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return,
        }
    };
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(1)));
    // 20 ms at a time, whole stereo samples, timed.
    let mut buf = vec![0u8; 3840];
    let mut have = 0;
    let mut clock = SampleClock::default();
    // Samples read so far: the sound's own time is its start plus as many.
    let mut read = 0u64;
    loop {
        match conn.read(&mut buf[have..]) {
            Ok(0) => return,
            Ok(n) => {
                have += n;
                let whole = have - have % 4;
                if whole >= 1920 || have == buf.len() {
                    let n = (whole / 4) as u64;
                    let fallback = clock.time(n as usize);
                    let at = {
                        let mut m = l.media.lock().unwrap_or_else(|p| p.into_inner());
                        m.audio_start.and_then(|a0| {
                            let rate = PCM_RATE as f64;
                            m.saw_sound(a0 + (read + n) as f64 / rate);
                            m.at(a0 + read as f64 / rate)
                        })
                    }
                    .unwrap_or(fallback);
                    read += n;
                    if l.audio.receiver_count() > 0 {
                        let _ = l.audio.send(Pcm {
                            at,
                            at_ms: epoch_ms(at),
                            data: axum::body::Bytes::copy_from_slice(&buf[..whole]),
                        });
                    }
                    buf.copy_within(whole..have, 0);
                    have -= whole;
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if gone() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// Sound of NDI (float planes at its rate) as listened to: stereo, 16-bit,
/// at `PCM_RATE` (the first two channels; one is heard on both sides).
fn ndi_pcm(a: &strata_mv_output::ndi::ReceivedAudio) -> Vec<u8> {
    let (left, right) = (a.channel(0), a.channel(if a.channels > 1 { 1 } else { 0 }));
    let n = left.len().min(right.len());
    if n == 0 || a.sample_rate == 0 {
        return Vec::new();
    }
    let step = a.sample_rate as f64 / PCM_RATE as f64;
    let out_n = (n as f64 / step).floor() as usize;
    let at = |s: &[f32], x: f64| {
        let i = x.floor() as usize;
        let k = (x - i as f64) as f32;
        let a = s.get(i).copied().unwrap_or(0.0);
        let b = s.get(i + 1).copied().unwrap_or(a);
        a + (b - a) * k
    };
    let mut out = Vec::with_capacity(out_n * 4);
    for j in 0..out_n {
        let x = j as f64 * step;
        for v in [at(left, x), at(right, x)] {
            let s = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
    out
}

/// Adds a line to the log of an input (its last few kilobytes).
fn keep_log(log: &Mutex<String>, line: &str) {
    let mut s = log.lock().unwrap_or_else(|p| p.into_inner());
    s.push_str(line);
    s.push('\n');
    if s.len() > 8192 {
        let cut = s.len() - 4096;
        *s = s[s.ceil_char_boundary(cut)..].to_string();
    }
}

/// Reads the levels ffmpeg writes in its log (`ametadata=mode=print` after
/// `astats`): `frame:… pts:…`, then `lavfi.astats.<channel>.Peak_level=…` and
/// `RMS_level=…` for each channel.
#[derive(Default)]
struct MeterLog {
    /// Peak and power of each channel (1-based) of the frame being read.
    frame: BTreeMap<usize, (f32, f32)>,
    /// `ebur128`'s values of the frame being read (`M`, `S`, `I`, `LRA`, `true_peak`).
    r128: BTreeMap<String, f32>,
    /// The loudness of the last frame read whole (taken by the caller).
    loudness: Option<strata_mv_core::signal::Loudness>,
    /// The time (PTS) of the frame being read, and of the last one read whole.
    pts: Option<f64>,
    done_pts: Option<f64>,
}

impl MeterLog {
    /// None: not a line of the meters. Some(Some(levels)): a frame read
    /// whole (the next one starts).
    fn line(&mut self, line: &str) -> Option<Option<Vec<strata_mv_core::signal::Level>>> {
        use strata_mv_core::signal::{Level, SILENCE};
        if !line.contains("Parsed_ametadata") {
            return None;
        }
        if let Some(rest) = line.split("lavfi.r128.").nth(1) {
            let (key, value) = rest.split_once('=')?;
            if let Ok(v) = value.trim().parse::<f32>() {
                self.r128.insert(key.to_string(), v);
            }
            return Some(None);
        }
        if let Some(rest) = line.split("lavfi.astats.").nth(1) {
            let (key, value) = rest.split_once('=')?;
            let (ch, what) = key.split_once('.')?;
            let ch: usize = ch.parse().ok()?;
            let v = value
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(SILENCE)
                .max(SILENCE);
            let e = self.frame.entry(ch).or_insert((SILENCE, SILENCE));
            match what {
                "Peak_level" => e.0 = v,
                "RMS_level" => e.1 = v,
                _ => {}
            }
            return Some(None);
        }
        if line.contains("frame:") && !self.r128.is_empty() {
            let r = std::mem::take(&mut self.r128);
            let get = |k: &str| r.get(k).copied().filter(|v| v.is_finite());
            if let (Some(m), Some(st), Some(i)) = (get("M"), get("S"), get("I")) {
                self.loudness = Some(strata_mv_core::signal::Loudness {
                    momentary: m.max(-70.0),
                    short_term: st.max(-70.0),
                    integrated: i.max(-70.0),
                    range: get("LRA").unwrap_or(0.0),
                    // Linear: in dBTP.
                    true_peak: strata_mv_core::signal::db(get("true_peak").unwrap_or(0.0)),
                });
            }
        }
        if line.contains("frame:") {
            self.done_pts = self.pts;
            self.pts = pts_time(line);
        }
        if line.contains("frame:") && !self.frame.is_empty() {
            let levels = std::mem::take(&mut self.frame)
                .into_values()
                .map(|(peak, rms)| Level {
                    peak,
                    rms,
                    hold: peak,
                })
                .collect();
            return Some(Some(levels));
        }
        Some(None)
    }
}

/// The lines of ffmpeg's log, read to the end: a line that is not UTF-8
/// (stream metadata in another charset) is kept as best it can be, not
/// taken as the end, which would leave ffmpeg's log unread.
fn log_lines(r: impl Read) -> impl Iterator<Item = String> {
    std::io::BufReader::new(r)
        .split(b'\n')
        .map_while(Result::ok)
        .map(|l| String::from_utf8_lossy(l.strip_suffix(b"\r").unwrap_or(&l)).into_owned())
}

/// The size and rate of the raw pictures ffmpeg sends, from its log line
/// `Stream #0:0: Video: rawvideo (RGBA …), rgba(…), 1920x1080 [SAR 1:1 …], …, 25 fps, …`.
fn decoded_format(line: &str) -> Option<(u32, u32, Option<f64>)> {
    if !line.contains("Video: rawvideo") {
        return None;
    }
    let mut size = None;
    let mut fps = None;
    for part in line.split(", ") {
        let word = part.trim().split(' ').next().unwrap_or("");
        if size.is_none() {
            if let Some((w, h)) = word.split_once('x') {
                if let (Ok(w), Ok(h)) = (w.parse::<u32>(), h.parse::<u32>()) {
                    size = Some((w, h));
                }
            }
        }
        if let Some(v) = part.trim().strip_suffix(" fps") {
            fps = v.trim().parse::<f64>().ok();
        }
    }
    let (w, h) = size.filter(|(w, h)| *w >= 16 && *h >= 16 && *w <= 8192 && *h <= 8192)?;
    Some((w, h, fps))
}

/// An NDI source, connected again when it goes away.
fn run_ndi(l: &Live) {
    let source = l.config.ndi_source.trim().to_string();
    if source.is_empty() {
        return l.say("no NDI source chosen");
    }
    while !l.stopped() {
        let mut rx = match strata_mv_output::ndi::NdiReceiver::open(&source) {
            Ok(r) => r,
            Err(e) => {
                l.say(e);
                if !l.pause(Duration::from_secs(5)) {
                    return;
                }
                continue;
            }
        };
        l.say(format!("Connecting to {source}…"));
        let mut shown = None;
        let mut loudness = R128::default();
        let mut ndi_clock = SampleClock::default();
        l.facts().reset(false);
        let (mut video_frames, mut audio_frames) = (0u64, 0u64);
        while !l.stopped() {
            let (mut pic, mut format, mut tc) = (None, ((0, 0), false), 0i64);
            let mut about: Option<(f32, i32, Option<String>)> = None;
            let mut sound_of: Option<(u32, usize, usize)> = None;
            let signals = &l.signals;
            match rx.capture(
                500,
                |f| {
                    pic = premultiplied(&f);
                    format = (f.rate, f.interlaced);
                    tc = f.timecode;
                    about = Some((
                        f.aspect,
                        f.format,
                        f.metadata.map(|m| m.chars().take(2048).collect()),
                    ));
                },
                |a| {
                    sound_of = Some((a.sample_rate, a.channels, a.samples));
                    // To the people listening to it.
                    if l.audio.receiver_count() > 0 {
                        let pcm = ndi_pcm(&a);
                        if !pcm.is_empty() {
                            let at = ndi_clock.time(pcm.len() / 4);
                            let _ = l.audio.send(Pcm {
                                at,
                                at_ms: epoch_ms(at),
                                data: axum::body::Bytes::from(pcm),
                            });
                        }
                    }
                    let levels: Vec<_> = (0..a.channels)
                        .map(|c| strata_mv_core::signal::Level::of(a.channel(c).iter().copied()))
                        .collect();
                    signals.measure(&l.config.id, &levels);
                    if let Some(v) = loudness.add(&a) {
                        signals.loudness(&l.config.id, v);
                    }
                },
            ) {
                Ok(_) => {}
                Err(e) => {
                    l.say(format!("{e} — trying again"));
                    break;
                }
            }
            // A sender sends a picture when it changes only (a screen, a
            // still multiview): connected after a first picture, it is there.
            l.hold(shown.is_some() && rx.connected() == Some(true));
            // What the source sends: for the analysis of the input.
            if about.is_some() || sound_of.is_some() || rx.metadata.is_some() {
                let mut facts = l.facts();
                let n = facts.ndi.get_or_insert_with(|| crate::probe::NdiView {
                    source: source.clone(),
                    ..Default::default()
                });
                if let Some(m) = rx.metadata.take() {
                    n.metadata = m;
                }
                if let Some((rate, channels, samples)) = sound_of {
                    audio_frames += 1;
                    n.audio_rate = rate;
                    n.audio_channels = channels;
                    n.audio_samples = samples;
                    n.audio_frames = audio_frames;
                }
                if let (Some((aspect, kind, meta)), Some(pm)) = (&about, &pic) {
                    video_frames += 1;
                    n.width = pm.width();
                    n.height = pm.height();
                    n.rate = format.0;
                    n.aspect = *aspect;
                    n.format = match kind {
                        0 => "entrelacé (deux trames)",
                        1 => "progressif",
                        2 => "trame 0",
                        3 => "trame 1",
                        _ => "?",
                    }
                    .to_string();
                    if let Some(m) = meta {
                        n.frame_metadata = m.clone();
                    }
                    n.video_frames = video_frames;
                }
            }
            if let Some(pm) = pic {
                let ((n, d), fields) = format;
                let fps = if d > 0 { n as f64 / d as f64 } else { 0.0 };
                if let Some(s) = ndi_timecode(tc, fps) {
                    if let Some(v) = l.facts().ndi.as_mut() {
                        v.timecode = s.clone();
                    }
                    signals.timecode(&l.config.id, s);
                }
                let now = (pm.width(), pm.height(), (n, d), fields);
                if shown != Some(now) {
                    shown = Some(now);
                    // An interlaced source counts its fields (shown one by one).
                    *l.rate.lock().unwrap_or_else(|p| p.into_inner()) =
                        (fps > 0.0).then_some(if fields { fps * 2.0 } else { fps });
                    l.interlaced.store(fields, Ordering::Relaxed);
                    l.say(match format_name(pm.height(), fps, fields) {
                        Some(name) => format!("Receiving {}×{} ({name})", pm.width(), pm.height()),
                        None => format!("Receiving {}×{}", pm.width(), pm.height()),
                    });
                }
                if fields {
                    let Some(size) = IntSize::from_wh(pm.width(), pm.height()) else {
                        continue;
                    };
                    for f in strata_mv_output::sdi::DeckLinkInput::fields(
                        pm.data(),
                        pm.width(),
                        pm.height(),
                        false,
                    ) {
                        if let Some(pm) = Pixmap::from_vec(f, size) {
                            l.put(pm);
                        }
                    }
                } else {
                    l.put(pm);
                }
            }
        }
        drop(rx);
        l.hold(false);
        if !l.pause(Duration::from_secs(1)) {
            return;
        }
    }
}

/// "1080i50", "720p59.94": a picture of `height` lines at `fps` frames a
/// second (two fields each when `fields`).
fn format_name(height: u32, fps: f64, fields: bool) -> Option<String> {
    if fps <= 0.0 {
        return None;
    }
    let rate = if fields { fps * 2.0 } else { fps };
    let rate = if (rate - rate.round()).abs() < 0.005 {
        format!("{}", rate.round())
    } else {
        format!("{:.2}", rate)
    };
    Some(format!("{height}{}{rate}", if fields { 'i' } else { 'p' }))
}

/// The formats an SDI input opens in, in turn, until a signal comes: a
/// card that detects the incoming format follows it from the first one;
/// one that cannot is tried in each until the picture comes in.
const SDI_FORMATS: &[(u32, u32, (u32, u32), bool)] = &[
    (1920, 1080, (25, 1), true),
    (1920, 1080, (50, 1), false),
    (1920, 1080, (25, 1), false),
    (1280, 720, (50, 1), false),
    (1920, 1080, (30000, 1001), true),
    (1920, 1080, (60000, 1001), false),
    (1920, 1080, (30000, 1001), false),
    (1280, 720, (60000, 1001), false),
    (3840, 2160, (50, 1), false),
    (3840, 2160, (25, 1), false),
    (3840, 2160, (60000, 1001), false),
    (1920, 1080, (24000, 1001), false),
    (1920, 1080, (24, 1), false),
    (720, 576, (25, 1), true),
    (720, 486, (30000, 1001), true),
];

/// An SDI input of a DeckLink card, in the format of the signal: detected
/// by the card when it can, else found by trying the usual ones. An
/// interlaced signal gives a picture per field (1080i50: 50 a second).
fn run_sdi(l: &Live) {
    let c = &l.config;
    let mut attempt = 0usize;
    let mut failures = 0usize;
    while !l.stopped() {
        let (w, h, rate, interlaced) = SDI_FORMATS[attempt % SDI_FORMATS.len()];
        let mut card =
            match strata_mv_output::sdi::DeckLinkInput::open(c.device, w, h, rate, interlaced) {
                Ok(card) => card,
                Err(e) => {
                    // A format the card does not take: the next one; none at all: wait.
                    attempt += 1;
                    failures += 1;
                    if failures >= SDI_FORMATS.len() {
                        failures = 0;
                        l.say(e);
                        if !l.pause(Duration::from_secs(5)) {
                            return;
                        }
                    }
                    continue;
                }
            };
        failures = 0;
        l.say(format!("Waiting for a signal ({})…", card.mode().name));
        l.facts().reset(false);
        let opened = Instant::now();
        let (mut seq, mut shown) = (0u64, None);
        let mut described = Instant::now() - Duration::from_secs(10);
        while !l.stopped() {
            match card.take(&mut seq) {
                Some((w, h, rgba)) => {
                    let mode = card.mode();
                    let fps = mode.fps();
                    let fields = mode.interlaced();
                    l.interlaced.store(fields, Ordering::Relaxed);
                    *l.rate.lock().unwrap_or_else(|p| p.into_inner()) =
                        (fps > 0.0).then_some(if fields { fps * 2.0 } else { fps });
                    if shown != Some((w, h, mode.id)) {
                        shown = Some((w, h, mode.id));
                        l.say(format!("Receiving {w}×{h} ({})", mode.name));
                    }
                    let Some(size) = IntSize::from_wh(w, h) else {
                        continue;
                    };
                    if fields {
                        let lower = mode.field == strata_mv_output::decklink::FIELD_LOWER;
                        for f in strata_mv_output::sdi::DeckLinkInput::fields(&rgba, w, h, lower) {
                            if let Some(pm) = Pixmap::from_vec(f, size) {
                                l.put(pm);
                            }
                        }
                    } else if let Some(pm) = Pixmap::from_vec(rgba, size) {
                        l.put(pm);
                    }
                    // What it carries: for the analysis, twice a second.
                    if described.elapsed() >= Duration::from_millis(500) {
                        described = Instant::now();
                        let view =
                            sdi_view(c.device, &mode, &card.info(), l.signals.levels(&c.id).len());
                        if !view.timecode.is_empty() {
                            l.signals.timecode(&c.id, view.timecode.clone());
                        }
                        l.facts().sdi = Some(view);
                    }
                    // Its embedded audio, measured since the last picture.
                    let audio = card.audio();
                    if !audio.is_empty() {
                        use strata_mv_core::signal::{db, Level};
                        let levels: Vec<Level> = audio
                            .iter()
                            .map(|&(peak, rms)| Level {
                                peak: db(peak),
                                rms: db(rms),
                                hold: db(peak),
                            })
                            .collect();
                        l.signals.measure(&c.id, &levels);
                    }
                }
                None => {
                    if card.signal() == Some(false) {
                        if shown.is_some() {
                            shown = None;
                            l.say("No signal");
                        }
                        // Nothing in this format on a card that cannot tell: the next one.
                        if !card.detects() && opened.elapsed() > Duration::from_millis(1500) {
                            attempt += 1;
                            break;
                        }
                    }
                    // Woken as soon as the card has the next frame.
                    card.wait(seq, 40);
                }
            }
        }
    }
}

/// What an SDI signal carries, for the analysis.
fn sdi_view(
    device: u32,
    mode: &strata_mv_output::decklink::DisplayMode,
    i: &strata_mv_output::decklink::SignalInfo,
    channels: usize,
) -> crate::probe::SdiView {
    let d = i.detected;
    let mut signal = Vec::new();
    if d & 1 != 0 {
        signal.push("YCbCr 4:2:2");
    }
    if d & 2 != 0 {
        signal.push("RGB 4:4:4");
    }
    if d & 8 != 0 {
        signal.push("12 bits");
    } else if d & 16 != 0 {
        signal.push("10 bits");
    } else if d & 32 != 0 {
        signal.push("8 bits");
    }
    if d & 4 != 0 {
        signal.push("3D (deux flux)");
    }
    let colorspace = match i.colorspace {
        Some(0x7236_3031) => "Rec. 601",
        Some(0x7237_3039) => "Rec. 709",
        Some(0x3230_3230) => "Rec. 2020",
        Some(0x5033_4436) => "DCI-P3 D65",
        Some(_) => "autre",
        None => "",
    };
    let hdr = match i.eotf {
        Some(0) => "SDR",
        Some(1) => "HDR (gamma)",
        Some(2) => "HDR10 / PQ",
        Some(3) => "HLG",
        Some(_) => "HDR",
        None => "",
    };
    crate::probe::SdiView {
        device,
        mode: mode.name.clone(),
        signal: signal.join(", "),
        field: if mode.interlaced() {
            if mode.field == strata_mv_output::decklink::FIELD_LOWER {
                "entrelacé, trame basse d’abord".into()
            } else {
                "entrelacé, trame haute d’abord".into()
            }
        } else {
            "progressif".into()
        },
        timecode: i.timecode.clone(),
        timecode_kind: match i.timecode_kind {
            1 => "RP 188 (ATC)",
            2 => "VITC",
            _ => "",
        }
        .to_string(),
        colorspace: colorspace.to_string(),
        hdr: hdr.to_string(),
        audio_channels: channels,
        anc: i
            .anc
            .iter()
            .map(|&(did, sdid, line, count)| crate::probe::Anc {
                did,
                sdid,
                line,
                name: crate::probe::anc_name(did, sdid).to_string(),
                count,
            })
            .collect(),
    }
}

/// An NDI picture (straight alpha, padded rows) as a premultiplied pixmap.
fn premultiplied(f: &strata_mv_output::ndi::Received) -> Option<Pixmap> {
    let (w, h) = (f.width as usize, f.height as usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = f.data.get(y * f.stride..y * f.stride + w * 4)?;
        for p in row.as_chunks::<4>().0 {
            if !f.alpha || p[3] == 255 {
                data.extend_from_slice(&[p[0], p[1], p[2], 255]);
            } else {
                let a = p[3] as u32;
                let m = |c: u8| ((c as u32 * a + 127) / 255) as u8;
                data.extend_from_slice(&[m(p[0]), m(p[1]), m(p[2]), p[3]]);
            }
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(f.width, f.height)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(kind: InputKind, url: &str, listen: bool) -> InputConfig {
        InputConfig {
            id: "in-1".into(),
            name: "Cam".into(),
            kind,
            url: url.into(),
            listen,
            ndi_source: String::new(),
            device: 0,
            hardware: String::new(),
            width: 1280,
            height: 720,
            fps: 25,
            buffer: 2,
            hw_decode: false,
            enabled: true,
            port: 0,
            latency: 120,
        }
    }

    fn live(fps: u32, buffer: u32) -> Live {
        let mut config = input(InputKind::Udp, "udp://127.0.0.1:1", false);
        (config.fps, config.buffer) = (fps, buffer);
        Live {
            config,
            picture: RwLock::new(None),
            paced: Mutex::new(Paced::default()),
            pool: Mutex::new(Vec::new()),
            received: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            last_at: Mutex::new(None),
            held_until: Mutex::new(None),
            message: Mutex::new(String::new()),
            stop: AtomicBool::new(false),
            child: Mutex::new(None),
            rate: Mutex::new(None),
            interlaced: AtomicBool::new(false),
            signals: Default::default(),
            no_audio: AtomicBool::new(false),
            no_info: AtomicBool::new(false),
            no_fps_mode: AtomicBool::new(false),
            relay: Mutex::new(None),
            facts: Arc::new(Mutex::new(crate::probe::Facts::default())),
            frames: tokio::sync::watch::channel((0, 0)).0,
            jpegs: Mutex::new(VecDeque::new()),
            recent: Mutex::new(VecDeque::new()),
            audio: tokio::sync::broadcast::channel(256).0,
            media: Default::default(),
        }
    }

    fn pic(w: u32) -> Pixmap {
        Pixmap::new(w, 1).unwrap()
    }

    /// Three pictures arriving at once come out one frame apart, after the buffer.
    #[test]
    fn a_burst_is_spread_out() {
        let l = live(10, 2); // 100 ms frames, 200 ms buffer
        let start = Instant::now();
        for w in 1..=3 {
            l.put(pic(w));
        }
        assert!(l.current().is_none());
        // Watched every few ms: each picture shows in turn, the first after the buffer.
        // (A loaded test machine may wake late; the order and the buffer still hold.)
        let mut seen: Vec<(u32, Duration)> = Vec::new();
        while start.elapsed() < Duration::from_millis(700) {
            if let Some(w) = l.current().map(|p| p.width()) {
                if seen.last().map(|s| s.0) != Some(w) {
                    seen.push((w, start.elapsed()));
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            seen.iter().map(|s| s.0).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "{seen:?}"
        );
        assert!(seen[0].1 >= Duration::from_millis(190), "{seen:?}");
        // Nothing new: the last one stays.
        assert_eq!(l.current().unwrap().width(), 3);
        assert_eq!(l.dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn sound_and_picture_of_one_time_are_shown_together() {
        let mut m = MediaClock::default();
        // The picture of time 5.0 comes; its sound of the same time comes 120 ms later
        // (the decoder's delay between them).
        m.saw_picture(5.0);
        std::thread::sleep(Duration::from_millis(120));
        m.saw_sound(5.0);
        let (p, s) = (m.at(5.0).unwrap(), m.at(5.0).unwrap());
        assert_eq!(p, s);
        // Both on the later one: each is there by then.
        let late = Instant::now() - p;
        assert!(late < Duration::from_millis(20), "{late:?}");
        assert_eq!(
            pts_time("[Parsed_showinfo_0 @ 0x1] n:   3 pts:  5520 pts_time:0.0613 duration:3600"),
            Some(0.0613)
        );
    }

    #[test]
    fn a_burst_of_sound_is_timed_by_its_rate() {
        let mut c = SampleClock::default();
        let first = c.time(960);
        // Three chunks of 20 ms come at once 60 ms later: each 20 ms after the one before.
        std::thread::sleep(Duration::from_millis(60));
        let t: Vec<Instant> = (0..3).map(|_| c.time(960)).collect();
        for w in t.windows(2) {
            let d = w[1] - w[0];
            assert!((d.as_secs_f64() - 0.02).abs() < 0.001, "{d:?}");
        }
        assert!(
            t[0] - first >= Duration::from_millis(19),
            "{:?}",
            t[0] - first
        );
    }

    #[test]
    fn no_buffer_shows_at_once_and_too_many_are_dropped() {
        let l = live(25, 0);
        l.put(pic(1));
        assert_eq!(l.current().unwrap().width(), 1);
        let l = live(25, 1);
        for w in 1..=8 {
            l.put(pic(w));
        }
        // At most buffer + 3 wait.
        assert_eq!(l.dropped.load(Ordering::Relaxed), 4);
    }

    /// Pictures at their rate, nothing drawing them: none is thrown away.
    #[test]
    fn an_input_nobody_shows_drops_nothing() {
        let l = live(50, 2);
        for w in 1..=12 {
            l.put(pic(w));
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(l.dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_decoded_format_is_read_from_the_log() {
        let line = "  Stream #0:0: Video: rawvideo (RGBA / 0x41424752), rgba(pc, gbr/unknown/unknown, progressive), 1280x720 [SAR 1:1 DAR 16:9], q=2-31, 737280 kb/s, 25 fps, 25 tbn";
        assert_eq!(decoded_format(line), Some((1280, 720, Some(25.0))));
        assert_eq!(
            decoded_format("  Stream #0:0: Video: h264, yuv420p, 1920x1080, 50 fps"),
            None
        );
    }

    /// A log line that is not UTF-8 (Latin-1 metadata) does not end the
    /// reading: the picture's line after it is still found.
    #[test]
    fn the_log_is_read_past_a_line_not_in_utf8() {
        let log: &[u8] = b"    service_name    : T\xe9l\xe9\r\nOutput #0, rawvideo, to 'pipe:1':\n  Stream #0:0: Video: rawvideo (RGBA / 0x41424752), rgba, 640x360, 25 fps\n";
        let lines: Vec<String> = log_lines(log).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "    service_name    : T\u{fffd}l\u{fffd}");
        assert_eq!(decoded_format(&lines[2]), Some((640, 360, Some(25.0))));
    }

    #[test]
    fn an_srt_listener_needs_only_its_port() {
        let mut c = input(InputKind::Srt, "", true);
        c.port = 10001;
        let a = c.ffmpeg_args(true, true).unwrap();
        // Received by the relay, which hands the bytes to the decoder.
        let relay = a.relay.unwrap();
        assert!(
            relay.contains(&"srt://0.0.0.0:10001?mode=listener&latency=120000".to_string()),
            "{relay:?}"
        );
        assert!(relay.windows(2).any(|w| w == ["-f", "data"]));
        assert!(a.decode.windows(2).any(|w| w == ["-i", "pipe:0"]));
        assert!(a
            .decode
            .windows(2)
            .any(|w| w == ["-vf", "showinfo=checksum=0"]));
        // No scaling: the source's own size.
        assert!(!a.decode.iter().any(|x| x.contains("scale=")));
        let mut c = input(InputKind::Srt, "srt://cam:9000", false);
        c.latency = 300;
        assert!(c
            .ffmpeg_args(true, false)
            .unwrap()
            .relay
            .unwrap()
            .contains(&"srt://cam:9000?latency=300000".to_string()));
    }

    #[test]
    fn stream_addresses() {
        let a = input(InputKind::Srt, "srt://0.0.0.0:9000", true)
            .ffmpeg_args(true, false)
            .unwrap();
        // An old listener given by its address keeps it.
        assert!(a
            .relay
            .unwrap()
            .contains(&"srt://0.0.0.0:9000?mode=listener&latency=120000".to_string()));
        // RTMP, HLS, RTSP: decoded straight from the source.
        let a = input(InputKind::Rtmp, "rtmp://0.0.0.0/live/cam", true)
            .ffmpeg_args(true, false)
            .unwrap();
        assert!(a.relay.is_none());
        assert!(a.decode.windows(2).any(|w| w == ["-listen", "1"]));
        assert!(input(InputKind::Hls, "srt://x", false)
            .ffmpeg_args(true, false)
            .is_err());
        assert!(input(InputKind::Udp, "", false)
            .ffmpeg_args(true, false)
            .is_err());
        let a = input(InputKind::Udp, "udp://@239.1.1.1:5000", false)
            .ffmpeg_args(true, false)
            .unwrap();
        assert!(a.relay.is_some());
        assert!(a.decode.windows(2).any(|w| w == ["-f", "rawvideo"]));
        // The audio measured on a second output, unless there is none.
        assert!(a.decode.windows(2).any(|w| w == ["-map", "0:a:0"]));
        assert!(!input(InputKind::Udp, "udp://@239.1.1.1:5000", false)
            .ffmpeg_args(false, false)
            .unwrap()
            .decode
            .iter()
            .any(|x| x == "0:a:0"));
        // RTP: its packets need its demuxer, not a relay.
        assert!(input(InputKind::Udp, "rtp://239.1.1.1:5000", false)
            .ffmpeg_args(true, false)
            .unwrap()
            .relay
            .is_none());
    }

    #[test]
    fn the_meters_are_read_from_the_log() {
        let mut m = MeterLog::default();
        assert!(m.line("Input #0, mpegts, from 'udp://…':").is_none());
        let p = "[Parsed_ametadata_2 @ 0x55d0c]";
        assert_eq!(
            m.line(&format!("{p} frame:0    pts:0       pts_time:0")),
            Some(None)
        );
        assert_eq!(
            m.line(&format!("{p} lavfi.astats.1.Peak_level=-6.020600")),
            Some(None)
        );
        assert_eq!(
            m.line(&format!("{p} lavfi.astats.1.RMS_level=-9.031400")),
            Some(None)
        );
        assert_eq!(
            m.line(&format!("{p} lavfi.astats.2.Peak_level=-inf")),
            Some(None)
        );
        let Some(Some(l)) = m.line(&format!("{p} frame:1    pts:1920    pts_time:0.04")) else {
            panic!("a frame read whole")
        };
        assert_eq!(l.len(), 2);
        assert!((l[0].peak + 6.0206).abs() < 1e-3 && (l[0].rms + 9.0314).abs() < 1e-3);
        assert_eq!(l[1].peak, strata_mv_core::signal::SILENCE);
    }

    #[test]
    fn formats_are_named_like_broadcast_ones() {
        assert_eq!(format_name(1080, 25.0, true).as_deref(), Some("1080i50"));
        assert_eq!(
            format_name(720, 60000.0 / 1001.0, false).as_deref(),
            Some("720p59.94")
        );
        assert_eq!(format_name(1080, 0.0, false), None);
    }

    #[test]
    fn ndi_pictures_are_premultiplied() {
        // One row of 2 pixels, padded to 12 bytes.
        let data = [200, 100, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        let f = strata_mv_output::ndi::Received {
            width: 2,
            height: 1,
            stride: 12,
            alpha: true,
            data: &data,
            rate: (25, 1),
            interlaced: false,
            timecode: 0,
            aspect: 0.0,
            format: 1,
            metadata: None,
        };
        let pm = premultiplied(&f).unwrap();
        assert_eq!(pm.data(), &[100, 50, 0, 128, 10, 20, 30, 255]);
    }

    /// A stream sent by ffmpeg comes back as pictures (needs ffmpeg).
    #[test]
    fn the_audio_of_a_stream_is_measured() {
        if strata_mv_output::ffmpeg::command()
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let port = 46_000 + (std::process::id() % 1000) as u16;
        let hub = Inputs::default();
        hub.apply(&[InputConfig {
            width: 320,
            height: 180,
            ..input(InputKind::Udp, &format!("udp://127.0.0.1:{port}"), false)
        }]);
        // Two channels: -6 dBFS and -20 dBFS.
        let mut send = strata_mv_output::ffmpeg::command()
            .args([
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=25",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.5*sin(2*PI*440*t)|0.1*sin(2*PI*880*t):s=48000",
                "-t",
                "8",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-g",
                "25",
                "-c:a",
                "aac",
                "-f",
                "mpegts",
            ])
            .arg(format!("udp://127.0.0.1:{port}?pkt_size=1316"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut levels = Vec::new();
        for _ in 0..80 {
            std::thread::sleep(Duration::from_millis(100));
            levels = hub.signals().levels("in-1");
            if levels.len() == 2 && levels[0].peak > -10.0 {
                break;
            }
        }
        let _ = send.kill();
        let _ = send.wait();
        assert_eq!(levels.len(), 2, "{:?}", hub.status());
        assert!((levels[0].peak + 6.0).abs() < 2.0, "{levels:?}");
        assert!((levels[1].peak + 20.0).abs() < 2.0, "{levels:?}");
        let view = strata_mv_render::LiveInputs::source(&hub, "in-1").unwrap();
        assert_eq!(view.levels.len(), 2);
    }

    #[test]
    fn a_udp_stream_is_received() {
        if strata_mv_output::ffmpeg::command()
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let port = 47_000 + (std::process::id() % 1000) as u16;
        let hub = Inputs::default();
        hub.apply(&[InputConfig {
            width: 320,
            height: 180,
            ..input(InputKind::Udp, &format!("udp://127.0.0.1:{port}"), false)
        }]);
        let mut send = strata_mv_output::ffmpeg::command()
            .args([
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=25",
                "-t",
                "8",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-g",
                "25",
                "-f",
                "mpegts",
            ])
            .arg(format!("udp://127.0.0.1:{port}?pkt_size=1316"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut got = false;
        for _ in 0..80 {
            std::thread::sleep(Duration::from_millis(100));
            if hub.received("in-1").unwrap_or(0) > 3 {
                got = true;
                break;
            }
        }
        let _ = send.kill();
        let _ = send.wait();
        assert!(got, "no picture: {:?}", hub.status());
        // Shown after the smoothing buffer.
        let mut pic = None;
        for _ in 0..50 {
            pic = strata_mv_render::LiveInputs::picture(&hub, "in-1");
            if pic.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let pic = pic.unwrap();
        assert_eq!((pic.width(), pic.height()), (320, 180));
        assert!(hub.status()[0].message.starts_with("Receiving"));
    }

    /// A stream output of Strata comes back through an input (needs ffmpeg).
    #[test]
    fn a_stream_output_comes_back_as_an_input() {
        use strata_mv_output::OutputSink;
        if strata_mv_output::ffmpeg::command()
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let port = 48_000 + (std::process::id() % 1000) as u16;
        let hub = Inputs::default();
        hub.apply(&[InputConfig {
            width: 320,
            height: 180,
            ..input(InputKind::Udp, &format!("udp://127.0.0.1:{port}"), false)
        }]);
        let mut out = strata_mv_output::stream::StreamSender::open(
            strata_mv_output::stream::StreamConfig {
                id: "s".into(),
                name: "S".into(),
                protocol: strata_mv_output::stream::Protocol::Udp,
                url: format!("udp://127.0.0.1:{port}"),
                listen: false,
                bitrate: 2000,
                codec: strata_mv_output::stream::Codec::H264,
                background: "#0000ff".into(),
                audio: false,
                hw_encode: false,
                enabled: true,
            },
            320,
            180,
            (25, 1),
            None,
        )
        .unwrap();
        // Transparent multiviews over the blue background.
        let frame = Arc::new(strata_mv_render::Frame::blank(320, 180));
        let mut got = false;
        for _ in 0..200 {
            let _ = out.push(&frame, std::time::SystemTime::now());
            std::thread::sleep(Duration::from_millis(40));
            if hub.received("in-1").unwrap_or(0) > 5 {
                got = true;
                break;
            }
        }
        assert!(got, "no picture: {:?}", hub.status());
        let pic = strata_mv_render::LiveInputs::picture(&hub, "in-1").unwrap();
        let p = pic.pixel(160, 90).unwrap();
        assert!(p.blue() > 200 && p.red() < 40 && p.green() < 40, "{p:?}");
    }

    /// SRT both ways between a stream output and an input of Strata: the
    /// input listens on its port and the output calls, then the output
    /// listens and the input calls (needs an ffmpeg with SRT).
    #[test]
    fn srt_streams_come_back_as_inputs() {
        use strata_mv_output::OutputSink;
        let has_srt = strata_mv_output::ffmpeg::command()
            .arg("-protocols")
            .output()
            .is_ok_and(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .any(|l| l.trim() == "srt")
            });
        if !has_srt {
            return;
        }
        let base = 49_000 + (std::process::id() % 500) as u16 * 2;
        for (port, input_listens) in [(base, true), (base + 1, false)] {
            let hub = Inputs::default();
            let mut i = input(
                InputKind::Srt,
                &format!("srt://127.0.0.1:{port}"),
                input_listens,
            );
            if input_listens {
                (i.url, i.port) = (String::new(), port);
            }
            hub.apply(&[i]);
            let host = if input_listens {
                "127.0.0.1"
            } else {
                "0.0.0.0"
            };
            let mut out = strata_mv_output::stream::StreamSender::open(
                strata_mv_output::stream::StreamConfig {
                    id: "s".into(),
                    name: "S".into(),
                    protocol: strata_mv_output::stream::Protocol::Srt,
                    url: format!("srt://{host}:{port}"),
                    listen: !input_listens,
                    bitrate: 2000,
                    codec: strata_mv_output::stream::Codec::H264,
                    background: "#0000ff".into(),
                    audio: true,
                    hw_encode: false,
                    enabled: true,
                },
                320,
                180,
                (25, 1),
                None,
            )
            .unwrap();
            let frame = Arc::new(strata_mv_render::Frame::blank(320, 180));
            let mut got = false;
            for _ in 0..300 {
                let _ = out.push(&frame, std::time::SystemTime::now());
                std::thread::sleep(Duration::from_millis(40));
                if hub.received("in-1").unwrap_or(0) > 5 {
                    got = true;
                    break;
                }
            }
            assert!(
                got,
                "input listens: {input_listens}, no picture: {:?}",
                hub.status()
            );
            // Shown after the smoothing buffer.
            let mut pic = None;
            for _ in 0..50 {
                let _ = out.push(&frame, std::time::SystemTime::now());
                pic = strata_mv_render::LiveInputs::picture(&hub, "in-1");
                if pic.is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            let pic = pic.expect("a picture after the buffer");
            assert_eq!((pic.width(), pic.height()), (320, 180));
            let p = pic.pixel(160, 90).unwrap();
            assert!(p.blue() > 200 && p.red() < 40 && p.green() < 40, "{p:?}");
        }
    }
}
