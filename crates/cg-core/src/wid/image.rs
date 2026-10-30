//! Image: fits (incl. original size and tile), focal point, crop, rounded /
//! circle mask, stroke, colorize, fallback picture, animated sequence.
//! The source and the basic fit stay the layer's `src` / `fit` (older
//! projects); `fitMode` adds Original / Tile on top.
//! Drawn by `cg-render/src/wid/image.rs` and `web/src/widgets/image.tsx` (keep in sync).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ActCtx;
use crate::model::{ImageFit, Layer, LayerKind};

/// The fit when it is not the layer's `fit`: Original or Tile, or any of
/// the five when the Operator chooses it (an exposed Fit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FitMode {
    /// Picture at its own size, centred.
    Original,
    /// Repeated at `tileScale` %.
    Tile,
    Contain,
    Cover,
    Stretch,
}

/// The fit in effect (the layer's `fit` unless `fitMode` is set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
    Stretch,
    Original,
    Tile,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mask {
    #[default]
    None,
    Rounded,
    Circle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Colorize {
    #[default]
    None,
    /// Every opaque pixel takes the colour (one-colour logos).
    Replace,
    /// Greys take the colour (monochrome pictures).
    Tint,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    #[default]
    Loop,
    Once,
    Count,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    /// Plays from the first frame at every take.
    #[default]
    In,
    /// Waits on the first frame: the operator plays it.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Focal {
    pub x: f32,
    pub y: f32,
}
impl Default for Focal {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Fractions of the picture cut on each side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

/// Settings of the widget, flattened into the layer. The stroke colour and
/// width are the animated `stroke` / `strokeWidth`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub fit_mode: Option<FitMode>,
    /// Tile: size of a tile, % of the picture.
    pub tile_scale: f32,
    /// Kept in view when Cover crops (fractions of the picture).
    pub focal: Focal,
    pub crop: Crop,
    pub mask: Mask,
    /// Rounded mask radius, px.
    pub radius: f32,
    pub colorize: Colorize,
    pub tint: String,
    /// Shown when the source cannot be found (empty = the built-in slate).
    pub fallback: String,
    /// Animated sequence: asset paths played instead of `src`.
    pub frames: Vec<String>,
    pub fps: f32,
    pub play_mode: PlayMode,
    pub loops: u32,
    pub start: Start,
    /// A Video layer (the Video widget): a video file played as frames, or
    /// a live input.
    pub video: bool,
    /// The video file the frames come from (asset path), to show its name.
    pub clip: String,
    /// A live video input (cg-server inputs) shown instead ("" = none).
    pub input: String,
    /// In and out points of the sequence (a video file), s at its speed:
    /// only the frames between are played (out 0 = to the end).
    pub clip_in: f32,
    pub clip_out: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fit_mode: None,
            tile_scale: 100.0,
            focal: Focal::default(),
            crop: Crop::default(),
            mask: Mask::None,
            radius: 28.0,
            colorize: Colorize::None,
            tint: "#ff3b5c".into(),
            fallback: String::new(),
            frames: Vec::new(),
            fps: 12.0,
            play_mode: PlayMode::Loop,
            loops: 3,
            start: Start::In,
            video: false,
            clip: String::new(),
            input: String::new(),
            clip_in: 0.0,
            clip_out: 0.0,
        }
    }
}

pub fn fit(c: &Config, legacy: ImageFit) -> Fit {
    match (c.fit_mode, legacy) {
        (Some(FitMode::Original), _) => Fit::Original,
        (Some(FitMode::Tile), _) => Fit::Tile,
        (Some(FitMode::Contain), _) => Fit::Contain,
        (Some(FitMode::Cover), _) => Fit::Cover,
        (Some(FitMode::Stretch), _) => Fit::Stretch,
        (None, ImageFit::Contain) => Fit::Contain,
        (None, ImageFit::Cover) => Fit::Cover,
        (None, ImageFit::Stretch) => Fit::Stretch,
    }
}

/// x, y, w, h.
pub type Rect = [f32; 4];

/// The visible frame in the box and its corner radius (circle = centred
/// circle of the smallest side).
pub fn frame_rect(c: &Config, w: f32, h: f32) -> (Rect, f32) {
    match c.mask {
        Mask::Circle => {
            let d = w.min(h);
            ([(w - d) / 2.0, (h - d) / 2.0, d, d], d / 2.0)
        }
        Mask::Rounded => (
            [0.0, 0.0, w, h],
            c.radius.max(0.0).min(w / 2.0).min(h / 2.0),
        ),
        Mask::None => ([0.0, 0.0, w, h], 0.0),
    }
}

/// The crop, each side capped so that 5 % of the picture is always left.
pub fn crop(c: &Config) -> Crop {
    let k = |v: f32| v.clamp(0.0, 0.95);
    let (l, t) = (k(c.crop.l), k(c.crop.t));
    Crop {
        l,
        t,
        r: c.crop.r.clamp(0.0, (0.95 - l).max(0.0)),
        b: c.crop.b.clamp(0.0, (0.95 - t).max(0.0)),
    }
}

/// Where the (cropped) picture `iw` × `ih` goes in a frame `fw` × `fh`:
/// the visible window and the whole picture, relative to the frame.
pub fn place(
    c: &Config,
    (iw, ih): (f32, f32),
    (fw, fh): (f32, f32),
    fit: Fit,
    cr: Crop,
) -> (Rect, Rect) {
    let (cw, ch) = (iw * (1.0 - cr.l - cr.r), ih * (1.0 - cr.t - cr.b));
    let (cw, ch) = (cw.max(1e-3), ch.max(1e-3));
    let (sx, sy, ox, oy);
    if fit == Fit::Stretch {
        (sx, sy, ox, oy) = (fw / cw, fh / ch, 0.0, 0.0);
    } else {
        let k = match fit {
            Fit::Contain => (fw / cw).min(fh / ch),
            Fit::Cover => (fw / cw).max(fh / ch),
            _ => 1.0,
        };
        let (mut x, mut y) = ((fw - cw * k) / 2.0, (fh - ch * k) / 2.0);
        if fit == Fit::Cover {
            // The focal point goes to the centre, as far as the picture still covers.
            let fx = (c.focal.x - cr.l) * iw * k;
            let fy = (c.focal.y - cr.t) * ih * k;
            // Rounding can make the lower bound a hair above 0: never a reversed range.
            x = (fw / 2.0 - fx).clamp((fw - cw * k).min(0.0), 0.0);
            y = (fh / 2.0 - fy).clamp((fh - ch * k).min(0.0), 0.0);
        }
        (sx, sy, ox, oy) = (k, k, x, y);
    }
    (
        [ox, oy, cw * sx, ch * sy],
        [ox - cr.l * iw * sx, oy - cr.t * ih * sy, iw * sx, ih * sy],
    )
}

/// Operator state of an animated sequence: started by hand, or taken over
/// by the operator (paused, played, restarted) when it starts at the In.
/// Times are wall clock (ms), so playback never drifts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub playing: bool,
    /// When frame 0 would have been shown (playing).
    pub at_ms: i64,
    /// Seconds played when paused.
    pub paused_el: f64,
    /// A sequence starting at the In that the operator paused, played or
    /// restarted: these times replace its time on air.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
    /// Times the operator reloaded the picture (the renderers read it again).
    #[serde(skip_serializing_if = "is_zero")]
    pub reload: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Where a sequence is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seq {
    pub idx: usize,
    /// 1-based loop.
    pub lap: u64,
    /// None = loops forever.
    pub loops: Option<u64>,
    pub ended: bool,
    pub el: f64,
    /// It plays (not paused, not ended).
    pub running: bool,
    /// Seconds left: to the end, or to the end of this loop when it loops forever.
    pub remaining: f64,
}

/// The frames played, between the in and out points: (first, count).
pub fn clip_range(c: &Config) -> (usize, usize) {
    let total = c.frames.len();
    if total == 0 {
        return (0, 0);
    }
    let fps = c.fps.clamp(0.1, 120.0);
    let first = ((c.clip_in.max(0.0) * fps).round() as usize).min(total - 1);
    let end = if c.clip_out > 0.0 {
        ((c.clip_out * fps).round() as usize).clamp(first + 1, total)
    } else {
        total
    };
    (first, end - first)
}

/// The sequence waits for the operator (the layer's manual start, or the
/// older `start: manual`).
pub fn manual(layer: &Layer, c: &Config) -> bool {
    layer.manual_start || c.start == Start::Manual
}

/// The operator drives its clock: started by hand, or taken over.
fn driven(c: &Config, manual: bool, st: &State) -> bool {
    manual || c.start == Start::Manual || st.own
}

/// Where the sequence is; `manual`: it starts by hand (`manual(layer, c)`).
pub fn seq(c: &Config, st: &State, manual: bool, now_ms: i64, age_secs: f64) -> Seq {
    let (first, count) = clip_range(c);
    let n = count.max(1) as u64;
    let el = if !driven(c, manual, st) {
        age_secs
    } else if st.playing {
        (now_ms - st.at_ms) as f64 / 1000.0
    } else {
        st.paused_el
    }
    .max(0.0);
    let loops = match c.play_mode {
        PlayMode::Loop => None,
        PlayMode::Once => Some(1),
        PlayMode::Count => Some(c.loops.max(1) as u64),
    };
    let fps = c.fps.clamp(0.1, 120.0) as f64;
    let mut f = (el * fps).floor() as u64;
    let mut ended = false;
    if let Some(l) = loops {
        if f >= n * l {
            f = n * l - 1;
            ended = true;
        }
    }
    let lap = n as f64 / fps;
    let remaining = match loops {
        Some(l) => (lap * l as f64 - el).max(0.0),
        None => lap - el % lap,
    };
    Seq {
        idx: first + (f % n) as usize,
        lap: f / n + 1,
        loops,
        ended,
        el,
        running: !ended && (!driven(c, manual, st) || st.playing),
        remaining,
    }
}

/// Operator actions of a sequence: `play`, `pause` (it freezes, also when
/// it starts at the In), `restart`; of any picture: `reload`.
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    _args: &Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let LayerKind::Image { x: c, .. } = &layer.kind else {
        return Err("not an image".into());
    };
    let now = ctx.now_ms;
    let s = seq(c, st, manual(layer, c), now, ctx.age_secs);
    let reload = st.reload;
    match op {
        "play" => {
            if s.running {
                return Ok(());
            }
            // Finished: plays again from the start.
            let el = if s.ended { 0.0 } else { s.el };
            st.playing = true;
            st.at_ms = now - (el * 1000.0) as i64;
            st.paused_el = el;
            st.own = true;
        }
        "pause" => {
            if !s.running {
                return Ok(());
            }
            *st = State {
                playing: false,
                at_ms: 0,
                paused_el: s.el,
                own: true,
                reload,
            };
        }
        "restart" => {
            *st = State {
                playing: true,
                at_ms: now,
                paused_el: 0.0,
                own: true,
                reload,
            };
        }
        "reload" => st.reload = st.reload.wrapping_add(1),
        _ => return Err(format!("unknown action `{op}`")),
    }
    Ok(())
}

/// A sequence starting by hand that the operator has not started yet.
pub fn waits_start(layer: &Layer, st: &State) -> bool {
    let LayerKind::Image { x: c, .. } = &layer.kind else {
        return false;
    };
    !c.frames.is_empty() && c.input.trim().is_empty() && manual(layer, c) && !st.own
}

/// Continue (F2): starts a sequence waiting for it (manual start, not on a
/// trigger).
pub fn on_continue(layer: &Layer, st: &mut State, ctx: &ActCtx) -> bool {
    if layer.start_on.as_deref() == Some("trigger") || !waits_start(layer, st) {
        return false;
    }
    act(layer, st, "play", &Value::Null, ctx).is_ok()
}

/// The trigger `name`: starts a sequence waiting for it.
pub fn on_trigger(layer: &Layer, st: &mut State, name: &str, ctx: &ActCtx) -> bool {
    if layer.start_on.as_deref() != Some("trigger")
        || !super::same_trigger(&layer.start_trigger, name)
        || !waits_start(layer, st)
    {
        return false;
    }
    act(layer, st, "play", &Value::Null, ctx).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim::Data;

    fn layer(json: &str) -> Layer {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn old_images_load() {
        let l = layer(
            r#"{"id":"a","name":"I","type":"image","props":{},"src":"/assets/a.png","fit":"cover"}"#,
        );
        let LayerKind::Image { x, fit: f, .. } = &l.kind else {
            panic!()
        };
        assert_eq!(*x, Config::default());
        assert_eq!(fit(x, *f), Fit::Cover);
        assert_eq!(frame_rect(x, 100.0, 50.0), ([0.0, 0.0, 100.0, 50.0], 0.0));
    }

    #[test]
    fn cover_keeps_the_focal_point() {
        let c = Config {
            focal: Focal { x: 1.0, y: 0.5 },
            ..Default::default()
        };
        // 200 × 100 picture in a 100 × 100 frame: scale 1, shifted to the right edge.
        let (win, pic) = place(
            &c,
            (200.0, 100.0),
            (100.0, 100.0),
            Fit::Cover,
            Crop::default(),
        );
        assert_eq!(win[0], -100.0);
        assert_eq!(pic[2], 200.0);
        // Contain of another ratio: the frame does not move, the picture fits in it.
        let (win, _) = place(
            &c,
            (400.0, 100.0),
            (100.0, 100.0),
            Fit::Contain,
            Crop::default(),
        );
        assert_eq!(win, [0.0, 37.5, 100.0, 25.0]);
    }

    #[test]
    fn crop_is_capped() {
        let c = Config {
            crop: Crop {
                l: 0.9,
                r: 0.9,
                t: 0.0,
                b: 0.0,
            },
            ..Default::default()
        };
        let k = crop(&c);
        assert!(k.l + k.r <= 0.95 + 1e-6);
    }

    #[test]
    fn sequence_playback() {
        let l = layer(
            r#"{"id":"a","name":"I","type":"image","props":{},"src":"","frames":["a","b","c"],"fps":10,"playMode":"once","start":"manual"}"#,
        );
        let LayerKind::Image { x: c, .. } = &l.kind else {
            panic!()
        };
        let data = Data::default();
        let ctx = |now_ms| ActCtx {
            now_ms,
            age_secs: 0.0,
            data: &data,
            fps: 50.0,
        };
        let mut st = State::default();
        assert_eq!(seq(c, &st, false, 5000, 9.0).idx, 0);
        act(&l, &mut st, "play", &Value::Null, &ctx(1000)).unwrap();
        assert_eq!(seq(c, &st, false, 1150, 0.0).idx, 1);
        act(&l, &mut st, "pause", &Value::Null, &ctx(1150)).unwrap();
        assert_eq!(seq(c, &st, false, 99_000, 0.0).idx, 1);
        act(&l, &mut st, "play", &Value::Null, &ctx(2000)).unwrap();
        // Once: stays on the last frame.
        let s = seq(c, &st, false, 9000, 0.0);
        assert!(s.ended && s.idx == 2);
        act(&l, &mut st, "restart", &Value::Null, &ctx(9000)).unwrap();
        assert_eq!(seq(c, &st, false, 9000, 0.0).idx, 0);
        assert!(act(&l, &mut st, "jump", &Value::Null, &ctx(0)).is_err());
    }

    #[test]
    fn pause_freezes_a_sequence_started_at_the_in() {
        let l = layer(
            r#"{"id":"a","name":"V","type":"image","props":{},"src":"","frames":["a","b","c","d","e"],"fps":10,"playMode":"once"}"#,
        );
        let LayerKind::Image { x: c, .. } = &l.kind else {
            panic!()
        };
        let data = Data::default();
        let ctx = |now_ms, age_secs| ActCtx {
            now_ms,
            age_secs,
            data: &data,
            fps: 50.0,
        };
        let mut st = State::default();
        // Plays with the time on air: 0.25 s in, frame 2, running, 0.25 s left.
        let s = seq(c, &st, false, 0, 0.25);
        assert_eq!((s.idx, s.running), (2, true));
        assert!((s.remaining - 0.25).abs() < 1e-9);
        act(&l, &mut st, "pause", &Value::Null, &ctx(1000, 0.25)).unwrap();
        // Frozen, whatever the time on air.
        let s = seq(c, &st, false, 50_000, 40.0);
        assert_eq!((s.idx, s.running), (2, false));
        act(&l, &mut st, "play", &Value::Null, &ctx(2000, 1.25)).unwrap();
        assert_eq!(seq(c, &st, false, 2100, 1.35).idx, 3);
        act(&l, &mut st, "reload", &Value::Null, &ctx(2100, 1.35)).unwrap();
        assert_eq!(st.reload, 1);
        act(&l, &mut st, "restart", &Value::Null, &ctx(3000, 2.25)).unwrap();
        assert_eq!((seq(c, &st, false, 3000, 2.25).idx, st.reload), (0, 1));
    }

    #[test]
    fn manual_start_waits_for_continue_or_its_trigger() {
        let l = layer(
            r#"{"id":"a","name":"V","type":"image","props":{},"src":"","frames":["a","b","c"],"fps":10,"playMode":"once","manualStart":true}"#,
        );
        let LayerKind::Image { x: c, .. } = &l.kind else {
            panic!()
        };
        let data = Data::default();
        let ctx = |now_ms| ActCtx {
            now_ms,
            age_secs: 9.0,
            data: &data,
            fps: 50.0,
        };
        let mut st = State::default();
        // Waits on its first frame whatever the time on air.
        assert!(waits_start(&l, &st));
        let s = seq(c, &st, manual(&l, c), 1000, 9.0);
        assert_eq!((s.idx, s.running), (0, false));
        // A trigger does not start it; Continue does, once.
        assert!(!on_trigger(&l, &mut st, "go", &ctx(1000)));
        assert!(on_continue(&l, &mut st, &ctx(1000)));
        assert!(!waits_start(&l, &st));
        assert!(!on_continue(&l, &mut st, &ctx(1100)));
        assert_eq!(seq(c, &st, manual(&l, c), 1150, 9.0).idx, 1);
        // On a trigger: only its own name starts it.
        let l = layer(
            r#"{"id":"a","name":"V","type":"image","props":{},"src":"","frames":["a","b","c"],"fps":10,"playMode":"once","manualStart":true,"startOn":"trigger","startTrigger":"Go"}"#,
        );
        let mut st = State::default();
        assert!(!on_continue(&l, &mut st, &ctx(1000)));
        assert!(!on_trigger(&l, &mut st, "stop", &ctx(1000)));
        assert!(on_trigger(&l, &mut st, "go", &ctx(1000)));
        assert!(st.playing && st.own);
        // The older `start: manual` waits the same way.
        let l = layer(
            r#"{"id":"a","name":"V","type":"image","props":{},"src":"","frames":["a","b"],"fps":10,"start":"manual"}"#,
        );
        let st = State::default();
        assert!(waits_start(&l, &st));
    }

    #[test]
    fn in_and_out_points() {
        let l = layer(
            r#"{"id":"a","name":"V","type":"image","props":{},"src":"","frames":["a","b","c","d","e","f","g","h","i","j"],"fps":10,"playMode":"once","clipIn":0.2,"clipOut":0.5}"#,
        );
        let LayerKind::Image { x: c, .. } = &l.kind else {
            panic!()
        };
        assert_eq!(clip_range(c), (2, 3));
        let st = State::default();
        assert_eq!(seq(c, &st, false, 0, 0.0).idx, 2);
        // Stays on the out point's last frame.
        let s = seq(c, &st, false, 0, 9.0);
        assert!(s.ended && s.idx == 4);
    }
}
