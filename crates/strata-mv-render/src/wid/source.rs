//! Source: a tile of a multiview (same picture as `web/src/widgets/source.tsx`;
//! settings and geometry in `strata-mv-core/src/wid/source.rs`).

use strata_mv_core::signal::SourceView;
use strata_mv_core::wid::source::{
    border_color, layout, no_signal_text, scale, segments, umd_text, Area, Config, Fit, UmdAlign,
};
use strata_mv_core::{Color, LayerKind, TextAlign, VerticalAlign};
use tiny_skia::{FillRule, Mask, PixmapPaint, Transform};

use super::fit::{fitted_path, rect, Fit as TextFit};
use super::Draw;
use crate::cache::Motion;

/// The meters' colours: green, yellow, red, and the track under them.
pub(crate) const METER: [&str; 3] = ["#2bd96b", "#f2c12e", "#ff3b3b"];
pub(crate) const TRACK: &str = "#000000a6";
/// A lamp that is off.
pub(crate) const LAMP_OFF: &str = "#ffffff26";

/// A live picture changes on every frame.
pub(crate) fn motion(_d: &mut Draw) -> Option<Motion> {
    Some(Motion::Moving)
}

fn fill_area(d: &mut Draw, a: Area, r: f32, c: Color, m: Option<&Mask>) {
    rect(d, a[0], a[1], a[2], a[3], r, c, m);
}

pub(crate) fn draw(d: &mut Draw) -> bool {
    let LayerKind::Source { x: c } = &d.layer.kind else {
        return false;
    };
    let c: &Config = c;
    let input = c.input.trim();
    let view: Option<SourceView> = (!input.is_empty())
        .then(|| d.rd.live_source(input))
        .flatten();
    let picture = (!input.is_empty())
        .then(|| d.rd.live_picture(input))
        .flatten();
    let channels = view.as_ref().map_or(0, |v| v.levels.len());
    let l = layout(c, d.w, d.h, channels);
    let tally = view.as_ref().map(|v| v.tally.clone()).unwrap_or_default();
    let boxed = d.boxed();
    let bm = boxed.as_ref();

    // The frame, in the tally's colour, and the picture's background.
    fill_area(
        d,
        [0.0, 0.0, d.w, d.h],
        0.0,
        Draw::hex(border_color(c, &tally)),
        bm,
    );
    fill_area(d, l.inner, 0.0, Draw::hex(&c.background), bm);

    // The picture, in its area.
    let v = l.video;
    match &picture {
        Some(img) if v[2] > 0.0 && v[3] > 0.0 => {
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            let (sx, sy) = match c.fit {
                Fit::Stretch => (v[2] / iw, v[3] / ih),
                Fit::Contain => {
                    let k = (v[2] / iw).min(v[3] / ih);
                    (k, k)
                }
                Fit::Cover => {
                    let k = (v[2] / iw).max(v[3] / ih);
                    (k, k)
                }
            };
            let (pw, ph) = (iw * sx, ih * sy);
            let tf = d.tf.pre_concat(
                Transform::from_translate(v[0] + (v[2] - pw) / 2.0, v[1] + (v[3] - ph) / 2.0)
                    .pre_scale(sx, sy),
            );
            if let Some(m) = d.clip(v[0], v[1], v[2], v[3]) {
                let paint = PixmapPaint {
                    opacity: d.opacity,
                    quality: tiny_skia::FilterQuality::Bilinear,
                    ..Default::default()
                };
                d.pm.draw_pixmap(0, 0, img.as_ref().as_ref(), &paint, tf, Some(&m));
            }
        }
        _ => {
            // No picture: the words, in the middle of the picture's area.
            let s = no_signal_text(c, d.lang);
            let size = (v[3] * 0.09).clamp(10.0, 64.0);
            if let Some(f) = d.font("Inter", 700) {
                let (p, _) = fitted_path(
                    &f,
                    &s,
                    size,
                    size * 0.08,
                    TextAlign::Center,
                    (v[0] + 8.0, v[1], (v[2] - 16.0).max(1.0), v[3]),
                    Some(TextFit {
                        min_size: size * 0.5,
                        min_squeeze: 70.0,
                    }),
                );
                if let Some(p) = p {
                    d.fill(&p, Draw::hex("#ffffff66"), FillRule::Winding, bm);
                }
            }
        }
    }

    // The format, top left.
    if c.badge {
        if let Some(fmt) = view
            .as_ref()
            .map(|v| v.format.clone())
            .filter(|f| !f.is_empty())
        {
            let size = (d.h * 0.045).clamp(9.0, 28.0);
            if let Some(f) = d.font("Inter", 700) {
                let tw = Draw::text_width(&f, &fmt, size, 0.0);
                let pad = (size * 0.4).round();
                let bx = [v[0] + pad, v[1] + pad, tw + 2.0 * pad, size * 1.5];
                fill_area(d, bx, size * 0.25, Draw::hex("#000000b3"), bm);
                d.text(
                    "Inter",
                    700,
                    size,
                    &fmt,
                    Draw::hex("#ffffff"),
                    TextAlign::Center,
                    VerticalAlign::Middle,
                    (bx[0], bx[1], bx[2], bx[3]),
                    bm,
                );
            }
        }
    }

    // The meters.
    if let Some(vw) = &view {
        for (i, a) in l.meters.iter().enumerate() {
            let Some(level) = vw.levels.get(i) else { break };
            fill_area(d, *a, 0.0, Draw::hex(TRACK), bm);
            for (from, to, k) in segments(&c.vu, level) {
                let y = a[1] + a[3] * (1.0 - to);
                let hgt = a[3] * (to - from);
                fill_area(
                    d,
                    [a[0], y, a[2], hgt],
                    0.0,
                    Draw::hex(METER[k as usize]),
                    bm,
                );
            }
            if c.vu.hold && level.hold > c.vu.min {
                let k = scale(&c.vu, level.hold);
                let zone = if level.hold >= c.vu.red {
                    2
                } else if level.hold >= c.vu.yellow {
                    1
                } else {
                    0
                };
                let th = (a[3] * 0.012).clamp(1.0, 3.0);
                let y = (a[1] + a[3] * (1.0 - k)).min(a[1] + a[3] - th);
                fill_area(d, [a[0], y, a[2], th], 0.0, Draw::hex(METER[zone]), bm);
            }
        }
    }

    // The UMD: its bar, its lamps, its text.
    if let Some(bar) = l.umd {
        fill_area(d, bar, 0.0, Draw::hex(&c.umd.background), bm);
        if let Some((pgm, pvw)) = l.lamps {
            let r = pgm[2] * 0.2;
            let on = |lit: bool, col: &str| Draw::hex(if lit { col } else { LAMP_OFF });
            fill_area(d, pgm, r, on(tally.program, &c.tally.program), bm);
            fill_area(d, pvw, r, on(tally.preview, &c.tally.preview), bm);
        }
        let s = umd_text(c, view.as_ref(), d.data);
        if let (Some(t), false) = (l.text, s.is_empty()) {
            let size = (bar[3] * 0.56).max(6.0);
            if let Some(f) = d.font(&c.umd.font, c.umd.weight) {
                let align = match c.umd.align {
                    UmdAlign::Left => TextAlign::Left,
                    UmdAlign::Center => TextAlign::Center,
                    UmdAlign::Right => TextAlign::Right,
                };
                let (p, _) = fitted_path(
                    &f,
                    &s,
                    size,
                    0.0,
                    align,
                    (t[0], t[1], t[2].max(1.0), t[3]),
                    Some(TextFit {
                        min_size: size * 0.6,
                        min_squeeze: 75.0,
                    }),
                );
                let clip = d.clip(t[0], t[1], t[2], t[3]);
                if let Some(p) = p {
                    d.fill(
                        &p,
                        Draw::hex(&c.umd.color),
                        FillRule::Winding,
                        clip.as_ref().or(bm),
                    );
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use strata_mv_core::signal::{Level, SourceView, Tally};
    use tiny_skia::Pixmap;

    use crate::{LiveInputs, Renderer};

    /// One input: a blue picture, on air, two channels (one loud, one quiet).
    struct One;
    impl LiveInputs for One {
        fn picture(&self, id: &str) -> Option<Arc<Pixmap>> {
            (id == "cam").then(|| {
                let mut p = Pixmap::new(160, 90).unwrap();
                p.fill(tiny_skia::Color::from_rgba8(0, 0, 255, 255));
                Arc::new(p)
            })
        }
        fn source(&self, id: &str) -> Option<SourceView> {
            (id == "cam").then(|| SourceView {
                name: "CAM 1".into(),
                format: "1080i50".into(),
                live: true,
                tally: Tally {
                    program: true,
                    ..Default::default()
                },
                levels: vec![
                    Level {
                        peak: -3.0,
                        rms: -10.0,
                        hold: -1.0,
                    },
                    Level::default(),
                ],
            })
        }
    }

    fn render(input: &str) -> crate::Frame {
        let project = strata_mv_core::sample::demo_project();
        let t: strata_mv_core::Template = serde_json::from_value(serde_json::json!({
            "id": "mv", "name": "MV", "duration": 50.0, "markers": [], "zone": "", "fields": [],
            "layers": [{
                "id": "s", "name": "Tile", "type": "source", "input": input, "border": 6.0,
                "props": {"x": 0.0, "y": 0.0, "width": 640.0, "height": 360.0}
            }]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("mv-source-tests"));
        r.set_live(Arc::new(One));
        r.render_template(&project, &t, "main", 0.0, &Default::default())
    }

    fn px(f: &crate::Frame, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * f.width + x) * 4) as usize;
        [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
    }

    #[test]
    fn a_tile_shows_its_picture_tally_umd_and_meters() {
        let f = render("cam");
        // The frame is red (on air), the picture blue.
        let edge = px(&f, 2, 100);
        assert!(edge[0] > 200 && edge[2] < 80, "red frame: {edge:?}");
        let mid = px(&f, 200, 150);
        assert!(mid[2] > 200 && mid[0] < 30, "blue picture: {mid:?}");
        // The UMD bar at the bottom, its on-air lamp lit (left).
        let lamp = px(&f, 6 + 8 + 10, 360 - 6 - 20);
        assert!(lamp[0] > 150 && lamp[1] < 90, "red lamp: {lamp:?}");
        // The first meter lit near the top (-3 dBFS: red), the second dark.
        let x1 = 640 - 6 - 6 - 6 - 2 - 6 + 3;
        let top = px(&f, x1, 40);
        assert!(
            top[0] > 200 && top[1] < 100,
            "red top of the loud meter: {top:?}"
        );
        let quiet = px(&f, 640 - 6 - 6 - 3, 200);
        assert!(
            quiet[1] < 120 && quiet[2] > 60,
            "quiet meter shows its track over the picture: {quiet:?}"
        );
    }

    #[test]
    fn a_tile_without_signal_keeps_its_frame() {
        let f = render("other");
        let edge = px(&f, 2, 100);
        assert!(
            edge[0] < 80 && edge[3] == 255,
            "frame without tally: {edge:?}"
        );
        let mid = px(&f, 100, 60);
        assert!(mid[2] < 60, "no picture: {mid:?}");
    }
}
