//! Live video inputs, shown by the Video widget.
//!
//! - Streams (SRT, UDP, RTMP, HLS, RTSP, any address ffmpeg reads) are
//!   decoded by ffmpeg into raw pictures; SRT and RTMP can also wait for the
//!   sender to connect (listener).
//! - NDI sources are received through the NDI runtime (the one the NDI
//!   outputs use).
//! - SDI inputs need DeckLink support, which this server is built without.
//!
//! Pictures off a network come unevenly (bursts, gaps). Each one is given a
//! time to be shown, `buffer` frames after it came and at least one frame
//! after the previous one: the renderers, drawing at their own pace, get an
//! even motion back, at the cost of that much delay. The previews
//! (`/api/inputs/<id>/live.mjpeg`) send the newest picture as JPEG.
//! Inputs keep running across engine restarts; saving the settings only
//! restarts the ones that changed.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, Read};
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
    #[serde(default = "cg_output::on")]
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

impl InputConfig {
    /// The ffmpeg arguments that decode this stream to raw RGBA on stdout.
    pub fn ffmpeg_args(&self) -> Result<Vec<String>, String> {
        // An SRT listener needs only its port.
        let own;
        let url = if self.kind == InputKind::Srt && self.listen && self.port > 0 {
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
            InputKind::Ndi | InputKind::Sdi | InputKind::St2110 => {
                return Err("not a stream".into())
            }
        }
        if self.hw_decode {
            a.extend(["-hwaccel", "auto"].map(String::from));
        }
        // The source's own size and rate.
        a.extend(["-i".into(), url, "-an".into()]);
        a.extend(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"].map(String::from));
        Ok(a)
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
    queue: VecDeque<(Instant, Arc<Pixmap>)>,
    shown: Option<Arc<Pixmap>>,
    last_due: Option<Instant>,
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
    message: Mutex<String>,
    stop: AtomicBool,
    /// The ffmpeg decoding it (killed to stop a blocked read).
    child: Mutex<Option<Child>>,
    /// Pictures a second of the source, once known (fields when interlaced).
    rate: Mutex<Option<f64>>,
    interlaced: AtomicBool,
}

impl Live {
    fn say(&self, m: impl Into<String>) {
        *self.message.lock().unwrap_or_else(|p| p.into_inner()) = m.into();
    }
    fn period(&self) -> Duration {
        let fps = (*self.rate.lock().unwrap_or_else(|p| p.into_inner()))
            .unwrap_or(self.config.fps as f64)
            .clamp(1.0, 120.0);
        Duration::from_secs_f64(1.0 / fps)
    }

    fn put(&self, pm: Pixmap) {
        let now = Instant::now();
        let pm = Arc::new(pm);
        *self.picture.write().unwrap_or_else(|p| p.into_inner()) = Some(pm.clone());
        self.received.fetch_add(1, Ordering::Relaxed);
        *self.last_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(now);
        // SDI comes off a clocked card, evenly: shown at once, no smoothing.
        let sdi = self.config.kind == InputKind::Sdi;
        let (period, frames) = (
            self.period(),
            if sdi { 0 } else { self.config.buffer.min(10) },
        );
        let delay = period * frames;
        let mut p = self.paced.lock().unwrap_or_else(|p| p.into_inner());
        // Time goes on even when nothing draws this input.
        self.advance(&mut p, now);
        let mut due = now + delay;
        if sdi {
            // The second field of a frame, handed over with the first: a field later.
            if let Some(d) = p.last_due.filter(|d| *d + period / 2 > now) {
                due = d + period;
            }
        } else if frames > 0 {
            // At least a frame after the previous one: bursts spread out.
            if let Some(next) = p.last_due.map(|d| d + period) {
                due = due.max(next);
            }
            // Further and further behind (a sender faster than its rate):
            // start again from now.
            if due > now + delay + period * 3 {
                due = now + delay;
                let n = p.queue.len() as u64;
                p.queue.clear();
                self.dropped.fetch_add(n, Ordering::Relaxed);
            }
        }
        p.last_due = Some(due);
        p.queue.push_back((due, pm));
        while p.queue.len() > frames as usize + 3 {
            if let Some((_, old)) = p.queue.pop_front() {
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
        while p.queue.front().is_some_and(|(due, _)| *due <= now) {
            let Some((_, pm)) = p.queue.pop_front() else {
                break;
            };
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
        if let Some(c) = self
            .child
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_mut()
        {
            let _ = c.kill();
        }
    }
}

/// The live inputs of the server.
#[derive(Default)]
pub struct Inputs {
    live: RwLock<BTreeMap<String, Arc<Live>>>,
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
                message: Mutex::new("Starting…".into()),
                stop: AtomicBool::new(false),
                child: Mutex::new(None),
                rate: Mutex::new(None),
                interlaced: AtomicBool::new(false),
            });
            let l = live.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("cg-input-{}", c.id))
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
                let fresh = l
                    .last_at
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(2));
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

    /// Pictures received so far (changes when a new one comes).
    pub fn received(&self, id: &str) -> Option<u64> {
        let map = self.live.read().unwrap_or_else(|p| p.into_inner());
        map.get(id).map(|l| l.received.load(Ordering::Relaxed))
    }

    /// The newest picture and its number (previews: not delayed by the smoothing).
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

    pub fn exists(&self, id: &str) -> bool {
        self.live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(id)
    }
}

impl cg_render::LiveInputs for Inputs {
    fn picture(&self, id: &str) -> Option<Arc<Pixmap>> {
        let live = self
            .live
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)?
            .clone();
        live.current()
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

/// `GET /api/inputs/ndi-sources`: the NDI sources seen on the network.
pub async fn ndi_sources(CurrentUser(me): CurrentUser) -> ApiResult<Json<Vec<String>>> {
    require(me.is_admin())?;
    tokio::task::spawn_blocking(|| cg_output::ndi::sources(6000))
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
    let jpeg = tokio::task::spawn_blocking(move || cg_render::jpeg(&pic, width, 75))
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
    let mut rx = cg_output::ndi::NdiReceiver::open(source)?;
    let until = Instant::now() + Duration::from_secs(4);
    while Instant::now() < until {
        let mut got = None;
        rx.capture(500, |f| {
            let fps = if f.rate.1 > 0 {
                f.rate.0 as f64 / f.rate.1 as f64
            } else {
                0.0
            };
            got = premultiplied(&f).map(|pm| (pm, fps, f.interlaced));
        })?;
        if let Some((pm, fps, interlaced)) = got {
            let (w, h) = (pm.width(), pm.height());
            return Ok(cg_render::jpeg(&pm, width, 75).map(|jpeg| NdiShot {
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
    let mut closing = s.closing.subscribe();
    let stream = futures_util::stream::unfold((hub, id, None::<u64>), move |(hub, id, last)| {
        let stop = *closing.borrow_and_update();
        async move {
            if stop {
                return None;
            }
            // The next picture (one at least every 2 s, so the page knows it is alive).
            let mut waited = 0;
            loop {
                let n = hub.received(&id)?;
                if Some(n) != last || waited >= 2000 {
                    // The newest picture: a preview is not delayed by the smoothing.
                    let pic = hub.latest(&id).map(|(_, p)| p);
                    let jpeg = match pic {
                        Some(p) => {
                            tokio::task::spawn_blocking(move || cg_render::jpeg(&p, width, 78))
                                .await
                                .ok()
                                .flatten()
                        }
                        None => None,
                    };
                    let Some(jpeg) = jpeg else {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        waited += 200;
                        if waited >= 2000 {
                            waited = 0;
                        }
                        continue;
                    };
                    let mut part = format!(
                        "--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                        jpeg.len()
                    )
                    .into_bytes();
                    part.extend_from_slice(&jpeg);
                    part.extend_from_slice(b"\r\n");
                    return Some((
                        Ok::<_, std::io::Error>(axum::body::Bytes::from(part)),
                        (hub, id, Some(n)),
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                waited += 20;
            }
        }
    });
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

/// A stream through ffmpeg, started again 2 s after it stops.
fn run_stream(l: &Live) {
    let c = &l.config;
    let args = match c.ffmpeg_args() {
        Ok(a) => a,
        Err(e) => return l.say(e),
    };
    while !l.stopped() {
        let spawned = cg_output::ffmpeg::command()
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
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
        let mut out = child.stdout.take();
        let log = Arc::new(Mutex::new(String::new()));
        // The picture ffmpeg sends: told in its log before the first one.
        let (format_tx, format_rx) = std::sync::mpsc::channel::<(u32, u32, Option<f64>)>();
        if let Some(err) = child.stderr.take() {
            let log = log.clone();
            std::thread::spawn(move || {
                let mut sent = false;
                let mut in_output = false;
                for line in log_lines(err) {
                    if line.starts_with("Output #") {
                        in_output = true;
                    }
                    if in_output && !sent {
                        if let Some(f) = decoded_format(&line) {
                            let _ = format_tx.send(f);
                            sent = true;
                        }
                    }
                    let mut s = log.lock().unwrap_or_else(|p| p.into_inner());
                    s.push_str(&line);
                    s.push('\n');
                    if s.len() > 8192 {
                        let cut = s.len() - 4096;
                        *s = s[s.ceil_char_boundary(cut)..].to_string();
                    }
                }
            });
        }
        *l.child.lock().unwrap_or_else(|p| p.into_inner()) = Some(child);
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
            loop {
                let mut buf = l.buffer(size);
                if out.read_exact(&mut buf).is_err() || l.stopped() {
                    break;
                }
                // Opaque or transparent black: already premultiplied.
                if let Some(pm) = IntSize::from_wh(w, h).and_then(|s| Pixmap::from_vec(buf, s)) {
                    l.put(pm);
                }
                if first {
                    first = false;
                    l.say(format!("Receiving {w}×{h}"));
                }
            }
        }
        if let Some(mut c) = l.child.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if l.stopped() {
            return;
        }
        let why = cg_output::ffmpeg::tail(&log.lock().unwrap_or_else(|p| p.into_inner()));
        l.say(format!("Stopped: {why} — trying again"));
        if !l.pause(Duration::from_secs(2)) {
            return;
        }
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
        let mut rx = match cg_output::ndi::NdiReceiver::open(&source) {
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
        while !l.stopped() {
            let (mut pic, mut format) = (None, ((0, 0), false));
            match rx.capture(500, |f| {
                pic = premultiplied(&f);
                format = (f.rate, f.interlaced);
            }) {
                Ok(_) => {}
                Err(e) => {
                    l.say(format!("{e} — trying again"));
                    break;
                }
            }
            if let Some(pm) = pic {
                let ((n, d), fields) = format;
                let fps = if d > 0 { n as f64 / d as f64 } else { 0.0 };
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
                    for f in cg_output::sdi::DeckLinkInput::fields(
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
        let mut card = match cg_output::sdi::DeckLinkInput::open(c.device, w, h, rate, interlaced) {
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
        let opened = Instant::now();
        let (mut seq, mut shown) = (0u64, None);
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
                        let lower = mode.field == cg_output::decklink::FIELD_LOWER;
                        for f in cg_output::sdi::DeckLinkInput::fields(&rgba, w, h, lower) {
                            if let Some(pm) = Pixmap::from_vec(f, size) {
                                l.put(pm);
                            }
                        }
                    } else if let Some(pm) = Pixmap::from_vec(rgba, size) {
                        l.put(pm);
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

/// An NDI picture (straight alpha, padded rows) as a premultiplied pixmap.
fn premultiplied(f: &cg_output::ndi::Received) -> Option<Pixmap> {
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
            message: Mutex::new(String::new()),
            stop: AtomicBool::new(false),
            child: Mutex::new(None),
            rate: Mutex::new(None),
            interlaced: AtomicBool::new(false),
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
        let a = c.ffmpeg_args().unwrap();
        assert!(
            a.contains(&"srt://0.0.0.0:10001?mode=listener&latency=120000".to_string()),
            "{a:?}"
        );
        // No scaling: the source's own size.
        assert!(!a.iter().any(|x| x.contains("scale=")));
        let mut c = input(InputKind::Srt, "srt://cam:9000", false);
        c.latency = 300;
        assert!(c
            .ffmpeg_args()
            .unwrap()
            .contains(&"srt://cam:9000?latency=300000".to_string()));
    }

    #[test]
    fn stream_addresses() {
        let a = input(InputKind::Srt, "srt://0.0.0.0:9000", true)
            .ffmpeg_args()
            .unwrap();
        // An old listener given by its address keeps it.
        assert!(a.contains(&"srt://0.0.0.0:9000?mode=listener&latency=120000".to_string()));
        let a = input(InputKind::Rtmp, "rtmp://0.0.0.0/live/cam", true)
            .ffmpeg_args()
            .unwrap();
        assert!(a.windows(2).any(|w| w == ["-listen", "1"]));
        assert!(input(InputKind::Hls, "srt://x", false)
            .ffmpeg_args()
            .is_err());
        assert!(input(InputKind::Udp, "", false).ffmpeg_args().is_err());
        let a = input(InputKind::Udp, "udp://@239.1.1.1:5000", false)
            .ffmpeg_args()
            .unwrap();
        assert!(a.windows(2).any(|w| w == ["-f", "rawvideo"]));
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
        let f = cg_output::ndi::Received {
            width: 2,
            height: 1,
            stride: 12,
            alpha: true,
            data: &data,
            rate: (25, 1),
            interlaced: false,
        };
        let pm = premultiplied(&f).unwrap();
        assert_eq!(pm.data(), &[100, 50, 0, 128, 10, 20, 30, 255]);
    }

    /// A stream sent by ffmpeg comes back as pictures (needs ffmpeg).
    #[test]
    fn a_udp_stream_is_received() {
        if cg_output::ffmpeg::command()
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
        let mut send = cg_output::ffmpeg::command()
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
            pic = cg_render::LiveInputs::picture(&hub, "in-1");
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
        use cg_output::OutputSink;
        if cg_output::ffmpeg::command()
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
        let mut out = cg_output::stream::StreamSender::open(
            cg_output::stream::StreamConfig {
                id: "s".into(),
                name: "S".into(),
                protocol: cg_output::stream::Protocol::Udp,
                url: format!("udp://127.0.0.1:{port}"),
                listen: false,
                bitrate: 2000,
                codec: cg_output::stream::Codec::H264,
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
        // Transparent graphics over the blue background.
        let frame = Arc::new(cg_render::Frame::blank(320, 180));
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
        let pic = cg_render::LiveInputs::picture(&hub, "in-1").unwrap();
        let p = pic.pixel(160, 90).unwrap();
        assert!(p.blue() > 200 && p.red() < 40 && p.green() < 40, "{p:?}");
    }

    /// SRT both ways between a stream output and an input of Strata: the
    /// input listens on its port and the output calls, then the output
    /// listens and the input calls (needs an ffmpeg with SRT).
    #[test]
    fn srt_streams_come_back_as_inputs() {
        use cg_output::OutputSink;
        let has_srt = cg_output::ffmpeg::command()
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
            let mut out = cg_output::stream::StreamSender::open(
                cg_output::stream::StreamConfig {
                    id: "s".into(),
                    name: "S".into(),
                    protocol: cg_output::stream::Protocol::Srt,
                    url: format!("srt://{host}:{port}"),
                    listen: !input_listens,
                    bitrate: 2000,
                    codec: cg_output::stream::Codec::H264,
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
            let frame = Arc::new(cg_render::Frame::blank(320, 180));
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
                pic = cg_render::LiveInputs::picture(&hub, "in-1");
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
