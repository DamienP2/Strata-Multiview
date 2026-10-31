//! The broadcast widgets (meters, loudness, timecode, status, alarm
//! journal): the items laid out by `strata-mv-core/src/wid/broadcast.rs`,
//! drawn as `web/src/widgets/broadcast.tsx` draws them.

use strata_mv_core::wid::broadcast::{
    alarm_log, loudness, meters, station_timecode, status, timecode, Item,
};
use strata_mv_core::LayerKind;
use tiny_skia::FillRule;

use super::fit::{fitted_path, rect, Fit as TextFit};
use super::Draw;
use crate::cache::Motion;

/// Their values change by themselves.
pub(crate) fn motion(_d: &mut Draw) -> Option<Motion> {
    Some(Motion::Moving)
}

fn paint(d: &mut Draw, items: &[Item]) {
    let boxed = d.boxed();
    let m = boxed.as_ref();
    for it in items {
        match it {
            Item::Rect { a, radius, color } => {
                rect(d, a[0], a[1], a[2], a[3], *radius, Draw::hex(color), m);
            }
            Item::Text {
                a,
                text,
                size,
                weight,
                mono,
                color,
                align,
            } => {
                if text.is_empty() || a[2] <= 0.0 || a[3] <= 0.0 {
                    continue;
                }
                let family = if *mono { "JetBrains Mono" } else { "Inter" };
                let Some(f) = d.font(family, *weight) else {
                    continue;
                };
                let (p, _) = fitted_path(
                    &f,
                    text,
                    *size,
                    0.0,
                    *align,
                    (a[0], a[1], a[2], a[3]),
                    Some(TextFit {
                        min_size: size * 0.55,
                        min_squeeze: 70.0,
                    }),
                );
                if let Some(p) = p {
                    d.fill(&p, Draw::hex(color), FillRule::Winding, m);
                }
            }
        }
    }
}

pub(crate) fn draw(d: &mut Draw) -> bool {
    let (w, h, lang) = (d.w, d.h, d.lang);
    let items = match &d.layer.kind {
        LayerKind::AudioMeter { x } => {
            let v = d.rd.live_source(x.input.trim());
            meters(x, w, h, v.as_ref())
        }
        LayerKind::Loudness { x } => {
            let v = d.rd.live_source(x.input.trim());
            loudness(x, w, h, v.as_ref(), lang)
        }
        LayerKind::Timecode { x } => {
            let fr = lang.to_lowercase().starts_with("fr");
            let (tc, tag) = if x.input.trim().is_empty() {
                let fps = if d.time.fps > 0.0 { d.time.fps } else { d.fps };
                (
                    station_timecode(d.time.now_ms, d.time.default_tz, fps),
                    (if fr { "STATION" } else { "HOUSE" }).to_string(),
                )
            } else {
                let v = d.rd.live_source(x.input.trim());
                (
                    v.as_ref().map(|v| v.timecode.clone()).unwrap_or_default(),
                    v.map(|v| v.name).unwrap_or_default(),
                )
            };
            timecode(x, w, h, &tc, &tag)
        }
        LayerKind::Status { x } => {
            let s = d.rd.live_system();
            status(x, w, h, &s, lang)
        }
        LayerKind::AlarmLog { x } => {
            let s = d.rd.live_system();
            alarm_log(x, w, h, &s, d.time.default_tz, lang)
        }
        _ => return false,
    };
    paint(d, &items);
    true
}
