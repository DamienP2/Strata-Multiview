//! Scoreboard: drawing (same picture as `web/src/widgets/scoreboard.tsx`,
//! line for line). The board is laid out for a row height, then drawn at
//! the largest row height that fits the layer's box (top left).

#[path = "scoreboard_paint.rs"]
pub(crate) mod paint;

use cg_core::wid::scoreboard::{
    card_color, clock_on, clock_text, fill, fmt_left, period_text, scores, shootout_on, short_of,
    shot_on, shot_text, susps_left, tennis_pts, tennis_tag, tr, Config, Crest, Kind, Layout, Order,
    ScoreAnim, State, Team, Version,
};
use cg_core::wid::shape::Cmd;
use cg_core::LayerKind;

use self::paint::{col, ink, mix, poly, rrect, tf, Pt, T};
use super::Draw;

const TAG_BG: &str = "#11142a";
const CELL_BG: &str = "#0b0d18";
const BG: &str = "#05060c";

/// Colours of the board from its background: base, score cells, clock / period cells.
struct Pal {
    bg: String,
    cell: String,
    tag: String,
}

fn pal(c: &Config) -> Pal {
    let bg = c.bg.trim();
    if bg.is_empty() || bg.eq_ignore_ascii_case(BG) {
        return Pal {
            bg: BG.into(),
            cell: CELL_BG.into(),
            tag: TAG_BG.into(),
        };
    }
    use cg_core::wid::map::hex_mix;
    Pal {
        bg: bg.into(),
        cell: hex_mix(bg, "#ffffff", 0.04),
        tag: hex_mix(bg, "#ffffff", 0.08),
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}
fn ease(k: f32) -> f32 {
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}
fn ease_out(k: f32) -> f32 {
    1.0 - (1.0 - k).powi(3)
}

/// Generated crest (shield, round, diamond with the initial) or the team's picture, `size` px square.
fn crest(p: &mut Pt, t: &Team, x: f32, y: f32, size: f32, data: &cg_core::anim::Data) {
    let logo = cg_core::anim::interpolate_text(&t.logo, data);
    if !logo.trim().is_empty() && p.image(&logo, x, y, size, size) {
        return;
    }
    let s = size / 100.0;
    let c1 = if col(&t.c1).0[3] > 0.0 {
        t.c1.clone()
    } else {
        "#334155".into()
    };
    let c2 = if col(&t.c2).0[3] > 0.0 {
        t.c2.clone()
    } else {
        "#ffffff".into()
    };
    let letter: String = short_of(t)
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or("?".into());
    let (shape, band, ty, fs): (Vec<Cmd>, Option<Vec<Cmd>>, f32, f32) = match t.crest {
        Crest::Round => (paint::ellipse(50.0, 50.0, 44.0, 44.0), None, 50.0, 44.0),
        Crest::Diamond => (
            poly(
                &[(50.0, 4.0), (95.0, 50.0), (50.0, 96.0), (5.0, 50.0)],
                true,
            ),
            Some(poly(
                &[(27.0, 47.5), (73.0, 47.5), (73.0, 52.5), (27.0, 52.5)],
                true,
            )),
            50.0,
            44.0,
        ),
        Crest::Shield => (
            vec![
                Cmd::Move(50.0, 4.0),
                Cmd::Line(92.0, 16.0),
                Cmd::Line(92.0, 48.0),
                Cmd::Cubic(92.0, 74.0, 72.0, 90.0, 50.0, 97.0),
                Cmd::Cubic(28.0, 90.0, 8.0, 74.0, 8.0, 48.0),
                Cmd::Line(8.0, 16.0),
                Cmd::Close,
            ],
            Some(poly(
                &[(8.0, 30.0), (92.0, 62.0), (92.0, 48.0), (8.0, 16.0)],
                true,
            )),
            58.0,
            40.0,
        ),
    };
    let sh = tf(&shape, x, y, s);
    p.fill(&sh, &c1);
    if let Some(b) = band {
        let a = p.alpha;
        p.alpha = a * 0.9;
        p.fill(&tf(&b, x, y, s), &c2);
        p.alpha = a;
    }
    if t.crest == Crest::Round {
        let a = p.alpha;
        p.alpha = a * 0.7;
        p.stroke(
            &tf(&paint::ellipse(50.0, 50.0, 31.0, 31.0), x, y, s),
            &c2,
            3.0 * s,
            false,
        );
        p.alpha = a;
    }
    p.stroke(&sh, &c2, 7.0 * s, true);
    p.text_halo(
        &letter,
        x + 50.0 * s,
        y + ty * s,
        col(ink(&c1)),
        col(&c1),
        5.0 * s,
        T::new(fs * s, 800).mid(),
    );
}

/// Possession arrow or serve ball, centred on (cx, cy).
fn poss_mark(p: &mut Pt, serve: bool, cx: f32, cy: f32, h: f32, mirror: bool) {
    if serve {
        p.circle(cx, cy, h * 0.24, "#84cc16");
        p.circle(cx - h * 0.03, cy - h * 0.03, h * 0.17, "#d9f99d");
    } else {
        let (w, hh) = (h * 0.32, h * 0.22);
        let d = if mirror { -1.0 } else { 1.0 };
        p.fill(
            &poly(
                &[
                    (cx - d * w / 2.0, cy - hh),
                    (cx + d * w / 2.0, cy),
                    (cx - d * w / 2.0, cy + hh),
                ],
                true,
            ),
            "#facc15",
        );
    }
}

/// One stats item of the full version.
enum Item {
    Txt(String, &'static str, u16),
    Dot(bool),
    Card(&'static str),
    /// A pill: text, background.
    Badge(String, &'static str),
    /// A kick of the shoot-out: scored, missed, to come.
    Pen(Option<bool>),
    Sep,
}

fn stats(c: &Config, st: &State, i: usize, lang: &str, now: i64) -> Vec<Item> {
    let pr = c.p();
    let t = st.team(i);
    let mut groups: Vec<Vec<Item>> = Vec::new();
    if shootout_on(c, st) {
        let n = t.pens.len().max(5);
        let mut g: Vec<Item> = (0..n).map(|k| Item::Pen(t.pens.get(k).copied())).collect();
        let goals = t.pens.iter().filter(|p| **p).count();
        g.push(Item::Txt(format!(" {goals}"), "#ffffff", 800));
        groups.push(g);
    }
    if c.fields.fouls {
        let mut g = vec![
            Item::Txt(format!("{} ", tr(c, lang, "FOULS")), "#ffffffbf", 700),
            Item::Txt(t.fouls.to_string(), "#ffffff", 800),
        ];
        if pr.bonus > 0 && t.fouls >= pr.bonus {
            g.push(Item::Badge(tr(c, lang, "BONUS"), "#ff3b5c"));
        }
        groups.push(g);
    }
    if c.fields.timeouts && pr.timeouts > 0 {
        let mut g = vec![Item::Txt(
            format!("{} ", tr(c, lang, "TO")),
            "#ffffffbf",
            700,
        )];
        let left = pr.timeouts.saturating_sub(t.to_used);
        for k in 0..pr.timeouts {
            g.push(Item::Dot(k < left));
        }
        groups.push(g);
    }
    if c.fields.cards && !pr.cards.is_empty() {
        let mut g = Vec::new();
        for card in pr.cards {
            let n = t.cards(card);
            if n > 0 {
                g.push(Item::Card(card_color(card)));
                if n > 1 {
                    g.push(Item::Txt(format!("×{n} "), "#ffffff", 700));
                }
            }
        }
        if !g.is_empty() {
            groups.push(g);
        }
    }
    if c.fields.susp {
        let g: Vec<Item> = susps_left(c, st, i, now)
            .into_iter()
            .map(|ms| Item::Badge(fmt_left(ms), "#0284c7"))
            .collect();
        if !g.is_empty() {
            groups.push(g);
        }
    }
    let mut out = Vec::new();
    for (k, g) in groups.into_iter().enumerate() {
        if k > 0 {
            out.push(Item::Sep);
        }
        out.extend(g);
    }
    out
}

/// Draws a stats line in (x, w) centred on cy; `right`: aligned right.
#[allow(clippy::too_many_arguments)]
fn draw_stats(
    p: &mut Pt,
    items: &[Item],
    x: f32,
    w: f32,
    cy: f32,
    fs: f32,
    color: &str,
    right: bool,
) {
    let widths: Vec<f32> = items
        .iter()
        .map(|it| match it {
            Item::Txt(s, _, wt) => p.tw(s, fs, *wt, 0.0),
            Item::Dot(_) => fs * 0.55 + 3.0,
            Item::Card(_) => fs * 0.6 + 3.0,
            Item::Badge(s, _) => p.tw(s, fs * 0.8, 800, 0.0) + 12.0 + 6.0,
            Item::Pen(_) => fs * 0.62 + 3.0,
            Item::Sep => fs * 0.9,
        })
        .collect();
    let total: f32 = widths.iter().sum();
    let mut cx = if right { x + w - total } else { x };
    let white = color == "#ffffff";
    for (it, iw) in items.iter().zip(widths) {
        match it {
            Item::Txt(s, c, wt) => {
                let c = if white { *c } else { color };
                p.text(s, cx, cy, c, T::new(fs, *wt));
            }
            Item::Dot(on) => p.circle(
                cx + fs * 0.275,
                cy,
                fs * 0.275,
                if *on { "#facc15" } else { "#ffffff33" },
            ),
            Item::Card(c) => p.rect(cx, cy - fs * 0.425, fs * 0.6, fs * 0.85, 2.0, c),
            Item::Pen(k) => {
                let r = fs * 0.31;
                match k {
                    Some(true) => p.circle(cx + r, cy, r, "#22c55e"),
                    Some(false) => p.circle(cx + r, cy, r, "#ef4444"),
                    None => p.circle(cx + r, cy, r, "#ffffff33"),
                }
            }
            Item::Badge(s, bg) => {
                p.rect(cx + 6.0, cy - fs * 0.5, iw - 6.0, fs, 4.0, bg);
                p.text(
                    s,
                    cx + 6.0 + (iw - 6.0) / 2.0,
                    cy,
                    "#ffffff",
                    T::new(fs * 0.8, 800).mid(),
                );
            }
            Item::Sep => p.text("|", cx + iw / 2.0, cy, "#ffffff4d", T::new(fs, 700).mid()),
        }
        cx += iw;
    }
}

/// Lays out (and unless dry, draws) the board at row height `h`; returns its size.
pub(crate) fn board(
    p: &mut Pt,
    c: &Config,
    st: &State,
    data: &cg_core::anim::Data,
    now: i64,
    hh: f32,
    lang: &str,
) -> (f32, f32) {
    let pr = c.p();
    let pl = pal(c);
    let shot = if shot_on(c, st) {
        Some(shot_text(c, st, now))
    } else {
        None
    };
    let h = hh;
    let n = c.team_count();
    let order: Vec<usize> = if st.swapped {
        (0..n).rev().collect()
    } else {
        (0..n).collect()
    };
    let full = st.version(c) == Version::Full;
    let sc = scores(c, st, data, now);
    let sfs = h * 0.62;
    let nfs = h * 0.4;
    let digits = c.digits.clamp(1, 4) as f32;
    let zero = p.tw("0", sfs, 800, 0.0);
    let reserve = digits * zero;
    let score_w = reserve + h * 0.5;
    let name_w = if full {
        h * c.name_w.clamp(1.0, 20.0) as f32
    } else {
        p.tw("WWW", nfs, 800, 0.0) + h * 0.3
    };
    let crest_w = if c.show_crest { h } else { 0.0 };
    let bar_w = h * 0.12;
    let poss_w = if c.fields.poss { h * 0.42 } else { 0.0 };
    let sets_w = if c.fields.sets && pr.kind == Kind::Sets {
        h * 0.7
    } else {
        0.0
    };
    let ck = if clock_on(c, st) {
        Some(clock_text(c, st, now))
    } else {
        None
    };
    let per = if c.fields.period {
        tr(c, lang, &period_text(c, st))
    } else {
        String::new()
    };
    let serve = pr.poss == "Serve";
    let radius = c.radius as f32;
    let gap = h * 0.0375;

    // Name: the long name when it fits, else the short one.
    let name_of = |p: &mut Pt, t: &Team, w: f32| -> String {
        let long = t.name.to_uppercase();
        if !full || p.tw(&long, nfs, 800, nfs * 0.02) > w {
            short_of(t)
        } else {
            long
        }
    };
    let name_block = |p: &mut Pt, t: &Team, x: f32, y: f32, w: f32, right: bool| {
        let s = name_of(p, t, w - h * 0.3);
        p.clip(&rrect(x, y, w, h, 0.0));
        let o = T::new(nfs, 800).ls(nfs * 0.02);
        if right {
            p.text(&s, x + w - h * 0.15, y + h / 2.0, ink(&t.c1), o.end());
        } else {
            p.text(&s, x + h * 0.15, y + h / 2.0, ink(&t.c1), o);
        }
        p.pop();
    };
    let score_cell = |p: &mut Pt, i: usize, t: &Team, x: f32, y: f32, w: f32| {
        let txt = sc.get(i).copied().unwrap_or(0).to_string();
        let real = p.tw(&txt, sfs, 800, 0.0);
        let sx = if real > reserve { reserve / real } else { 1.0 };
        let mut bg = col(&pl.cell);
        let mut fg = col("#ffffff");
        let mut scale = 1.0;
        let fx = st.fx.sc.get(&i);
        let k = fx.map(|f| (now - f.at) as f32 / 700.0).unwrap_or(1.0);
        p.clip(&rrect(x, y, w, h, 0.0));
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        match (fx, c.anim.score) {
            (Some(f), ScoreAnim::Flip) if (0.0..1.0).contains(&k) => {
                p.rect_c(x, y, w, h, 0.0, bg);
                let (s, sy) = if k < 0.5 {
                    (f.from.clone(), (k * std::f32::consts::PI).cos())
                } else {
                    (txt.clone(), ((k - 1.0) * std::f32::consts::PI).cos())
                };
                let r2 = p.tw(&s, sfs, 800, 0.0);
                let sx2 = if r2 > reserve { reserve / r2 } else { 1.0 };
                p.text_c(&s, cx, cy, fg, T::new(sfs, 800).mid().scale(sx2, sy));
                p.rect(x, cy - h * 0.005, w, h * 0.01, 0.0, "#ffffff2e");
                p.pop();
                return;
            }
            (Some(_), ScoreAnim::Highlight) if (0.0..1.0).contains(&k) => {
                let kk = (std::f32::consts::PI * (k * 1.2).min(1.0)).sin();
                bg = mix(bg, col(&t.c1), kk);
                scale = 1.0 + 0.18 * kk;
                if kk > 0.4 {
                    fg = col(ink(&t.c1));
                }
            }
            _ => {}
        }
        p.rect_c(x, y, w, h, 0.0, bg);
        p.text_c(
            &txt,
            cx,
            cy,
            fg,
            T::new(sfs * scale, 800).mid().scale(sx, 1.0),
        );
        p.pop();
    };

    let (w0, h0);
    if pr.kind == Kind::Tennis {
        let tn = &st.tennis;
        let cell_w = h * 0.72;
        let g_w = h * 0.85;
        let p_w = h * 1.25;
        let slots = c.best_of.clamp(3, 5) as usize - 1;
        let row_w = bar_w + poss_w + crest_w + name_w + slots as f32 * cell_w + g_w + p_w;
        let hd = h * 0.56;
        let tag = tr(c, lang, tennis_tag(tn));
        w0 = row_w;
        h0 = hd + 2.0 * h + 2.0 * gap;
        p.clip(&rrect(0.0, 0.0, w0, h0, radius));
        p.rect(0.0, 0.0, w0, hd, 0.0, &pl.tag);
        let mut x = h * 0.25;
        let pfs = h * 0.3;
        p.text(
            &per,
            x,
            hd / 2.0,
            "#ffffff",
            T::new(pfs, 800).ls(pfs * 0.08),
        );
        x += p.tw(&per, pfs, 800, pfs * 0.08) + h * 0.3;
        if !tag.is_empty() {
            let tw = p.tw(&tag, pfs, 800, pfs * 0.08);
            let bg = if tn.tb {
                "#ff3b5c"
            } else if tn.free {
                "#f59e0b"
            } else {
                "#7c5cff"
            };
            p.rect(x, hd / 2.0 - pfs * 0.75, tw + h * 0.3, pfs * 1.5, 5.0, bg);
            p.text(
                &tag,
                x + h * 0.15,
                hd / 2.0,
                "#ffffff",
                T::new(pfs, 800).ls(pfs * 0.08),
            );
        }
        let mut y = hd;
        for &i in order.iter().take(2) {
            let t = c.team(i);
            y += gap;
            p.rect(0.0, y - gap, w0, gap, 0.0, "#1a1d30");
            p.rect(0.0, y, w0, h, 0.0, &pl.cell);
            p.rect(0.0, y, bar_w, h * 0.7, 0.0, &t.c1);
            p.rect(0.0, y + h * 0.7, bar_w, h * 0.3, 0.0, &t.c2);
            let mut x = bar_w;
            p.rect(x, y, poss_w + crest_w + name_w, h, 0.0, &t.c1);
            if poss_w > 0.0 && st.poss == i {
                poss_mark(p, serve, x + poss_w / 2.0, y + h / 2.0, h, false);
            }
            x += poss_w;
            if crest_w > 0.0 {
                crest(
                    p,
                    &t,
                    x + (crest_w - h * 0.74) / 2.0,
                    y + h * 0.13,
                    h * 0.74,
                    data,
                );
            }
            x += crest_w;
            name_block(p, &t, x, y, name_w, false);
            x += name_w;
            for k in 0..slots {
                p.rect(x, y, cell_w, h, 0.0, "#ffffff0a");
                p.rect(x, y, 1.0, h, 0.0, "#ffffff14");
                if let Some(s) = tn.sets.get(k) {
                    let won = s[i] > s[1 - i];
                    p.text(
                        &s[i].to_string(),
                        x + cell_w / 2.0,
                        y + h / 2.0,
                        if won { "#ffffff" } else { "#ffffff8c" },
                        T::new(h * 0.46, if won { 800 } else { 600 }).mid(),
                    );
                }
                x += cell_w;
            }
            p.rect(x, y, g_w, h, 0.0, "#1b1f38");
            p.text(
                &tn.games[i].to_string(),
                x + g_w / 2.0,
                y + h / 2.0,
                "#ffffff",
                T::new(h * 0.56, 800).mid(),
            );
            x += g_w;
            let pts = tennis_pts(tn, i);
            p.rect(
                x,
                y,
                p_w,
                h,
                0.0,
                if pts == "AD" { "#facc15" } else { "#ffffff" },
            );
            p.text(
                &pts,
                x + p_w / 2.0,
                y + h / 2.0,
                &pl.cell,
                T::new(h * 0.56, 800).mid(),
            );
            y += h;
        }
        p.pop();
    } else if c.layout == Layout::Stacked {
        let stat_w = if full
            && (c.fields.fouls
                || c.fields.timeouts
                || c.fields.cards
                || c.fields.susp
                || shootout_on(c, st))
        {
            h * 3.6
        } else {
            0.0
        };
        let row_w = bar_w + crest_w + name_w + poss_w + stat_w + sets_w + score_w;
        let hd = if c.fields.period || ck.is_some() {
            h * 0.6
        } else {
            0.0
        };
        w0 = row_w;
        h0 = hd + n as f32 * (h + gap);
        let top = c.order != Order::ClockLast;
        p.clip(&rrect(0.0, 0.0, w0, h0, radius));
        let head_y = if top { 0.0 } else { h0 - hd };
        if hd > 0.0 {
            p.rect(0.0, head_y, w0, hd, 0.0, &pl.tag);
            let pfs = h * 0.3;
            p.text(
                &per,
                h * 0.25,
                head_y + hd / 2.0,
                "#c7cbe0",
                T::new(pfs, 800).ls(pfs * 0.08),
            );
            if let Some((txt, _, ended)) = &ck {
                let mut rx = w0 - h * 0.25;
                if let Some((s, end)) = &shot {
                    let sfs = h * 0.42;
                    p.text(
                        s,
                        rx,
                        head_y + hd / 2.0,
                        if *end { "#ff3b5c" } else { "#ffd200" },
                        T::new(sfs, 800).end(),
                    );
                    rx -= p.tw("88", sfs, 800, 0.0) + h * 0.3;
                }
                if st.added > 0 {
                    let a = format!("+{}", st.added);
                    let afs = h * 0.34;
                    let aw = p.tw(&a, afs, 800, 0.0) + h * 0.2;
                    p.rect(
                        rx - aw,
                        head_y + hd / 2.0 - afs * 0.7,
                        aw,
                        afs * 1.4,
                        5.0,
                        "#ff3b5c",
                    );
                    p.text(
                        &a,
                        rx - aw / 2.0,
                        head_y + hd / 2.0,
                        "#ffffff",
                        T::new(afs, 800).mid(),
                    );
                    rx -= aw + h * 0.15;
                }
                p.text(
                    txt,
                    rx,
                    head_y + hd / 2.0,
                    if *ended { "#ff3b5c" } else { "#ffffff" },
                    T::new(h * 0.42, 800).end(),
                );
            }
        }
        let mut y = if top { hd } else { 0.0 };
        for &i in &order {
            let t = c.team(i);
            p.rect(0.0, y, w0, gap, 0.0, &pl.bg);
            y += gap;
            p.rect(0.0, y, w0, h, 0.0, &t.c1);
            p.rect(0.0, y, bar_w, h, 0.0, &t.c2);
            let mut x = bar_w;
            if crest_w > 0.0 {
                crest(
                    p,
                    &t,
                    x + (crest_w - h * 0.74) / 2.0,
                    y + h * 0.13,
                    h * 0.74,
                    data,
                );
            }
            x += crest_w;
            name_block(p, &t, x, y, name_w, false);
            x += name_w;
            if poss_w > 0.0 && st.poss == i {
                poss_mark(p, serve, x + poss_w / 2.0, y + h / 2.0, h, false);
            }
            x += poss_w;
            if stat_w > 0.0 {
                let items = stats(c, st, i, lang, now);
                p.clip(&rrect(x, y, stat_w, h, 0.0));
                draw_stats(
                    p,
                    &items,
                    x,
                    stat_w,
                    y + h / 2.0,
                    h * 0.24,
                    ink(&t.c1),
                    false,
                );
                p.pop();
            }
            x += stat_w;
            if sets_w > 0.0 {
                p.rect(x, y, sets_w, h, 0.0, "#00000059");
                p.text(
                    &st.volley.won[i.min(1)].to_string(),
                    x + sets_w / 2.0,
                    y + h / 2.0,
                    "#facc15",
                    T::new(h * 0.46, 800).mid(),
                );
            }
            x += sets_w;
            score_cell(p, i, &t, x, y, score_w);
            y += h;
        }
        p.pop();
    } else {
        let show_clk = c.fields.period || ck.is_some();
        let shot_w = if shot.is_some() { h * 0.95 } else { 0.0 };
        let clock_w = if show_clk {
            if ck.is_some() {
                h * 2.9 + shot_w
            } else {
                h * 2.1
            }
        } else {
            0.0
        };
        let team_w = bar_w + crest_w + name_w + poss_w + sets_w + score_w;
        // Sequence: teams (the second mirrored when 2), the clock block placed by `order`.
        let mut seq: Vec<Option<(usize, bool)>> = order
            .iter()
            .enumerate()
            .map(|(k, &i)| Some((i, n == 2 && k == 1)))
            .collect();
        if show_clk {
            match c.order {
                Order::ClockLast => seq.push(None),
                Order::ClockMid if n == 2 => seq.insert(1, None),
                _ => seq.insert(0, None),
            }
        }
        let mut x = 0.0;
        let mut pos = Vec::new();
        let mut clock_x = 0.0;
        for (k, b) in seq.iter().enumerate() {
            if k > 0 {
                x += gap;
            }
            match b {
                None => {
                    clock_x = x;
                    x += clock_w;
                }
                Some((i, m)) => {
                    pos.push((*i, x, *m));
                    x += team_w;
                }
            }
        }
        w0 = x;
        let ex_h = h * 0.56;
        let has_stats = full
            && order
                .iter()
                .any(|&i| !stats(c, st, i, lang, now).is_empty());
        let tab = ck.is_some() && st.added > 0;
        h0 = h + if has_stats {
            ex_h
        } else if tab {
            h * 0.42
        } else {
            0.0
        };
        // Row
        p.clip(&rrect(0.0, 0.0, w0, h, radius));
        p.rect(0.0, 0.0, w0, h, 0.0, &pl.bg);
        if show_clk {
            p.rect(clock_x, 0.0, clock_w, h, 0.0, &pl.tag);
            let (pf, cf) = (h * 0.25, h * 0.46);
            let (lp, lc) = (
                if c.fields.period { pf * 1.1 } else { 0.0 },
                if ck.is_some() { cf * 1.05 } else { 0.0 },
            );
            let top = (h - lp - lc) / 2.0;
            let cx = clock_x + (clock_w - shot_w) / 2.0;
            if let Some((s, end)) = &shot {
                // The shot clock right of the match clock.
                let sx = clock_x + clock_w - shot_w;
                p.rect(sx, h * 0.2, 1.0, h * 0.6, 0.0, "#ffffff26");
                p.text(
                    s,
                    sx + shot_w / 2.0,
                    h / 2.0,
                    if *end { "#ff3b5c" } else { "#ffd200" },
                    T::new(h * 0.5, 800).mid(),
                );
            }
            if c.fields.period {
                p.text(
                    &per,
                    cx,
                    top + lp / 2.0,
                    "#c7cbe0",
                    T::new(pf, 800).ls(pf * 0.08).mid(),
                );
            }
            if let Some((txt, _, ended)) = &ck {
                p.text(
                    txt,
                    cx,
                    top + lp + lc / 2.0,
                    if *ended { "#ff3b5c" } else { "#ffffff" },
                    T::new(cf, 800).mid(),
                );
            }
        }
        for &(i, bx, mirror) in &pos {
            let t = c.team(i);
            p.rect(bx, 0.0, team_w, h, 0.0, &t.c1);
            // Parts left to right (reversed when mirrored).
            let mut parts: Vec<(u8, f32)> = vec![
                (0, bar_w),
                (1, crest_w),
                (2, name_w),
                (3, poss_w),
                (4, sets_w),
                (5, score_w),
            ];
            if mirror {
                parts.reverse();
            }
            let mut x = bx;
            for (kind, w) in parts {
                if w <= 0.0 {
                    continue;
                }
                match kind {
                    0 => p.rect(x, 0.0, w, h, 0.0, &t.c2),
                    1 => crest(p, &t, x + (w - h * 0.74) / 2.0, h * 0.13, h * 0.74, data),
                    2 => name_block(p, &t, x, 0.0, w, mirror),
                    3 => {
                        if st.poss == i {
                            poss_mark(p, serve, x + w / 2.0, h / 2.0, h, mirror)
                        }
                    }
                    4 => {
                        p.rect(x, 0.0, w, h, 0.0, "#00000059");
                        p.text(
                            &st.volley.won[i.min(1)].to_string(),
                            x + w / 2.0,
                            h / 2.0,
                            "#facc15",
                            T::new(h * 0.46, 800).mid(),
                        );
                    }
                    _ => score_cell(p, i, &t, x, 0.0, w),
                }
                x += w;
            }
        }
        p.pop();
        if has_stats {
            for &(i, bx, mirror) in &pos {
                let t = c.team(i);
                p.rect(bx, h, team_w, ex_h, 0.0, "#0b0d18eb");
                p.rect(bx, h, team_w, gap, 0.0, &t.c2);
                let items = stats(c, st, i, lang, now);
                p.clip(&rrect(bx, h, team_w, ex_h, 0.0));
                draw_stats(
                    p,
                    &items,
                    bx + h * 0.2,
                    team_w - h * 0.4,
                    h + ex_h / 2.0,
                    h * 0.3,
                    "#ffffff",
                    mirror,
                );
                p.pop();
            }
        }
        if tab {
            let tx = clock_x + (clock_w - shot_w) / 2.0 - h * 0.45;
            p.rect(tx, h, h * 0.9, h * 0.42, h * 0.1, "#ff3b5c");
            p.rect(tx, h, h * 0.9, h * 0.1, 0.0, "#ff3b5c");
            p.text(
                &format!("+{}", st.added),
                tx + h * 0.45,
                h + h * 0.21,
                "#ffffff",
                T::new(h * 0.32, 800).mid(),
            );
        }
    }

    // Event banner under the board.
    let mut total_h = h0;
    if c.anim.banner {
        let by = h0 + h * 0.18;
        let bh = h * 0.95;
        total_h = by + bh;
        if let Some(ev) = &st.fx.ev {
            let dur = (c.anim.ev_dur.max(0.5) * 1000.0) as f32;
            let k = (now - ev.at) as f32 / dur;
            if (0.0..1.0).contains(&k) {
                let k_in = ease_out(clamp01(k / 0.12));
                let k_out = ease(clamp01((k - 0.86) / 0.14));
                let (l, r) = (k_out * w0, w0 - (1.0 - k_in) * w0);
                if r > l {
                    p.clip(&rrect(l, by, r - l, bh, 0.0));
                    let ec = col(&ev.color);
                    let mut ec2 = ec;
                    ec2.0[3] *= 0.8;
                    p.fill_lin(
                        &rrect(0.0, by, w0, bh, radius),
                        (0.0, by),
                        (w0, by),
                        &[(0.0, ec), (0.7, ec), (1.0, ec2)],
                    );
                    let fg = ink(&ev.color);
                    let pulse = 1.0 + 0.04 * (k * 40.0).sin() * (1.0 - clamp01(k * 4.0));
                    let tfs = h * 0.6;
                    let tx = h * 0.35;
                    // In the project's language (or the Designer's words).
                    let text = fill(&tr(c, lang, &ev.text), &ev.vars);
                    let sub = fill(&tr(c, lang, &ev.sub), &ev.vars);
                    let tw = p.tw(&text, tfs, 800, tfs * 0.02);
                    p.text(
                        &text,
                        tx + tw / 2.0,
                        by + bh / 2.0,
                        fg,
                        T::new(tfs, 800)
                            .ls(tfs * 0.02)
                            .mid()
                            .scale(pulse, pulse)
                            .skew(0.2),
                    );
                    let sx = tx + tw + h * 0.35;
                    p.clip(&rrect(sx, by, (w0 - sx - h * 0.2).max(0.0), bh, 0.0));
                    let a = p.alpha;
                    p.alpha = a * 0.9;
                    p.text(&sub, sx, by + bh / 2.0, fg, T::new(h * 0.32, 800));
                    p.alpha = a;
                    p.pop();
                    // Shine
                    let shx = -0.1 * w0 + 1.2 * w0 * clamp01(k * 2.2);
                    let sw = h * 0.5;
                    let sk = bh * 0.36;
                    let a = p.alpha;
                    p.alpha = a * 0.3;
                    p.fill(
                        &poly(
                            &[
                                (shx + sk, by),
                                (shx + sk + sw, by),
                                (shx + sw - sk, by + bh),
                                (shx - sk, by + bh),
                            ],
                            true,
                        ),
                        "#ffffff",
                    );
                    p.alpha = a;
                    p.pop();
                }
            }
        }
    }
    (w0, total_h)
}

/// Drawn from what `board` paints: recorded (nothing painted), summed up:
/// the clocks, the animations of a score change or an event change it while
/// they run; a board at rest is the same.
pub(crate) fn motion(d: &mut Draw, st: &State) -> Option<crate::cache::Motion> {
    let LayerKind::Scoreboard { x: c } = &d.layer.kind else {
        return None;
    };
    let now = d.time.now_ms;
    let data = d.data;
    let (w, h) = (d.w, d.h);
    let lang = d.lang.to_string();
    let (bw, bh) = {
        let mut p = Pt::new(d, true);
        p.font = c.font.clone();
        board(&mut p, c, st, data, now, 100.0, &lang)
    };
    let row = (100.0 * (w / bw.max(1.0)).min(h / bh.max(1.0))).max(1.0);
    let mut p = Pt::new(d, true);
    p.font = c.font.clone();
    p.rec = Some(Default::default());
    board(&mut p, c, st, data, now, row, &lang);
    p.rec
        .map(|h| crate::cache::Motion::Key(std::hash::Hasher::finish(&h)))
}

pub(crate) fn draw(d: &mut Draw, st: &State) -> bool {
    let LayerKind::Scoreboard { x: c } = &d.layer.kind else {
        return false;
    };
    let now = d.time.now_ms;
    let data = d.data;
    let (w, h) = (d.w, d.h);
    let lang = d.lang.to_string();
    let (bw, bh) = {
        let mut p = Pt::new(d, true);
        p.font = c.font.clone();
        board(&mut p, c, st, data, now, 100.0, &lang)
    };
    let row = (100.0 * (w / bw.max(1.0)).min(h / bh.max(1.0))).max(1.0);
    let mut p = Pt::new(d, false);
    p.font = c.font.clone();
    board(&mut p, c, st, data, now, row, &lang);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::wid::scoreboard::{preset, Sport};

    #[test]
    fn stats_speak_the_project_language_or_the_designers_words() {
        let mut c = Config {
            preset: Sport::Basketball,
            fields: preset(Sport::Basketball).fields,
            ..Default::default()
        };
        let st = State::default();
        let has = |items: &[Item], s: &str| {
            items
                .iter()
                .any(|it| matches!(it, Item::Txt(t, _, _) if t == s))
        };
        assert!(has(&stats(&c, &st, 0, "fr", 0), "FAUTES "));
        assert!(has(&stats(&c, &st, 0, "en", 0), "FOULS "));
        c.texts.insert("FOULS".into(), "F".into());
        assert!(has(&stats(&c, &st, 0, "fr", 0), "F "));
    }

    #[test]
    fn the_shot_clock_shows_in_yellow_next_to_the_match_clock() {
        let project = cg_core::sample::demo_project();
        let board = |shot: bool| {
            let t: cg_core::Template = serde_json::from_value(serde_json::json!({
                "id": "sb", "name": "T", "duration": 30.0, "markers": [], "zone": "", "fields": [],
                "layers": [{
                    "id": "s", "name": "Score", "type": "scoreboard", "preset": "basketball",
                    "fields": {"period": true, "clock": true, "shot": shot},
                    "clock": {"dir": "down", "len": 10.0, "extra": 5.0, "tenths": true},
                    "anim": {"banner": false},
                    "props": {"x": 0.0, "y": 0.0, "width": 1600.0, "height": 120.0}
                }]
            }))
            .unwrap();
            let mut r = crate::Renderer::new(std::env::temp_dir().join("cg-render-test-assets"));
            let f = r.render_template(&project, &t, "main", 10.0, &Default::default());
            (0..f.width * 120)
                .filter(|k| {
                    let i = (*k * 4) as usize;
                    let p = &f.data[i..i + 4];
                    p[3] > 200 && p[0] > 230 && p[1] > 190 && p[1] < 230 && p[2] < 40
                })
                .count()
        };
        assert!(board(true) > 30, "yellow digits of the shot clock");
        assert_eq!(board(false), 0);
    }
}
