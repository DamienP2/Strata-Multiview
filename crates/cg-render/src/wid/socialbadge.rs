//! Social badge: drawing (same picture as `web/src/widgets/socialbadge.tsx`):
//! the social wall's badge or avatar, as large as the box allows, centred.

use cg_core::anim::interpolate_text;
use cg_core::wid::socialbadge::{Show, State};
use cg_core::LayerKind;

use super::scoreboard::paint::Pt;
use super::socialwall::{avatar, badge};
use super::Draw;
use crate::cache::Motion;

fn paint(p: &mut Pt) {
    let layer = p.d.layer;
    let LayerKind::SocialBadge { x: c } = &layer.kind else {
        return;
    };
    let data = p.d.data;
    let (w, h) = (p.d.w, p.d.h);
    let s = w.min(h);
    p.font = c.font.clone();
    match c.show {
        Show::Platform => {
            let id = interpolate_text(&c.platform, data);
            badge(p, id.trim(), "", (w - s) / 2.0, (h - s) / 2.0, s);
        }
        Show::Avatar => {
            let src = interpolate_text(&c.avatar, data);
            let name = interpolate_text(&c.author, data);
            avatar(p, &src, &name, w / 2.0, h / 2.0, s / 2.0);
        }
    }
}

/// What it paints, summed up (a picture coming in changes it).
pub(crate) fn motion(d: &mut Draw, _st: &State) -> Option<Motion> {
    if !matches!(d.layer.kind, LayerKind::SocialBadge { .. }) {
        return None;
    }
    let mut p = Pt::new(d, true);
    p.rec = Some(Default::default());
    paint(&mut p);
    p.rec.map(|h| Motion::Key(std::hash::Hasher::finish(&h)))
}

pub(crate) fn draw(d: &mut Draw, _st: &State) -> bool {
    if !matches!(d.layer.kind, LayerKind::SocialBadge { .. }) {
        return false;
    }
    paint(&mut Pt::new(d, false));
    true
}
