//! The scopes (`strata_mv_core::wid::scope`): background, the trace of the
//! input's newest picture (`crate::scopes`), graticule and labels over it,
//! as `web/src/widgets/scope.tsx` draws them.

use strata_mv_core::wid::scope::{layout, trace_area, ScopeKind, Transfer};
use strata_mv_core::LayerKind;
use tiny_skia::{BlendMode, FilterQuality, PixmapPaint, Transform};

use super::broadcast::paint;
use super::Draw;
use crate::scopes::{rgb_of, ScopeRequest};

pub(crate) fn draw(d: &mut Draw) -> bool {
    let LayerKind::Scope { x } = &d.layer.kind else {
        return false;
    };
    let c = x.as_ref().clone();
    let input = c.input.trim().to_string();
    let view = (!input.is_empty())
        .then(|| d.rd.live_source(&input))
        .flatten();
    let video = view.map(|v| v.video).unwrap_or_default();
    let t = Transfer::of(&video.transfer);
    let (under, over) = layout(&c, d.w, d.h, t);
    paint(d, &under);
    let a = trace_area(&c, d.w, d.h);
    // The trace at the size it shows on this output.
    let k = (d.tf.sx.hypot(d.tf.ky)).max(0.05);
    if !input.is_empty() && a[2] > 1.0 && a[3] > 1.0 {
        let r = ScopeRequest {
            kind: c.kind,
            w: (a[2] * k).round() as u32,
            h: (a[3] * k).round() as u32,
            intensity: (c.intensity * 100.0).round().clamp(0.0, 1000.0) as u16,
            color: rgb_of(&c.color),
            transfer: t,
            wide: video.wide(),
        }
        .bounded();
        if let Some(img) = d.rd.live_scope(&input, &r) {
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            // False colour: the picture's shape, fitted and centred.
            let s = (a[2] / iw).min(a[3] / ih);
            let (sx, sy) = if c.kind == ScopeKind::FalseColor {
                (s, s)
            } else {
                (a[2] / iw, a[3] / ih)
            };
            let (pw, ph) = (iw * sx, ih * sy);
            let tf = d.tf.pre_concat(
                Transform::from_translate(a[0] + (a[2] - pw) / 2.0, a[1] + (a[3] - ph) / 2.0)
                    .pre_scale(sx, sy),
            );
            if let Some(m) = d.clip(a[0], a[1], a[2], a[3]) {
                let p = PixmapPaint {
                    opacity: d.opacity,
                    quality: FilterQuality::Bilinear,
                    blend_mode: if c.kind == ScopeKind::FalseColor {
                        BlendMode::SourceOver
                    } else {
                        BlendMode::Screen
                    },
                };
                d.pm.draw_pixmap(0, 0, img.as_ref().as_ref(), &p, tf, Some(&m));
            }
        }
    }
    paint(d, &over);
    true
}
