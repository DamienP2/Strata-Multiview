//! Playout state machine for an output channel.
//!
//! A channel holds several *playout layers* (z-ordered); each one can carry a
//! graphic on air. Take in plays to the on-air marker, stopping at the pause
//! markers on the way; take out plays from the on-air marker to the end,
//! stopping at the pauses after it, before the graphic is removed. A pause
//! resumes on Continue, after its delay, or on its trigger.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::anim::Data;
use crate::model::{
    Layer, LayerKind, MarkerKind, Project, Resume, RowChange, RundownItem, Template, TriggerEvent,
    ValueChange, ZoneTransition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Moving toward `target`.
    Playing,
    /// Holding on a pause or on the on-air marker.
    Holding,
    /// Playing the out animation; removed when it reaches the end.
    Out,
    /// Holding on a pause of the out animation.
    OutHold,
}

/// A take waiting for the graphic on air in its zone to finish going out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedTake {
    pub template_id: String,
    pub data: Data,
    #[serde(default)]
    pub rundown_id: Option<String>,
    /// First hold frame of the template.
    pub target: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayoutItem {
    pub template_id: String,
    pub data: Data,
    pub frame: f64,
    pub target: f64,
    pub phase: Phase,
    /// Rundown item that triggered it, if any (for operator tally).
    #[serde(default)]
    pub rundown_id: Option<String>,
    /// Frames spent on air since the take, including holds (drives tickers,
    /// rolls and timers).
    #[serde(default)]
    pub age: f64,
    /// Comes in once this graphic has finished going out (sequential zones).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<QueuedTake>,
    /// Widgets started, paused or reset by the operator, by layer id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clocks: BTreeMap<String, LayerClock>,
    /// Frames spent holding at the current stop (timed pauses).
    #[serde(default)]
    pub held: f64,
    /// Out animation: `target` is a pause, not the end.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pause_ahead: bool,
    /// The graphic as it was before a data set row change, fading out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade: Option<Fade>,
    /// Values a data source changed, moving in the layers showing them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
    /// What the operator changed in the spec widgets (`wid`), by layer id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub widgets: BTreeMap<String, serde_json::Value>,
    /// Auto next has taken the graphic after this one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub advanced: bool,
    /// The graphic this one replaced in a « Fade » zone, fading out under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaving: Option<Box<Leaving>>,
    /// What its Triggers watch, by node, since the take: a value as it was
    /// (it changes), another graphic seen on air (it goes out), an action
    /// done (it acts once each time its input becomes true).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub watch: BTreeMap<String, String>,
}

/// Text widths for the events that need them (a crawl's passes): the
/// renderer's fonts on the server.
pub trait TextWidth {
    fn width(
        &mut self,
        family: &str,
        weight: u16,
        text: &str,
        size: f32,
        letter_spacing: f32,
    ) -> f32;
}

/// Without fonts: a rough width.
pub struct EstimatedWidth;

impl TextWidth for EstimatedWidth {
    fn width(
        &mut self,
        _family: &str,
        _weight: u16,
        text: &str,
        size: f32,
        letter_spacing: f32,
    ) -> f32 {
        let n = text.chars().count() as f32;
        n * (size * 0.56 + letter_spacing)
    }
}

/// A graphic replaced in a « Fade » zone: drawn under the new one while it fades out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leaving {
    pub item: PlayoutItem,
    /// Frames since the new graphic was taken.
    pub frame: f64,
    pub length: f64,
}

impl Leaving {
    /// Opacity of the graphic going, 1 → 0.
    pub fn opacity(&self) -> f32 {
        (1.0 - self.frame / self.length.max(1.0)).clamp(0.0, 1.0) as f32
    }
}

/// A cross-fade from the data the graphic had (drawn fading out above it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fade {
    pub data: Data,
    /// Frames since the change.
    pub frame: f64,
    pub length: f64,
}

/// A value a data source changed on air, moving in the layers showing it
/// (slide, flip, count: see [`crate::model::ValueChange`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub key: String,
    pub kind: ValueChange,
    /// The layer moving (slide, flip); empty: every layer showing the value.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layer: String,
    /// The value before.
    pub from: String,
    /// Frames since the change.
    pub frame: f64,
    pub length: f64,
}

impl Change {
    /// How far it is, 0 → 1, eased out (fast, then settling).
    pub fn eased(&self) -> f64 {
        let k = (self.frame / self.length.max(1.0)).clamp(0.0, 1.0);
        1.0 - (1.0 - k).powi(3)
    }
}

/// A number written `s` (spaces and a decimal comma allowed): its value, its
/// decimals and whether the comma is the decimal separator.
fn number(s: &str) -> Option<(f64, usize, bool)> {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}'))
        .collect();
    if t.is_empty()
        || !t
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | ','))
    {
        return None;
    }
    let comma = t.contains(',') && !t.contains('.');
    let v: f64 = t.replace(',', ".").parse().ok()?;
    let dec = t.rsplit_once(['.', ',']).map_or(0, |(_, d)| d.len());
    Some((v, dec, comma))
}

/// What a value counting from `from` to `to` shows `k` (0 → 1) of the way,
/// written as `to` is; `None` when they are not both numbers.
pub fn count_value(from: &str, to: &str, k: f64) -> Option<String> {
    let (a, _, _) = number(from)?;
    let (b, dec, comma) = number(to)?;
    if k >= 1.0 {
        return Some(to.to_string());
    }
    let p = 10f64.powi(dec as i32);
    let v = format!(
        "{:.*}",
        dec,
        ((a + (b - a) * k.clamp(0.0, 1.0)) * p).round() / p
    );
    Some(if comma { v.replace('.', ",") } else { v })
}

/// The data a graphic shows now: its counting values on their way.
pub fn shown_data(item: &PlayoutItem) -> std::borrow::Cow<'_, crate::anim::Data> {
    if !item.changes.iter().any(|c| c.kind == ValueChange::Count) {
        return std::borrow::Cow::Borrowed(&item.data);
    }
    let mut d = item.data.clone();
    for c in item.changes.iter().filter(|c| c.kind == ValueChange::Count) {
        if let Some(v) = d
            .get(&c.key)
            .and_then(|to| count_value(&c.from, to, c.eased()))
        {
            d.insert(c.key.clone(), v);
        }
    }
    std::borrow::Cow::Owned(d)
}

/// A layer as JSON, to look for the data keys it shows ([`layer_shows`]):
/// without its own settings by key.
pub fn layer_json(l: &crate::model::Layer) -> String {
    if l.live_changes.is_empty() {
        return serde_json::to_string(l).unwrap_or_default();
    }
    let mut l = l.clone();
    l.live_changes.clear();
    serde_json::to_string(&l).unwrap_or_default()
}

/// Whether a layer (as JSON, [`layer_json`]) shows the data value `key`: in
/// a text (`{{key}}`) or bound to it.
pub fn layer_shows(json: &str, key: &str) -> bool {
    json.contains(&format!("{{{{{key}}}}}")) || json.contains(&format!("\"{key}\""))
}

/// The values a data source (or a data set following it) just changed on a
/// graphic on air, shown as the layers showing them say
/// ([`crate::model::Layer::live_changes`]): a cross-fade, a slide, a flip, a
/// count, out and in again, at once otherwise. `old` is the data before.
pub fn live_changes(project: &Project, item: &mut PlayoutItem, old: &crate::anim::Data) {
    if matches!(item.phase, Phase::Out | Phase::OutHold) || item.next.is_some() {
        return;
    }
    let Some(t) = project.template(&item.template_id) else {
        return;
    };
    let changed: Vec<(String, String, String)> = item
        .data
        .iter()
        .filter_map(|(k, now)| {
            let was = old.get(k)?;
            (was != now).then(|| (k.clone(), was.clone(), now.clone()))
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    // Set on the variable, before it was the layers'.
    let on_binding = |key: &str| {
        let b = t.fields.iter().find(|f| f.key == key)?.binding.as_ref()?;
        Some(crate::model::LiveChange {
            kind: b.change?,
            secs: b.change_secs,
        })
    };
    let fps = project.frame_rate.as_f64().max(1.0);
    let mut fade: Option<f64> = None;
    let mut out_in = false;
    let mut counted: Vec<&str> = Vec::new();
    for l in &t.layers {
        if matches!(l.kind, crate::model::LayerKind::Group { .. }) {
            continue;
        }
        let json = layer_json(l);
        for (key, was, now) in &changed {
            if !layer_shows(&json, key) {
                continue;
            }
            let Some(lc) = l.live_changes.get(key).copied().or_else(|| on_binding(key)) else {
                continue;
            };
            let length = (lc.secs.unwrap_or(0.6).clamp(0.05, 10.0) * fps)
                .round()
                .max(1.0);
            match lc.kind {
                ValueChange::Cut => {}
                // The value on air stays (until a take or an update).
                ValueChange::Hold => {
                    item.data.insert(key.clone(), was.clone());
                }
                ValueChange::OutIn => out_in = true,
                ValueChange::Fade => fade = Some(fade.map_or(length, |l| l.max(length))),
                ValueChange::Count => {
                    // Not numbers: a cross-fade.
                    if number(was).is_none() || number(now).is_none() {
                        fade = Some(fade.map_or(length, |l| l.max(length)));
                        continue;
                    }
                    if counted.contains(&key.as_str()) {
                        continue;
                    }
                    counted.push(key);
                    // From what shows now (a count on its way).
                    let from = item
                        .changes
                        .iter()
                        .find(|c| &c.key == key && c.kind == ValueChange::Count)
                        .and_then(|c| count_value(&c.from, was, c.eased()))
                        .unwrap_or_else(|| was.clone());
                    item.changes
                        .retain(|c| !(&c.key == key && c.kind == ValueChange::Count));
                    item.changes.push(Change {
                        key: key.clone(),
                        kind: ValueChange::Count,
                        layer: String::new(),
                        from,
                        frame: 0.0,
                        length,
                    });
                }
                kind => {
                    item.changes.retain(|c| !(&c.key == key && c.layer == l.id));
                    item.changes.push(Change {
                        key: key.clone(),
                        kind,
                        layer: l.id.clone(),
                        from: was.clone(),
                        frame: 0.0,
                        length,
                    });
                }
            }
        }
    }
    if out_in {
        let take = QueuedTake {
            template_id: item.template_id.clone(),
            data: item.data.clone(),
            rundown_id: item.rundown_id.clone(),
            target: hold_target(t, 0.0),
        };
        let new = std::mem::replace(&mut item.data, old.clone());
        if begin_out(project, item) {
            item.changes.clear();
            item.next = Some(take);
        } else {
            item.data = new;
        }
        return;
    }
    if let Some(length) = fade {
        item.fade = Some(Fade {
            data: old.clone(),
            frame: 0.0,
            length,
        });
    }
}

impl Fade {
    /// Opacity of the old data, 1 → 0.
    pub fn opacity(&self) -> f32 {
        (1.0 - self.frame / self.length.max(1.0)).clamp(0.0, 1.0) as f32
    }
}

/// Own time of a widget layer (timer, ticker, roll…), in frames on air.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerClock {
    /// Subtracted from the item's age.
    #[serde(default)]
    pub offset: f64,
    /// Age at which the widget was paused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_at: Option<f64>,
}

/// What the operator does to a widget's internal animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidgetControl {
    /// Runs (resumes where it was paused).
    Start,
    Pause,
    /// Start when paused, pause when running.
    Toggle,
    /// Back to zero, keeping it running or paused.
    Reset,
    /// Back to zero and running.
    Restart,
}

/// Widgets whose animation follows the time on air.
pub fn is_timed(kind: &LayerKind) -> bool {
    match kind {
        LayerKind::Clock { mode, .. } => {
            matches!(
                mode,
                crate::model::ClockMode::Elapsed | crate::model::ClockMode::Timer
            )
        }
        LayerKind::Text { scroll, .. } => !scroll.is_none(),
        LayerKind::Ticker { .. }
        | LayerKind::Roll { .. }
        | LayerKind::Counter { .. }
        | LayerKind::Progress { .. }
        | LayerKind::Rotator { .. }
        | LayerKind::Typewriter { .. }
        | LayerKind::Slideshow { .. }
        | LayerKind::BarChart { .. } => true,
        // A Lottie animation: the operator pauses, plays and restarts it
        // (its layers then follow the group's clock, see `lottie_frame`).
        LayerKind::Group { lottie, .. } => lottie.is_some(),
        _ => false,
    }
}

impl PlayoutItem {
    /// The frame the layers of a Lottie group are drawn at: the template's,
    /// unless the operator controls the group (paused, played, restarted, or
    /// waiting for a manual start): then its own clock, up to the on-air
    /// point (the Out still plays the template's frames).
    pub fn lottie_frame(&self, group: &Layer, frame: f64, hold: f64) -> f64 {
        let LayerKind::Group {
            lottie: Some(_), ..
        } = &group.kind
        else {
            return frame;
        };
        let driven = self.clocks.contains_key(&group.id) || group.manual_start;
        if !driven || frame > hold + 1e-6 {
            return frame;
        }
        self.layer_age(group).min(hold)
    }
}

impl PlayoutItem {
    fn start(q: QueuedTake) -> Self {
        Self {
            template_id: q.template_id,
            data: q.data,
            frame: 0.0,
            target: q.target,
            phase: Phase::Playing,
            rundown_id: q.rundown_id,
            age: 0.0,
            next: None,
            clocks: BTreeMap::new(),
            held: 0.0,
            pause_ahead: false,
            fade: None,
            changes: Vec::new(),
            widgets: BTreeMap::new(),
            advanced: Default::default(),
            watch: BTreeMap::new(),
            leaving: None,
        }
    }

    /// Time on air of one layer, in frames: the item's age, unless the
    /// operator paused, reset or has yet to start (manual start) the widget.
    pub fn layer_age(&self, layer: &Layer) -> f64 {
        match self.clocks.get(&layer.id) {
            Some(c) => (c.paused_at.unwrap_or(self.age) - c.offset).max(0.0),
            None if layer.manual_start => 0.0,
            None => self.age,
        }
    }

    /// Continue (F2) on the graphic: the widgets waiting for it go on (a timed
    /// layer waiting for its start, a widget's next page…). True when one did.
    fn continue_widgets(&mut self, t: &Template, fps: f64) -> bool {
        let mut done = false;
        let data = self.data.clone();
        for l in &t.layers {
            let ctx = crate::wid::ActCtx {
                now_ms: now_ms(),
                age_secs: self.layer_age(l) / fps.max(1.0),
                fps: fps.max(1.0),
                data: &data,
            };
            if is_timed(&l.kind)
                && l.manual_start
                && l.start_on.as_deref() != Some("trigger")
                && !self.clocks.contains_key(&l.id)
            {
                self.control(l, WidgetControl::Start);
                done = true;
            } else if crate::wid::widget_id(&l.kind).is_some() {
                let mut st = self
                    .widgets
                    .get(&l.id)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                if crate::wid::on_continue(l, &mut st, &ctx) {
                    self.widgets.insert(l.id.clone(), st);
                    done = true;
                }
            }
        }
        done
    }

    /// The trigger `name` fired: the widgets waiting for it go on.
    fn trigger_widgets(&mut self, t: &Template, name: &str, fps: f64) -> bool {
        let mut done = false;
        let data = self.data.clone();
        for l in &t.layers {
            let ctx = crate::wid::ActCtx {
                now_ms: now_ms(),
                age_secs: self.layer_age(l) / fps.max(1.0),
                fps: fps.max(1.0),
                data: &data,
            };
            if is_timed(&l.kind)
                && l.manual_start
                && l.start_on.as_deref() == Some("trigger")
                && crate::wid::same_trigger(&l.start_trigger, name)
                && !self.clocks.contains_key(&l.id)
            {
                self.control(l, WidgetControl::Start);
                done = true;
            } else if crate::wid::widget_id(&l.kind).is_some() {
                let mut st = self
                    .widgets
                    .get(&l.id)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                if crate::wid::on_trigger(l, &mut st, name, &ctx) {
                    self.widgets.insert(l.id.clone(), st);
                    done = true;
                }
            }
        }
        done
    }

    fn control(&mut self, layer: &Layer, control: WidgetControl) {
        let age = self.age;
        let c = self
            .clocks
            .entry(layer.id.clone())
            .or_insert(if layer.manual_start {
                LayerClock {
                    offset: age,
                    paused_at: Some(age),
                }
            } else {
                LayerClock::default()
            });
        let running = c.paused_at.is_none();
        match control {
            WidgetControl::Start => {
                if let Some(p) = c.paused_at.take() {
                    c.offset += age - p;
                }
            }
            WidgetControl::Pause => {
                c.paused_at.get_or_insert(age);
            }
            WidgetControl::Toggle => {
                if running {
                    c.paused_at = Some(age);
                } else if let Some(p) = c.paused_at.take() {
                    c.offset += age - p;
                }
            }
            WidgetControl::Reset => {
                c.offset = age;
                if !running {
                    c.paused_at = Some(age);
                }
            }
            WidgetControl::Restart => {
                c.offset = age;
                c.paused_at = None;
            }
        }
    }
}

/// Wall-clock time, Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Removes the "set by hand" marks that `done` says are over (the data
/// binding fills those values again). Returns true when one went.
fn release_manual(data: &mut Data, done: impl Fn(&str) -> bool) -> bool {
    let keys: Vec<String> = data
        .iter()
        .filter(|(k, v)| k.ends_with("__manual") && done(v))
        .map(|(k, _)| k.clone())
        .collect();
    for k in &keys {
        data.remove(k);
    }
    !keys.is_empty()
}

/// Commands address a zone by id (`zone`) or a raw playout layer (`layer`).
/// A take with neither plays in the template's zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Command {
    #[serde(rename_all = "camelCase")]
    Take {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        template_id: String,
        #[serde(default)]
        data: Option<Data>,
        #[serde(default)]
        rundown_id: Option<String>,
    },
    Continue {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
    },
    TakeOut {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
    },
    Update {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        data: Data,
        /// The old values fade out (half a second) instead of a cut.
        #[serde(default)]
        fade: bool,
    },
    /// Starts, pauses or resets the internal animation of widgets (timer,
    /// stopwatch, ticker, roll, counter…) of the graphic on air. `target` is
    /// a layer id or name; `*` means every timed widget of the graphic.
    #[serde(rename_all = "camelCase")]
    Widget {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        target: String,
        control: WidgetControl,
    },
    /// An operator action on a widget of the graphic on air (`target`: layer
    /// id or name), e.g. `{"op": "next"}` on a table or `{"op": "add",
    /// "args": {"team": 0, "points": 3}}` on a scoreboard.
    #[serde(rename_all = "camelCase")]
    Act {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        target: String,
        op: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    /// Shows another row of a data set of the graphic on air (`dataset`: its
    /// key or name; `row`: 1 = first), the way the data set says: out and
    /// in again, a cross-fade, or at once.
    Row {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        dataset: String,
        row: u32,
    },
    /// Resumes every graphic holding on a pause that waits for this trigger.
    Trigger { name: String },
    /// Removes the graphic(s) at once; without zone or layer, everything.
    Clear {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
    },
}

impl Command {
    pub fn take(zone: &str, template_id: &str) -> Self {
        Command::Take {
            zone: Some(zone.into()),
            layer: None,
            template_id: template_id.into(),
            data: None,
            rundown_id: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PlayoutError {
    #[error("unknown template `{0}`")]
    UnknownTemplate(String),
    #[error("the template `{0}` is empty: nothing to put on air")]
    EmptyTemplate(String),
    #[error("unknown zone `{0}`")]
    UnknownZone(String),
    #[error("say which zone (or layer) to play in")]
    NoZone,
    #[error("no timed widget `{0}` in the graphic on air")]
    UnknownWidget(String),
    #[error("nothing on air on layer {0}")]
    EmptyLayer(u32),
    #[error("no data set `{0}` in the graphic on air")]
    UnknownDataset(String),
    #[error("{0}")]
    Widget(String),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelState {
    pub layers: BTreeMap<u32, PlayoutItem>,
}

fn hold_target(template: &Template, from: f64) -> f64 {
    template.next_stop_after(from)
}

/// Plays on from the stop the item holds at (or is heading to). Holding on
/// air waits for Take out; nothing happens while the out animation moves.
/// The value an event node watches changed since the take (the first look
/// only notes it).
fn changed_since(item: &mut PlayoutItem, id: &str, value: String) -> bool {
    match item.watch.get(id) {
        Some(was) => *was != value,
        None => {
            item.watch.insert(id.to_string(), value);
            false
        }
    }
}

/// The row after the one a data set shows (the first after the last when it
/// loops); None at its end.
fn next_row(project: &Project, item: &PlayoutItem, dataset: &str) -> Option<u32> {
    let t = project.template(&item.template_id)?;
    let ds = t.datasets.iter().find(|d| d.key == dataset)?;
    let num = |k: String| item.data.get(&k).and_then(|v| v.trim().parse::<f64>().ok());
    let n = num(ds.count_key()).unwrap_or(0.0) as u32;
    let row = num(ds.row_key()).unwrap_or(1.0).max(1.0) as u32;
    if row < n {
        Some(row + 1)
    } else if item.data.get(&ds.loop_key()).is_some_and(|l| l == "1") && n > 1 {
        Some(1)
    } else {
        None
    }
}

/// The event of a Triggers node (`id`) has happened on air.
#[allow(clippy::too_many_arguments)]
fn happened(
    id: &str,
    on: &TriggerEvent,
    t: &Template,
    item: &mut PlayoutItem,
    on_air: &std::collections::HashSet<String>,
    now: i64,
    fps: f64,
    default_tz: &str,
    m: &mut dyn TextWidth,
) -> bool {
    let layer = |id: &str| t.layers.iter().find(|l| l.id == id);
    let state = |item: &PlayoutItem, id: &str| {
        item.widgets
            .get(id)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    let age_secs = item.age / fps;
    let data = shown_data(item).into_owned();
    match on {
        TriggerEvent::ScrollEnd { layer: id, passes } => {
            let Some(l) = layer(id) else { return false };
            let v = state(item, id);
            if let Some((pass, ended)) = crate::wid::ticker::progress(
                l,
                &serde_json::from_value(v.clone()).unwrap_or_default(),
                &data,
                now,
                age_secs,
                fps,
                m,
            ) {
                return if *passes > 0 {
                    pass > *passes || ended
                } else {
                    ended
                };
            }
            crate::wid::credits::ended(
                l,
                &serde_json::from_value(v).unwrap_or_default(),
                &data,
                now,
                age_secs,
                m,
            )
            .unwrap_or(false)
        }
        TriggerEvent::TimerZero { layer: id } | TriggerEvent::TimerAt { layer: id, .. } => {
            let Some(l) = layer(id) else { return false };
            let st = serde_json::from_value(state(item, id)).unwrap_or_default();
            let age_ms = (age_secs * 1000.0) as i64;
            let Some((v, countdown, zero)) =
                crate::wid::timer::reading(l, &st, &data, now, age_ms, default_tz)
            else {
                return false;
            };
            match on {
                TriggerEvent::TimerAt { seconds, .. } => {
                    let at = (seconds * 1000.0).round() as i64;
                    if countdown {
                        v <= at
                    } else {
                        v >= at
                    }
                }
                _ => zero,
            }
        }
        TriggerEvent::VideoEnd { layer: id } => {
            let Some(l) = layer(id) else { return false };
            let LayerKind::Image { x: c, .. } = &l.kind else {
                return false;
            };
            if c.frames.is_empty() {
                return false;
            }
            let st = serde_json::from_value(state(item, id)).unwrap_or_default();
            crate::wid::image::seq(c, &st, crate::wid::image::manual(l, c), now, age_secs).ended
        }
        TriggerEvent::LastRow { dataset } => {
            let Some(ds) = t.datasets.iter().find(|d| &d.key == dataset) else {
                return false;
            };
            let num = |k: String| data.get(&k).and_then(|v| v.trim().parse::<f64>().ok());
            let count = num(ds.count_key()).unwrap_or(0.0);
            count > 0.0 && num(ds.row_key()).unwrap_or(1.0) >= count
        }
        TriggerEvent::DataChange { key } => {
            let v = data.get(key).cloned().unwrap_or_default();
            changed_since(item, id, v)
        }
        TriggerEvent::DataIs { key, op, value } => {
            TriggerEvent::holds(op, data.get(key).map(String::as_str).unwrap_or(""), value)
        }
        TriggerEvent::Graphic { template, out } => {
            let now_on = on_air.contains(template);
            if !*out {
                return now_on;
            }
            // Out: once seen on air since the take.
            let seen = format!("{id}:seen");
            if now_on {
                item.watch.insert(seen, "1".into());
                return false;
            }
            item.watch.get(&seen).is_some_and(|v| v == "1")
        }
    }
}

fn resume(t: &Template, item: &mut PlayoutItem) -> bool {
    match item.phase {
        Phase::Out => false,
        Phase::OutHold => {
            item.target = t.next_stop_after(item.frame);
            item.pause_ahead = item.target < t.duration - 1e-6;
            item.phase = Phase::Out;
            item.held = 0.0;
            true
        }
        Phase::Playing | Phase::Holding => {
            let from = item.target.max(item.frame);
            if from >= t.hold_frame() - 1e-6 {
                return false;
            }
            item.frame = from;
            item.target = t.next_stop_after(from).min(t.hold_frame());
            item.phase = Phase::Playing;
            item.held = 0.0;
            true
        }
    }
}

fn resolve(
    project: &Project,
    zone: &Option<String>,
    layer: Option<u32>,
) -> Result<Option<u32>, PlayoutError> {
    match zone.as_deref().filter(|z| !z.is_empty()) {
        Some(z) => project
            .zone(z)
            .map(|z| Some(z.layer))
            .ok_or_else(|| PlayoutError::UnknownZone(z.into())),
        None => Ok(layer),
    }
}

/// Starts the out animation; false if the template has none (remove now).
fn begin_out(project: &Project, item: &mut PlayoutItem) -> bool {
    let Some(t) = project.template(&item.template_id) else {
        return false;
    };
    let out = t.hold_frame();
    if t.duration <= out + 1e-6 {
        return false;
    }
    if matches!(item.phase, Phase::Out | Phase::OutHold) {
        // Out again: straight to the end, through the pauses.
        item.target = t.duration;
    } else {
        item.frame = item.frame.max(out);
        item.target = t.next_stop_after(item.frame);
    }
    item.pause_ahead = item.target < t.duration - 1e-6;
    item.phase = Phase::Out;
    item.held = 0.0;
    true
}

/// The data of a graphic on air changed (`old` → `item.data`): its widgets
/// that keep what they show react (`wid::on_data`: a chart freezes the data
/// of the take, or tweens to the new values).
fn data_changed(project: &Project, item: &mut PlayoutItem, old: &crate::anim::Data) {
    widgets_react(project, item, old, crate::wid::on_data);
}

/// The data of a graphic on air was changed by its data sources (`old` →
/// `item.data`, the server's bindings): the widgets that play a change of
/// their list react (`wid::on_source_data`: the social wall's cards).
pub fn source_data_changed(project: &Project, item: &mut PlayoutItem, old: &crate::anim::Data) {
    widgets_react(project, item, old, crate::wid::on_source_data);
}

type React = fn(
    &crate::model::Layer,
    &mut serde_json::Value,
    &crate::anim::Data,
    &crate::wid::ActCtx,
) -> bool;

fn widgets_react(project: &Project, item: &mut PlayoutItem, old: &crate::anim::Data, react: React) {
    let Some(t) = project.template(&item.template_id) else {
        return;
    };
    let fps = project.frame_rate.as_f64().max(1.0);
    let ctx = crate::wid::ActCtx {
        now_ms: now_ms(),
        age_secs: item.age / fps,
        fps,
        data: &item.data,
    };
    for l in &t.layers {
        let mut st = item
            .widgets
            .get(&l.id)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        if react(l, &mut st, old, &ctx) {
            item.widgets.insert(l.id.clone(), st);
        }
    }
}

impl ChannelState {
    pub fn apply(&mut self, project: &Project, cmd: Command) -> Result<(), PlayoutError> {
        match cmd {
            Command::Take {
                zone,
                layer,
                template_id,
                data,
                rundown_id,
            } => {
                let t = project
                    .template(&template_id)
                    .ok_or_else(|| PlayoutError::UnknownTemplate(template_id.clone()))?;
                if !t.playable() {
                    return Err(PlayoutError::EmptyTemplate(t.name.clone()));
                }
                let layer = match resolve(project, &zone, layer)? {
                    Some(l) => l,
                    None => project
                        .zone(&t.zone)
                        .map(|z| z.layer)
                        .ok_or(PlayoutError::NoZone)?,
                };
                let mut merged = t.default_data();
                merged.extend(data.unwrap_or_default());
                let take = QueuedTake {
                    template_id,
                    data: merged,
                    rundown_id,
                    target: hold_target(t, 0.0),
                };
                // The same rundown item on air elsewhere (its zone changed since): it goes out there.
                if let Some(rid) = take.rundown_id.clone() {
                    let stale: Vec<u32> = self
                        .layers
                        .iter()
                        .filter(|(l, it)| **l != layer && it.rundown_id.as_deref() == Some(&*rid))
                        .map(|(l, _)| *l)
                        .collect();
                    for l in stale {
                        let it = self.layers.get_mut(&l).expect("listed");
                        it.next = None;
                        if !begin_out(project, it) {
                            self.layers.remove(&l);
                        }
                    }
                }
                let transition = project
                    .zone_at(layer)
                    .map_or(ZoneTransition::Sequential, |z| z.transition);
                match self.layers.get_mut(&layer) {
                    // Zone busy: the graphic on air goes out first.
                    Some(current) if transition == ZoneTransition::Sequential => {
                        if begin_out(project, current) {
                            current.next = Some(take);
                        } else {
                            *current = PlayoutItem::start(take);
                        }
                    }
                    // Zone busy, « Fade »: the new graphic comes in over the one on air, which fades out.
                    Some(current) if transition == ZoneTransition::Fade => {
                        let mut old = std::mem::replace(current, PlayoutItem::start(take));
                        old.next = None;
                        old.leaving = None;
                        let fps = project.frame_rate.as_f64().max(1.0);
                        current.leaving = Some(Box::new(Leaving {
                            item: old,
                            frame: 0.0,
                            length: (fps * 0.5).round(),
                        }));
                    }
                    _ => {
                        self.layers.insert(layer, PlayoutItem::start(take));
                    }
                }
            }
            Command::Continue { zone, layer } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                let t = project
                    .template(&item.template_id)
                    .ok_or_else(|| PlayoutError::UnknownTemplate(item.template_id.clone()))?;
                // A widget waiting for Continue (its next page, its start) takes it first;
                // the timeline's pause plays on once none does.
                if !item.continue_widgets(t, project.frame_rate.as_f64()) {
                    resume(t, item);
                }
            }
            Command::Trigger { name } => {
                // Values set by hand until this trigger let go.
                for item in self.layers.values_mut() {
                    release_manual(&mut item.data, |m| {
                        m.strip_prefix("trigger:")
                            .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
                    });
                }
                let is = |r: &Resume| matches!(r, Resume::Trigger { name: n } if n.trim().eq_ignore_ascii_case(name.trim()));
                let mut gone = Vec::new();
                for (layer, item) in self.layers.iter_mut() {
                    let Some(t) = project.template(&item.template_id) else {
                        continue;
                    };
                    // Widgets waiting for this trigger (a page, a start) go on.
                    item.trigger_widgets(t, &name, project.frame_rate.as_f64());
                    if !matches!(item.phase, Phase::Holding | Phase::OutHold) {
                        continue;
                    }
                    // On air, its IN/OUT point set to go out on this trigger: it goes out.
                    let on_air =
                        item.phase == Phase::Holding && item.frame >= t.hold_frame() - 1e-6;
                    if on_air {
                        if t.markers
                            .iter()
                            .any(|m| m.kind == MarkerKind::Out && is(&m.resume))
                            && !begin_out(project, item)
                        {
                            gone.push(*layer);
                        }
                        continue;
                    }
                    if t.pause_at(item.frame).is_some_and(|m| is(&m.resume)) {
                        resume(t, item);
                    }
                }
                for layer in gone {
                    self.layers.remove(&layer);
                }
            }
            Command::TakeOut { zone, layer } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                // Out also cancels a pending take.
                item.next = None;
                if !begin_out(project, item) {
                    self.layers.remove(&layer);
                }
            }
            Command::Update {
                zone,
                layer,
                data,
                fade,
            } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                // Rows going on by themselves: the time counts from now.
                let mut data = data;
                for k in data
                    .keys()
                    .filter_map(|k| k.strip_suffix("__auto"))
                    .map(String::from)
                    .collect::<Vec<_>>()
                {
                    if item.data.get(&format!("{k}__auto")) != data.get(&format!("{k}__auto")) {
                        data.insert(format!("{k}__shown"), format!("{}", item.age.round()));
                    }
                }
                // Update what is (about to be) on air.
                match &mut item.next {
                    Some(q) => q.data.extend(data),
                    None => {
                        let changed = data.iter().any(|(k, v)| item.data.get(k) != Some(v));
                        if fade && changed {
                            let fps = project.frame_rate.as_f64().max(1.0);
                            item.fade = Some(Fade {
                                data: item.data.clone(),
                                frame: 0.0,
                                length: (fps * 0.5).round(),
                            });
                        }
                        let old = changed.then(|| item.data.clone());
                        item.data.extend(data);
                        if let Some(old) = old {
                            data_changed(project, item, &old);
                        }
                    }
                }
            }
            Command::Row {
                zone,
                layer,
                dataset,
                row,
            } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                let t = project
                    .template(&item.template_id)
                    .ok_or_else(|| PlayoutError::UnknownTemplate(item.template_id.clone()))?;
                let ds = t
                    .datasets
                    .iter()
                    .find(|d| d.key == dataset || d.name.eq_ignore_ascii_case(dataset.trim()))
                    .ok_or_else(|| PlayoutError::UnknownDataset(dataset.clone()))?;
                let row = (ds.row_key(), row.max(1).to_string());
                // Already going out to come back: the row it comes back with.
                if let Some(q) = &mut item.next {
                    q.data.insert(row.0, row.1);
                    return Ok(());
                }
                if matches!(item.phase, Phase::Out | Phase::OutHold) {
                    item.data.insert(row.0, row.1);
                    return Ok(());
                }
                match ds.change_in(&item.data) {
                    RowChange::OutIn => {
                        let mut data = item.data.clone();
                        data.insert(row.0.clone(), row.1.clone());
                        // Comes in again: its time on the row starts with the hold.
                        data.remove(&ds.shown_key());
                        let take = QueuedTake {
                            template_id: item.template_id.clone(),
                            data,
                            rundown_id: item.rundown_id.clone(),
                            target: hold_target(t, 0.0),
                        };
                        if begin_out(project, item) {
                            item.next = Some(take);
                        } else {
                            item.data.insert(row.0, row.1);
                        }
                    }
                    RowChange::Fade => {
                        let fps = project.frame_rate.as_f64().max(1.0);
                        let old = item.data.clone();
                        item.fade = Some(Fade {
                            data: old.clone(),
                            frame: 0.0,
                            length: (fps * 0.5).round(),
                        });
                        item.data.insert(row.0, row.1);
                        item.data
                            .insert(ds.shown_key(), format!("{}", item.age.round()));
                        data_changed(project, item, &old);
                    }
                    RowChange::Cut => {
                        let old = item.data.clone();
                        item.data.insert(row.0, row.1);
                        item.data
                            .insert(ds.shown_key(), format!("{}", item.age.round()));
                        data_changed(project, item, &old);
                    }
                }
            }
            Command::Widget {
                zone,
                layer,
                target,
                control,
            } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                let t = project
                    .template(&item.template_id)
                    .ok_or_else(|| PlayoutError::UnknownTemplate(item.template_id.clone()))?;
                let all = target == "*";
                let targets: Vec<&Layer> = t
                    .layers
                    .iter()
                    .filter(|l| {
                        is_timed(&l.kind)
                            && (all || l.id == target || l.name.eq_ignore_ascii_case(target.trim()))
                    })
                    .collect();
                if targets.is_empty() {
                    return Err(PlayoutError::UnknownWidget(target));
                }
                for l in targets {
                    item.control(l, control);
                }
            }
            Command::Act {
                zone,
                layer,
                target,
                op,
                args,
            } => {
                let layer = resolve(project, &zone, layer)?.ok_or(PlayoutError::NoZone)?;
                let item = self
                    .layers
                    .get_mut(&layer)
                    .ok_or(PlayoutError::EmptyLayer(layer))?;
                let t = project
                    .template(&item.template_id)
                    .ok_or_else(|| PlayoutError::UnknownTemplate(item.template_id.clone()))?;
                let l = t
                    .layers
                    .iter()
                    .find(|l| {
                        crate::wid::widget_id(&l.kind).is_some()
                            && (l.id == target || l.name.eq_ignore_ascii_case(target.trim()))
                    })
                    .ok_or_else(|| PlayoutError::UnknownWidget(target.clone()))?;
                let fps = project.frame_rate.as_f64().max(1.0);
                // The layer's own time on air (still while it waits for its start, or paused).
                let ctx = crate::wid::ActCtx {
                    now_ms: now_ms(),
                    age_secs: item.layer_age(l) / fps,
                    fps,
                    data: &item.data,
                };
                let mut st = item
                    .widgets
                    .get(&l.id)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                crate::wid::act(l, &mut st, &op, &args, &ctx).map_err(PlayoutError::Widget)?;
                item.widgets.insert(l.id.clone(), st);
            }
            Command::Clear { zone, layer } => match resolve(project, &zone, layer)? {
                Some(l) => {
                    self.layers.remove(&l);
                }
                None => self.layers.clear(),
            },
        }
        Ok(())
    }

    /// Advances every item by `frames`. Returns true if a timeline moved
    /// (holding items only age, which clients extrapolate on their own).
    pub fn tick(&mut self, frames: f64) -> bool {
        let mut changed = false;
        let mut finished = Vec::new();
        for (layer, item) in self.layers.iter_mut() {
            item.age += frames;
            if let Some(f) = &mut item.fade {
                f.frame += frames;
                changed = true;
                if f.frame >= f.length {
                    item.fade = None;
                }
            }
            if !item.changes.is_empty() {
                for c in &mut item.changes {
                    c.frame += frames;
                }
                item.changes.retain(|c| c.frame < c.length);
                changed = true;
            }
            if let Some(l) = &mut item.leaving {
                l.frame += frames;
                l.item.age += frames;
                changed = true;
                if l.frame >= l.length {
                    item.leaving = None;
                }
            }
            if matches!(item.phase, Phase::Holding | Phase::OutHold) {
                item.held += frames;
                continue;
            }
            changed = true;
            item.frame = (item.frame + frames).min(item.target);
            if item.frame >= item.target {
                item.held = 0.0;
                if item.phase == Phase::Out {
                    // A pause of the out animation stops it before the end.
                    if item.pause_ahead {
                        item.phase = Phase::OutHold;
                    } else {
                        finished.push(*layer);
                    }
                } else {
                    item.phase = Phase::Holding;
                }
            }
        }
        for layer in finished {
            if let Some(q) = self.layers.remove(&layer).and_then(|i| i.next) {
                self.layers.insert(layer, PlayoutItem::start(q));
            }
        }
        changed
    }

    /// Resumes the pauses whose delay is over. `fps`: project frame rate.
    pub fn auto_resume(&mut self, project: &Project) -> bool {
        let fps = project.frame_rate.as_f64().max(1.0);
        let mut changed = false;
        let mut gone = Vec::new();
        for (layer, item) in self.layers.iter_mut() {
            if !matches!(item.phase, Phase::Holding | Phase::OutHold) {
                continue;
            }
            let Some(t) = project.template(&item.template_id) else {
                continue;
            };
            // On air with an automatic out: goes out once held long enough.
            let on_air = item.phase == Phase::Holding && item.frame >= t.hold_frame() - 1e-6;
            let auto_out = t
                .markers
                .iter()
                .find(|m| m.kind == MarkerKind::Out)
                .and_then(|m| match m.resume {
                    Resume::After { seconds } => Some(seconds.max(0.0) * fps),
                    _ => None,
                });
            if on_air {
                if auto_out.is_some_and(|a| item.held >= a) {
                    if !begin_out(project, item) {
                        gone.push(*layer);
                    }
                    changed = true;
                }
                continue;
            }
            let due = t
                .pause_at(item.frame)
                .is_some_and(|m| matches!(m.resume, Resume::After { seconds } if item.held >= seconds.max(0.0) * fps));
            if due {
                changed |= resume(t, item);
            }
        }
        for layer in gone {
            self.layers.remove(&layer);
        }
        changed
    }

    /// Auto next between graphics: a graphic of a playlist set to auto next
    /// that has been on air for its seconds takes the next graphic of the
    /// playlist (the first after the last when it loops); it goes out unless
    /// it holds anyway or the next one plays in its zone (which replaces
    /// it). Returns true when something was taken.
    pub fn auto_next(&mut self, project: &Project) -> bool {
        self.auto_next_taken(project).is_some()
    }

    /// [`Self::auto_next`], with the rundown items it took (their data read
    /// "on take" is read for them); `None` when nothing changed.
    pub fn auto_next_taken(&mut self, project: &Project) -> Option<Vec<String>> {
        let fps = project.frame_rate.as_f64().max(1.0);
        let zone_of = |r: &RundownItem| {
            if r.zone.is_empty() {
                project
                    .template(&r.template_id)
                    .map(|t| t.zone.clone())
                    .unwrap_or_default()
            } else {
                r.zone.clone()
            }
        };
        let mut takes = Vec::new();
        let mut outs = Vec::new();
        for (layer, item) in self.layers.iter_mut() {
            if item.advanced || matches!(item.phase, Phase::Out | Phase::OutHold) {
                continue;
            }
            let Some(r) = item
                .rundown_id
                .as_ref()
                .and_then(|id| project.rundown.iter().find(|x| &x.id == id))
            else {
                continue;
            };
            let Some(secs) = r.auto_secs else { continue };
            let Some(list) = project
                .playlists
                .iter()
                .find(|p| p.id == r.playlist && p.auto_next)
            else {
                continue;
            };
            // Its time on air counts from the on-air point, after the in
            // animation and the pauses before it.
            let Some(t) = project.template(&item.template_id) else {
                continue;
            };
            let on_air = item.phase == Phase::Holding && item.frame >= t.hold_frame() - 1e-6;
            if !on_air || item.held / fps < secs.max(0.0) {
                continue;
            }
            item.advanced = true;
            let items: Vec<&RundownItem> = project
                .rundown
                .iter()
                .filter(|x| x.playlist == r.playlist)
                // An empty template is not part of the rundown.
                .filter(|x| {
                    x.id == r.id
                        || project
                            .template(&x.template_id)
                            .is_some_and(|t| t.playable())
                })
                .collect();
            let at = items.iter().position(|x| x.id == r.id).unwrap_or(0);
            let next = items
                .get(at + 1)
                .or(if list.looping { items.first() } else { None });
            let same_zone = next.is_some_and(|n| zone_of(n) == zone_of(r));
            if let Some(n) = next {
                takes.push((*n).clone());
            }
            if !r.hold && !same_zone {
                outs.push(*layer);
            }
        }
        for layer in &outs {
            if let Some(item) = self.layers.get_mut(layer) {
                if !begin_out(project, item) {
                    self.layers.remove(layer);
                }
            }
        }
        let changed = !takes.is_empty() || !outs.is_empty();
        let ids: Vec<String> = takes.iter().map(|n| n.id.clone()).collect();
        for n in takes {
            let Some(t) = project.template(&n.template_id) else {
                continue;
            };
            let mut data = t.default_data();
            data.extend(n.data.clone());
            let zone = zone_of(&n);
            let _ = self.apply(
                project,
                Command::Take {
                    zone: (!zone.is_empty()).then_some(zone),
                    layer: None,
                    template_id: n.template_id.clone(),
                    data: Some(data),
                    rundown_id: Some(n.id.clone()),
                },
            );
        }
        changed.then_some(ids)
    }

    /// Countdowns reaching zero with a trigger set: the triggers to fire
    /// (each zero fires once; the widget's state remembers it).
    pub fn zero_triggers(&mut self, project: &Project, default_tz: &str) -> Vec<String> {
        let now = now_ms();
        let fps = project.frame_rate.as_f64().max(1.0);
        let mut out = Vec::new();
        for item in self.layers.values_mut() {
            if matches!(item.phase, Phase::Out | Phase::OutHold) {
                continue;
            }
            let Some(t) = project.template(&item.template_id) else {
                continue;
            };
            for l in &t.layers {
                let st: crate::wid::timer::State = item
                    .widgets
                    .get(&l.id)
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let age_ms = (item.age / fps * 1000.0) as i64;
                if let Some((name, at)) =
                    crate::wid::timer::due_trigger(l, &st, &item.data, now, age_ms, default_tz)
                {
                    let mut st = st;
                    st.fired = Some(at);
                    if let Ok(v) = serde_json::to_value(&st) {
                        item.widgets.insert(l.id.clone(), v);
                    }
                    out.push(name);
                }
            }
        }
        out
    }

    /// The template's Triggers, on air: their actions run when their input
    /// is true. A pause or the on-air point set to « Déclencheurs »
    /// (`Resume::Triggers`) plays on or goes out; a trigger is fired and a
    /// data set goes to its next row once each time their input becomes
    /// true. The event nodes are watched here. Returns what was done (for
    /// the as-run log).
    pub fn event_resumes(
        &mut self,
        project: &Project,
        default_tz: &str,
        m: &mut dyn TextWidth,
    ) -> Vec<String> {
        use crate::logic::AirAction;
        let now = now_ms();
        let fps = project.frame_rate.as_f64().max(1.0);
        // The graphics on air (another one's in / out).
        let on_air: std::collections::HashSet<String> = self
            .layers
            .values()
            .filter(|i| !matches!(i.phase, Phase::Out | Phase::OutHold))
            .map(|i| i.template_id.clone())
            .collect();
        let mut done = Vec::new();
        let mut gone = Vec::new();
        let mut fires = Vec::new();
        let mut rows = Vec::new();
        for (layer, item) in self.layers.iter_mut() {
            if matches!(item.phase, Phase::Out) {
                continue;
            }
            let Some(t) = project.template(&item.template_id) else {
                continue;
            };
            if !crate::logic::drives_air(&t.triggers) {
                continue;
            }
            let data = shown_data(item).into_owned();
            let actions = crate::logic::air_actions(&t.triggers, &data, &mut |id, on| {
                happened(id, on, t, item, &on_air, now, fps, default_tz, m)
            });
            // Once each time their input becomes true.
            let once = |item: &mut PlayoutItem, node: &str| {
                let key = format!("act:{node}");
                let first = item.watch.get(&key).is_none_or(|v| v != "1");
                item.watch.insert(key, "1".into());
                first
            };
            let asked: std::collections::HashSet<String> = actions
                .iter()
                .filter_map(|a| match a {
                    AirAction::Fire { node, .. } | AirAction::NextRow { node, .. } => {
                        Some(format!("act:{node}"))
                    }
                    _ => None,
                })
                .collect();
            item.watch
                .iter_mut()
                .filter(|(k, _)| k.starts_with("act:") && !asked.contains(*k))
                .for_each(|(_, v)| *v = "0".into());
            for a in &actions {
                match a {
                    AirAction::Fire { node, name } if once(item, node) => fires.push(name.clone()),
                    AirAction::NextRow { node, dataset } if once(item, node) => {
                        rows.push((*layer, dataset.clone()))
                    }
                    _ => {}
                }
            }
            // A pause or the on-air point set to « Déclencheurs ».
            if !matches!(item.phase, Phase::Holding | Phase::OutHold) {
                continue;
            }
            let on_point = item.phase == Phase::Holding && item.frame >= t.hold_frame() - 1e-6;
            let waits = if on_point {
                t.markers.iter().find(|m| m.kind == MarkerKind::Out)
            } else {
                t.pause_at(item.frame)
            };
            let Some(marker) = waits.filter(|w| w.resume == Resume::Triggers) else {
                continue;
            };
            let go = if on_point {
                actions.contains(&AirAction::Out)
            } else {
                actions.contains(&AirAction::Resume(marker.id.clone()))
            };
            if !go {
                continue;
            }
            done.push(t.name.clone());
            if on_point {
                if !begin_out(project, item) {
                    gone.push(*layer);
                }
            } else {
                resume(t, item);
            }
        }
        for layer in gone {
            self.layers.remove(&layer);
        }
        for name in fires {
            if self
                .apply(project, Command::Trigger { name: name.clone() })
                .is_ok()
            {
                done.push(format!("⚡ {name}"));
            }
        }
        for (layer, dataset) in rows {
            let Some(row) = self
                .layers
                .get(&layer)
                .and_then(|item| next_row(project, item, &dataset))
            else {
                continue;
            };
            if self
                .apply(
                    project,
                    Command::Row {
                        zone: None,
                        layer: Some(layer),
                        dataset: dataset.clone(),
                        row,
                    },
                )
                .is_ok()
            {
                done.push(format!("{dataset} → {row}"));
            }
        }
        done
    }

    /// Data sets going on by themselves: the next row once the graphic has
    /// held on air (on the on-air marker, after its in animation) for the
    /// data set's seconds. Returns true when a row changed.
    pub fn auto_rows(&mut self, project: &Project) -> bool {
        let fps = project.frame_rate.as_f64().max(1.0);
        let mut due = Vec::new();
        for (layer, item) in &self.layers {
            if item.phase != Phase::Holding || item.next.is_some() {
                continue;
            }
            let Some(t) = project.template(&item.template_id) else {
                continue;
            };
            // Holding on the on-air marker, not on a pause of the in animation.
            if item.frame < t.hold_frame() - 1e-6 {
                continue;
            }
            for ds in &t.datasets {
                let num = |k: String| item.data.get(&k).and_then(|v| v.trim().parse::<f64>().ok());
                let Some(secs) = num(ds.auto_key()).filter(|s| *s > 0.0) else {
                    continue;
                };
                let shown =
                    num(ds.shown_key()).map_or(item.held, |at| (item.age - at).min(item.held));
                if shown < secs * fps {
                    continue;
                }
                let n = num(ds.count_key()).unwrap_or(0.0) as u32;
                let row = num(ds.row_key()).unwrap_or(1.0).max(1.0) as u32;
                let next = if row < n {
                    row + 1
                } else if item.data.get(&ds.loop_key()).is_some_and(|l| l == "1") && n > 1 {
                    1
                } else {
                    continue;
                };
                due.push((*layer, ds.key.clone(), next));
            }
        }
        let changed = !due.is_empty();
        for (layer, dataset, row) in due {
            let _ = self.apply(
                project,
                Command::Row {
                    zone: None,
                    layer: Some(layer),
                    dataset,
                    row,
                },
            );
        }
        changed
    }

    /// Lets go of the values set by hand for a time (`until:<unix ms>`)
    /// once it is over. Returns true when one did.
    pub fn expire_manual(&mut self, now_ms: u64) -> bool {
        let mut changed = false;
        for item in self.layers.values_mut() {
            changed |= release_manual(&mut item.data, |m| {
                m.strip_prefix("until:")
                    .and_then(|t| t.trim().parse::<u64>().ok())
                    .is_some_and(|t| t <= now_ms)
            });
            if let Some(q) = &mut item.next {
                changed |= release_manual(&mut q.data, |m| {
                    m.strip_prefix("until:")
                        .and_then(|t| t.trim().parse::<u64>().ok())
                        .is_some_and(|t| t <= now_ms)
                });
            }
        }
        changed
    }

    pub fn is_animating(&self) -> bool {
        self.layers.values().any(|i| {
            i.fade.is_some()
                || !i.changes.is_empty()
                || i.leaving.is_some()
                || !matches!(i.phase, Phase::Holding | Phase::OutHold)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Resume;
    use crate::model::{FrameRate, Marker};

    fn project() -> Project {
        Project {
            guides: Default::default(),
            design_order: Vec::new(),
            language: "fr".into(),
            template_secs: None,
            colors: Vec::new(),
            id: "p".into(),
            name: "p".into(),
            description: String::new(),
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::default(),
            templates: vec![Template {
                logic: Default::default(),
                triggers: Default::default(),
                dataflow: None,
                datasets: vec![],
                revision: 0,
                marker_model: crate::model::MARKER_MODEL,
                id: "t".into(),
                name: "t".into(),
                duration: 100.0,
                markers: vec![
                    Marker {
                        id: "a".into(),
                        frame: 20.0,
                        name: "in".into(),
                        kind: MarkerKind::Pause,
                        resume: Default::default(),
                    },
                    Marker {
                        id: "b".into(),
                        frame: 40.0,
                        name: "step".into(),
                        kind: MarkerKind::Pause,
                        resume: Default::default(),
                    },
                    Marker {
                        id: "c".into(),
                        frame: 80.0,
                        name: "out".into(),
                        kind: MarkerKind::Out,
                        resume: Default::default(),
                    },
                ],
                fields: vec![],
                // Something to show: an empty template is not taken.
                layers: vec![serde_json::from_str(
                    r#"{"id":"l","name":"T","type":"text","text":"Hi","fontFamily":"Inter","props":{}}"#,
                )
                .unwrap()],
                zone: "z".into(),
            }],
            rundown: vec![],
            playlists: vec![],
            formats: vec![],
            drafts: vec![],
            starter: None,
            zones: vec![crate::model::Zone {
                id: "z".into(),
                name: "Zone".into(),
                layer: 1,
                color: String::new(),
                transition: ZoneTransition::Sequential,
                areas: Default::default(),
            }],
        }
    }

    #[test]
    fn auto_next_between_graphics() {
        use crate::model::{Playlist, RundownItem, Zone};
        let mut p = project();
        let fps = p.frame_rate.as_f64();
        let mut t2 = p.templates[0].clone();
        t2.id = "t2".into();
        t2.zone = "z2".into();
        p.templates.push(t2);
        p.zones.push(Zone {
            id: "z2".into(),
            layer: 2,
            ..p.zones[0].clone()
        });
        p.playlists.push(Playlist {
            id: "pl".into(),
            name: "Show".into(),
            auto_next: true,
            looping: false,
        });
        let item = |id: &str, t: &str, secs: Option<f64>, hold: bool| RundownItem {
            id: id.into(),
            name: id.into(),
            template_id: t.into(),
            zone: String::new(),
            playlist: "pl".into(),
            layer: 0,
            data: Default::default(),
            read: Default::default(),
            auto_secs: secs,
            hold,
        };
        p.rundown = vec![
            item("a", "t", Some(1.0), false),
            item("b", "t2", None, false),
        ];
        let take_a = |st: &mut ChannelState, p: &Project| {
            st.apply(
                p,
                Command::Take {
                    zone: Some("z".into()),
                    layer: None,
                    template_id: "t".into(),
                    data: None,
                    rundown_id: Some("a".into()),
                },
            )
            .unwrap();
        };
        // Frame by frame, like the engine.
        let run = |st: &mut ChannelState, p: &Project, frames: f64| {
            for _ in 0..frames.round() as usize {
                st.tick(1.0);
                st.auto_resume(p);
            }
        };
        // The pauses before the on-air point: none for now.
        let pauses = std::mem::take(&mut p.templates[0].markers);
        p.templates[0]
            .markers
            .extend(pauses.iter().filter(|m| m.kind == MarkerKind::Out).cloned());
        let on_air = p.templates[0].hold_frame();
        let mut st = ChannelState::default();
        take_a(&mut st, &p);
        run(&mut st, &p, on_air + fps * 0.5);
        assert!(!st.auto_next(&p));
        run(&mut st, &p, fps * 0.6);
        assert!(st.auto_next(&p));
        // b in its zone; a goes out.
        assert_eq!(st.layers[&2].rundown_id.as_deref(), Some("b"));
        assert_eq!(st.layers[&1].phase, Phase::Out);
        assert!(!st.auto_next(&p), "only once");
        // Hold anyway: a stays.
        p.rundown[0].hold = true;
        let mut st = ChannelState::default();
        take_a(&mut st, &p);
        run(&mut st, &p, on_air + fps * 1.2);
        assert!(st.auto_next(&p));
        assert_ne!(st.layers[&1].phase, Phase::Out);
        // A pause before the on-air point: the time counts from the on-air
        // point, not from the take.
        p.rundown[0].hold = false;
        p.templates[0].markers = pauses;
        let mut st = ChannelState::default();
        take_a(&mut st, &p);
        run(&mut st, &p, fps * 5.0);
        assert!(!st.auto_next(&p), "held on a pause");
        st.apply(
            &p,
            Command::Continue {
                zone: Some("z".into()),
                layer: None,
            },
        )
        .unwrap();
        run(&mut st, &p, fps * 5.0);
        assert!(!st.auto_next(&p), "held on the second pause");
        st.apply(
            &p,
            Command::Continue {
                zone: Some("z".into()),
                layer: None,
            },
        )
        .unwrap();
        run(&mut st, &p, on_air - 40.0 + fps * 0.5);
        assert!(!st.auto_next(&p), "on air for half a second");
        run(&mut st, &p, fps * 0.6);
        assert!(st.auto_next(&p));
        // Playlist without auto next: nothing.
        p.playlists[0].auto_next = false;
        let mut st = ChannelState::default();
        take_a(&mut st, &p);
        st.tick(fps * 5.0);
        assert!(!st.auto_next(&p));
    }

    #[test]
    fn dataset_row_changes() {
        use crate::model::{DataSet, RowChange};
        let mut p = project();
        p.templates[0].datasets.push(DataSet {
            key: "ds".into(),
            name: "Guests".into(),
            source: "s".into(),
            list: String::new(),
            steps: vec![],
            change: RowChange::OutIn,
        });
        let mut st = ChannelState::default();
        st.apply(&p, Command::take("z", "t")).unwrap();
        st.tick(500.0);
        let row = |r| Command::Row {
            zone: Some("z".into()),
            layer: None,
            dataset: "guests".into(),
            row: r,
        };
        // Out › In: out with the old row, back with the new one.
        st.apply(&p, row(2)).unwrap();
        let item = st.layers.values().next().unwrap();
        assert_eq!(item.phase, Phase::Out);
        assert_eq!(item.next.as_ref().unwrap().data["ds__row"], "2");
        assert!(!item.data.contains_key("ds__row"));
        // Asked again while going out: the row it comes back with.
        st.apply(&p, row(3)).unwrap();
        assert_eq!(
            st.layers
                .values()
                .next()
                .unwrap()
                .next
                .as_ref()
                .unwrap()
                .data["ds__row"],
            "3"
        );
        st.tick(500.0);
        let item = st.layers.values().next().unwrap();
        assert_eq!(item.data["ds__row"], "3");
        // Fade: the old data fades out over half a second.
        st.tick(500.0);
        p.templates[0].datasets[0].change = RowChange::Fade;
        st.apply(&p, row(4)).unwrap();
        let item = st.layers.values().next().unwrap();
        assert_eq!(item.data["ds__row"], "4");
        assert_eq!(item.fade.as_ref().unwrap().data["ds__row"], "3");
        assert!(st.is_animating());
        st.tick(100.0);
        assert!(st.layers.values().next().unwrap().fade.is_none());
        assert_eq!(
            st.apply(
                &p,
                Command::Row {
                    zone: Some("z".into()),
                    layer: None,
                    dataset: "nope".into(),
                    row: 1
                }
            ),
            Err(PlayoutError::UnknownDataset("nope".into()))
        );
    }

    #[test]
    fn dataset_auto_and_manual() {
        use crate::model::{DataSet, RowChange};
        let mut p = project();
        p.templates[0].datasets.push(DataSet {
            key: "ds".into(),
            name: "Guests".into(),
            source: "s".into(),
            list: String::new(),
            steps: vec![],
            change: RowChange::Cut,
        });
        let mut st = ChannelState::default();
        let data: Data = [("ds__auto", "2"), ("ds__count", "3"), ("ds__loop", "1")]
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .into();
        st.apply(
            &p,
            Command::Take {
                zone: Some("z".into()),
                layer: None,
                template_id: "t".into(),
                data: Some(data),
                rundown_id: None,
            },
        )
        .unwrap();
        let fps = p.frame_rate.as_f64();
        // On the way in (pauses on its way): not counting yet.
        st.tick(20.0);
        assert!(!st.auto_rows(&p));
        st.apply(
            &p,
            Command::Continue {
                zone: Some("z".into()),
                layer: None,
            },
        )
        .unwrap();
        st.tick(20.0);
        st.apply(
            &p,
            Command::Continue {
                zone: Some("z".into()),
                layer: None,
            },
        )
        .unwrap();
        st.tick(100.0);
        let item = st.layers.values().next().unwrap();
        assert_eq!(item.phase, Phase::Holding);
        assert!(!st.auto_rows(&p));
        st.tick(2.0 * fps);
        assert!(st.auto_rows(&p));
        assert_eq!(st.layers.values().next().unwrap().data["ds__row"], "2");
        // The time on a row starts again with each row.
        st.tick(fps);
        assert!(!st.auto_rows(&p));
        st.tick(fps);
        assert!(st.auto_rows(&p));
        st.tick(2.0 * fps);
        assert!(st.auto_rows(&p));
        assert_eq!(st.layers.values().next().unwrap().data["ds__row"], "1");
        // Set by hand: until a trigger, until a time.
        let item = st.layers.values_mut().next().unwrap();
        item.data.insert("x__manual".into(), "trigger:End".into());
        item.data.insert("y__manual".into(), "until:1000".into());
        assert!(!st.expire_manual(999));
        assert!(st.expire_manual(1000));
        st.apply(&p, Command::Trigger { name: "end".into() })
            .unwrap();
        assert!(!st
            .layers
            .values()
            .next()
            .unwrap()
            .data
            .contains_key("x__manual"));
    }

    #[test]
    fn full_cycle() {
        let p = project();
        let mut ch = ChannelState::default();
        ch.apply(
            &p,
            Command::Take {
                zone: None,
                layer: Some(1),
                template_id: "t".into(),
                data: None,
                rundown_id: None,
            },
        )
        .unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        let it = &ch.layers[&1];
        assert_eq!((it.frame, it.phase), (20.0, Phase::Holding));

        ch.apply(
            &p,
            Command::Continue {
                zone: None,
                layer: Some(1),
            },
        )
        .unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        assert_eq!(ch.layers[&1].frame, 40.0);

        // No more pauses: continue goes to the out marker and holds.
        ch.apply(
            &p,
            Command::Continue {
                zone: None,
                layer: Some(1),
            },
        )
        .unwrap();
        for _ in 0..50 {
            ch.tick(1.0);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (80.0, Phase::Holding)
        );

        ch.apply(
            &p,
            Command::TakeOut {
                zone: Some("z".into()),
                layer: None,
            },
        )
        .unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        assert!(ch.layers.is_empty());
    }

    fn take(ch: &mut ChannelState, p: &Project) {
        ch.apply(
            p,
            Command::Take {
                zone: None,
                layer: Some(1),
                template_id: "t".into(),
                data: None,
                rundown_id: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_trigger_takes_out_on_air() {
        let mut p = project();
        let t = &mut p.templates[0];
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        t.markers[0].resume = Resume::Trigger { name: "End".into() };
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        for _ in 0..85 {
            ch.tick(1.0);
            ch.auto_resume(&p);
        }
        assert_eq!(ch.layers[&1].phase, Phase::Holding);
        // Another trigger: it stays; its own: it goes out.
        ch.apply(
            &p,
            Command::Trigger {
                name: "other".into(),
            },
        )
        .unwrap();
        assert_eq!(ch.layers[&1].phase, Phase::Holding);
        ch.apply(&p, Command::Trigger { name: "end".into() })
            .unwrap();
        assert_eq!(ch.layers[&1].phase, Phase::Out);
    }

    /// A Logic graph: its nodes and wires (from, to, port).
    fn graph(
        nodes: Vec<(&str, crate::logic::NodeKind)>,
        edges: &[(&str, &str, &str)],
    ) -> crate::logic::Logic {
        crate::logic::Logic {
            nodes: nodes
                .into_iter()
                .map(|(id, kind)| crate::logic::Node {
                    id: id.into(),
                    kind,
                    x: 0.0,
                    y: 0.0,
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(f, t, p)| crate::logic::Edge {
                    from: (*f).into(),
                    to: (*t).into(),
                    port: (*p).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn the_logic_takes_out_on_air() {
        use crate::logic::{Cmp, NodeKind};
        let mut p = project();
        let t = &mut p.templates[0];
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        t.markers[0].resume = Resume::Triggers;
        t.triggers = graph(
            vec![
                (
                    "f",
                    NodeKind::Field {
                        key: "status".into(),
                    },
                ),
                (
                    "v",
                    NodeKind::Text {
                        value: "final".into(),
                    },
                ),
                ("c", NodeKind::Condition { cmp: Cmp::Eq }),
                ("o", NodeKind::Out {}),
            ],
            &[("f", "c", "a"), ("v", "c", "b"), ("c", "o", "when")],
        );
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        let mut w = EstimatedWidth;
        for _ in 0..85 {
            ch.tick(1.0);
            ch.auto_resume(&p);
            assert!(ch.event_resumes(&p, "UTC", &mut w).is_empty());
        }
        assert_eq!(ch.layers[&1].phase, Phase::Holding);
        ch.layers
            .get_mut(&1)
            .unwrap()
            .data
            .insert("status".into(), "final".into());
        assert_eq!(ch.event_resumes(&p, "UTC", &mut w).len(), 1);
        assert_eq!(ch.layers[&1].phase, Phase::Out);
    }

    #[test]
    fn an_event_node_waits_for_the_change() {
        use crate::logic::{BoolOp, NodeKind};
        let mut p = project();
        let t = &mut p.templates[0];
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        t.markers[0].resume = Resume::Triggers;
        t.triggers = graph(
            vec![
                (
                    "e",
                    NodeKind::Event {
                        on: TriggerEvent::DataChange {
                            key: "score".into(),
                        },
                    },
                ),
                (
                    "g",
                    NodeKind::Event {
                        on: TriggerEvent::Graphic {
                            template: "other".into(),
                            out: false,
                        },
                    },
                ),
                ("or", NodeKind::Bool { op: BoolOp::Or }),
                ("o", NodeKind::Out {}),
            ],
            &[("e", "or", "a"), ("g", "or", "b"), ("or", "o", "when")],
        );
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        ch.layers
            .get_mut(&1)
            .unwrap()
            .data
            .insert("score".into(), "1".into());
        let mut w = EstimatedWidth;
        for _ in 0..90 {
            ch.tick(1.0);
            ch.event_resumes(&p, "UTC", &mut w);
        }
        assert_eq!(ch.layers[&1].phase, Phase::Holding);
        ch.layers
            .get_mut(&1)
            .unwrap()
            .data
            .insert("score".into(), "2".into());
        ch.event_resumes(&p, "UTC", &mut w);
        assert_eq!(ch.layers[&1].phase, Phase::Out);
    }

    #[test]
    fn without_logic_on_the_marker_the_graph_does_nothing() {
        use crate::logic::NodeKind;
        let mut p = project();
        let t = &mut p.templates[0];
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        t.triggers = graph(
            vec![
                ("n", NodeKind::Constant { value: 1.0 }),
                ("o", NodeKind::Out {}),
            ],
            &[("n", "o", "when")],
        );
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        let mut w = EstimatedWidth;
        for _ in 0..90 {
            ch.tick(1.0);
            ch.event_resumes(&p, "UTC", &mut w);
        }
        assert_eq!(ch.layers[&1].phase, Phase::Holding);
    }

    #[test]
    fn a_trigger_node_fires_its_trigger_once() {
        use crate::logic::NodeKind;
        let mut p = project();
        let t = &mut p.templates[0];
        // Its first pause waits for « go »; the Triggers fire it when « ready » is set.
        t.markers[0].resume = Resume::Trigger { name: "go".into() };
        t.triggers = graph(
            vec![
                (
                    "r",
                    NodeKind::Field {
                        key: "ready".into(),
                    },
                ),
                ("f", NodeKind::Fire { name: "go".into() }),
            ],
            &[("r", "f", "when")],
        );
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        let mut w = EstimatedWidth;
        for _ in 0..30 {
            ch.tick(1.0);
            assert!(ch.event_resumes(&p, "UTC", &mut w).is_empty());
        }
        let pause = p.templates[0].markers[0].frame;
        assert_eq!(ch.layers[&1].frame, pause);
        ch.layers
            .get_mut(&1)
            .unwrap()
            .data
            .insert("ready".into(), "1".into());
        let mut fired = 0;
        for _ in 0..10 {
            fired += ch
                .event_resumes(&p, "UTC", &mut w)
                .iter()
                .filter(|d| d.starts_with('⚡'))
                .count();
            ch.tick(1.0);
        }
        assert_eq!(fired, 1);
        assert!(ch.layers[&1].frame > pause);
    }

    #[test]
    fn conditions_compare_numbers_as_numbers() {
        assert!(TriggerEvent::holds(">", "10", "9"));
        assert!(TriggerEvent::holds("<=", "2,5", "2.5"));
        assert!(TriggerEvent::holds("=", "Final", "final"));
        assert!(TriggerEvent::holds("contains", "Score final", "FINAL"));
        assert!(!TriggerEvent::holds("!=", "3", "3.0"));
    }

    #[test]
    fn timed_and_triggered_pauses() {
        let mut p = project();
        let t = &mut p.templates[0];
        t.markers[0].resume = Resume::After { seconds: 0.5 };
        t.markers[1].resume = Resume::Trigger {
            name: "Goal".into(),
        };
        let fps = p.frame_rate.as_f64();
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        for _ in 0..20 {
            ch.tick(1.0);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (20.0, Phase::Holding)
        );
        // Resumes by itself after half a second.
        for _ in 0..(fps * 0.5) as usize + 1 {
            ch.tick(1.0);
            ch.auto_resume(&p);
        }
        for _ in 0..25 {
            ch.tick(1.0);
            ch.auto_resume(&p);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (40.0, Phase::Holding)
        );
        // Waits for its trigger, whatever the delay.
        ch.apply(
            &p,
            Command::Trigger {
                name: "other".into(),
            },
        )
        .unwrap();
        ch.tick(1.0);
        assert_eq!(ch.layers[&1].frame, 40.0);
        ch.apply(
            &p,
            Command::Trigger {
                name: "goal".into(),
            },
        )
        .unwrap();
        for _ in 0..50 {
            ch.tick(1.0);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (80.0, Phase::Holding)
        );
    }

    #[test]
    fn pauses_in_the_out_animation() {
        let mut p = project();
        p.templates[0].markers.push(Marker {
            id: "d".into(),
            frame: 90.0,
            name: "Pause".into(),
            kind: MarkerKind::Pause,
            resume: Resume::Manual,
        });
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        let cont = Command::Continue {
            zone: None,
            layer: Some(1),
        };
        for _ in 0..3 {
            for _ in 0..50 {
                ch.tick(1.0);
            }
            ch.apply(&p, cont.clone()).unwrap();
        }
        assert_eq!(ch.layers[&1].frame, 80.0);
        ch.apply(
            &p,
            Command::TakeOut {
                zone: None,
                layer: Some(1),
            },
        )
        .unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (90.0, Phase::OutHold)
        );
        ch.apply(&p, cont).unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        assert!(ch.layers.is_empty());
    }

    #[test]
    fn auto_out_after_the_hold() {
        let mut p = project();
        let fps = p.frame_rate.as_f64();
        let t = &mut p.templates[0];
        t.markers.retain(|m| m.kind == MarkerKind::Out);
        let hold = t.hold_frame();
        t.markers[0].resume = Resume::After {
            seconds: 10.0 / fps,
        };
        let mut ch = ChannelState::default();
        take(&mut ch, &p);
        for _ in 0..(hold as usize + 5) {
            ch.tick(1.0);
        }
        assert_eq!(
            (ch.layers[&1].frame, ch.layers[&1].phase),
            (hold, Phase::Holding)
        );
        assert!(!ch.auto_resume(&p));
        for _ in 0..10 {
            ch.tick(1.0);
        }
        assert!(ch.auto_resume(&p));
        assert_eq!(ch.layers[&1].phase, Phase::Out);
    }

    #[test]
    fn unknown_template() {
        let mut ch = ChannelState::default();
        let r = ch.apply(
            &project(),
            Command::Take {
                zone: None,
                layer: Some(0),
                template_id: "x".into(),
                data: None,
                rundown_id: None,
            },
        );
        assert_eq!(r, Err(PlayoutError::UnknownTemplate("x".into())));
    }

    #[test]
    fn update_cuts_or_fades() {
        let p = project();
        let mut ch = ChannelState::default();
        let take = Command::Take {
            zone: None,
            layer: Some(0),
            template_id: "t".into(),
            data: None,
            rundown_id: None,
        };
        ch.apply(&p, take).unwrap();
        let update = |v: &str, fade: bool| Command::Update {
            zone: None,
            layer: Some(0),
            data: [("name".to_string(), v.to_string())].into(),
            fade,
        };
        ch.apply(&p, update("A", false)).unwrap();
        assert!(ch.layers[&0].fade.is_none(), "cut");
        ch.apply(&p, update("B", true)).unwrap();
        let f = ch.layers[&0].fade.as_ref().expect("fade");
        assert_eq!(f.data["name"], "A");
        assert_eq!(ch.layers[&0].data["name"], "B");
    }

    #[test]
    fn a_source_change_moves_as_its_binding_says() {
        use crate::model::{Field, FieldBinding, FieldKind};
        assert_eq!(count_value("9", "20", 0.5).as_deref(), Some("15"));
        assert_eq!(count_value("1,5", "2,75", 0.0).as_deref(), Some("1,50"));
        assert_eq!(count_value("9", "20", 1.0).as_deref(), Some("20"));
        assert!(count_value("Nice", "20", 0.5).is_none());
        let mut p = project();
        p.templates[0].fields.push(Field {
            key: "temp".into(),
            label: "Temp".into(),
            kind: FieldKind::Text,
            default: String::new(),
            binding: Some(FieldBinding {
                source: "sheet".into(),
                path: "b2".into(),
                filters: Vec::new(),
                sort: None,
                list: None,
                steps: Vec::new(),
                change: Some(ValueChange::Count),
                change_secs: Some(1.0),
            }),
            min: None,
            max: None,
            options: Vec::new(),
        });
        if let crate::model::LayerKind::Text { text, .. } = &mut p.templates[0].layers[0].kind {
            *text = "{{temp}} °C".into();
        }
        let mut ch = ChannelState::default();
        let take = Command::Take {
            zone: None,
            layer: Some(0),
            template_id: "t".into(),
            data: Some([("temp".to_string(), "9".to_string())].into()),
            rundown_id: None,
        };
        ch.apply(&p, take).unwrap();
        let item = ch.layers.get_mut(&0).unwrap();
        let old = item.data.clone();
        item.data.insert("temp".into(), "20".into());
        live_changes(&p, item, &old);
        assert_eq!(item.changes.len(), 1);
        assert_eq!(shown_data(item)["temp"], "9");
        let fps = p.frame_rate.as_f64();
        ch.tick(fps);
        let item = &ch.layers[&0];
        assert!(item.changes.is_empty(), "done after its second");
        assert_eq!(shown_data(item)["temp"], "20");
        // Not a number: a cross-fade.
        let item = ch.layers.get_mut(&0).unwrap();
        let old = item.data.clone();
        item.data.insert("temp".into(), "N/A".into());
        live_changes(&p, item, &old);
        assert!(item.changes.is_empty() && item.fade.is_some());
    }

    #[test]
    fn a_data_set_value_moves_as_its_layer_says() {
        use crate::model::{LiveChange, Template};
        let mut p = project();
        let t: &mut Template = &mut p.templates[0];
        if let crate::model::LayerKind::Text { text, .. } = &mut t.layers[0].kind {
            *text = "{{scores_team}}".into();
        }
        t.layers[0].live_changes.insert(
            "scores_team".into(),
            LiveChange {
                kind: ValueChange::Slide,
                secs: None,
            },
        );
        let mut other = t.layers[0].clone();
        other.id = "l2".into();
        if let crate::model::LayerKind::Text { text, .. } = &mut other.kind {
            *text = "Hi".into();
        }
        t.layers.push(other);
        let mut ch = ChannelState::default();
        let take = Command::Take {
            zone: None,
            layer: Some(0),
            template_id: "t".into(),
            data: Some([("scores_team".to_string(), "Nice".to_string())].into()),
            rundown_id: None,
        };
        ch.apply(&p, take).unwrap();
        let item = ch.layers.get_mut(&0).unwrap();
        let old = item.data.clone();
        item.data.insert("scores_team".into(), "Lyon".into());
        live_changes(&p, item, &old);
        assert_eq!(item.changes.len(), 1);
        let c = &item.changes[0];
        assert_eq!(
            (c.layer.as_str(), c.kind, c.from.as_str()),
            ("l", ValueChange::Slide, "Nice")
        );
        // Held: the value on air stays.
        p.templates[0].layers[0].live_changes.insert(
            "scores_team".into(),
            LiveChange {
                kind: ValueChange::Hold,
                secs: None,
            },
        );
        let item = ch.layers.get_mut(&0).unwrap();
        item.changes.clear();
        let old = item.data.clone();
        item.data.insert("scores_team".into(), "Metz".into());
        live_changes(&p, item, &old);
        assert_eq!(item.data["scores_team"], "Lyon");
        assert!(item.changes.is_empty());
        // Set to cut (none): at once.
        p.templates[0].layers[0].live_changes.clear();
        let item = ch.layers.get_mut(&0).unwrap();
        item.changes.clear();
        let old = item.data.clone();
        item.data.insert("scores_team".into(), "Lens".into());
        live_changes(&p, item, &old);
        assert!(item.changes.is_empty() && item.fade.is_none());
    }

    #[test]
    fn an_empty_template_is_not_taken() {
        let mut p = project();
        p.templates[0].layers[0].visible = false;
        let mut ch = ChannelState::default();
        let r = ch.apply(
            &p,
            Command::Take {
                zone: None,
                layer: Some(0),
                template_id: "t".into(),
                data: None,
                rundown_id: None,
            },
        );
        assert_eq!(r, Err(PlayoutError::EmptyTemplate("t".into())));
    }

    #[test]
    fn sequential_zone_chains_out_then_in() {
        let p = project();
        let mut ch = ChannelState::default();
        let take = |rid: &str| Command::Take {
            zone: None,
            layer: None,
            template_id: "t".into(),
            data: None,
            rundown_id: Some(rid.into()),
        };
        ch.apply(&p, take("a")).unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        // Second take in the busy zone: "a" goes out, "b" waits.
        ch.apply(&p, take("b")).unwrap();
        let it = &ch.layers[&1];
        assert_eq!((it.phase, it.frame), (Phase::Out, 80.0));
        assert_eq!(it.next.as_ref().unwrap().rundown_id.as_deref(), Some("b"));
        for _ in 0..20 {
            ch.tick(1.0);
        }
        let it = &ch.layers[&1];
        assert_eq!(it.rundown_id.as_deref(), Some("b"));
        assert!(it.next.is_none() && it.frame < 20.0);

        // Cut zones replace at once.
        let mut p = p;
        p.zones[0].transition = ZoneTransition::Cut;
        ch.apply(&p, take("c")).unwrap();
        assert_eq!(ch.layers[&1].rundown_id.as_deref(), Some("c"));
        assert_eq!(ch.layers[&1].frame, 0.0);
    }

    #[test]
    fn fade_zone_crosses_the_two_graphics() {
        let mut p = project();
        p.zones[0].transition = ZoneTransition::Fade;
        let mut ch = ChannelState::default();
        let take = |rid: &str| Command::Take {
            zone: None,
            layer: None,
            template_id: "t".into(),
            data: None,
            rundown_id: Some(rid.into()),
        };
        ch.apply(&p, take("a")).unwrap();
        for _ in 0..30 {
            ch.tick(1.0);
        }
        // The new graphic comes in at once; the one on air fades out under it.
        ch.apply(&p, take("b")).unwrap();
        let it = &ch.layers[&1];
        assert_eq!((it.rundown_id.as_deref(), it.frame), (Some("b"), 0.0));
        let going = it.leaving.as_ref().expect("fading out");
        assert_eq!(going.item.rundown_id.as_deref(), Some("a"));
        assert_eq!(going.opacity(), 1.0);
        assert!(ch.is_animating());
        let len = going.length as usize;
        for _ in 0..len / 2 {
            ch.tick(1.0);
        }
        let o = ch.layers[&1].leaving.as_ref().unwrap().opacity();
        assert!(o > 0.4 && o < 0.6, "{o}");
        for _ in 0..len {
            ch.tick(1.0);
        }
        assert!(ch.layers[&1].leaving.is_none());
    }

    #[test]
    fn migrates_layers_to_zones() {
        let mut p = crate::sample::demo_project();
        p.zones.clear();
        for t in &mut p.templates {
            t.zone.clear();
        }
        for r in &mut p.rundown {
            r.zone.clear();
        }
        assert!(p.ensure_zones());
        assert!(p
            .rundown
            .iter()
            .all(|r| p.zone(&r.zone).map(|z| z.layer) == Some(r.layer)));
        assert!(p.templates.iter().all(|t| p.zone(&t.zone).is_some()));
        assert!(!p.ensure_zones());
    }

    #[test]
    fn hold_anyway_becomes_operator() {
        let mut p = crate::sample::demo_project();
        assert!(!p.ensure_zones());
        p.rundown[0].hold = true;
        p.rundown[0].auto_secs = Some(8.0);
        p.rundown[1].auto_secs = Some(5.0);
        assert!(p.ensure_zones());
        assert!(!p.rundown[0].hold);
        assert_eq!(
            p.rundown[0].auto_secs, None,
            "the operator takes the next one"
        );
        assert_eq!(p.rundown[1].auto_secs, Some(5.0), "others keep their time");
        assert!(!p.ensure_zones());
    }

    #[test]
    fn widget_clocks() {
        let mut p = crate::sample::demo_project();
        let ticker = p.template("ticker").unwrap().clone();
        let crawl = ticker
            .layers
            .iter()
            .find(|l| is_timed(&l.kind))
            .unwrap()
            .clone();
        let mut ch = ChannelState::default();
        ch.apply(&p, Command::take("ticker", "ticker")).unwrap();
        let key = ch.layers.keys().next().copied().unwrap();
        let zone = Some(key);
        let ctl = |c| Command::Widget {
            zone: None,
            layer: zone,
            target: crawl.name.clone(),
            control: c,
        };
        let age = |ch: &ChannelState| ch.layers[&key].layer_age(&crawl);
        ch.tick(10.0);
        assert_eq!(age(&ch), 10.0);
        ch.apply(&p, ctl(WidgetControl::Pause)).unwrap();
        ch.tick(5.0);
        assert_eq!(age(&ch), 10.0);
        ch.apply(&p, ctl(WidgetControl::Toggle)).unwrap();
        ch.tick(5.0);
        assert_eq!(age(&ch), 15.0);
        ch.apply(&p, ctl(WidgetControl::Reset)).unwrap();
        ch.tick(3.0);
        assert_eq!(age(&ch), 3.0);
        assert!(matches!(
            ch.apply(
                &p,
                Command::Widget {
                    zone: None,
                    layer: zone,
                    target: "nope".into(),
                    control: WidgetControl::Start
                }
            ),
            Err(PlayoutError::UnknownWidget(_))
        ));

        // Manual start: waits at zero until started.
        let t = p.templates.iter_mut().find(|t| t.id == "ticker").unwrap();
        t.layers
            .iter_mut()
            .find(|l| l.id == crawl.id)
            .unwrap()
            .manual_start = true;
        let crawl = p
            .template("ticker")
            .unwrap()
            .layers
            .iter()
            .find(|l| l.id == crawl.id)
            .unwrap()
            .clone();
        let mut ch = ChannelState::default();
        ch.apply(&p, Command::take("ticker", "ticker")).unwrap();
        ch.tick(20.0);
        let item = |ch: &ChannelState| ch.layers.values().next().unwrap().layer_age(&crawl);
        assert_eq!(item(&ch), 0.0);
        ch.apply(
            &p,
            Command::Widget {
                zone: None,
                layer: zone,
                target: "*".into(),
                control: WidgetControl::Start,
            },
        )
        .unwrap();
        ch.tick(4.0);
        assert_eq!(item(&ch), 4.0);
    }

    #[test]
    fn continue_serves_the_widgets_before_the_pause() {
        // A table chained on Continue (3 pages) with the template's pause markers.
        let mut p = project();
        let mut c = crate::wid::table::tests::sample(10);
        c.auto_pages = true;
        c.auto_on = crate::wid::table::AutoOn::Continue;
        let table = crate::wid::table::tests::layer(c);
        p.templates[0].layers.push(table.clone());
        let mut ch = ChannelState::default();
        ch.apply(&p, Command::take("z", "t")).unwrap();
        let key = *ch.layers.keys().next().unwrap();
        let cont = Command::Continue {
            zone: Some("z".into()),
            layer: None,
        };
        // Held on the first pause (frame 20); the table on page 1.
        ch.tick(30.0);
        let page = |ch: &ChannelState| {
            let st: crate::wid::table::State = serde_json::from_value(
                ch.layers[&key]
                    .widgets
                    .get(&table.id)
                    .cloned()
                    .unwrap_or_default(),
            )
            .unwrap_or_default();
            crate::wid::table::current_page(&st, 3, 0).0
        };
        assert_eq!(ch.layers[&key].frame, 20.0);
        assert_eq!(page(&ch), 0);
        // F2, F2: the pages turn, the timeline stays on its pause.
        ch.apply(&p, cont.clone()).unwrap();
        assert_eq!(page(&ch), 1);
        assert_eq!(ch.layers[&key].frame, 20.0);
        ch.apply(&p, cont.clone()).unwrap();
        assert_eq!(page(&ch), 2);
        ch.tick(5.0);
        assert_eq!(ch.layers[&key].frame, 20.0);
        // The last page: F2 is the timeline's.
        ch.apply(&p, cont).unwrap();
        ch.tick(5.0);
        assert!(ch.layers[&key].frame > 20.0);
        assert_eq!(page(&ch), 2);

        // A ticker waiting for a trigger starts on it, not on Continue.
        let mut p = crate::sample::demo_project();
        let t = p.templates.iter_mut().find(|t| t.id == "ticker").unwrap();
        let crawl = t.layers.iter_mut().find(|l| is_timed(&l.kind)).unwrap();
        crawl.manual_start = true;
        crawl.start_on = Some("trigger".into());
        crawl.start_trigger = "go".into();
        let crawl = crawl.clone();
        let mut ch = ChannelState::default();
        ch.apply(&p, Command::take("ticker", "ticker")).unwrap();
        let key = *ch.layers.keys().next().unwrap();
        ch.tick(10.0);
        ch.apply(
            &p,
            Command::Continue {
                zone: Some("ticker".into()),
                layer: None,
            },
        )
        .unwrap();
        ch.tick(5.0);
        assert_eq!(ch.layers[&key].layer_age(&crawl), 0.0);
        ch.apply(&p, Command::Trigger { name: "GO".into() })
            .unwrap();
        ch.tick(4.0);
        assert_eq!(ch.layers[&key].layer_age(&crawl), 4.0);
    }

    #[test]
    fn lottie_groups_follow_their_clock() {
        let mut p = crate::sample::demo_project();
        let t = p.templates.iter_mut().find(|t| t.id == "ticker").unwrap();
        let group: Layer = serde_json::from_str(
            r#"{"id":"lot","name":"Anim","type":"group","props":{},"lottie":"/assets/a.json"}"#,
        )
        .unwrap();
        assert!(is_timed(&group.kind));
        t.layers.push(group.clone());
        let hold = t.hold_frame();
        let mut ch = ChannelState::default();
        ch.apply(&p, Command::take("ticker", "ticker")).unwrap();
        let key = *ch.layers.keys().next().unwrap();
        let at = |ch: &ChannelState, frame: f64| ch.layers[&key].lottie_frame(&group, frame, hold);
        ch.tick(4.0);
        // Not controlled: the template's frame.
        assert_eq!(at(&ch, 4.0), 4.0);
        ch.apply(
            &p,
            Command::Widget {
                zone: None,
                layer: Some(key),
                target: "Anim".into(),
                control: WidgetControl::Pause,
            },
        )
        .unwrap();
        ch.tick(6.0);
        // Paused: frozen at 4 while the template plays on (or holds).
        assert!(hold > 4.0);
        assert_eq!(at(&ch, hold), 4.0);
        // The Out plays the template's frames.
        assert_eq!(at(&ch, hold + 5.0), hold + 5.0);
    }

    #[test]
    fn drafts_stay_off_air_until_published() {
        let mut p = crate::sample::demo_project();
        let mut t = p.template("lower-third").unwrap().clone();
        let before = t.duration;
        t.duration = before + 10.0;
        p.save_draft(t);
        // On air still uses the published template; the designer sees the draft.
        assert_eq!(p.template("lower-third").unwrap().duration, before);
        assert_eq!(p.draft("lower-third").unwrap().duration, before + 10.0);
        assert_eq!(p.design_templates().count(), p.templates.len());
        assert_eq!(p.publish("lower-third"), Some(1));
        assert_eq!(p.template("lower-third").unwrap().duration, before + 10.0);
        assert!(p.drafts.is_empty());
        // A new template is a draft until it is published.
        let mut n = p.template("clock").unwrap().clone();
        n.id = "new".into();
        p.save_draft(n);
        assert!(p.template("new").is_none());
        assert_eq!(p.design_templates().count(), p.templates.len() + 1);
        let mut ch = ChannelState::default();
        assert!(ch.apply(&p, Command::take("top-right", "new")).is_err());
        p.publish("new");
        assert!(ch.apply(&p, Command::take("top-right", "new")).is_ok());
    }
}
