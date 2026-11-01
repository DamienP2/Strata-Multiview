//! The widgets of the spec, one module each: settings (`Config`, flattened
//! into the layer), operator state (`State`) and the actions that change it.
//!
//! Operator actions are a pure function of (layer, state, action, time): the
//! server applies them to the multiview on air (`Command::Act`) and the
//! Designer asks the server to apply them to its own try-out state, so the
//! logic lives here only. Both renderers read the state as JSON.

pub mod broadcast;
pub mod chart;
pub mod ellipse;
pub mod image;
pub mod polygon;
pub mod quad;
pub mod scope;
pub mod shape;
pub mod signal_info;
pub mod source;
pub mod text;
pub mod ticker;
pub mod timer;
pub mod value;

use serde_json::Value;

use crate::anim::Data;
use crate::model::{Layer, LayerKind};

/// Inputs of an action besides its arguments.
#[derive(Debug, Clone, Copy)]
pub struct ActCtx<'a> {
    /// Wall-clock time, Unix milliseconds (timestamp-based widgets never drift).
    pub now_ms: i64,
    /// Seconds the multiview has been on air.
    pub age_secs: f64,
    /// Frames per second of the channel (a crawl moves a whole number of
    /// pixels per frame).
    pub fps: f64,
    /// Values of the multiview.
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
        LayerKind::Clock { .. } => "timer",
        LayerKind::Ticker { .. } => "ticker",
        LayerKind::Source { .. } => "source",
        LayerKind::AudioMeter { .. } => "audioMeter",
        LayerKind::Loudness { .. } => "loudness",
        LayerKind::Timecode { .. } => "timecode",
        LayerKind::Status { .. } => "status",
        LayerKind::AlarmLog { .. } => "alarmLog",
        LayerKind::Scope { .. } => "scope",
        LayerKind::SignalInfo { .. } => "signalInfo",
        LayerKind::Chart { .. } => "chart",
        LayerKind::Value { .. } => "value",
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
        LayerKind::Clock { .. } => go!(timer),
        LayerKind::Ticker { .. } => go!(ticker),
        _ => Err("this layer has no operator actions".into()),
    }
}

/// Continue (F2) pressed on the multiview: a widget waiting for it goes on
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
        LayerKind::Image { .. } => go!(image),
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
        LayerKind::Image { .. } => go!(image),
        _ => false,
    }
}

/// The two names match as trigger names do (trimmed, any case).
pub fn same_trigger(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}
