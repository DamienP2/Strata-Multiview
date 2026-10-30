//! Social wall: drawing (same picture as `web/src/widgets/socialwall.tsx`),
//! with the scoreboard's painter. The cards are laid out by
//! `cg_core::wid::socialwall::layout`; the badges and avatars drawn here
//! are also the social badge widget's.

use cg_core::wid::socialwall::{
    avatar_color, header_h, initials, layout, line_h, platform, Config, Glyph, Placed, State,
    AVATAR_INK, CITY_WEIGHT, NAME_WEIGHT, TEXT_WEIGHT, TIKTOK_EDGES,
};
use cg_core::LayerKind;

use super::scoreboard::paint::{ellipse, poly, rrect, tf, Pt, T};
use super::Draw;
use crate::cache::Motion;

/// A picture covering the box (cut to the clip in effect); false when missing.
pub(crate) fn cover(p: &mut Pt, src: &str, x: f32, y: f32, w: f32, h: f32) -> bool {
    let Some((iw, ih)) = p.d.image_size(src) else {
        return p.image(src, x, y, w, h);
    };
    let s = (w / iw.max(1) as f32).max(h / ih.max(1) as f32);
    let (bw, bh) = (iw as f32 * s, ih as f32 * s);
    p.image(src, x + (w - bw) / 2.0, y + (h - bh) / 2.0, bw, bh)
}

/// An avatar in the circle (cx, cy, r): the initials on the name's colour,
/// under the picture when there is one.
pub(crate) fn avatar(p: &mut Pt, src: &str, name: &str, cx: f32, cy: f32, r: f32) {
    p.circle(cx, cy, r, avatar_color(name));
    p.text(
        &initials(name),
        cx,
        cy,
        AVATAR_INK,
        T::new((r * 0.8).round(), 800).mid(),
    );
    if !src.trim().is_empty() {
        p.clip(&ellipse(cx, cy, r, r));
        cover(p, src.trim(), cx - r, cy - r, r * 2.0, r * 2.0);
        p.pop();
    }
}

/// The glyph of a platform in the unit square (white, or `c`).
fn glyph(p: &mut Pt, g: Glyph, letter: &str, x: f32, y: f32, s: f32, c: &str, bg: &str) {
    let u = |cmds: Vec<cg_core::wid::shape::Cmd>| tf(&cmds, x, y, s);
    match g {
        Glyph::Play => p.fill(
            &u(poly(&[(0.38, 0.29), (0.38, 0.71), (0.73, 0.5)], true)),
            c,
        ),
        Glyph::Bubble => {
            p.fill(&u(rrect(0.24, 0.22, 0.52, 0.42, 0.06)), c);
            p.fill(&u(poly(&[(0.34, 0.62), (0.34, 0.8), (0.5, 0.62)], true)), c);
            p.fill(&u(rrect(0.4, 0.32, 0.07, 0.17, 0.0)), bg);
            p.fill(&u(rrect(0.55, 0.32, 0.07, 0.17, 0.0)), bg);
        }
        Glyph::F => p.text(
            "f",
            x + s * 0.54,
            y + s * 0.53,
            c,
            T::new(s * 0.78, 800).mid(),
        ),
        Glyph::X => p.text("X", x + s * 0.5, y + s * 0.5, c, T::new(s * 0.6, 700).mid()),
        Glyph::Camera => {
            p.stroke(&u(rrect(0.24, 0.24, 0.52, 0.52, 0.16)), c, s * 0.08, true);
            p.stroke(&u(ellipse(0.5, 0.5, 0.12, 0.12)), c, s * 0.08, true);
            p.fill(&u(ellipse(0.65, 0.35, 0.04, 0.04)), c);
        }
        Glyph::Note => {
            p.fill(&u(ellipse(0.42, 0.66, 0.13, 0.13)), c);
            p.fill(&u(rrect(0.49, 0.2, 0.08, 0.47, 0.0)), c);
            p.fill(
                &u(poly(
                    &[(0.57, 0.2), (0.75, 0.32), (0.75, 0.42), (0.57, 0.31)],
                    true,
                )),
                c,
            );
        }
        Glyph::Phone => {
            p.stroke(&u(ellipse(0.52, 0.48, 0.23, 0.23)), c, s * 0.08, true);
            p.fill(&u(poly(&[(0.22, 0.8), (0.29, 0.6), (0.4, 0.72)], true)), c);
        }
        Glyph::Dots => {
            p.fill(&u(rrect(0.2, 0.24, 0.6, 0.42, 0.1)), c);
            p.fill(&u(poly(&[(0.3, 0.64), (0.3, 0.8), (0.46, 0.64)], true)), c);
            for k in 0..3 {
                p.fill(&u(ellipse(0.36 + k as f32 * 0.14, 0.45, 0.045, 0.045)), bg);
            }
        }
        Glyph::Letter => p.text(
            letter,
            x + s * 0.5,
            y + s * 0.5,
            c,
            T::new(s * 0.6, 800).mid(),
        ),
    }
}

/// A platform's badge: a rounded square of side `s` at (x, y).
pub(crate) fn badge(p: &mut Pt, id: &str, name: &str, x: f32, y: f32, s: f32) {
    let (bg, g) = platform(id);
    p.fill(&rrect(x, y, s, s, s * 0.28), bg);
    let letter: String = name
        .chars()
        .chain(id.chars())
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    if g == Glyph::Note {
        // TikTok: the glyph doubled in cyan and pink at its edges.
        let o = s * 0.05;
        glyph(p, g, &letter, x - o, y - o, s, TIKTOK_EDGES[0], bg);
        glyph(p, g, &letter, x + o, y + o, s, TIKTOK_EDGES[1], bg);
    }
    glyph(p, g, &letter, x, y, s, "#ffffff", bg);
}

/// One card at its place.
fn paint_card(p: &mut Pt, c: &Config, pl: &Placed) {
    let (k, m) = (&pl.card, &pl.msg);
    let (x, y) = (pl.x, pl.y);
    let w = p.d.w;
    p.alpha = pl.alpha.clamp(0.0, 1.0);
    let shape = rrect(x, y, w, k.h, c.radius);
    p.fill(&shape, &c.bg);
    if c.edge {
        p.clip(&shape);
        p.rect(x, y, c.edge_w, k.h, 0.0, platform(&m.platform).0);
        p.pop();
    }
    let top = y + c.pad;
    if c.avatar {
        let r = c.avatar_size / 2.0;
        avatar(p, &m.avatar, &m.name, x + k.left + r, top + r, r);
    }
    let cy = top + header_h(c) / 2.0;
    let x0 = x + k.x0;
    p.text(
        &k.name,
        x0,
        cy,
        &c.name_color,
        T::new(c.name_size, NAME_WEIGHT),
    );
    let gap = c.name_size * 0.3;
    let mut after = x0 + k.name_w + gap;
    if c.badge {
        let bs = cg_core::wid::socialwall::badge_size(c);
        badge(p, &m.platform, &m.platform_name, after, cy - bs / 2.0, bs);
        after += bs + gap;
    }
    if !k.city.is_empty() {
        p.text(
            &k.city,
            after,
            cy,
            &c.muted,
            T::new(c.name_size * 0.85, CITY_WEIGHT),
        );
    }
    let lh = line_h(c);
    let t0 = top + header_h(c) + c.text_size * 0.15;
    for (i, l) in k.lines.iter().enumerate() {
        p.text(
            l,
            x0,
            t0 + lh * (i as f32 + 0.5),
            &c.text_color,
            T::new(c.text_size, TEXT_WEIGHT),
        );
    }
    if let Some((py, pw, ph)) = k.photo {
        let r = c.radius * 0.6;
        p.rect(x0, y + py, pw, ph, r, "#00000014");
        p.clip(&rrect(x0, y + py, pw, ph, r));
        cover(p, &m.photo, x0, y + py, pw, ph);
        p.pop();
    }
    p.alpha = 1.0;
}

fn wall(p: &mut Pt, c: &Config, st: &State) {
    let (w, h, now) = (p.d.w, p.d.h, p.d.time.now_ms);
    let data = p.d.data;
    let cards = {
        let mut dry = Pt::new(p.d, true);
        dry.font = c.font.clone();
        layout(
            c,
            st,
            data,
            w,
            h,
            now,
            &mut |s: &str, size: f32, weight: u16| dry.tw(s, size, weight, 0.0),
        )
    };
    p.font = c.font.clone();
    p.clip(&rrect(0.0, 0.0, w, h, 0.0));
    for pl in &cards {
        paint_card(p, c, pl);
    }
    p.pop();
}

/// What it paints, summed up (cards moving: a new sum each frame).
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<Motion> {
    let LayerKind::SocialWall { x: c } = &d.layer.kind else {
        return None;
    };
    let mut p = Pt::new(d, true);
    p.rec = Some(Default::default());
    wall(&mut p, c, st);
    p.rec.map(|h| Motion::Key(std::hash::Hasher::finish(&h)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::SocialWall { x: c } = &d.layer.kind else {
        return false;
    };
    let mut p = Pt::new(d, false);
    wall(&mut p, c, st);
    true
}

#[cfg(test)]
mod tests {
    use crate::Renderer;

    fn render(layer: serde_json::Value, data: &[(&str, String)]) -> impl Fn(u32, u32) -> [u8; 4] {
        let project = cg_core::sample::demo_project();
        let t: cg_core::Template = serde_json::from_value(serde_json::json!({
            "id": "sw", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
            "layers": [layer]
        }))
        .unwrap();
        let mut r = Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
        let d: cg_core::anim::Data = data
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        let f = r.render_template(&project, &t, "main", 10.0, &d);
        move |x: u32, y: u32| {
            let i = ((y * f.width + x) * 4) as usize;
            [f.data[i], f.data[i + 1], f.data[i + 2], f.data[i + 3]]
        }
    }

    #[test]
    fn cards_and_badges_are_drawn() {
        let list = serde_json::json!([
            {"id": "1", "platform": "youtube", "name": "Sophie L.", "city": "Lyon", "text": "Pourquoi la réforme ?"},
            {"id": "2", "platform": "x", "name": "@claire.v", "text": "Et les retraités ?"}
        ]);
        let layer = serde_json::json!({
            "id": "w", "name": "Mur", "type": "socialWall", "items": "{{wall}}", "bg": "#ffffff",
            "props": {"x": 100.0, "y": 100.0, "width": 600.0, "height": 600.0}
        });
        let px = render(layer, &[("wall", list.to_string())]);
        // The first card's white background, near its top-left corner.
        assert_eq!(px(130, 105)[..3], [255, 255, 255]);
        // The avatar (left of the texts): a coloured, opaque disc.
        let a = px(100 + 16 + 20, 100 + 16 + 20);
        assert!(a[3] > 200 && a != [255, 255, 255, 255], "{a:?}");
        // A red YouTube badge somewhere on the first card's header row.
        let red = (100..700).any(|x| {
            let p = px(x, 100 + 16 + 13);
            p[0] > 220 && p[1] < 40 && p[2] < 80
        });
        assert!(red, "a red badge");
        // Below the two cards: nothing.
        assert_eq!(px(400, 650)[3], 0);
    }
}
