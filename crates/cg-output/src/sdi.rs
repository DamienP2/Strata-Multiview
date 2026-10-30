//! SDI through Blackmagic DeckLink cards: outputs (fill + key) and inputs.
//!
//! The DeckLink SDK is reached through a small C++ shim (`decklink/`,
//! bindings in [`crate::decklink`]) built with the `decklink` feature. The
//! runtime, Blackmagic Desktop Video, is loaded when a card is used: without
//! it, or in a build without the shim, opening a device fails with
//! `OutputError::Unsupported` and the rest of the server keeps working.
//!
//! Output: frames are handed over as BGRA 8-bit premultiplied
//! (bmdFormat8BitBGRA), which the card turns into fill (and key, through its
//! keyer: external = fill on one SDI, key on the next; internal = the fill
//! keyed over the card's SDI input). They are scheduled a few frames ahead
//! of the card's playback clock from a sender thread of their own, the
//! newest frame winning, so the channel never waits on the driver. That
//! clock, locked to the card's reference input when one is connected, is
//! offered to the channel as its [`FrameClock`] when the output is set to
//! genlock.
//!
//! Input: the card captures 8-bit 4:2:2 (UYVY) and follows the incoming
//! format when it can detect it; [`DeckLinkInput::take`] converts the newest
//! frame to RGBA (BT.709).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use cg_render::Frame;
use serde::{Deserialize, Serialize};

use crate::decklink::{InputHandle, Keyer, OutputHandle, Pushed};
use crate::{FrameClock, OutputError, OutputSink};

pub use crate::decklink::devices;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiConfig {
    #[serde(default)]
    pub id: String,
    /// Off: kept in the settings, not started.
    #[serde(default = "crate::on")]
    pub enabled: bool,
    pub name: String,
    /// The SDI output of the Hardware settings this sends to ("" = none):
    /// the server fills the fields below from it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hardware: String,
    /// DeckLink device index (as listed by Desktop Video).
    pub device: u32,
    /// Use the card's external keyer (fill on SDI A, key on SDI B); off:
    /// the card's internal keyer over its SDI input.
    #[serde(default = "yes")]
    pub external_key: bool,
    /// Fill only, no key.
    #[serde(default)]
    pub fill_only: bool,
    /// The card is wired to the house reference (genlock). A channel with
    /// the DeckLink clock reference runs on the card's frame clock (see
    /// [`OutputSink::frame_clock`]).
    #[serde(default)]
    pub genlock: bool,
    /// Genlock offset in lines (the card's reference input timing offset).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub genlock_offset: i32,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

impl SdiConfig {
    pub fn keyer(&self) -> Keyer {
        if self.fill_only {
            Keyer::None
        } else if self.external_key {
            Keyer::External
        } else {
            Keyer::Internal
        }
    }
}

/// This build drives DeckLink cards (the shim is built in; the cards still
/// need Blackmagic Desktop Video at run time).
pub const SUPPORTED: bool = crate::decklink::BUILT;

fn yes() -> bool {
    true
}

/// The frame waiting for the sender thread (the newest wins).
#[derive(Default)]
struct Slot {
    state: Mutex<SlotState>,
    ready: Condvar,
}

#[derive(Default)]
struct SlotState {
    /// The newest frame, with the time it shows.
    frame: Option<(Arc<Frame>, SystemTime)>,
    /// Set by the output on drop, or by the thread when it stops.
    closed: bool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct DeckLinkOutput {
    config: SdiConfig,
    slot: Arc<Slot>,
    /// µs the sender thread spent converting and copying frames.
    busy_us: Arc<AtomicU64>,
    clock: Option<Arc<dyn FrameClock>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// The card's frame clock (shares the output; the card is closed once the
/// output and every clock are gone). Interlaced, it ticks per field: at the
/// card's frame boundary, then half a frame later (the channel renders one
/// image a field).
struct CardClock {
    card: Arc<OutputHandle>,
    /// Half a card frame (interlaced), None: progressive.
    field: Option<Duration>,
    /// The second field of the frame begun at this instant is still to come.
    second: Mutex<Option<Instant>>,
}

impl FrameClock for CardClock {
    fn wait(&self, timeout: Duration) -> bool {
        if let Some(field) = self.field {
            if let Some(at) = lock(&self.second).take() {
                let due = at + field;
                let now = Instant::now();
                if due > now {
                    std::thread::sleep(due - now);
                }
                return true;
            }
            let ticked = self.card.wait(timeout);
            if ticked {
                *lock(&self.second) = Some(Instant::now());
            }
            return ticked;
        }
        self.card.wait(timeout)
    }
    fn locked(&self) -> Option<bool> {
        self.card.reference_locked()
    }
}

impl DeckLinkOutput {
    /// Opens the device's output in the mode matching the channel format
    /// (width × height at `rate` frames per second, as num/den).
    /// `interlaced`: `rate` counts fields (50 for 1080i50); the card plays
    /// half as many frames, each woven from two images in a row.
    pub fn open(
        config: SdiConfig,
        width: u32,
        height: u32,
        rate: (u32, u32),
        interlaced: bool,
    ) -> Result<Self, OutputError> {
        let offset = config.genlock.then_some(config.genlock_offset);
        let card_rate = if interlaced {
            (rate.0, rate.1.saturating_mul(2))
        } else {
            rate
        };
        let handle = OutputHandle::open(
            config.device,
            width,
            height,
            card_rate,
            config.keyer(),
            offset,
            interlaced,
        )
        .map_err(OutputError::Unsupported)?;
        let handle = Arc::new(handle);
        tracing::info!(
            name = config.name,
            device = config.device,
            mode = handle.mode.name,
            keyer = ?config.keyer(),
            genlock = config.genlock,
            "DeckLink output started"
        );
        // Offered always: a channel with the DeckLink reference follows it
        // (locked to the house reference when one comes into the card).
        let field = interlaced
            .then(|| Duration::from_secs_f64(card_rate.1 as f64 / card_rate.0.max(1) as f64 / 2.0));
        let clock = Some(Arc::new(CardClock {
            card: handle.clone(),
            field,
            second: Mutex::new(None),
        }) as Arc<dyn FrameClock>);
        // Interlaced: which field comes first in time.
        let weave = field.map(|f| Weave {
            field: f,
            lower_first: handle.mode.field == crate::decklink::FIELD_LOWER,
        });
        let slot = Arc::new(Slot::default());
        let busy_us = Arc::new(AtomicU64::new(0));
        let thread = {
            let (slot, busy, name) = (slot.clone(), busy_us.clone(), config.name.clone());
            let opaque = config.fill_only;
            std::thread::Builder::new()
                .name(format!("sdi-{}", config.name))
                .spawn(move || send(name, handle, slot, busy, opaque, weave))
                .map_err(OutputError::Io)?
        };
        Ok(Self {
            config,
            slot,
            busy_us,
            clock,
            thread: Some(thread),
        })
    }

    /// RGBA → BGRA swizzle, the native DeckLink pixel format.
    pub fn to_bgra(frame: &Frame, out: &mut Vec<u8>) {
        swizzle(&frame.data, out, false);
    }
}

/// RGBA → BGRA; `opaque`: alpha forced to 255 (fill only: the premultiplied
/// fill is then the picture over black).
fn swizzle(rgba: &[u8], out: &mut Vec<u8>, opaque: bool) {
    out.resize(rgba.len() / 4 * 4, 0);
    swizzle_into(rgba, out, opaque);
}

fn swizzle_into(rgba: &[u8], out: &mut [u8], opaque: bool) {
    let dst = out.as_chunks_mut::<4>().0;
    for (d, s) in dst.iter_mut().zip(rgba.as_chunks::<4>().0) {
        *d = [s[2], s[1], s[0], if opaque { 255 } else { s[3] }];
    }
}

/// Interlaced output: the field time, and the field order.
#[derive(Clone, Copy)]
struct Weave {
    field: Duration,
    lower_first: bool,
}

/// Two images a field apart woven into one frame (BGRA): the rows of the
/// field shown first from `first`, the others from `second`.
fn weave_into(first: &Frame, second: &Frame, lower_first: bool, opaque: bool, out: &mut Vec<u8>) {
    out.resize(first.data.len(), 0);
    let w = first.width as usize * 4;
    for (y, row) in out.chunks_exact_mut(w).enumerate() {
        // Row 0 belongs to the upper field.
        let upper = y % 2 == 0;
        let from = if upper != lower_first { first } else { second };
        swizzle_into(&from.data[y * w..(y + 1) * w], row, opaque);
    }
}

fn send(
    name: String,
    card: Arc<OutputHandle>,
    slot: Arc<Slot>,
    busy: Arc<AtomicU64>,
    opaque: bool,
    weave: Option<Weave>,
) {
    let mut bgra = Vec::new();
    let mut last: Option<Arc<Frame>> = None;
    // Interlaced: the image of the first field, waiting for the second.
    let mut pending: Option<(Arc<Frame>, SystemTime)> = None;
    let mut woven: Option<(Arc<Frame>, Arc<Frame>)> = None;
    let (mut failed, mut warned_at) = (0u32, None::<Instant>);
    loop {
        let frame = {
            let mut s = lock(&slot.state);
            loop {
                if s.closed {
                    break None;
                }
                if let Some(f) = s.frame.take() {
                    break Some(f);
                }
                s = slot.ready.wait(s).unwrap_or_else(|p| p.into_inner());
            }
        };
        let Some((frame, at)) = frame else { break };
        let start = Instant::now();
        if frame.width != card.width || frame.height != card.height {
            if warned_at.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
                tracing::warn!(
                    name,
                    "frame {}x{} does not match the SDI mode {}",
                    frame.width,
                    frame.height,
                    card.mode.name
                );
                warned_at = Some(Instant::now());
            }
            continue;
        }
        if let Some(wv) = weave {
            // The next field after the image waiting: one frame of both.
            let follows = pending.as_ref().is_some_and(|(_, t)| {
                at.duration_since(*t)
                    .is_ok_and(|d| d > wv.field / 2 && d < wv.field * 3 / 2)
            });
            if !follows {
                pending = Some((frame, at));
                continue;
            }
            let (first, _) = pending.take().expect("pending field");
            // The same two pictures as the frame before (nothing moved): woven already.
            let same = woven
                .as_ref()
                .is_some_and(|(a, b)| Arc::ptr_eq(a, &first) && Arc::ptr_eq(b, &frame));
            if !same {
                weave_into(&first, &frame, wv.lower_first, opaque, &mut bgra);
                woven = Some((first, frame.clone()));
            }
        } else if !last.as_ref().is_some_and(|l| Arc::ptr_eq(l, &frame)) {
            // The same picture as the last one (nothing moved): converted already.
            // Else only the rows that changed since the last one.
            match crate::changed_rows(&frame, last.as_deref()) {
                Some(rows) if bgra.len() == frame.data.len() => {
                    let w = frame.width as usize * 4;
                    for (y, _) in rows.iter().enumerate().filter(|(_, c)| **c) {
                        let r = y * w..(y + 1) * w;
                        swizzle_into(&frame.data[r.clone()], &mut bgra[r], opaque);
                    }
                }
                _ => swizzle(&frame.data, &mut bgra, opaque),
            }
            last = Some(frame.clone());
        }
        match card.push(&bgra, card.width as usize * 4) {
            Pushed::Queued | Pushed::Dropped => failed = 0,
            Pushed::Failed => {
                failed += 1;
                if failed == 1 || failed % 250 == 0 {
                    tracing::warn!(name, failed, "DeckLink refused a frame");
                }
            }
        }
        busy.fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
    }
    lock(&slot.state).closed = true;
    tracing::info!(name, "DeckLink output stopped");
}

impl OutputSink for DeckLinkOutput {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn push(&mut self, frame: &Arc<Frame>, at: SystemTime) -> Result<(), OutputError> {
        let mut s = lock(&self.slot.state);
        if s.closed {
            return Err(OutputError::Closed);
        }
        let replaced = s.frame.replace((frame.clone(), at)).is_some();
        drop(s);
        self.slot.ready.notify_one();
        if replaced {
            Err(OutputError::Dropped)
        } else {
            Ok(())
        }
    }

    fn work_time(&self) -> Option<Duration> {
        Some(Duration::from_micros(self.busy_us.load(Ordering::Relaxed)))
    }

    fn frame_clock(&self) -> Option<Arc<dyn FrameClock>> {
        self.clock.clone()
    }
}

impl Drop for DeckLinkOutput {
    fn drop(&mut self) {
        lock(&self.slot.state).closed = true;
        self.slot.ready.notify_one();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// An SDI input of a DeckLink card.
pub struct DeckLinkInput {
    card: InputHandle,
    uyvy: Vec<u8>,
}

impl DeckLinkInput {
    /// Opens the device's input in the mode matching width × height at
    /// `rate` (frames; an interlaced mode when `interlaced`); the card then
    /// follows the incoming format if it can.
    pub fn open(
        device: u32,
        width: u32,
        height: u32,
        rate: (u32, u32),
        interlaced: bool,
    ) -> Result<Self, String> {
        let card = InputHandle::open(device, width, height, rate, interlaced)?;
        tracing::info!(device, mode = card.mode.name, "DeckLink input started");
        Ok(Self {
            card,
            uyvy: Vec::new(),
        })
    }

    /// The newest captured picture, as (width, height, RGBA), when newer
    /// than `*seq` (updated). Start with `*seq = 0`.
    pub fn take(&mut self, seq: &mut u64) -> Option<(u32, u32, Vec<u8>)> {
        let (w, h) = self.card.take(&mut self.uyvy, seq)?;
        let mut rgba = Vec::new();
        crate::colorspace::uyvy_to_rgba709_par(&self.uyvy, w as usize, h as usize, &mut rgba);
        Some((w, h, rgba))
    }

    /// A signal is coming in (None: nothing received yet).
    pub fn signal(&self) -> Option<bool> {
        self.card.signal()
    }

    /// Waits up to `timeout_ms` for a frame newer than `seq` (woken as soon
    /// as the card hands one over).
    pub fn wait(&self, seq: u64, timeout_ms: u32) -> bool {
        self.card.wait(seq, timeout_ms)
    }

    /// The display mode captured: the one the card detected in the
    /// signal, else the one it was opened in.
    pub fn mode(&self) -> crate::decklink::DisplayMode {
        self.card.current_mode()
    }

    /// The card follows the incoming format by itself.
    pub fn detects(&self) -> bool {
        self.card.detects
    }

    /// The two fields of an interlaced picture (RGBA), each made a whole
    /// picture (a missing line is the mean of the lines around it), in the
    /// order they were shot: a 1080i50 signal gives 50 pictures a second.
    pub fn fields(rgba: &[u8], width: u32, height: u32, lower_first: bool) -> [Vec<u8>; 2] {
        let (w, h) = (width as usize * 4, height as usize);
        if h < 2 || rgba.len() < w * h {
            return [rgba.to_vec(), rgba.to_vec()];
        }
        let field = |parity: usize| {
            let mut out = vec![0u8; w * h];
            for (y, row) in out.chunks_exact_mut(w).enumerate() {
                if y % 2 == parity {
                    row.copy_from_slice(&rgba[y * w..(y + 1) * w]);
                    continue;
                }
                let up = if y > 0 { y - 1 } else { y + 1 };
                let down = if y + 1 < h { y + 1 } else { y - 1 };
                let (a, b) = (&rgba[up * w..(up + 1) * w], &rgba[down * w..(down + 1) * w]);
                for ((o, a), b) in row.iter_mut().zip(a).zip(b) {
                    *o = (*a as u16 + *b as u16).div_ceil(2) as u8;
                }
            }
            out
        };
        if lower_first {
            [field(1), field(0)]
        } else {
            [field(0), field(1)]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_interlaced_picture_splits_into_its_fields() {
        // Rows 0 and 2 (upper field) = 10, rows 1 and 3 = 30.
        let mut rgba = Vec::new();
        for v in [10u8, 30, 10, 30] {
            rgba.extend([v; 4]);
        }
        let [a, b] = DeckLinkInput::fields(&rgba, 1, 4, false);
        let rows = |p: &[u8]| p.chunks(4).map(|x| x[0]).collect::<Vec<_>>();
        // Upper first: its own lines, the others between them.
        assert_eq!(rows(&a), [10, 10, 10, 10]);
        assert_eq!(rows(&b), [30, 30, 30, 30]);
        let [a, _] = DeckLinkInput::fields(&rgba, 1, 4, true);
        assert_eq!(rows(&a), [30, 30, 30, 30]);
    }

    #[test]
    fn fields_are_woven_in_time_order() {
        let img = |v: u8| {
            let mut f = Frame::blank(1, 4);
            f.data.copy_from_slice(&[v; 16]);
            f
        };
        let (a, b) = (img(10), img(20));
        let mut out = Vec::new();
        // Upper field first (1080i50): rows 0 and 2 from the earlier image.
        weave_into(&a, &b, false, false, &mut out);
        let rows: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        assert_eq!(rows, [10, 20, 10, 20]);
        // Lower field first (525i): rows 1 and 3 from the earlier image.
        weave_into(&a, &b, true, false, &mut out);
        let rows: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        assert_eq!(rows, [20, 10, 20, 10]);
    }

    #[test]
    fn bgra_swizzle() {
        let mut out = Vec::new();
        swizzle(&[1, 2, 3, 4, 5, 6, 7, 0], &mut out, false);
        assert_eq!(out, [3, 2, 1, 4, 7, 6, 5, 0]);
        swizzle(&[1, 2, 3, 4, 5, 6, 7, 0], &mut out, true);
        assert_eq!(out, [3, 2, 1, 255, 7, 6, 5, 255]);
        let mut f = Frame::blank(2, 1);
        f.data.copy_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
        DeckLinkOutput::to_bgra(&f, &mut out);
        assert_eq!(out, [30, 20, 10, 40, 70, 60, 50, 80]);
    }

    #[test]
    fn keyer_from_config() {
        let mut c = SdiConfig {
            id: String::new(),
            enabled: true,
            name: "sdi".into(),
            hardware: String::new(),
            device: 0,
            external_key: true,
            fill_only: false,
            genlock: false,
            genlock_offset: 0,
        };
        assert_eq!(c.keyer(), Keyer::External);
        c.external_key = false;
        assert_eq!(c.keyer(), Keyer::Internal);
        c.fill_only = true;
        assert_eq!(c.keyer(), Keyer::None);
    }

    #[test]
    fn open_fails_cleanly_without_a_card() {
        // No DeckLink card (or no shim) here: an error, never a panic.
        let c = SdiConfig {
            id: String::new(),
            enabled: true,
            name: "sdi".into(),
            hardware: String::new(),
            device: 0,
            external_key: true,
            fill_only: false,
            genlock: true,
            genlock_offset: 0,
        };
        if devices().map(|d| d.is_empty()).unwrap_or(true) {
            assert!(matches!(
                DeckLinkOutput::open(c, 1920, 1080, (25, 1), false),
                Err(OutputError::Unsupported(_))
            ));
            assert!(DeckLinkInput::open(0, 1920, 1080, (25, 1), false).is_err());
        }
    }
}
