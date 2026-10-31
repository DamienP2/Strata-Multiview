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
use crate::model::{Layer, LayerKind, Project, Template, ValueChange, ZoneTransition};

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
        LayerKind::Ticker { .. } => true,
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
}

/// Wall-clock time, Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Commands address a zone by id (`zone`) or a raw playout layer (`layer`).
/// A take with neither plays in the template's zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Command {
    /// Puts a multiview on air (on its zone's layer, or the one given).
    #[serde(rename_all = "camelCase")]
    Take {
        #[serde(default)]
        zone: Option<String>,
        #[serde(default)]
        layer: Option<u32>,
        template_id: String,
    },
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

impl ChannelState {
    pub fn apply(&mut self, project: &Project, cmd: Command) -> Result<(), PlayoutError> {
        match cmd {
            Command::Take {
                zone,
                layer,
                template_id,
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
                let take = QueuedTake {
                    template_id,
                    data: t.default_data(),
                    rundown_id: None,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multiview_goes_on_air_and_is_cleared() {
        let p = crate::sample::demo_project();
        let t = &p.templates[0];
        let mut st = ChannelState::default();
        st.apply(&p, Command::take(&t.zone, &t.id)).unwrap();
        let layer = p.zone(&t.zone).unwrap().layer;
        assert_eq!(st.layers[&layer].template_id, t.id);
        // Another one on the same zone replaces it.
        let other = &p.templates[2];
        st.apply(
            &p,
            Command::Take {
                zone: Some(t.zone.clone()),
                layer: None,
                template_id: other.id.clone(),
            },
        )
        .unwrap();
        for _ in 0..500 {
            st.tick(1.0);
        }
        assert_eq!(st.layers[&layer].template_id, other.id);
        st.apply(
            &p,
            Command::Clear {
                zone: None,
                layer: None,
            },
        )
        .unwrap();
        assert!(st.layers.is_empty());
    }

    #[test]
    fn an_unknown_multiview_is_refused() {
        let p = crate::sample::demo_project();
        let mut st = ChannelState::default();
        let e = st
            .apply(
                &p,
                Command::Take {
                    zone: None,
                    layer: Some(1),
                    template_id: "nope".into(),
                },
            )
            .unwrap_err();
        assert_eq!(e, PlayoutError::UnknownTemplate("nope".into()));
    }
}
