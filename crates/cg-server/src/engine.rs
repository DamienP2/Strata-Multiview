//! Playout: one clock thread per channel ticks at the channel frame rate,
//! advances the timelines, notifies clients and feeds the channel's outputs.
//!
//! Smoothness comes from three things:
//! - the clock aims at absolute deadlines (start + n × period), sleeping most
//!   of the way and spinning the last fraction of a millisecond, so it
//!   neither drifts nor bunches frames up;
//! - each picture is rendered one tick ahead of when it is sent: at every tick
//!   the outputs get the latest finished frame, the previous one again when a
//!   render is late, so they keep an even cadence instead of skipping;
//! - each output sends from its own thread, the newest frame winning: a slow
//!   one (ffmpeg behind, a network hiccup) holds back neither the renderer nor
//!   the other outputs. Frames are shared, never copied.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use cg_core::playout::Phase;
use cg_core::{ChannelState, Project};
use cg_output::ndi::NdiSender;
use cg_output::sdi::DeckLinkOutput;
use cg_output::st2110::St2110Sender;
use cg_output::stream::StreamSender;
use cg_output::{FrameClock, OutputError, OutputSink};
use cg_render::slate::Slate;
use cg_render::{Frame, FrameCache, OutputFormat, Renderer};

use crate::config::ChannelConfig;
use crate::hardware::ClockReference;
use crate::perf::{ClockStats, PictureStats, SinkStats};
use crate::state::{ChannelRt, OutputStatus, Shared};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A started output.
struct Opened {
    id: String,
    kind: &'static str,
    name: String,
    sink: Box<dyn OutputSink>,
}

fn open_outputs(
    cfg: &ChannelConfig,
    outputs: &crate::config::OutputsConfig,
    size: (u32, u32),
    period: Duration,
    status: &OutputStatus,
    hls_root: &std::path::Path,
) -> Vec<Opened> {
    let mut sinks = Vec::new();
    // Switched off: shown as such, not started.
    let off = |id: &str| status.set(&cfg.id, id, true, "Off");
    let mut opened = |id: &str,
                      name: &str,
                      kind: &'static str,
                      r: Result<Box<dyn OutputSink>, OutputError>| match r {
        Ok(sink) => {
            status.set(&cfg.id, id, true, "Running");
            sinks.push(Opened {
                id: id.to_string(),
                kind,
                name: name.to_string(),
                sink,
            });
        }
        Err(e) => {
            tracing::error!(
                channel = cfg.name,
                output = name,
                "cannot start {kind} output: {e}"
            );
            status.set_detailed(&cfg.id, id, false, short(&e), e.to_string());
        }
    };
    for o in &outputs.st2110 {
        if !o.enabled {
            off(&o.id);
            continue;
        }
        let r = St2110Sender::start(o.clone(), period).map(|s| Box::new(s) as Box<dyn OutputSink>);
        opened(&o.id, &o.name, "st2110", r);
    }
    for o in &outputs.sdi {
        if !o.enabled {
            off(&o.id);
            continue;
        }
        let r = DeckLinkOutput::open(
            o.clone(),
            size.0,
            size.1,
            (cfg.frame_rate.num, cfg.frame_rate.den),
            cfg.interlaced,
        )
        .map(|s| Box::new(s) as Box<dyn OutputSink>);
        opened(&o.id, &o.name, "sdi", r);
    }
    let rate = (cfg.frame_rate.num, cfg.frame_rate.den);
    for o in &outputs.ndi {
        if !o.enabled {
            off(&o.id);
            continue;
        }
        let r = NdiSender::open(o.clone(), rate).map(|s| Box::new(s) as Box<dyn OutputSink>);
        opened(&o.id, &o.name, "ndi", r);
    }
    for o in &outputs.streams {
        if !o.enabled {
            off(&o.id);
            continue;
        }
        let hls = (o.protocol == cg_output::stream::Protocol::Hls).then(|| hls_root.join(&o.id));
        let r = StreamSender::open(o.clone(), size.0, size.1, rate, hls)
            .map(|s| Box::new(s) as Box<dyn OutputSink>);
        opened(&o.id, &o.name, "stream", r);
    }
    if let Some(path) = cg_output::ndi::loaded_path() {
        for o in &outputs.ndi {
            if sinks.iter().any(|s| s.id == o.id) {
                status.set_detailed(
                    &cfg.id,
                    &o.id,
                    true,
                    "Running",
                    format!("NDI runtime: {path}"),
                );
            }
        }
    }
    sinks
}

/// An error as the outputs page shows it (details stay in the log).
fn short(e: &OutputError) -> String {
    match e {
        OutputError::Unsupported(m) => m.split(" (").next().unwrap_or(m).to_string(),
        e => e.to_string(),
    }
}

/// Starts every channel. The tasks end by themselves once the cycle closes
/// (`state.closing`), after their outputs have stopped.
pub fn start(state: &Shared) -> Vec<tokio::task::JoinHandle<()>> {
    cg_output::ndi::set_library_path(&state.config.ndi_library);
    crate::webimages::install(state.assets_dir());
    state
        .channels
        .values()
        .map(|ch| tokio::spawn(run_channel(state.clone(), ch.clone())))
        .collect()
}

/// The project is `None` when nothing is loaded: outputs keep sending black.
/// (tick, project, state, when the frame is shown: Unix ms — widgets that
/// move by the clock, a ticker, a timer, are drawn at that instant, not when
/// the frame happens to be rendered).
type RenderJob = (u64, Option<Arc<Project>>, ChannelState, f64);

// ---------------------------------------------------------------- outputs --

/// A frame and the tick it is for.
type Timed = (Arc<Frame>, SystemTime);

/// The next frame for one output's thread: the newest wins.
#[derive(Default)]
struct Mailbox {
    /// The frame and its tick; closed.
    slot: Mutex<(Option<Timed>, bool)>,
    ready: Condvar,
}

struct OutputThread {
    mailbox: Arc<Mailbox>,
    stats: Arc<SinkStats>,
    thread: std::thread::JoinHandle<()>,
}

impl OutputThread {
    fn start(o: Opened, stats: Arc<SinkStats>, status: Arc<OutputStatus>, channel: String) -> Self {
        let mailbox = Arc::new(Mailbox::default());
        let thread = std::thread::Builder::new()
            .name(format!("cg-out-{}", o.id))
            .spawn({
                let (mailbox, stats) = (mailbox.clone(), stats.clone());
                move || output_loop(o, &mailbox, &stats, &status, &channel)
            })
            .expect("spawn output thread");
        Self {
            mailbox,
            stats,
            thread,
        }
    }

    fn send(&self, frame: Arc<Frame>, at: SystemTime) {
        let mut slot = lock(&self.mailbox.slot);
        // The previous one was never taken: the output is behind.
        if slot.0.replace((frame, at)).is_some() {
            self.stats.dropped();
        }
        self.mailbox.ready.notify_one();
    }

    /// Stops the thread once its frame is out; the output closes with it.
    fn close(self) {
        lock(&self.mailbox.slot).1 = true;
        self.mailbox.ready.notify_one();
        let _ = self.thread.join();
    }
}

fn output_loop(
    o: Opened,
    mailbox: &Mailbox,
    stats: &SinkStats,
    status: &OutputStatus,
    channel: &str,
) {
    crate::rt::prioritize(crate::rt::Work::Output);
    let Opened { id, mut sink, .. } = o;
    let mut failing = false;
    let mut looked = Instant::now() - Duration::from_secs(5);
    loop {
        let (frame, at) = {
            let mut slot = lock(&mailbox.slot);
            loop {
                if let Some(f) = slot.0.take() {
                    break f;
                }
                if slot.1 {
                    return;
                }
                slot = mailbox.ready.wait(slot).unwrap_or_else(|p| p.into_inner());
            }
        };
        let t = Instant::now();
        let r = sink.push(&frame, at);
        stats.sent(t.elapsed(), r.is_ok());
        match r {
            Ok(()) | Err(OutputError::Dropped) => {
                if failing {
                    failing = false;
                    status.set(channel, &id, true, "Running");
                }
            }
            Err(e) => {
                // Report a failure once, not every frame.
                if !failing {
                    tracing::warn!(output = sink.name(), "{e}");
                    status.set_detailed(channel, &id, false, short(&e), e.to_string());
                    failing = true;
                }
            }
        }
        if looked.elapsed() >= Duration::from_secs(1) {
            looked = Instant::now();
            stats.set_process(sink.process());
            if let Some(w) = sink.work_time() {
                stats.set_work(w);
            }
        }
    }
}

// --------------------------------------------------------------- pictures --

/// Frames finished by the render threads, by tick.
type Done = Arc<Mutex<std::collections::BTreeMap<u64, Arc<Frame>>>>;

/// One picture (main or a rendition) with outputs: its render threads, the
/// frames they finished, and its outputs. The frame of tick n is sent at tick
/// n + `ahead`: a frame that takes a little longer than one period to render
/// (a big picture loading, a busy moment) is still on time.
struct Picture {
    jobs: mpsc::SyncSender<RenderJob>,
    done: Done,
    ahead: u64,
    /// The last frame sent (its tick), sent again when the next is late.
    last: Option<(u64, Arc<Frame>)>,
    stats: Arc<PictureStats>,
    outputs: Vec<OutputThread>,
    /// The live source under the graphics (it then holds the outputs).
    mixer: Option<Mixer>,
    renders: Vec<std::thread::JoinHandle<()>>,
    /// The frame clock of its first output that has one (a DeckLink card).
    card: Option<Arc<dyn FrameClock>>,
}

impl Picture {
    /// Gives every output the frame of this tick: the newest one rendered
    /// for it or before, else the last one again.
    fn dispatch(&mut self, tick: u64, at: SystemTime) {
        let Some(due) = tick.checked_sub(self.ahead) else {
            return;
        };
        let newest = {
            let mut done = lock(&self.done);
            let later = done.split_off(&(due + 1));
            let ready = std::mem::replace(&mut *done, later);
            ready.into_iter().next_back()
        };
        match newest {
            Some((n, f)) if self.last.as_ref().is_none_or(|(m, _)| n > *m) => {
                if n < due {
                    self.stats.repeated();
                }
                self.last = Some((n, f));
            }
            _ => {
                if self.last.is_some() {
                    self.stats.repeated();
                }
            }
        }
        if let Some((_, frame)) = &self.last {
            if let Some(m) = &self.mixer {
                m.send(frame.clone(), at);
            }
            for o in &self.outputs {
                o.send(frame.clone(), at);
            }
        }
    }

    /// Asks for a frame; skipped when the render threads are that far behind.
    fn request(&self, job: RenderJob) {
        let _ = self.jobs.try_send(job);
    }

    fn close(self) {
        drop(self.jobs);
        for r in self.renders {
            let _ = r.join();
        }
        if let Some(m) = self.mixer {
            m.close();
        }
        for o in self.outputs {
            o.close();
        }
    }
}

/// Keys a picture's graphics over the newest picture of a live source as
/// each frame goes out, on a thread of its own (the clock never waits): the
/// video is not delayed by the frames rendered ahead, only by the keying.
struct Mixer {
    frames: mpsc::SyncSender<(Arc<Frame>, SystemTime)>,
    thread: std::thread::JoinHandle<()>,
}

impl Mixer {
    fn start(
        name: String,
        source: String,
        inputs: Arc<crate::inputs::Inputs>,
        outputs: Vec<OutputThread>,
    ) -> Self {
        let (frames, rx) = mpsc::sync_channel::<(Arc<Frame>, SystemTime)>(2);
        let thread = std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                crate::rt::prioritize(crate::rt::Work::Render);
                use cg_render::LiveInputs;
                // The last keyed frame, with the graphics and video it came from.
                let mut last: Option<(Arc<Frame>, Arc<tiny_skia::Pixmap>, Arc<Frame>)> = None;
                // A source of another size, fitted to the picture (once per source picture).
                let mut fitted: Option<(Arc<tiny_skia::Pixmap>, Arc<tiny_skia::Pixmap>)> = None;
                while let Ok((frame, at)) = rx.recv() {
                    let video = inputs.picture(&source).map(|live| {
                        if (live.width(), live.height()) == (frame.width, frame.height) {
                            return live;
                        }
                        match &fitted {
                            Some((from, f)) if Arc::ptr_eq(from, &live) => f.clone(),
                            _ => {
                                let f = Arc::new(cg_render::fit_picture(
                                    &live,
                                    frame.width,
                                    frame.height,
                                ));
                                fitted = Some((live, f.clone()));
                                f
                            }
                        }
                    });
                    let out = match video {
                        Some(live) => match &last {
                            Some((g, v, keyed))
                                if Arc::ptr_eq(g, &frame) && Arc::ptr_eq(v, &live) =>
                            {
                                keyed.clone()
                            }
                            _ => {
                                let keyed = Arc::new(cg_render::over_live(&frame, &live));
                                last = Some((frame, live, keyed.clone()));
                                keyed
                            }
                        },
                        None => frame,
                    };
                    for o in &outputs {
                        o.send(out.clone(), at);
                    }
                }
                for o in outputs {
                    o.close();
                }
            })
            .expect("spawn mixer thread");
        Self { frames, thread }
    }

    /// Hands over a frame to go out (dropped when the keying is behind:
    /// the outputs then repeat the last one).
    fn send(&self, frame: Arc<Frame>, at: SystemTime) {
        let _ = self.frames.try_send((frame, at));
    }

    fn close(self) {
        drop(self.frames);
        let _ = self.thread.join();
    }
}

/// One render thread: takes the next job, renders it, files the frame under its tick.
/// `cache` is shared by the picture's render threads: a frame where nothing
/// moved is the same `Arc` again (the outputs then skip their conversion),
/// and only what moves is drawn over the kept picture of what does not.
#[allow(clippy::too_many_arguments)]
fn render_loop(
    mut renderer: Renderer,
    out: OutputFormat,
    tz: String,
    slate: Option<Slate>,
    rx: Arc<Mutex<mpsc::Receiver<RenderJob>>>,
    done: Done,
    stats: Arc<PictureStats>,
    cache: Arc<FrameCache>,
) {
    crate::rt::prioritize(crate::rt::Work::Render);
    // Nothing loaded: the slate (or nothing), drawn once, the same frame every time.
    let mut idle: Option<Arc<Frame>> = None;
    loop {
        let job = lock(&rx).recv();
        let Ok((tick, project, channel, shown_ms)) = job else {
            return;
        };
        let t = Instant::now();
        let frame = match project {
            Some(p) => renderer.render_channel_cached(&p, &channel, out, &tz, shown_ms, &cache),
            None => idle
                .get_or_insert_with(|| {
                    Arc::new(match &slate {
                        Some(s) => renderer.render_idle(s, out),
                        None => Frame::blank(out.width, out.height),
                    })
                })
                .clone(),
        };
        stats.rendered(t.elapsed());
        let mut d = lock(&done);
        d.insert(tick, frame);
        // Never more than a few waiting (nothing sends them while stopping):
        // the frames ahead (4 at most) and one.
        while d.len() > 5 {
            d.pop_first();
        }
    }
}

/// Opens a channel's outputs and starts the render threads of each picture that has some.
fn start_pictures(state: &Shared, cfg: &ChannelConfig) -> Vec<Picture> {
    let period = cfg.frame_rate.frame_duration();
    let ahead = cfg.render_ahead.clamp(1, 4);
    let mut pictures = Vec::new();
    for (id, width, height, outputs) in cfg.pictures() {
        let hls = state.hls_dir().join(&cfg.id).join(id);
        let opened = open_outputs(
            cfg,
            outputs,
            (width, height),
            period,
            &state.output_status,
            &hls,
        );
        if opened.is_empty() {
            continue;
        }
        let name = cfg
            .renditions
            .iter()
            .find(|r| r.id == id)
            .map_or("Principal", |r| r.name.as_str());
        let stats = state.perf.picture(&cfg.id, id, name, width, height);
        let card = opened.iter().find_map(|o| o.sink.frame_clock());
        let outputs = opened
            .into_iter()
            .map(|o| {
                let s = stats.output(&o.id, o.kind, &o.name);
                OutputThread::start(o, s, state.output_status.clone(), cfg.id.clone())
            })
            .collect();
        // As many threads as frames ahead: each frame has a period per thread to render.
        let (jobs, rx) = mpsc::sync_channel::<RenderJob>(1);
        let rx = Arc::new(Mutex::new(rx));
        let done: Done = Default::default();
        let cache = Arc::new(FrameCache::new());
        let renders = (0..ahead)
            .map(|k| {
                std::thread::Builder::new()
                    .name(format!("cg-render-{}-{id}-{k}", cfg.id))
                    .spawn({
                        let assets = state.assets_dir();
                        let out = OutputFormat { width, height };
                        let tz = state.config.timezone.clone();
                        let slate = cfg.slate_of(id);
                        let live = state.inputs.clone();
                        let (rx, done, stats) = (rx.clone(), done.clone(), stats.clone());
                        let cache = cache.clone();
                        move || {
                            let mut r = Renderer::new(assets);
                            r.set_live(live);
                            render_loop(r, out, tz, slate, rx, done, stats, cache)
                        }
                    })
                    .expect("spawn render thread")
            })
            .collect();
        // A picture fed through the channel's source gets the live video
        // under its graphics as each frame goes out.
        let (outputs, mixer) = match cfg.background.clone() {
            Some(source) if !cfg.graphics_only.iter().any(|p| p == id) => {
                let name = format!("cg-mix-{}-{id}", cfg.id);
                (
                    Vec::new(),
                    Some(Mixer::start(name, source, state.inputs.clone(), outputs)),
                )
            }
            _ => (outputs, None),
        };
        pictures.push(Picture {
            jobs,
            done,
            ahead: ahead as u64,
            last: None,
            stats,
            outputs,
            mixer,
            renders,
            card,
        });
    }
    pictures
}

// ----------------------------------------------------------------- memory --

/// Gives memory back to the system once the pictures are quiet (after a
/// busy scene was cleared, thumbnails were drawn…): the spare frame buffers,
/// and what the allocator keeps of freed memory (glibc keeps it otherwise).
pub async fn give_back_memory() {
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if cg_render::pool::release_idle(Duration::from_secs(3)) {
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            let _ = tokio::task::spawn_blocking(|| {
                // SAFETY: malloc_trim only returns free pages of the heap.
                unsafe { libc::malloc_trim(0) }
            })
            .await;
        }
    }
}

// ------------------------------------------------------------------ clock --

/// Sets the flag when dropped (the channel task was cancelled).
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

async fn run_channel(state: Shared, ch: Arc<ChannelRt>) {
    let id = ch.config.id.clone();
    state.output_status.reset(&id);
    state.perf.reset_channel(&id);
    let stop = Arc::new(AtomicBool::new(false));
    let _guard = StopOnDrop(stop.clone());
    let (done_tx, mut done) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::runtime::Handle::current();
    let spawned = std::thread::Builder::new()
        .name(format!("cg-clock-{id}"))
        .spawn({
            let (state, ch, stop) = (state.clone(), ch.clone(), stop.clone());
            move || {
                // Opening outputs can take a moment (ffmpeg, NDI runtime).
                let pictures = start_pictures(&state, &ch.config);
                let stats = state.perf.clock(&ch.config.id);
                clock(&handle, &state, &ch, pictures, &stop, &stats);
                let _ = done_tx.send(());
            }
        });
    if let Err(e) = spawned {
        tracing::error!(
            channel = ch.config.name,
            "cannot start the channel clock: {e}"
        );
        return;
    }
    let mut closing = state.closing.subscribe();
    tokio::select! {
        _ = closing.wait_for(|c| *c) => {}
        _ = &mut done => return,
    }
    // Ends the clock, then waits for the outputs to close (an SRT port, an
    // NDI name: the next engine cycle may open the same ones).
    stop.store(true, Ordering::Relaxed);
    let _ = done.await;
}

/// Waits for `due`: sleeps most of the way, then spins the rest (sleeping
/// alone can wake up a millisecond or more late).
fn wait_until(due: Instant) {
    let spin = if cfg!(windows) {
        Duration::from_micros(1500)
    } else {
        Duration::from_micros(300)
    };
    loop {
        let now = Instant::now();
        if now >= due {
            return;
        }
        let left = due - now;
        if left > spin {
            std::thread::sleep(left - spin);
        } else {
            // A pure spin: yielding here hands the core to any busy process,
            // and on a loaded machine the clock then wakes a whole time slice late.
            std::hint::spin_loop();
        }
    }
}

fn clock(
    rt: &tokio::runtime::Handle,
    state: &Shared,
    ch: &Arc<ChannelRt>,
    mut pictures: Vec<Picture>,
    stop: &AtomicBool,
    stats: &ClockStats,
) {
    crate::rt::prioritize(crate::rt::Work::Clock);
    let rate = ch.config.frame_rate;
    let period = rate.frame_duration();
    let reference = ch
        .config
        .clock
        .unwrap_or(state.config.hardware.sync.reference);
    let tai = state.config.hardware.sync.tai_offset;
    let card = pictures.iter().find_map(|p| p.card.clone());
    let auto = reference == ClockReference::Auto;
    // What Auto follows now: the card when its reference input is locked,
    // else PTP when locked, else the card's own clock (an SDI output is on
    // it), else the server's time.
    let choose = |card: &Option<Arc<dyn FrameClock>>| {
        let ptp = state.perf.sync.get().ptp.locked;
        match card {
            Some(c) if c.locked() == Some(true) => ClockReference::Decklink,
            _ if ptp => ClockReference::Ptp,
            Some(_) => ClockReference::Decklink,
            None => ClockReference::Free,
        }
    };
    let mut followed = match (reference, &card) {
        (ClockReference::Auto, _) => choose(&card),
        // An SDI output is on the card's clock: the channel follows it, so
        // the card never has to repeat or drop a frame to stay in step.
        (ClockReference::Free, Some(_)) => {
            tracing::info!(
                channel = ch.config.name,
                "the channel runs on its DeckLink output's clock"
            );
            ClockReference::Decklink
        }
        (ClockReference::Decklink, None) => {
            tracing::warn!(
                channel = ch.config.name,
                "no DeckLink output to follow: the channel runs on the server's time"
            );
            ClockReference::Free
        }
        (r, _) => r,
    };
    let pace_for = |r: ClockReference| match (&card, r) {
        (Some(clock), ClockReference::Decklink) => Pace::card(clock.clone(), period),
        // PTP or the server's time: the same grid on the system clock (PTP
        // sets it when phc2sys runs, NTP otherwise).
        _ => Pace::Grid(Grid::new(rate, tai)),
    };
    let mut pace = pace_for(followed);
    stats.set_reference(followed, &ch.config.name);
    let check_every = (rate.num as u64 / rate.den.max(1) as u64).max(1);
    let mut n: u64 = 0;
    let mut saved_at = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        n += 1;
        // Auto: about once a second, the reference that is best now.
        if auto && n.is_multiple_of(check_every) {
            let now = choose(&card);
            if now != followed {
                tracing::info!(
                    channel = ch.config.name,
                    "the channel now follows {}",
                    match now {
                        ClockReference::Decklink => "its DeckLink card",
                        ClockReference::Ptp => "PTP",
                        _ => "the server's time",
                    }
                );
                if (now == ClockReference::Decklink) != (followed == ClockReference::Decklink) {
                    pace = pace_for(now);
                }
                followed = now;
                stats.set_reference(followed, &ch.config.name);
            }
        }
        let tick = pace.next();
        stats.ticked(tick.late);
        if let Pace::Card { clock, .. } = &pace {
            if n % 25 == 1 {
                stats.set_locked(clock.locked());
            }
        }
        if tick.skipped > 0 {
            stats.skipped(tick.skipped);
            n += tick.skipped;
        }
        let at = tick.at;
        for p in &mut pictures {
            p.dispatch(n, at);
        }
        let (project, snapshot) = rt.block_on(advance(state, ch, &mut saved_at));
        for p in &pictures {
            // Shown `ahead` ticks from now.
            let shown = at + period * p.ahead as u32;
            let shown_ms = shown
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64() * 1000.0);
            p.request((n, project.clone(), snapshot.clone(), shown_ms));
        }
    }
    for p in pictures {
        p.close();
    }
}

/// A tick of the clock: when its frame is shown, how late it came, how
/// many frames were skipped to catch up.
struct Tick {
    at: SystemTime,
    late: Duration,
    skipped: u64,
}

/// When the ticks of a channel fall.
enum Pace {
    /// On the frame grid of the system clock (set by PTP when phc2sys
    /// runs, by NTP otherwise).
    Grid(Grid),
    /// On the frame boundaries of a DeckLink output (its genlock); on its
    /// own while the card does not tick.
    Card {
        clock: Arc<dyn FrameClock>,
        period: Duration,
        start: Instant,
        wall: SystemTime,
        last: Instant,
        /// Ticks so far: frame times fall one period apart on the card's
        /// grid, whatever the jitter of its callbacks.
        n: u64,
    },
}

impl Pace {
    fn card(clock: Arc<dyn FrameClock>, period: Duration) -> Self {
        let now = Instant::now();
        Pace::Card {
            clock,
            period,
            start: now,
            wall: SystemTime::now(),
            last: now,
            n: 0,
        }
    }

    /// Waits for the next tick.
    fn next(&mut self) -> Tick {
        match self {
            Pace::Card {
                clock,
                period,
                start,
                wall,
                last,
                n,
            } => {
                let due = *last + *period;
                if !clock.wait(*period * 2) {
                    wait_until(due);
                }
                let now = Instant::now();
                let gap = now - *last;
                *last = now;
                // The card ticks on its own reference: lateness is how far a
                // tick strays from one period after the previous one.
                let late = now.saturating_duration_since(due);
                let skipped = if gap > *period * 2 {
                    (gap.as_secs_f64() / period.as_secs_f64()) as u64 - 1
                } else {
                    0
                };
                *n += 1 + skipped;
                // Frame times one period apart on the card's grid (a callback
                // that comes a little early or late does not move what the
                // frame shows); the grid is set again on the system clock
                // when the two drift a frame apart.
                let grid = Duration::from_secs_f64(*n as f64 * period.as_secs_f64());
                let real = now - *start;
                if real.abs_diff(grid) > *period {
                    *start = now - grid;
                }
                Tick {
                    at: *wall + grid,
                    late,
                    skipped,
                }
            }
            Pace::Grid(g) => g.next(),
        }
    }
}

/// SMPTE ST 2059-1 alignment: frame k starts k frame periods after the PTP
/// epoch (1970-01-01 TAI), so every device locked to the same grandmaster
/// shows its frames at the same instants. Steady-clock instants are mapped
/// to TAI again every second (phc2sys slews the system clock, and may step it).
struct Grid {
    num: u128,
    den: u128,
    /// TAI − UTC.
    tai: Duration,
    period: Duration,
    anchor: Instant,
    anchor_tai: Duration,
    /// The next frame.
    k: u64,
}

const NS: u128 = 1_000_000_000;

impl Grid {
    fn new(rate: cg_core::FrameRate, tai_offset: u32) -> Self {
        let mut g = Grid {
            num: rate.num.max(1) as u128,
            den: rate.den.max(1) as u128,
            tai: Duration::from_secs(tai_offset.into()),
            period: rate.frame_duration(),
            anchor: Instant::now(),
            anchor_tai: Duration::ZERO,
            k: 0,
        };
        g.reanchor();
        g.k = g.index_at(g.anchor_tai) + 1;
        g
    }

    fn reanchor(&mut self) {
        self.anchor = Instant::now();
        let utc = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        self.anchor_tai = utc + self.tai;
    }

    fn now_tai(&self) -> Duration {
        self.anchor_tai + self.anchor.elapsed()
    }

    /// TAI time frame k starts.
    fn time_of(&self, k: u64) -> Duration {
        let ns = k as u128 * self.den * NS / self.num;
        Duration::new((ns / NS) as u64, (ns % NS) as u32)
    }

    /// The frame showing at TAI time t.
    fn index_at(&self, t: Duration) -> u64 {
        (t.as_nanos() * self.num / (self.den * NS)) as u64
    }

    /// When frame k starts, on the steady clock.
    fn due(&self, k: u64) -> Instant {
        let t = self.time_of(k);
        if t >= self.anchor_tai {
            self.anchor + (t - self.anchor_tai)
        } else {
            self.anchor
                .checked_sub(self.anchor_tai - t)
                .unwrap_or(self.anchor)
        }
    }

    fn next(&mut self) -> Tick {
        if self.anchor.elapsed() >= Duration::from_secs(1) {
            self.reanchor();
        }
        // The system clock went back: start again from the frame of now.
        if self.due(self.k) > Instant::now() + self.period * 2 {
            self.k = self.index_at(self.now_tai()) + 1;
        }
        let due = self.due(self.k);
        wait_until(due);
        let late = Instant::now().saturating_duration_since(due);
        let mut skipped = 0;
        if late > self.period * 2 {
            let now = self.index_at(self.now_tai());
            skipped = now.saturating_sub(self.k);
            self.k = now;
        }
        let at = SystemTime::UNIX_EPOCH + self.time_of(self.k).saturating_sub(self.tai);
        self.k += 1;
        Tick { at, late, skipped }
    }
}

/// Writes a step the engine took by itself to the as-run log.
fn log_auto(
    state: &Shared,
    ch: &ChannelRt,
    p: &Project,
    action: &str,
    layer: u32,
    template_id: &str,
    rundown_id: Option<&str>,
) {
    let label = rundown_id
        .and_then(|id| p.rundown.iter().find(|r| r.id == id))
        .map(|r| r.name.clone())
        .or_else(|| p.template(template_id).map(|t| t.name.clone()))
        .unwrap_or_default();
    let zone = p
        .zones
        .iter()
        .find(|z| z.layer == layer)
        .map(|z| z.id.clone())
        .unwrap_or_default();
    crate::asrun::record(
        state,
        crate::asrun::Entry {
            at: crate::state::now_ms(),
            channel_id: ch.config.id.clone(),
            channel_name: ch.config.name.clone(),
            project_id: p.id.clone(),
            project_name: p.name.clone(),
            action: action.into(),
            label,
            zone,
            actor: crate::antenna::Actor::auto(match action {
                "auto-next" => "auto next",
                _ => "marker",
            }),
            values: Default::default(),
            detail: String::new(),
        },
    );
}

/// One tick of the channel: timelines, automatic steps, clients told.
async fn advance(
    state: &Shared,
    ch: &ChannelRt,
    saved_at: &mut Instant,
) -> (Option<Arc<Project>>, ChannelState) {
    let channel_fps = ch.config.frame_rate.as_f64().max(1.0);
    // Without waiting on the library (a project being saved).
    let project = ch.loaded.get();
    let pid = project.as_ref().map(|p| p.id.clone());
    // Timelines are authored in project frames: re-time them for the
    // channel rate (a 25p project plays at the right speed on 50p).
    let step = project
        .as_ref()
        .map(|p| p.frame_rate.as_f64() / channel_fps)
        .unwrap_or(1.0);
    let (changed, snapshot) = {
        let mut st = ch.state.write().await;
        let mut changed = st.tick(step);
        if let Some(p) = &project {
            let before: Vec<_> = st
                .layers
                .iter()
                .map(|(l, it)| {
                    (
                        *l,
                        it.phase,
                        it.frame,
                        it.template_id.clone(),
                        it.rundown_id.clone(),
                    )
                })
                .collect();
            if st.auto_resume(p) {
                changed = true;
                // What the engine did by itself, in the as-run log.
                for (layer, phase, frame, template_id, rundown_id) in &before {
                    let now = st.layers.get(layer);
                    let action = match (phase, now.map(|it| it.phase)) {
                        (Phase::Holding, None | Some(Phase::Out | Phase::OutHold)) => "auto-out",
                        (Phase::Holding, Some(Phase::Playing))
                        | (Phase::OutHold, Some(Phase::Out)) => "auto-continue",
                        (Phase::OutHold, None) => "auto-continue",
                        _ => continue,
                    };
                    if now.is_some_and(|it| {
                        it.template_id != *template_id || it.frame < *frame - 1e-6
                    }) {
                        continue;
                    }
                    log_auto(
                        state,
                        ch,
                        p,
                        action,
                        *layer,
                        template_id,
                        rundown_id.as_deref(),
                    );
                }
            }
            // Auto next between the graphics of a playlist.
            if let Some(taken) = st.auto_next_taken(p) {
                for id in &taken {
                    if let Some((layer, it)) = st
                        .layers
                        .iter()
                        .find(|(_, it)| it.rundown_id.as_deref() == Some(id))
                    {
                        log_auto(state, ch, p, "auto-next", *layer, &it.template_id, Some(id));
                    }
                }
                let store = state.data.store();
                crate::state::bind_state(p, &mut st, &store, None, None, false);
                // Items reading their sources "on take" read them now.
                for id in &taken {
                    crate::state::bind_state(p, &mut st, &store, None, Some(id), false);
                }
                changed = true;
            }
            // Countdowns at zero firing their trigger.
            for name in st.zero_triggers(p, &state.config.timezone) {
                crate::asrun::record(
                    state,
                    crate::asrun::Entry {
                        at: crate::state::now_ms(),
                        channel_id: ch.config.id.clone(),
                        channel_name: ch.config.name.clone(),
                        project_id: p.id.clone(),
                        project_name: p.name.clone(),
                        action: "trigger".into(),
                        label: name.clone(),
                        zone: String::new(),
                        actor: crate::antenna::Actor::auto("countdown"),
                        values: Default::default(),
                        detail: String::new(),
                    },
                );
                let _ = st.apply(p, cg_core::playout::Command::Trigger { name });
                changed = true;
            }
            // Pauses and on-air points waiting for an event that happened.
            let fired = {
                static WIDTHS: std::sync::OnceLock<std::sync::Mutex<cg_render::TextWidths>> =
                    std::sync::OnceLock::new();
                let widths = WIDTHS.get_or_init(|| {
                    std::sync::Mutex::new(cg_render::TextWidths::new(state.assets_dir()))
                });
                let mut w = widths.lock().unwrap_or_else(|e| e.into_inner());
                st.event_resumes(p, &state.config.timezone, &mut *w)
            };
            for name in fired {
                crate::asrun::record(
                    state,
                    crate::asrun::Entry {
                        at: crate::state::now_ms(),
                        channel_id: ch.config.id.clone(),
                        channel_name: ch.config.name.clone(),
                        project_id: p.id.clone(),
                        project_name: p.name.clone(),
                        action: "event".into(),
                        label: name,
                        zone: String::new(),
                        actor: crate::antenna::Actor::auto("event"),
                        values: Default::default(),
                        detail: String::new(),
                    },
                );
                changed = true;
            }
            // Rows going on by themselves, values set by hand for a time:
            // the data is read again for them.
            let rows = st.auto_rows(p);
            let freed = st.expire_manual(crate::state::now_ms().max(0) as u64);
            if rows || freed {
                crate::state::bind_state(p, &mut st, &state.data.store(), None, None, false);
                changed = true;
            }
        }
        (changed, st.clone())
    };
    if changed {
        // The state just made, without locking it again.
        state.publish_channel(ch, snapshot.clone());
    }
    // What is on air, on disk at most once a second (a restart puts it back).
    if saved_at.elapsed() >= Duration::from_secs(1) && ch.dirty.swap(false, Ordering::Relaxed) {
        *saved_at = Instant::now();
        let (dir, id, snap) = (
            state.data_dir.clone(),
            ch.config.id.clone(),
            snapshot.clone(),
        );
        tokio::task::spawn_blocking(move || crate::state::save_on_air(&dir, &id, pid, snap));
    }
    (project, snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Slow(Arc<Mutex<Vec<u64>>>);

    impl OutputSink for Slow {
        fn name(&self) -> &str {
            "slow"
        }
        fn push(&mut self, f: &Arc<Frame>, _: SystemTime) -> Result<(), OutputError> {
            std::thread::sleep(Duration::from_millis(30));
            lock(&self.0).push(f.width as u64);
            Ok(())
        }
    }

    /// A slow output gets the newest frame and never holds back the sender.
    #[test]
    fn a_slow_output_takes_the_newest_frame() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let perf = crate::perf::Perf::default();
        let pic = perf.picture("c", "main", "Main", 1, 1);
        let stats = pic.output("o", "ndi", "O");
        let o = OutputThread::start(
            Opened {
                id: "o".into(),
                kind: "ndi",
                name: "O".into(),
                sink: Box::new(Slow(got.clone())),
            },
            stats,
            Arc::new(OutputStatus::default()),
            "c".into(),
        );
        // Sending never waits for the slow sink (the sleeps between are not counted).
        let mut sending = Duration::ZERO;
        for w in 1..=10 {
            let t = Instant::now();
            o.send(Arc::new(Frame::blank(w, 1)), SystemTime::now());
            sending += t.elapsed();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(sending < Duration::from_millis(100), "{sending:?}");
        o.close();
        let got = lock(&got).clone();
        // The first, then whatever was newest each time it was free; the last one always.
        assert_eq!(got.first(), Some(&1));
        assert_eq!(got.last(), Some(&10));
        assert!(got.len() < 10 && got.windows(2).all(|w| w[0] < w[1]));
    }

    /// Frames rendered ahead go out in tick order; a missing one repeats the last.
    #[test]
    fn frames_rendered_ahead_go_out_in_order() {
        let got = Arc::new(Mutex::new(Vec::new()));
        struct Record(Arc<Mutex<Vec<u64>>>);
        impl OutputSink for Record {
            fn name(&self) -> &str {
                "rec"
            }
            fn push(&mut self, f: &Arc<Frame>, _: SystemTime) -> Result<(), OutputError> {
                lock(&self.0).push(f.width as u64);
                Ok(())
            }
        }
        let perf = crate::perf::Perf::default();
        let stats = perf.picture("c", "main", "Main", 1, 1);
        let out = OutputThread::start(
            Opened {
                id: "o".into(),
                kind: "ndi",
                name: "O".into(),
                sink: Box::new(Record(got.clone())),
            },
            stats.output("o", "ndi", "O"),
            Arc::new(OutputStatus::default()),
            "c".into(),
        );
        let (jobs, _rx) = mpsc::sync_channel(1);
        let mut pic = Picture {
            jobs,
            done: Default::default(),
            ahead: 2,
            last: None,
            stats,
            outputs: vec![out],
            mixer: None,
            renders: Vec::new(),
            card: None,
        };
        // Frames of ticks 1, 2 and 4 (3 never came), each `width` wide = its tick.
        for t in [1u64, 2, 4] {
            lock(&pic.done).insert(t, Arc::new(Frame::blank(t as u32, 1)));
        }
        for tick in 3..=6 {
            pic.dispatch(tick, SystemTime::now());
            std::thread::sleep(Duration::from_millis(20));
        }
        pic.close();
        assert_eq!(*lock(&got), vec![1, 2, 2, 4]);
    }

    #[test]
    fn the_clock_waits_precisely() {
        // Measured against a plain sleep to the same deadline, taken in turn:
        // a loaded test machine (a shared CI runner) delays both alike, so
        // this checks the wait itself, not how busy the machine is.
        let late = |wait: &dyn Fn(Instant)| {
            let due = Instant::now() + Duration::from_millis(5);
            wait(due);
            Instant::now() - due
        };
        let pairs: Vec<(Duration, Duration)> = (0..40)
            .map(|_| {
                (
                    late(&|d| wait_until(d)),
                    late(&|d| std::thread::sleep(d.saturating_duration_since(Instant::now()))),
                )
            })
            .collect();
        // Pair by pair, ours is not the later one as a rule: a wait that is
        // late every time (a whole time slice) loses nearly every pair, while
        // the noise of a busy runner falls on both sides alike.
        let lost = pairs
            .iter()
            .filter(|(ours, plain)| *ours > *plain + Duration::from_micros(500))
            .count();
        assert!(
            lost <= 30,
            "{lost} of 40 later than a plain sleep: {pairs:?}"
        );
    }

    #[test]
    fn the_ptp_grid_counts_frames_from_the_epoch() {
        let g = Grid::new(cg_core::FrameRate::new(60000, 1001), 37);
        // Frame k of 59.94 starts at k × 1001/60000 s.
        assert_eq!(g.time_of(60), Duration::from_nanos(1_001_000_000));
        assert_eq!(g.index_at(Duration::from_nanos(1_001_000_000)), 60);
        assert_eq!(g.index_at(Duration::from_nanos(1_000_999_999)), 59);
        // Today's frame numbers do not overflow.
        let now = Duration::from_secs(1_800_000_000);
        let k = g.index_at(now);
        assert!(g.time_of(k) <= now && now < g.time_of(k + 1));
    }

    #[test]
    fn ptp_ticks_fall_on_the_grid() {
        let mut g = Grid::new(cg_core::FrameRate::new(50, 1), 37);
        let tai = Duration::from_secs(37);
        let mut last = None;
        for _ in 0..5 {
            let t = g.next();
            let since = t.at.duration_since(SystemTime::UNIX_EPOCH).unwrap() + tai;
            // Whole multiples of 20 ms in TAI.
            assert_eq!(since.as_nanos() % 20_000_000, 0, "{since:?}");
            if let Some(l) = last {
                assert!(t.at > l);
            }
            last = Some(t.at);
            // Shown when the system clock says so (give or take a busy machine).
            let off = SystemTime::now()
                .duration_since(t.at)
                .unwrap_or_else(|e| e.duration());
            assert!(off < Duration::from_millis(40), "{off:?}");
        }
    }

    /// A card ticking every 10 ms, then stopping.
    struct FakeCard {
        ticks: Mutex<u32>,
    }

    impl FrameClock for FakeCard {
        fn wait(&self, timeout: Duration) -> bool {
            let mut t = lock(&self.ticks);
            if *t == 0 {
                std::thread::sleep(timeout);
                return false;
            }
            *t -= 1;
            std::thread::sleep(Duration::from_millis(10));
            true
        }
        fn locked(&self) -> Option<bool> {
            Some(true)
        }
    }

    #[test]
    fn a_card_drives_the_clock_and_its_absence_does_not_stop_it() {
        let period = Duration::from_millis(200);
        let mut pace = Pace::card(
            Arc::new(FakeCard {
                ticks: Mutex::new(3),
            }),
            period,
        );
        let start = Instant::now();
        let mut last = SystemTime::UNIX_EPOCH;
        for _ in 0..3 {
            let t = pace.next();
            assert!(t.at > last);
            last = t.at;
        }
        // The card's own pace (10 ms), not the channel's (200 ms).
        assert!(
            start.elapsed() < Duration::from_millis(400),
            "{:?}",
            start.elapsed()
        );
        // The card stops: the clock goes on (late, as it waited for the card).
        let t = pace.next();
        assert!(t.at > last);
    }
}
