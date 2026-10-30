//! The widgets of the spec, one module each: settings (`Config`, flattened
//! into the layer), operator state (`State`) and the actions that change it.
//!
//! Operator actions are a pure function of (layer, state, action, time): the
//! server applies them to the graphic on air (`Command::Act`) and the
//! Designer asks the server to apply them to its own try-out state, so the
//! logic lives here only. Both renderers read the state as JSON.

pub mod chart;
pub mod counter;
pub mod credits;
pub mod ellipse;
pub mod image;
pub mod map;
pub mod pen;
pub mod polygon;
pub mod progress;
pub mod qrcode;
pub mod quad;
pub mod scoreboard;
pub mod shape;
pub mod socialbadge;
pub mod socialwall;
pub mod table;
pub mod text;
pub mod ticker;
pub mod timer;
pub mod typewriter;
pub mod web;

use serde_json::Value;

use crate::anim::Data;
use crate::model::{Layer, LayerKind};

/// Inputs of an action besides its arguments.
#[derive(Debug, Clone, Copy)]
pub struct ActCtx<'a> {
    /// Wall-clock time, Unix milliseconds (timestamp-based widgets never drift).
    pub now_ms: i64,
    /// Seconds the graphic has been on air.
    pub age_secs: f64,
    /// Frames per second of the channel (a crawl moves a whole number of
    /// pixels per frame).
    pub fps: f64,
    /// Values of the graphic.
    pub data: &'a Data,
}

/// Name of the widget module drawing this layer, if it is one of the spec's.
pub fn widget_id(kind: &LayerKind) -> Option<&'static str> {
    Some(match kind {
        LayerKind::Rect { .. } => "quad",
        LayerKind::Polygon { .. } => "polygon",
        LayerKind::Ellipse { .. } => "ellipse",
        LayerKind::Text { .. } => "text",
        LayerKind::Image { .. } => "image",
        LayerKind::Pen { .. } => "pen",
        LayerKind::Clock { .. } => "timer",
        LayerKind::Table { .. } => "table",
        LayerKind::Ticker { .. } => "ticker",
        LayerKind::Credits { .. } => "credits",
        LayerKind::Typewriter { .. } => "typewriter",
        LayerKind::Counter { .. } => "counter",
        LayerKind::Progress { .. } => "progress",
        LayerKind::Chart { .. } => "chart",
        LayerKind::Map { .. } => "map",
        LayerKind::QrCode { .. } => "qrcode",
        LayerKind::Scoreboard { .. } => "scoreboard",
        LayerKind::Web { .. } => "web",
        LayerKind::SocialWall { .. } => "socialwall",
        LayerKind::SocialBadge { .. } => "socialbadge",
        _ => return None,
    })
}

/// Applies `op` to the layer's state (`Null` = the widget's default state).
pub fn act(
    layer: &Layer,
    state: &mut Value,
    op: &str,
    args: &Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    macro_rules! go {
        ($m:ident) => {{
            let mut st: $m::State = serde_json::from_value(state.clone()).unwrap_or_default();
            $m::act(layer, &mut st, op, args, ctx)?;
            *state = serde_json::to_value(st).map_err(|e| e.to_string())?;
            Ok(())
        }};
    }
    match &layer.kind {
        LayerKind::Rect { .. } => go!(quad),
        LayerKind::Polygon { .. } => go!(polygon),
        LayerKind::Ellipse { .. } => go!(ellipse),
        LayerKind::Text { .. } => go!(text),
        LayerKind::Image { .. } => go!(image),
        LayerKind::Pen { .. } => go!(pen),
        LayerKind::Clock { .. } => go!(timer),
        LayerKind::Table { .. } => go!(table),
        LayerKind::Ticker { .. } => go!(ticker),
        LayerKind::Credits { .. } => go!(credits),
        LayerKind::Typewriter { .. } => go!(typewriter),
        LayerKind::Counter { .. } => go!(counter),
        LayerKind::Progress { .. } => go!(progress),
        LayerKind::Chart { .. } => go!(chart),
        LayerKind::Map { .. } => go!(map),
        LayerKind::QrCode { .. } => go!(qrcode),
        LayerKind::Scoreboard { .. } => go!(scoreboard),
        LayerKind::Web { .. } => go!(web),
        LayerKind::SocialWall { .. } => go!(socialwall),
        LayerKind::SocialBadge { .. } => go!(socialbadge),
        _ => Err("this layer has no operator actions".into()),
    }
}

/// Continue (F2) pressed on the graphic: a widget waiting for it goes on
/// (its next page, step, view, its start…) and takes the press. True when
/// the layer's state changed (the timeline's pause then stays).
pub fn on_continue(layer: &Layer, state: &mut Value, ctx: &ActCtx) -> bool {
    macro_rules! go {
        ($m:ident) => {{
            let mut st: $m::State = serde_json::from_value(state.clone()).unwrap_or_default();
            if !$m::on_continue(layer, &mut st, ctx) {
                return false;
            }
            match serde_json::to_value(st) {
                Ok(v) => {
                    *state = v;
                    true
                }
                Err(_) => false,
            }
        }};
    }
    match &layer.kind {
        LayerKind::Table { .. } => go!(table),
        LayerKind::Image { .. } => go!(image),
        LayerKind::Chart { .. } => go!(chart),
        LayerKind::Map { .. } => go!(map),
        LayerKind::Typewriter { .. } => go!(typewriter),
        LayerKind::Credits { .. } => go!(credits),
        _ => false,
    }
}

/// The trigger `name` fired: a widget waiting for it goes on. True when the
/// layer's state changed.
pub fn on_trigger(layer: &Layer, state: &mut Value, name: &str, ctx: &ActCtx) -> bool {
    macro_rules! go {
        ($m:ident) => {{
            let mut st: $m::State = serde_json::from_value(state.clone()).unwrap_or_default();
            if !$m::on_trigger(layer, &mut st, name, ctx) {
                return false;
            }
            match serde_json::to_value(st) {
                Ok(v) => {
                    *state = v;
                    true
                }
                Err(_) => false,
            }
        }};
    }
    match &layer.kind {
        LayerKind::Table { .. } => go!(table),
        LayerKind::Image { .. } => go!(image),
        LayerKind::Chart { .. } => go!(chart),
        LayerKind::Map { .. } => go!(map),
        LayerKind::Typewriter { .. } => go!(typewriter),
        LayerKind::Credits { .. } => go!(credits),
        _ => false,
    }
}

/// The two names match as trigger names do (trimmed, any case).
pub fn same_trigger(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}

/// The graphic's data changed on air by its data sources (`old` →
/// `ctx.data`): only the widgets that play a change of their list react
/// (the social wall); the others follow the source as it is. True when the
/// layer's state changed.
pub fn on_source_data(layer: &Layer, state: &mut Value, old: &Data, ctx: &ActCtx) -> bool {
    match &layer.kind {
        LayerKind::SocialWall { .. } => on_data(layer, state, old, ctx),
        _ => false,
    }
}

/// The graphic's data changed on air (`old` → `ctx.data`, by an Update, a
/// data set row, a connector): widgets that keep what they show react (a
/// chart freezes the data of the take or tweens to the new values). True
/// when the layer's state changed.
pub fn on_data(layer: &Layer, state: &mut Value, old: &Data, ctx: &ActCtx) -> bool {
    match &layer.kind {
        LayerKind::Chart { .. } => {
            let mut st: chart::State = serde_json::from_value(state.clone()).unwrap_or_default();
            if !chart::on_data(layer, &mut st, old, ctx) {
                return false;
            }
            match serde_json::to_value(st) {
                Ok(v) => {
                    *state = v;
                    true
                }
                Err(_) => false,
            }
        }
        LayerKind::SocialWall { .. } => {
            let mut st: socialwall::State =
                serde_json::from_value(state.clone()).unwrap_or_default();
            if !socialwall::on_data(layer, &mut st, old, ctx) {
                return false;
            }
            match serde_json::to_value(st) {
                Ok(v) => {
                    *state = v;
                    true
                }
                Err(_) => false,
            }
        }
        _ => false,
    }
}
