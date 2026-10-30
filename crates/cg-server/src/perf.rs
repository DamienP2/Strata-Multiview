//! How hard the server works: the machine (processors, memory), each
//! channel's clock, pictures and outputs, the live inputs, the ffmpeg they
//! run; what more the machine can take, and what goes wrong.
//!
//! The engine and the inputs count (frames, time spent, drops) in atomics;
//! once a second the sampler turns the counts into rates and keeps the last
//! ten seconds. `GET /api/system/metrics` puts that together with the
//! settings.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::hardware::ClockReference;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Seconds of history behind "in the last 10 s".
const WINDOW: usize = 10;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// One picture of a channel (main or rendition) rendered for its outputs.
pub struct PictureStats {
    pub name: String,
    pub width: u32,
    pub height: u32,
    rendered: AtomicU64,
    render_us: AtomicU64,
    render_max_us: AtomicU64,
    /// Ticks that sent the previous frame again (the next one was not ready).
    repeated: AtomicU64,
    outputs: Mutex<Vec<Arc<SinkStats>>>,
}

impl PictureStats {
    pub fn rendered(&self, took: Duration) {
        let us = took.as_micros() as u64;
        self.rendered.fetch_add(1, Ordering::Relaxed);
        self.render_us.fetch_add(us, Ordering::Relaxed);
        self.render_max_us.fetch_max(us, Ordering::Relaxed);
    }
    pub fn repeated(&self) {
        self.repeated.fetch_add(1, Ordering::Relaxed);
    }
    pub fn output(&self, id: &str, kind: &'static str, name: &str) -> Arc<SinkStats> {
        let s = Arc::new(SinkStats {
            id: id.into(),
            kind,
            name: name.into(),
            sent: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            busy_us: AtomicU64::new(0),
            pid: AtomicU32::new(0),
            work_us: AtomicU64::new(0),
        });
        lock(&self.outputs).push(s.clone());
        s
    }
}

/// One output (NDI, ST 2110, SDI, stream).
pub struct SinkStats {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    sent: AtomicU64,
    dropped: AtomicU64,
    busy_us: AtomicU64,
    /// Its ffmpeg (0: none).
    pid: AtomicU32,
    /// µs its own threads worked (outputs that send from threads of their own).
    work_us: AtomicU64,
}

impl SinkStats {
    /// A frame handed to the output (`ok`: taken, else dropped by it).
    pub fn sent(&self, took: Duration, ok: bool) {
        if ok {
            self.sent.fetch_add(1, Ordering::Relaxed);
        } else {
            self.dropped();
        }
        self.busy_us
            .fetch_add(took.as_micros() as u64, Ordering::Relaxed);
    }
    pub fn dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_work(&self, total: Duration) {
        self.work_us
            .store(total.as_micros() as u64, Ordering::Relaxed);
    }
    pub fn set_process(&self, pid: Option<u32>) {
        self.pid.store(pid.unwrap_or(0), Ordering::Relaxed);
    }
}

/// A channel clock.
pub struct ClockStats {
    late_max_us: AtomicU64,
    skipped: AtomicU64,
    /// What the clock follows, and the channel's name.
    reference: Mutex<(ClockReference, String)>,
    /// A DeckLink reference: locked (1), not (0), unknown (-1).
    locked: std::sync::atomic::AtomicI8,
}

impl Default for ClockStats {
    fn default() -> Self {
        Self {
            late_max_us: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            reference: Mutex::new((ClockReference::Free, String::new())),
            locked: std::sync::atomic::AtomicI8::new(-1),
        }
    }
}

impl ClockStats {
    pub fn set_reference(&self, r: ClockReference, channel: &str) {
        *lock(&self.reference) = (r, channel.to_string());
    }
    pub fn reference(&self) -> ClockReference {
        lock(&self.reference).0
    }
    pub fn set_locked(&self, locked: Option<bool>) {
        self.locked
            .store(locked.map_or(-1, i8::from), Ordering::Relaxed);
    }
    pub fn locked(&self) -> Option<bool> {
        match self.locked.load(Ordering::Relaxed) {
            -1 => None,
            v => Some(v == 1),
        }
    }
    pub fn ticked(&self, late: Duration) {
        self.late_max_us
            .fetch_max(late.as_micros() as u64, Ordering::Relaxed);
    }
    pub fn skipped(&self, n: u64) {
        self.skipped.fetch_add(n, Ordering::Relaxed);
    }
}

/// What a live input reports to the sampler.
pub struct InputCounts {
    pub id: String,
    pub received: u64,
    pub dropped: u64,
    pub pid: Option<u32>,
}

/// A counter turned into a rate, with the last ten seconds.
#[derive(Default)]
struct Track {
    last: u64,
    hist: VecDeque<f64>,
}

impl Track {
    /// The new total; returns the change since the last second.
    fn total(&mut self, v: u64) -> f64 {
        let d = v.saturating_sub(self.last) as f64;
        self.last = v;
        self.push(d)
    }
    fn push(&mut self, v: f64) -> f64 {
        if self.hist.len() == WINDOW {
            self.hist.pop_front();
        }
        self.hist.push_back(v);
        v
    }
    /// Mean of the last `k` seconds (steadier than one second alone).
    fn avg(&self, k: usize) -> f64 {
        let n = self.hist.len().min(k);
        if n == 0 {
            return 0.0;
        }
        self.hist.iter().rev().take(n).sum::<f64>() / n as f64
    }
    fn sum(&self) -> f64 {
        self.hist.iter().sum()
    }
    fn max(&self) -> f64 {
        self.hist.iter().copied().fold(0.0, f64::max)
    }
}

/// Rates of one second, by key.
#[derive(Default, Clone)]
pub struct Rates {
    pub fps: f64,
    pub ms: f64,
    pub max_ms: f64,
    /// Over the last 10 s.
    pub count10: f64,
    /// Cores.
    pub cpu: f64,
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub cpu: f32,
    pub cores: usize,
    pub cpu_name: String,
    pub mem_used: u64,
    pub mem_total: u64,
    pub hostname: String,
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessLoad {
    /// % of the machine.
    pub cpu: f32,
    pub mem: u64,
    pub ffmpeg_cpu: f32,
    pub ffmpeg: usize,
}

/// The last second, as the API gives it.
#[derive(Default, Clone)]
pub struct Sampled {
    pub machine: Machine,
    pub process: ProcessLoad,
    /// Machine CPU of the last 10 s (sustained load).
    pub cpu10: f32,
    /// By "channel/picture".
    pub pictures: HashMap<String, Rates>,
    /// By "channel/picture/output".
    pub outputs: HashMap<String, Rates>,
    /// By channel: clock lateness (max_ms) and skipped ticks (count10).
    pub clocks: HashMap<String, Rates>,
    /// By input id.
    pub inputs: HashMap<String, Rates>,
}

#[derive(Default)]
struct Tracks {
    by_key: HashMap<String, Track>,
}

impl Tracks {
    fn get(&mut self, key: String) -> &mut Track {
        self.by_key.entry(key).or_default()
    }
}

pub struct Perf {
    pictures: Mutex<BTreeMap<(String, String), Arc<PictureStats>>>,
    clocks: Mutex<BTreeMap<String, Arc<ClockStats>>>,
    sys: Mutex<System>,
    tracks: Mutex<Tracks>,
    sampled: RwLock<Sampled>,
    /// NTP / PTP status.
    pub sync: Arc<crate::sync::Watch>,
}

impl Default for Perf {
    fn default() -> Self {
        Self {
            sync: Default::default(),
            pictures: Default::default(),
            clocks: Default::default(),
            sys: Mutex::new(System::new()),
            tracks: Default::default(),
            sampled: Default::default(),
        }
    }
}

impl Perf {
    /// A channel starts (again): its old counts go.
    pub fn reset_channel(&self, channel: &str) {
        lock(&self.pictures).retain(|(c, _), _| c != channel);
        lock(&self.clocks).remove(channel);
        let prefix = format!("{channel}/");
        lock(&self.tracks)
            .by_key
            .retain(|k, _| !k.starts_with(&prefix) && k != channel);
    }

    pub fn clock_of(&self, channel: &str) -> Option<Arc<ClockStats>> {
        lock(&self.clocks).get(channel).cloned()
    }

    pub fn clock(&self, channel: &str) -> Arc<ClockStats> {
        lock(&self.clocks)
            .entry(channel.into())
            .or_default()
            .clone()
    }

    pub fn picture(
        &self,
        channel: &str,
        id: &str,
        name: &str,
        width: u32,
        height: u32,
    ) -> Arc<PictureStats> {
        let s = Arc::new(PictureStats {
            name: name.into(),
            width,
            height,
            rendered: AtomicU64::new(0),
            render_us: AtomicU64::new(0),
            render_max_us: AtomicU64::new(0),
            repeated: AtomicU64::new(0),
            outputs: Mutex::new(Vec::new()),
        });
        lock(&self.pictures).insert((channel.into(), id.into()), s.clone());
        s
    }

    /// The pictures of a channel and their outputs (id, stats).
    pub fn pictures_of(
        &self,
        channel: &str,
    ) -> Vec<(String, Arc<PictureStats>, Vec<Arc<SinkStats>>)> {
        lock(&self.pictures)
            .iter()
            .filter(|((c, _), _)| c == channel)
            .map(|((_, id), p)| (id.clone(), p.clone(), lock(&p.outputs).clone()))
            .collect()
    }

    pub fn sampled(&self) -> Sampled {
        self.sampled
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// One second's worth: counts into rates, processors and memory.
    pub fn sample(&self, dt: f64, inputs: Vec<InputCounts>) {
        let dt = dt.max(0.2);
        let mut out = Sampled::default();
        let mut pids: Vec<(String, u32)> = Vec::new();
        {
            let mut tr = lock(&self.tracks);
            for ((ch, id), p) in lock(&self.pictures).iter() {
                let key = format!("{ch}/{id}");
                let n = tr
                    .get(format!("{key}#n"))
                    .total(p.rendered.load(Ordering::Relaxed));
                let us = tr
                    .get(format!("{key}#us"))
                    .total(p.render_us.load(Ordering::Relaxed));
                let max = p.render_max_us.swap(0, Ordering::Relaxed) as f64;
                let max10 = {
                    let t = tr.get(format!("{key}#max"));
                    t.push(max);
                    t.max()
                };
                tr.get(format!("{key}#rep"))
                    .total(p.repeated.load(Ordering::Relaxed));
                let rep10 = tr.get(format!("{key}#rep")).sum();
                out.pictures.insert(
                    key.clone(),
                    Rates {
                        fps: tr.get(format!("{key}#n")).avg(3) / dt,
                        ms: if n > 0.0 { us / n / 1000.0 } else { 0.0 },
                        max_ms: max10 / 1000.0,
                        count10: rep10,
                        cpu: us / 1e6 / dt,
                    },
                );
                for o in lock(&p.outputs).iter() {
                    let ok = format!("{key}/{}", o.id);
                    let n = tr
                        .get(format!("{ok}#n"))
                        .total(o.sent.load(Ordering::Relaxed));
                    let busy = tr
                        .get(format!("{ok}#us"))
                        .total(o.busy_us.load(Ordering::Relaxed))
                        + tr.get(format!("{ok}#work"))
                            .total(o.work_us.load(Ordering::Relaxed));
                    tr.get(format!("{ok}#drop"))
                        .total(o.dropped.load(Ordering::Relaxed));
                    let drop10 = tr.get(format!("{ok}#drop")).sum();
                    let pid = o.pid.load(Ordering::Relaxed);
                    if pid != 0 {
                        pids.push((ok.clone(), pid));
                    }
                    let fps = tr.get(format!("{ok}#n")).avg(3) / dt;
                    out.outputs.insert(
                        ok,
                        Rates {
                            fps,
                            ms: if n > 0.0 { busy / n / 1000.0 } else { 0.0 },
                            count10: drop10,
                            cpu: busy / 1e6 / dt,
                            ..Default::default()
                        },
                    );
                }
            }
            for (ch, c) in lock(&self.clocks).iter() {
                let late = c.late_max_us.swap(0, Ordering::Relaxed) as f64;
                let late10 = {
                    let t = tr.get(format!("{ch}#late"));
                    t.push(late);
                    t.max()
                };
                tr.get(format!("{ch}#skip"))
                    .total(c.skipped.load(Ordering::Relaxed));
                let skip10 = tr.get(format!("{ch}#skip")).sum();
                out.clocks.insert(
                    ch.clone(),
                    Rates {
                        max_ms: late10 / 1000.0,
                        count10: skip10,
                        ..Default::default()
                    },
                );
            }
            for i in &inputs {
                let key = format!("input:{}", i.id);
                tr.get(format!("{key}#n")).total(i.received);
                tr.get(format!("{key}#drop")).total(i.dropped);
                let drop10 = tr.get(format!("{key}#drop")).sum();
                if let Some(pid) = i.pid {
                    pids.push((key.clone(), pid));
                }
                out.inputs.insert(
                    i.id.clone(),
                    Rates {
                        fps: tr.get(format!("{key}#n")).avg(3) / dt,
                        count10: drop10,
                        ..Default::default()
                    },
                );
            }
            // Forget what is gone (removed inputs, outputs).
            let live: std::collections::HashSet<String> = out
                .pictures
                .keys()
                .chain(out.outputs.keys())
                .cloned()
                .chain(out.clocks.keys().cloned())
                .chain(inputs.iter().map(|i| format!("input:{}", i.id)))
                .chain(["machine".to_string()])
                .collect();
            tr.by_key
                .retain(|k, _| live.contains(k.split('#').next().unwrap_or(k)));
        }

        // Processors and memory.
        let mut sys = lock(&self.sys);
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let me = Pid::from_u32(std::process::id());
        let mut watch: Vec<Pid> = pids.iter().map(|(_, p)| Pid::from_u32(*p)).collect();
        watch.push(me);
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&watch),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let cores = sys.cpus().len().max(1);
        let cpu = sys.global_cpu_usage();
        out.machine = Machine {
            cpu,
            cores,
            cpu_name: sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            mem_used: sys.used_memory(),
            mem_total: sys.total_memory(),
            hostname: System::host_name().unwrap_or_default(),
        };
        let (mut ff_cpu, mut ff_n) = (0.0f32, 0usize);
        for (key, pid) in &pids {
            let Some(p) = sys.process(Pid::from_u32(*pid)) else {
                continue;
            };
            // % of one core → cores.
            let c = p.cpu_usage() as f64 / 100.0;
            ff_cpu += p.cpu_usage();
            ff_n += 1;
            if let Some(r) = out.outputs.get_mut(key) {
                // Its thread (conversion, sending) and its ffmpeg.
                r.cpu += c;
            } else if let Some(id) = key.strip_prefix("input:") {
                if let Some(r) = out.inputs.get_mut(id) {
                    r.cpu = c;
                }
            }
        }
        if let Some(p) = sys.process(me) {
            out.process = ProcessLoad {
                cpu: p.cpu_usage() / cores as f32,
                mem: p.memory(),
                ffmpeg_cpu: ff_cpu / cores as f32,
                ffmpeg: ff_n,
            };
        }
        drop(sys);
        {
            let mut tr = lock(&self.tracks);
            let t = tr.get("machine#cpu".into());
            t.push(cpu as f64);
            out.cpu10 = (t.sum() / t.hist.len().max(1) as f64) as f32;
        }
        *self.sampled.write().unwrap_or_else(|p| p.into_inner()) = out;
    }
}

/// Samples once a second, for as long as the server runs.
pub fn spawn_sampler(perf: Arc<Perf>, inputs: Arc<crate::inputs::Inputs>) {
    crate::sync::spawn(perf.sync.clone());
    std::thread::Builder::new()
        .name("cg-perf".into())
        .spawn(move || {
            let mut at = Instant::now();
            // CPU usage needs two looks: the first one only starts counting.
            perf.sample(1.0, inputs.counts());
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let dt = at.elapsed().as_secs_f64();
                at = Instant::now();
                perf.sample(dt, inputs.counts());
                perf.watch();
            }
        })
        .expect("spawn perf sampler");
}

// ------------------------------------------------------------- warnings --

#[derive(Clone, Serialize)]
pub struct Warning {
    pub level: &'static str,
    /// English, with {vars}: the web app translates it.
    pub text: &'static str,
    pub vars: serde_json::Map<String, serde_json::Value>,
}

impl Warning {
    fn new(level: &'static str, text: &'static str, vars: &[(&str, serde_json::Value)]) -> Self {
        Self {
            level,
            text,
            vars: vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        }
    }
    /// The English sentence, for the log.
    pub fn english(&self) -> String {
        let mut s = self.text.to_string();
        for (k, v) in &self.vars {
            let v = match v {
                serde_json::Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            s = s.replace(&format!("{{{k}}}"), &v);
        }
        s
    }
}

impl Perf {
    /// Logs warnings when they start (once, not every second).
    fn watch(&self) {
        let mut w = machine_warnings(&self.sampled());
        w.extend(self.sync_warnings(|_| true));
        static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let now: Vec<String> = w.iter().map(|w| w.text.to_string()).collect();
        let mut seen = lock(&SEEN);
        for x in &w {
            if !seen.iter().any(|s| s == x.text) {
                tracing::warn!("{}", x.english());
            }
        }
        *seen = now;
    }
}

impl Perf {
    /// Lost or missing synchronisation, for the channels `shown` lets through.
    pub fn sync_warnings(&self, shown: impl Fn(&str) -> bool) -> Vec<Warning> {
        let status = self.sync.get();
        let mut w = Vec::new();
        if status.checked_at == 0 {
            return w;
        }
        if status.ntp.synced == Some(false) {
            w.push(Warning::new(
                "warn",
                "The system clock is not synchronised (NTP)",
                &[],
            ));
        }
        for (id, c) in lock(&self.clocks).iter() {
            if !shown(id) {
                continue;
            }
            let (reference, name) = lock(&c.reference).clone();
            match reference {
                ClockReference::Ptp if !status.ptp.locked => {
                    let why = status
                        .ptp
                        .message
                        .clone()
                        .or_else(|| status.ptp.port_state.clone())
                        .unwrap_or_default();
                    w.push(Warning::new(
                        "error",
                        "{channel}: not locked to PTP ({why})",
                        &[("channel", name.into()), ("why", why.into())],
                    ));
                }
                ClockReference::Decklink if c.locked() == Some(false) => {
                    w.push(Warning::new(
                        "error",
                        "{channel}: the DeckLink card has no reference signal",
                        &[("channel", name.into())],
                    ));
                }
                _ => {}
            }
        }
        w
    }
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

pub fn machine_warnings(s: &Sampled) -> Vec<Warning> {
    let mut w = Vec::new();
    if s.machine.cores > 0 && s.cpu10 >= 85.0 {
        w.push(Warning::new(
            "error",
            "The machine is at {cpu} % of its processors",
            &[("cpu", (s.cpu10.round() as i64).into())],
        ));
    } else if s.cpu10 >= 70.0 {
        w.push(Warning::new(
            "warn",
            "The machine is at {cpu} % of its processors",
            &[("cpu", (s.cpu10.round() as i64).into())],
        ));
    }
    if s.machine.mem_total > 0 && s.machine.mem_used as f64 / s.machine.mem_total as f64 > 0.9 {
        w.push(Warning::new(
            "warn",
            "Memory almost full: {used} of {total}",
            &[
                ("used", gb(s.machine.mem_used).into()),
                ("total", gb(s.machine.mem_total).into()),
            ],
        ));
    }
    w
}

// ------------------------------------------------------------- capacity --

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapacityItem {
    pub kind: &'static str,
    pub format: String,
    pub unit: f64,
    pub measured: bool,
    pub more: u32,
    pub max: u32,
    pub now: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacity {
    pub budget: f64,
    pub used: f64,
    pub free: f64,
    pub items: Vec<CapacityItem>,
}

/// A kind of load: what one of them typically costs (cores, at the
/// reference pixel rate), and what was measured on this server.
pub struct Cost {
    pub kind: &'static str,
    /// Cores for one at `REF_*`.
    pub typical: f64,
    /// Measured cores per pixel per second, and how many were measured.
    pub per_px: f64,
    pub samples: u32,
    pub now: u32,
}

/// 1920 × 1080 at 50 frames a second.
pub const REF_OUT: f64 = 1920.0 * 1080.0 * 50.0;
/// 1920 × 1080 at 25 frames a second.
pub const REF_IN: f64 = 1920.0 * 1080.0 * 25.0;

impl Cost {
    pub fn new(kind: &'static str, typical: f64) -> Self {
        Self {
            kind,
            typical,
            per_px: 0.0,
            samples: 0,
            now: 0,
        }
    }
    /// One that runs: `cores` for `px` pixels a second (None: not measurable).
    pub fn add(&mut self, cores: Option<f64>, px: f64) {
        self.now += 1;
        if let Some(c) = cores.filter(|c| *c > 0.0 && px > 0.0) {
            self.per_px += c / px;
            self.samples += 1;
        }
    }
    pub fn item(
        &self,
        px: f64,
        reference: f64,
        format: String,
        budget: f64,
        free: f64,
    ) -> CapacityItem {
        let measured = self.samples > 0;
        let unit = if measured {
            self.per_px / self.samples as f64 * px
        } else {
            self.typical * px / reference
        }
        .max(0.01);
        CapacityItem {
            kind: self.kind,
            format,
            unit: (unit * 100.0).round() / 100.0,
            measured,
            more: (free / unit).floor().max(0.0) as u32,
            max: (budget / unit).floor().max(0.0) as u32,
            now: self.now,
        }
    }
}

pub fn format_name(w: u32, h: u32, fps: f64) -> String {
    let f = if (fps - fps.round()).abs() < 0.01 {
        format!("{}", fps.round())
    } else {
        format!("{fps:.2}")
    };
    format!("{w}×{h} {f}p")
}

// ------------------------------------------------------------------ API --

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::api::{require, ApiResult};
use crate::auth::CurrentUser;
use crate::state::Shared;

fn round(v: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (v * m).round() / m
}

/// `GET /api/system/metrics`: the machine, the channels the user sees (their
/// pictures and outputs), the inputs, what more fits, what goes wrong.
pub async fn metrics(State(s): State<Shared>, CurrentUser(me): CurrentUser) -> Json<Value> {
    let sm = s.perf.sampled();
    let status = s.output_status.snapshot();
    let mut warnings = machine_warnings(&sm);
    warnings.extend(s.perf.sync_warnings(|id| me.sees_channel(id)));
    let mut costs = [
        Cost::new("picture", 0.5),
        Cost::new("stream", 1.6),
        Cost::new("ndi", 0.35),
        Cost::new("st2110", 0.5),
        Cost::new("sdi", 0.2),
    ];
    let cost = |costs: &mut [Cost; 5], kind: &str| costs.iter_mut().position(|c| c.kind == kind);
    let mut channels = Vec::new();
    let mut rates = Vec::new();
    for c in &s.config.channels {
        let Some(ch) = s.channels.get(&c.id) else {
            continue;
        };
        let visible = me.sees_channel(&c.id);
        let project_id = ch.project_id.read().await.clone();
        let on_air = ch.state.read().await.layers.len();
        let rate = c.frame_rate.as_f64().max(1.0);
        rates.push(rate);
        let frame_ms = 1000.0 / rate;
        let clock = sm.clocks.get(&c.id).cloned().unwrap_or_default();
        let stats = s.perf.clock_of(&c.id);
        if visible && clock.max_ms > frame_ms / 2.0 {
            warnings.push(Warning::new(
                "warn",
                "{channel}: the clock ticked up to {ms} ms late",
                &[
                    ("channel", c.name.clone().into()),
                    ("ms", round(clock.max_ms, 1).into()),
                ],
            ));
        }
        let mut pictures = Vec::new();
        let mut load_max = 0.0f64;
        for (pid, ps, sinks) in s.perf.pictures_of(&c.id) {
            let key = format!("{}/{pid}", c.id);
            let r = sm.pictures.get(&key).cloned().unwrap_or_default();
            let load = r.ms / frame_ms * 100.0;
            load_max = load_max.max(load);
            // Rendering an empty channel says little about rendering graphics.
            let rendered = ps.width as f64 * ps.height as f64 * r.fps;
            costs[0].add((on_air > 0 && r.fps >= 1.0).then_some(r.cpu), rendered);
            let vars = |extra: (&'static str, Value)| -> [(&'static str, Value); 3] {
                [
                    ("channel", c.name.clone().into()),
                    ("picture", ps.name.clone().into()),
                    extra,
                ]
            };
            if visible && load > 80.0 {
                warnings.push(Warning::new(
                    "warn",
                    "{channel} › {picture}: rendering takes {load} % of the frame time",
                    &vars(("load", (load.round() as i64).into())),
                ));
            }
            if visible && r.count10 > 0.0 {
                warnings.push(Warning::new(
                    "warn",
                    "{channel} › {picture}: {n} frames shown twice in the last 10 s (rendering late)",
                    &vars(("n", (r.count10 as i64).into())),
                ));
            }
            let mut outputs = Vec::new();
            for o in sinks {
                let rr = sm
                    .outputs
                    .get(&format!("{key}/{}", o.id))
                    .cloned()
                    .unwrap_or_default();
                if let Some(i) = cost(&mut costs, o.kind) {
                    // Per frame actually sent: one that cannot keep up costs more than it shows.
                    let sent = ps.width as f64 * ps.height as f64 * rr.fps;
                    costs[i].add((rr.fps >= 1.0).then_some(rr.cpu), sent);
                }
                let st = status.get(&c.id).and_then(|m| m.get(&o.id));
                if visible && rr.count10 > 0.0 {
                    warnings.push(Warning::new(
                        "warn",
                        "{output}: {n} frames dropped in the last 10 s",
                        &[
                            ("output", o.name.clone().into()),
                            ("n", (rr.count10 as i64).into()),
                        ],
                    ));
                }
                outputs.push(json!({
                    "id": o.id,
                    "kind": o.kind,
                    "name": o.name,
                    "fps": round(rr.fps, 1),
                    "dropped": rr.count10 as u64,
                    "ok": st.is_none_or(|x| x.ok),
                    "message": st.map(|x| x.message.clone()).unwrap_or_default(),
                    "cpu": round(rr.cpu, 2),
                }));
            }
            pictures.push(json!({
                "id": pid,
                "name": ps.name,
                "width": ps.width,
                "height": ps.height,
                "fps": round(r.fps, 1),
                "renderMs": round(r.ms, 2),
                "renderMaxMs": round(r.max_ms, 2),
                "load": round(load, 1),
                "repeated": r.count10 as u64,
                "outputs": outputs,
            }));
        }
        if visible {
            channels.push(json!({
                "id": c.id,
                "name": c.name,
                "projectId": project_id,
                "onAir": on_air,
                "rate": rate,
                "jitterMs": round(clock.max_ms, 2),
                "clock": stats.as_ref().map(|c| c.reference()),
                "clockLocked": stats.as_ref().and_then(|c| c.locked()),
                "load": round(load_max, 1),
                "pictures": pictures,
            }));
        }
    }

    // Inputs.
    let (mut in_stream, mut in_ndi) = (Cost::new("inputStream", 0.35), Cost::new("inputNdi", 0.3));
    let status_in = s.inputs.status();
    let mut inputs = Vec::new();
    for i in &s.config.inputs {
        let r = sm.inputs.get(&i.id).cloned().unwrap_or_default();
        let st = status_in.iter().find(|x| x.id == i.id);
        let live = st.is_some_and(|x| x.live);
        // The source's rate when known (streams detect it).
        let nominal = st
            .and_then(|x| x.fps)
            .unwrap_or(i.fps as f64)
            .clamp(1.0, 120.0);
        let px = i.width as f64 * i.height as f64 * nominal;
        use crate::inputs::InputKind;
        match i.kind {
            InputKind::Ndi => in_ndi.add(None, px),
            InputKind::Sdi | InputKind::St2110 => {}
            _ => in_stream.add(
                (live && r.fps >= 1.0).then_some(r.cpu),
                i.width as f64 * i.height as f64 * r.fps,
            ),
        }
        if live && r.fps > 0.0 && r.fps < nominal * 0.9 {
            warnings.push(Warning::new(
                "warn",
                "{input} receives {fps} fps instead of {nominal}",
                &[
                    ("input", i.name.clone().into()),
                    ("fps", round(r.fps, 1).into()),
                    ("nominal", nominal.into()),
                ],
            ));
        }
        if r.count10 > 0.0 {
            warnings.push(Warning::new(
                "warn",
                "{input}: {n} pictures thrown away in the last 10 s (too many waiting)",
                &[
                    ("input", i.name.clone().into()),
                    ("n", (r.count10 as i64).into()),
                ],
            ));
        }
        // 25 on 50 shows each picture twice, evenly; 30 on 50 does not.
        let even = |a: f64, b: f64| ((a / b) - (a / b).round()).abs() < 0.01;
        if let Some(rate) = rates
            .iter()
            .find(|r| !even(**r, nominal) && !even(nominal, **r))
        {
            warnings.push(Warning::new(
                "info",
                "{input} runs at {fps} fps on a {rate} fps channel: motion will not be smooth",
                &[
                    ("input", i.name.clone().into()),
                    ("fps", nominal.into()),
                    ("rate", round(*rate, 2).into()),
                ],
            ));
        }
        inputs.push(json!({
            "id": i.id,
            "name": i.name,
            "kind": i.kind,
            "live": live,
            "fps": round(r.fps, 1),
            "nominal": nominal,
            "dropped": r.count10 as u64,
            "bufferMs": round(i.buffer.min(10) as f64 * 1000.0 / nominal, 0),
            "cpu": round(r.cpu, 2),
        }));
    }

    // What more fits: 80 % of the machine is the budget.
    let cores = sm.machine.cores.max(1) as f64;
    let budget = cores * 0.8;
    let used = sm.machine.cpu as f64 / 100.0 * cores;
    let free = (budget - used).max(0.0);
    let (w, h, rate) = s.config.channels.first().map_or((1920, 1080, 50.0), |c| {
        (c.width, c.height, c.frame_rate.as_f64())
    });
    let out_px = w as f64 * h as f64 * rate;
    let fmt = format_name(w, h, rate);
    let (iw, ih, ifps) = s
        .config
        .inputs
        .iter()
        .find(|i| {
            !matches!(
                i.kind,
                crate::inputs::InputKind::Ndi
                    | crate::inputs::InputKind::Sdi
                    | crate::inputs::InputKind::St2110
            )
        })
        .map_or((1920, 1080, 25.0), |i| {
            (i.width, i.height, i.fps.clamp(1, 120) as f64)
        });
    let in_px = iw as f64 * ih as f64 * ifps;
    let ifmt = format_name(iw, ih, ifps);
    let mut items: Vec<CapacityItem> = costs
        .iter()
        .map(|c| {
            let f = if c.kind == "stream" {
                format!("{fmt} H.264")
            } else {
                fmt.clone()
            };
            c.item(out_px, REF_OUT, f, budget, free)
        })
        .collect();
    items.push(in_stream.item(in_px, REF_IN, ifmt.clone(), budget, free));
    items.push(in_ndi.item(in_px, REF_IN, ifmt, budget, free));

    Json(json!({
        "at": crate::state::now_ms(),
        "machine": sm.machine,
        "process": sm.process,
        "channels": channels,
        "inputs": inputs,
        "capacity": Capacity {
            budget: round(budget, 2),
            used: round(used, 2),
            free: round(free, 2),
            items,
        },
        "sync": s.perf.sync.get(),
        "warnings": warnings,
    }))
}

#[derive(serde::Deserialize)]
pub struct LogQuery {
    #[serde(default)]
    after: u64,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

/// `GET /api/system/logs?after=<seq>&level=<warn>&limit=<n>`: the recent log.
pub async fn logs(
    CurrentUser(me): CurrentUser,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    require(me.is_admin())?;
    let (entries, last) = crate::logs::since(
        q.after,
        q.level.as_deref(),
        q.limit.unwrap_or(1000).min(5000),
    );
    Ok(Json(json!({
        "entries": entries,
        "last": last,
        "file": crate::logs::FILE.get().map(|p| p.display().to_string()),
    })))
}

/// `GET /api/system/logs/download`: the recent log as a text file.
pub async fn logs_download(CurrentUser(me): CurrentUser) -> ApiResult<impl IntoResponse> {
    require(me.is_admin())?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"strata.log\"",
            ),
        ],
        crate::logs::text(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o_sent(p: &PictureStats) {
        for o in lock(&p.outputs).iter() {
            o.sent(Duration::from_millis(2), true);
        }
    }

    #[test]
    fn counts_become_rates() {
        let perf = Perf::default();
        let p = perf.picture("news", "main", "Main", 1920, 1080);
        let o = p.output("ndi-1", "ndi", "NEWS");
        perf.sample(1.0, vec![]);
        // Three steady seconds at 50 frames.
        for _ in 0..3 {
            for _ in 0..50 {
                p.rendered(Duration::from_millis(4));
                o.sent(Duration::from_millis(2), true);
            }
            perf.sample(1.0, vec![]);
        }
        p.repeated();
        o.dropped();
        perf.sample(1.0, vec![]);
        let s = perf.sampled();
        // The rate is the mean of the last three seconds: 50, 50, then none.
        let r = &s.pictures["news/main"];
        assert!((r.fps - 100.0 / 3.0).abs() < 0.01, "{}", r.fps);
        assert_eq!(r.count10, 1.0);
        let o = &s.outputs["news/main/ndi-1"];
        assert_eq!(o.count10, 1.0);
        // Time and load are those of the last second that had frames.
        let s0 = {
            for _ in 0..50 {
                p.rendered(Duration::from_millis(4));
                o_sent(&p);
            }
            perf.sample(1.0, vec![]);
            perf.sampled()
        };
        let r = &s0.pictures["news/main"];
        assert!((r.ms - 4.0).abs() < 0.01 && (r.cpu - 0.2).abs() < 0.01);
        assert!((s0.outputs["news/main/ndi-1"].cpu - 0.1).abs() < 0.01);
        // Seconds later, nothing more: no frames, the drop still counts.
        for _ in 0..3 {
            perf.sample(1.0, vec![]);
        }
        let s = perf.sampled();
        assert_eq!(s.pictures["news/main"].fps, 0.0);
        assert_eq!(s.outputs["news/main/ndi-1"].count10, 1.0);
        perf.reset_channel("news");
        perf.sample(1.0, vec![]);
        assert!(perf.sampled().pictures.is_empty());
    }

    #[test]
    fn capacity_from_measures_or_typical_costs() {
        let mut c = Cost::new("stream", 1.6);
        // Nothing runs: the typical cost, scaled to the format.
        let i = c.item(REF_OUT / 2.0, REF_OUT, "x".into(), 8.0, 4.0);
        assert!(!i.measured && i.unit == 0.8 && i.more == 5 && i.max == 10);
        // One runs at 1080p50 using 2 cores.
        c.add(Some(2.0), REF_OUT);
        let i = c.item(REF_OUT, REF_OUT, "x".into(), 8.0, 4.0);
        assert!(i.measured && i.unit == 2.0 && i.more == 2 && i.max == 4 && i.now == 1);
    }
}
