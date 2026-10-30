//! The pictures kept between frames (scaled pictures, roll strips, faded
//! zones, the picture of the graphics over what moves) give the frames drawn
//! without them: a frame only depends on what is on air, never on what was
//! drawn before.

use std::sync::Arc;

use cg_core::{sample, ChannelState, Command, Project};
use cg_render::{FrameCache, OutputFormat, Renderer};

const OUT: OutputFormat = OutputFormat {
    width: 640,
    height: 360,
};

/// The assets folder, written once: the tests run together, and a renderer
/// reading the picture while another test writes it again would get a
/// broken file.
fn assets() -> std::path::PathBuf {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("cg-render-caches-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A small picture shown large (scaled once, then kept).
        let mut img = image::RgbaImage::new(16, 10);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([
                (x * 15) as u8,
                (y * 25) as u8,
                200,
                if x == 0 { 120 } else { 255 },
            ]);
        }
        img.save(dir.join("small.png")).unwrap();
        dir
    })
    .clone()
}

fn take(st: &mut ChannelState, p: &Project, template: &str, layer: u32) {
    take_in(st, p, template, Some(layer), None);
}

/// Takes `template` on `layer`, or in zone `zone`.
fn take_in(
    st: &mut ChannelState,
    p: &Project,
    template: &str,
    layer: Option<u32>,
    zone: Option<&str>,
) {
    let credits: String = (0..30).map(|i| format!("Role {i}\nName {i}\n\n")).collect();
    st.apply(
        p,
        Command::Take {
            zone: zone.map(str::to_string),
            layer,
            template_id: template.into(),
            data: Some([("credits".to_string(), credits)].into()),
            rundown_id: None,
        },
    )
    .unwrap();
}

/// Renders `frames` with a renderer drawing everything whole and, taking
/// turns, two sharing a frame cache; all the same, byte for byte; and the
/// same as a renderer that draws only that frame. The cache.
fn same_everywhere(p: &Arc<Project>, frames: &[(ChannelState, f64)]) -> FrameCache {
    let dir = assets();
    let mut plain = Renderer::new(&dir);
    plain.set_cache(false);
    let (mut a, mut b) = (Renderer::new(&dir), Renderer::new(&dir));
    let cache = FrameCache::new();
    for (i, (st, ms)) in frames.iter().enumerate() {
        let want = plain.render_channel_at(p, st, OUT, "UTC", *ms);
        let r = if i % 2 == 0 { &mut a } else { &mut b };
        let got = r.render_channel_cached(p, st, OUT, "UTC", *ms, &cache);
        assert!(got.data == want.data, "frame {i} differs with the cache");
        if i % 5 == 4 {
            let fresh = Renderer::new(&dir).render_channel_at(p, st, OUT, "UTC", *ms);
            assert!(
                fresh.data == want.data,
                "frame {i} depends on the frames before"
            );
        }
    }
    cache
}

/// A static bug and clock over a ticker that moves: their picture is kept
/// and laid over the ticker each frame; changed, it is drawn again.
#[test]
fn graphics_over_what_moves_are_kept() {
    let p = Arc::new(sample::demo_project());
    let mut st = ChannelState::default();
    take(&mut st, &p, "ticker", 5);
    take(&mut st, &p, "score-bug", 20);
    for _ in 0..120 {
        st.tick(1.0);
    }
    let mut frames = Vec::new();
    let mut ms = 1_800_000_000_000.0;
    for i in 0..16 {
        if i == 8 {
            // The bug changes: drawn again (the same frames as drawn whole).
            st.apply(
                &p,
                Command::Update {
                    zone: None,
                    layer: Some(20),
                    data: [("home".to_string(), "XYZ".to_string())].into(),
                    fade: false,
                },
            )
            .unwrap();
        }
        frames.push((st.clone(), ms));
        st.tick(1.0);
        ms += 20.0;
    }
    let cache = same_everywhere(&p, &frames);
    assert!(cache.tops() >= 10, "{}", cache.tops());
}

/// Credits rolling (a strip drawn once, shown through the box): the same
/// frames however they are drawn, and they move.
#[test]
fn a_roll_is_the_same_from_its_strip() {
    let p = Arc::new(sample::demo_project());
    let mut st = ChannelState::default();
    take_in(&mut st, &p, "credits", None, Some("full-screen"));
    let mut frames = Vec::new();
    let mut ms = 1_800_000_000_000.0;
    for _ in 0..40 {
        frames.push((st.clone(), ms));
        for _ in 0..3 {
            st.tick(1.0);
        }
        ms += 60.0;
    }
    same_everywhere(&p, &frames);
    let dir = assets();
    let mut r = Renderer::new(&dir);
    let f1 = r.render_channel_at(&p, &frames[20].0, OUT, "UTC", frames[20].1);
    let f2 = r.render_channel_at(&p, &frames[21].0, OUT, "UTC", frames[21].1);
    assert!(f1.data != f2.data, "the roll moves");
}

/// A small picture shown large in a rounded frame, sliding in then at
/// rest: scaled once at rest, the same pixels as scaled each frame.
#[test]
fn a_scaled_picture_is_the_same_kept() {
    let t: cg_core::Template = serde_json::from_value(serde_json::json!({
        "id": "pic", "name": "Picture", "duration": 40.0, "zone": "", "fields": [],
        "markers": [],
        "layers": [{
            "id": "img", "name": "Image", "type": "image", "src": "/assets/small.png",
            "fit": "cover", "mask": "rounded", "radius": 28.0,
            "props": {"x": 100.0, "y": 60.0, "width": 800.0, "height": 560.0},
            "tracks": {"x": [{"frame": 0.0, "value": -300.0}, {"frame": 10.0, "value": 100.5}]}
        }]
    }))
    .unwrap();
    let mut p = sample::demo_project();
    p.templates.push(t);
    let p = Arc::new(p);
    let mut st = ChannelState::default();
    take_in(&mut st, &p, "pic", None, Some("full-screen"));
    let mut frames = Vec::new();
    let mut ms = 1_800_000_000_000.0;
    for _ in 0..24 {
        frames.push((st.clone(), ms));
        st.tick(1.0);
        ms += 20.0;
    }
    same_everywhere(&p, &frames);
    // Something is drawn.
    let dir = assets();
    let f = Renderer::new(&dir).render_channel_at(&p, &frames[23].0, OUT, "UTC", frames[23].1);
    assert!(f.data.chunks(4).filter(|px| px[3] == 255).count() > 10_000);
}
