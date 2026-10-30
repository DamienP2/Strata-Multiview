//! Frames kept between renders, so that the engine draws only what moves.
//!
//! A frame is drawn as a list of steps ("ops": a layer, or a layer with its
//! children when they are drawn together, e.g. under a drop shadow). Before
//! drawing, the renderer *plans* the frame: it walks the same layers and asks
//! each one how its picture depends on time ([`Motion`]: not at all, only
//! through a value such as a clock's digits, or every instant). With the
//! state of the graphics on air (their timeline frame, data, operator state:
//! everything but their age), that tells whether a step draws the same
//! pixels as in an earlier frame:
//!
//! - nothing moves and the scene is the one of the frame kept: that frame is
//!   given again (the same `Arc`, so the outputs know it too);
//! - else the picture after the first steps that did not change (the static
//!   layers under a ticker, a lower graphic under one coming in) is kept and
//!   copied, and only the steps after it are drawn on it;
//! - the graphics over the last one that moves (a bug over a ticker, a
//!   clock over a lower third coming in) are drawn apart, on a picture of
//!   their own laid over the frame ([`top_item`]): that picture is kept too,
//!   and laid again while they do not change.
//!
//! Drawing goes on in the same order on the same picture, so frames are the
//! same, pixel for pixel, as drawn whole (drawn whole, the graphics over the
//! last that moves are drawn apart and laid over the frame as well). A layer whose dependence on time is
//! not known is taken as moving. A widget may draw in two steps (the map:
//! its base, then its pulsing markers over it, `Renderer::sub_step`) so that
//! the picture between them can be kept.
//!
//! Only the engine uses this (`Renderer::render_channel_cached`, one cache
//! per channel picture, shared by its render threads);
//! `Renderer::set_cache(false)` draws every frame whole, from scratch.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cg_core::{PlayoutItem, Project};
use tiny_skia::Pixmap;

use crate::{Frame, OutputFormat};

/// How a layer's picture depends on time (see `wid::motion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Motion {
    /// The same whatever the time (for the same state of the graphic).
    Still,
    /// Depends on time only through this value (a clock's digits, a counter
    /// at rest on its value…): the same value, the same pixels.
    Key(u64),
    /// Changes with time, or not known not to.
    Moving,
}

impl Motion {
    /// A key from anything hashable (shown text, numbers as bits…).
    pub fn of<T: Hash + ?Sized>(v: &T) -> Motion {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        v.hash(&mut h);
        Motion::Key(h.finish())
    }

    /// Both together (a layer and its children drawn as one step).
    pub fn and(self, other: Motion) -> Motion {
        match (self, other) {
            (Motion::Moving, _) | (_, Motion::Moving) => Motion::Moving,
            (Motion::Still, m) | (m, Motion::Still) => m,
            (Motion::Key(a), Motion::Key(b)) => Motion::of(&(a, b)),
        }
    }
}

/// One step of a frame, as planned.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Op {
    pub motion: Motion,
    /// Steps of the layer's subtree, its own included (1 for a leaf).
    pub span: usize,
    /// The graphic on air it belongs to (index in [`Scene::items`]).
    pub item: usize,
}

/// What a frame is drawn from, its time aside.
#[derive(Clone)]
pub(crate) struct Scene {
    pub project: Arc<Project>,
    pub out: OutputFormat,
    pub tz: String,
    /// Font database and picture cache versions of the renderer.
    pub fonts: u64,
    pub assets: u64,
    /// The graphics on air (playout layer, state with age and hold time zeroed:
    /// they matter only through the motion keys).
    pub items: Vec<(u32, PlayoutItem)>,
    pub ops: Vec<Op>,
}

impl Scene {
    fn same_frame(&self, o: &Scene) -> bool {
        Arc::ptr_eq(&self.project, &o.project)
            && self.out == o.out
            && self.tz == o.tz
            && self.fonts == o.fonts
            && self.assets == o.assets
    }

    /// Nothing in it moves.
    pub fn still(&self) -> bool {
        !self.ops.iter().any(|o| o.motion == Motion::Moving)
    }

    /// The first `n` steps are the same as in `o` (and do not move).
    fn same_start(&self, o: &Scene, n: usize) -> bool {
        if n > self.ops.len() || n > o.ops.len() || !self.same_frame(o) {
            return false;
        }
        let a = &self.ops[..n];
        let b = &o.ops[..n];
        if a.iter().zip(b).any(|(x, y)| {
            x.motion == Motion::Moving
                || y.motion == Motion::Moving
                || x.motion != y.motion
                || x.item != y.item
        }) {
            return false;
        }
        // The graphics those steps belong to.
        let items = a.last().map_or(0, |o| o.item + 1);
        self.items.len() >= items
            && o.items.len() >= items
            && self.items[..items] == o.items[..items]
    }

    /// The whole frame is the same as `o`'s.
    fn same(&self, o: &Scene) -> bool {
        self.ops.len() == o.ops.len()
            && self.items == o.items
            && self.still()
            && o.still()
            && self.same_start(o, self.ops.len())
    }

    /// How many first steps are the same as in `o` (and do not move).
    fn common(&self, o: &Scene) -> usize {
        if !self.same_frame(o) {
            return 0;
        }
        let mut n = 0;
        for (i, (x, y)) in self.ops.iter().zip(&o.ops).enumerate() {
            if x.motion == Motion::Moving
                || y.motion == Motion::Moving
                || x.motion != y.motion
                || x.item != y.item
                || self.items.get(x.item) != o.items.get(y.item)
            {
                break;
            }
            n = i + 1;
        }
        n
    }
}

/// The first of the graphics over the last one that moves (none: nothing
/// moves, or nothing is over what moves): those are drawn on a picture of
/// their own, laid over the frame (and kept while they are the same).
pub(crate) fn top_item(ops: &[Op], items: usize) -> Option<usize> {
    let last = ops
        .iter()
        .filter(|o| o.motion == Motion::Moving)
        .map(|o| o.item)
        .max()?;
    (last + 1 < items && ops.iter().any(|o| o.item > last)).then_some(last + 1)
}

impl Scene {
    /// Graphics `t..` and their steps are the same as in `o` (none moving).
    fn same_top(&self, o: &Scene, t: usize) -> bool {
        if !self.same_frame(o) || self.items.len() != o.items.len() || t > self.items.len() {
            return false;
        }
        if self.items[t..] != o.items[t..] {
            return false;
        }
        let mine = self.ops.iter().filter(|x| x.item >= t);
        let theirs = o.ops.iter().filter(|x| x.item >= t);
        mine.clone().count() == theirs.clone().count()
            && mine.zip(theirs).all(|(x, y)| {
                x.motion != Motion::Moving
                    && x.motion == y.motion
                    && x.item == y.item
                    && x.span == y.span
            })
    }
}

/// The picture of the graphics over what moves (`top_item`), in pieces
/// (`scaled::tiles`).
struct Top {
    scene: Scene,
    item: usize,
    picture: Arc<crate::scaled::Tiles>,
}

/// The picture after the first steps of a scene.
struct Start {
    scene: Scene,
    /// Steps drawn in it.
    at: usize,
    picture: Arc<Pixmap>,
}

/// The frames kept for one picture of a channel, shared by its render
/// threads (frames are rendered ahead, in parallel: whichever thread draws
/// a frame, it is the same for the same scene).
#[derive(Default)]
pub struct FrameCache {
    /// The last frame where nothing moved.
    still: Mutex<Option<(Scene, Arc<Frame>)>>,
    /// The picture under what moves.
    start: Mutex<Option<Start>>,
    /// The scene planned last (where the next one starts to differ).
    last: Mutex<Option<Scene>>,
    /// The picture of the graphics over what moves.
    top: Mutex<Option<Top>>,
    /// Frames where that picture was laid again.
    topped: AtomicU64,
    /// Frames given again, frames drawn on a kept picture.
    reused: AtomicU64,
    started: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// What the renderer does with a planned frame.
pub(crate) enum Plan {
    /// The frame kept: nothing to draw.
    Same(Arc<Frame>),
    /// Draw from step `from` (0: from a blank picture, else on `picture`),
    /// keeping the picture before step `keep` (when Some) for the next frames.
    Draw {
        from: usize,
        picture: Option<Arc<Pixmap>>,
        keep: Option<usize>,
    },
}

impl FrameCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Frames given again whole, and frames drawn on a kept picture of
    /// their first layers, so far.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.reused.load(Ordering::Relaxed),
            self.started.load(Ordering::Relaxed),
        )
    }

    /// Frames where the kept picture of the graphics over what moves was
    /// laid again, so far.
    pub fn tops(&self) -> u64 {
        self.topped.load(Ordering::Relaxed)
    }

    /// Forgets everything kept (frees the pictures).
    pub fn clear(&self) {
        *lock(&self.still) = None;
        *lock(&self.start) = None;
        *lock(&self.last) = None;
        *lock(&self.top) = None;
    }

    /// The kept picture of graphics `t..` of `scene` (and its place), when
    /// they are the same as when it was drawn.
    pub(crate) fn top(&self, scene: &Scene, t: usize) -> Option<Arc<crate::scaled::Tiles>> {
        let top = lock(&self.top);
        let top = top.as_ref()?;
        if top.item != t || !scene.same_top(&top.scene, t) {
            return None;
        }
        self.topped.fetch_add(1, Ordering::Relaxed);
        Some(top.picture.clone())
    }

    /// Keeps the picture of graphics `t..` of `scene` (its pieces).
    pub(crate) fn keep_top(&self, scene: &Scene, t: usize, picture: Arc<crate::scaled::Tiles>) {
        *lock(&self.top) = Some(Top {
            scene: scene.clone(),
            item: t,
            picture,
        });
    }

    pub(crate) fn plan(&self, scene: &Scene) -> Plan {
        if scene.still() {
            if let Some((s, f)) = lock(&self.still).as_ref() {
                if s.same(scene) {
                    self.reused.fetch_add(1, Ordering::Relaxed);
                    return Plan::Same(f.clone());
                }
            }
        }
        let (from, picture) = match lock(&self.start).as_ref() {
            Some(st) if st.at > 0 && scene.same_start(&st.scene, st.at) => {
                self.started.fetch_add(1, Ordering::Relaxed);
                (st.at, Some(st.picture.clone()))
            }
            _ => (0, None),
        };
        // Where this frame starts to differ from the previous one: the
        // picture before it is kept, for the next frames (only once the
        // start has been the same twice: no copy while all moves).
        let diff = lock(&self.last)
            .replace(scene.clone())
            .map_or(0, |last| scene.common(&last));
        let keep = (diff > from && diff < scene.ops.len()).then_some(diff);
        Plan::Draw {
            from,
            picture,
            keep,
        }
    }

    /// The steps of graphic `i` of `scene` (its ops not planned yet) when
    /// it was planned in the scene before, the same, with none of its steps
    /// depending on time: they are the same (no need to walk its layers).
    pub(crate) fn still_steps(&self, scene: &Scene, i: usize) -> Option<Vec<Op>> {
        let last = lock(&self.last);
        let last = last.as_ref()?;
        if !last.same_frame(scene) {
            return None;
        }
        let item = scene.items.get(i)?;
        let j = last.items.iter().position(|x| x == item)?;
        let ops: Vec<Op> = last
            .ops
            .iter()
            .filter(|o| o.item == j)
            .map(|o| Op { item: i, ..*o })
            .collect();
        ops.iter().all(|o| o.motion == Motion::Still).then_some(ops)
    }

    /// The picture before step `at` of `scene`, for the next frames.
    pub(crate) fn keep_start(&self, scene: &Scene, at: usize, picture: Pixmap) {
        let mut s = scene.clone();
        s.ops.truncate(at);
        *lock(&self.start) = Some(Start {
            scene: s,
            at,
            picture: Arc::new(picture),
        });
    }

    /// A frame where nothing moves, given again while the scene is the same.
    pub(crate) fn keep_still(&self, scene: &Scene, frame: Arc<Frame>) {
        *lock(&self.still) = Some((scene.clone(), frame));
    }
}
