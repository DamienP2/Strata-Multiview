//! The example projects: complete, on-air ready channel packages (news,
//! sport, elections, weather, gaming…), embedded in the server. Each one
//! is `examples/ex-NN-slug.json`, `{"meta": ExampleInfo, "project": Project,
//! "sources": [ExampleSource]}`, written by tools/examples/*.py; build.rs
//! lists them. The data sources its fields read are created with the
//! project (when missing), with a sample value, so it works out of the box.

use cg_core::Project;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleInfo {
    pub id: String,
    pub name: String,
    /// News, Sport, Elections, Weather, Finance, Entertainment, Gaming…
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub description: String,
    /// One line for the example's card (the description is the long text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// The template shown on the example's card (default: the first one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// Name and description in English (the main ones are in French).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<Translation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

/// An example as listed: its card plus what it contains.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleSummary {
    #[serde(flatten)]
    pub info: ExampleInfo,
    /// Names of its templates.
    pub templates: Vec<String>,
    /// Its formats besides 16:9 ("9:16"…).
    pub formats: Vec<String>,
}

/// A shared data source an example reads, with a sample value (a push
/// source keeps it until automation sends another one).
#[derive(Debug, Clone, Deserialize)]
pub struct ExampleSource {
    #[serde(flatten)]
    pub source: crate::datasources::Source,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ExampleFile {
    meta: ExampleInfo,
    project: Project,
    #[serde(default)]
    sources: Vec<ExampleSource>,
}

const FILES: &[&str] = include!(concat!(env!("OUT_DIR"), "/examples.rs"));

fn all() -> impl Iterator<Item = ExampleFile> {
    FILES
        .iter()
        .filter_map(|j| match serde_json::from_str::<ExampleFile>(j) {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::error!("bad example project: {e}");
                None
            }
        })
}

pub fn list() -> Vec<ExampleSummary> {
    all()
        .map(|d| ExampleSummary {
            templates: d.project.templates.iter().map(|t| t.name.clone()).collect(),
            formats: d.project.formats.iter().map(|f| f.name.clone()).collect(),
            info: d.meta,
        })
        .collect()
}

/// Written in an example, the id of the project made from it: a data
/// source of that project only (`social-{{project}}`, the source of the
/// project's social messages) and the fields reading it.
pub const PROJECT: &str = "{{project}}";

/// A copy of an example, ready to be saved as the new project `project_id`,
/// and the data sources it reads (shared ones, and the project's own).
pub fn project(id: &str, project_id: &str) -> Option<(Project, Vec<ExampleSource>)> {
    let (json, d) = FILES.iter().find_map(|j| {
        let d = serde_json::from_str::<ExampleFile>(j).ok()?;
        (d.meta.id == id).then_some((j, d))
    })?;
    let own: Vec<bool> = d
        .sources
        .iter()
        .map(|s| s.source.id.contains(PROJECT))
        .collect();
    let mut d = if json.contains(PROJECT) {
        serde_json::from_str(&json.replace(PROJECT, project_id)).ok()?
    } else {
        d
    };
    for (s, own) in d.sources.iter_mut().zip(own) {
        s.source.project = own.then(|| project_id.to_string());
    }
    Some((d.project, d.sources))
}

/// The example and the template of its card.
pub fn cover(id: &str) -> Option<(Project, cg_core::Template)> {
    let d = all().find(|d| d.meta.id == id)?;
    let t = d
        .meta
        .cover
        .as_deref()
        .and_then(|c| d.project.template(c))
        .or(d.project.templates.first())?
        .clone();
    Some((d.project, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Share of the pixels of a frame that are not fully transparent.
    fn coverage(frame: &cg_render::Frame) -> f64 {
        let px = &frame.data;
        let n = px.len() / 4;
        let painted = px.as_chunks::<4>().0.iter().filter(|p| p[3] > 0).count();
        painted as f64 / n.max(1) as f64
    }

    /// A channel with `t` taken in its zone and played to its on-air point
    /// (Continue through the pauses before it).
    fn on_air(p: &Project, t: &cg_core::Template) -> cg_core::ChannelState {
        let mut st = cg_core::ChannelState::default();
        st.apply(p, cg_core::Command::take(&t.zone, &t.id))
            .expect("taken");
        let hold = t.hold_frame();
        for _ in 0..5000 {
            let item = st.layers.values().next().expect("on air");
            if item.frame >= hold - 1e-6 && item.phase == cg_core::playout::Phase::Holding {
                break;
            }
            if item.phase == cg_core::playout::Phase::Holding {
                let _ = st.apply(
                    p,
                    cg_core::Command::Continue {
                        zone: Some(t.zone.clone()),
                        layer: None,
                    },
                );
            }
            st.tick(1.0);
        }
        st
    }

    const HD: cg_render::OutputFormat = cg_render::OutputFormat {
        width: 1920,
        height: 1080,
    };

    /// Renders `n` frames of a channel as the engine does (50 fps: the
    /// timelines tick, the clock advances 20 ms a frame); ms per frame.
    fn play(
        r: &mut cg_render::Renderer,
        p: &std::sync::Arc<Project>,
        st: &mut cg_core::ChannelState,
        n: usize,
        t0: f64,
    ) -> f64 {
        let step = p.frame_rate.as_f64() / 50.0;
        let cache = cg_render::FrameCache::new();
        let start = std::time::Instant::now();
        for i in 0..n {
            let ms = t0 + i as f64 * 20.0;
            std::hint::black_box(r.render_channel_cached(p, st, HD, "Europe/Paris", ms, &cache));
            st.tick(step);
        }
        start.elapsed().as_secs_f64() * 1000.0 / n as f64
    }

    /// Playout render bench (not a test): every template of every example
    /// on air at 1920×1080 as the engine renders it, 50 fps: its first 25
    /// frames coming in, then 50 frames at its on-air point (the clock
    /// running: tickers, timers and clocks move), then 150 frames of the
    /// running cases (ex-02 ticker band, ex-03 match clock, ex-16 quiz timer,
    /// ex-19 strip with two tickers). Prints ms per frame.
    /// `cargo test --release -p cg-server --bin strata-server -- --ignored bench_playout --nocapture`
    /// `STRATA_BENCH=ex-19-matinale/horoscope`: that template only (to profile it);
    /// `STRATA_BENCH_PROJECT=file.json`: the templates of that project (`project/<id>`);
    /// `STRATA_BENCH_NOCACHE=1`: every frame drawn whole, from scratch.
    #[test]
    #[ignore]
    fn bench_playout() {
        let mut r = cg_render::Renderer::new(std::env::temp_dir().join("strata-example-bench"));
        r.set_cache(std::env::var_os("STRATA_BENCH_NOCACHE").is_none());
        let t0 = 1_800_000_000_000.0;
        let only = std::env::var("STRATA_BENCH").unwrap_or_default();
        let (mut all_in, mut all_on, mut n) = (0.0, 0.0, 0);
        // STRATA_BENCH_PROJECT=project.json: that project's templates instead.
        let projects: Vec<(String, Project)> = match std::env::var("STRATA_BENCH_PROJECT") {
            Ok(path) => {
                let json = std::fs::read_to_string(&path).expect("project file");
                vec![(
                    "project".into(),
                    serde_json::from_str(&json).expect("a project"),
                )]
            }
            Err(_) => all().map(|d| (d.meta.id, d.project)).collect(),
        };
        for (id, p) in projects {
            let p = std::sync::Arc::new(p);
            let mut times = Vec::new();
            for t in &p.templates {
                if !only.is_empty() && only != format!("{id}/{}", t.id) {
                    continue;
                }
                let mut st = on_air(&p, t);
                play(&mut r, &p, &mut st.clone(), 2, t0); // warm up (fonts, pictures)
                let mut taken = cg_core::ChannelState::default();
                taken
                    .apply(&p, cg_core::Command::take(&t.zone, &t.id))
                    .expect("taken");
                let coming = play(&mut r, &p, &mut taken, 25, t0);
                let held = play(&mut r, &p, &mut st, 50, t0);
                times.push((coming, held, t.name.clone()));
            }
            if times.is_empty() {
                continue;
            }
            let k = times.len() as f64;
            let coming = times.iter().map(|t| t.0).sum::<f64>() / k;
            let held = times.iter().map(|t| t.1).sum::<f64>() / k;
            (all_in, all_on, n) = (all_in + coming, all_on + held, n + 1);
            let worst = times
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .expect("templates");
            println!(
                "{id:<26} coming in {coming:6.2}  on air {held:6.2} ms/frame   worst on air {:6.2} ({})",
                worst.1, worst.2
            );
        }
        let n = n.max(1) as f64;
        println!(
            "ALL examples: coming in {:.2}  on air {:.2} ms/frame",
            all_in / n,
            all_on / n
        );
        if !only.is_empty() {
            return;
        }
        for (ex, tid) in [
            ("ex-02-info-continu", "bandeau"),
            ("ex-03-football", "score"),
            ("ex-16-quiz-tv", "timer"),
            ("ex-19-matinale", "strip"),
        ] {
            let d = all().find(|d| d.meta.id == ex).expect("example");
            let p = std::sync::Arc::new(d.project);
            let t = p.template(tid).expect("template");
            let mut st = on_air(&p, t);
            play(&mut r, &p, &mut st.clone(), 2, t0);
            let ms = play(&mut r, &p, &mut st, 150, t0);
            println!("running {ex}/{tid}: {ms:.2} ms/frame");
        }
    }

    /// A renderer drawing every frame whole, from scratch, and two with the
    /// cache (as the engine's render threads, sharing their frame cache).
    fn renderers() -> [cg_render::Renderer; 3] {
        let assets = std::env::temp_dir().join("strata-example-identity");
        let mut plain = cg_render::Renderer::new(&assets);
        plain.set_cache(false);
        [
            plain,
            cg_render::Renderer::new(&assets),
            cg_render::Renderer::new(&assets),
        ]
    }

    /// Renders `frames` (channel state, shown at ms) whole and with a frame
    /// cache (the two cached renderers taking turns); every frame must be the
    /// same, byte for byte. Returns the cache's counts (frames given again,
    /// frames drawn on a kept picture).
    fn same_with_cache(
        what: &str,
        [plain, a, b]: &mut [cg_render::Renderer; 3],
        p: &std::sync::Arc<Project>,
        frames: &[(cg_core::ChannelState, f64)],
        out: cg_render::OutputFormat,
    ) -> (u64, u64) {
        let cache = cg_render::FrameCache::new();
        for (i, (st, ms)) in frames.iter().enumerate() {
            let want = plain.render_channel_at(p, st, out, "Europe/Paris", *ms);
            let r = if i % 2 == 0 { &mut *a } else { &mut *b };
            let got = r.render_channel_cached(p, st, out, "Europe/Paris", *ms, &cache);
            if got.data != want.data {
                let diff = got
                    .data
                    .iter()
                    .zip(&want.data)
                    .filter(|(x, y)| x != y)
                    .count();
                panic!("{what}: frame {i} differs with the cache ({diff} bytes)");
            }
        }
        cache.counts()
    }

    /// Frames drawn with the cache (frames given again, static layers kept
    /// under what moves) are the frames drawn whole: every template of the
    /// examples `part` of 4 coming in, on air (consecutive frames), going out.
    /// Returns how many frames were given again.
    fn cached_frames_of(part: usize) -> u64 {
        let small = cg_render::OutputFormat {
            width: 480,
            height: 270,
        };
        let t0 = 1_800_000_000_000.0;
        let mut reused = 0;
        let mut rs = renderers();
        for d in all().skip(part).step_by(4) {
            let p = std::sync::Arc::new(d.project);
            for t in &p.templates {
                let mut frames = Vec::new();
                let mut st = cg_core::ChannelState::default();
                st.apply(&p, cg_core::Command::take(&t.zone, &t.id))
                    .expect("taken");
                let mut ms = t0;
                // Coming in.
                for _ in 0..8 {
                    st.tick(1.0);
                    ms += 20.0;
                }
                frames.push((st.clone(), ms));
                // On air: consecutive frames.
                let mut st = on_air(&p, t);
                for _ in 0..3 {
                    frames.push((st.clone(), ms));
                    st.tick(1.0);
                    ms += 20.0;
                }
                // Going out.
                let _ = st.apply(
                    &p,
                    cg_core::Command::TakeOut {
                        zone: Some(t.zone.clone()),
                        layer: None,
                    },
                );
                for _ in 0..6 {
                    st.tick(1.0);
                    ms += 20.0;
                }
                frames.push((st.clone(), ms));
                let what = format!("{}/{}", d.meta.id, t.id);
                reused += same_with_cache(&what, &mut rs, &p, &frames, small).0;
            }
        }
        reused
    }

    // In four parts, run in parallel.
    #[test]
    fn cached_frames_are_the_frames_drawn_whole_1() {
        // Graphics at rest are given again.
        assert!(cached_frames_of(0) > 10);
    }

    #[test]
    fn cached_frames_are_the_frames_drawn_whole_2() {
        assert!(cached_frames_of(1) > 10);
    }

    #[test]
    fn cached_frames_are_the_frames_drawn_whole_3() {
        assert!(cached_frames_of(2) > 10);
    }

    #[test]
    fn cached_frames_are_the_frames_drawn_whole_4() {
        assert!(cached_frames_of(3) > 10);
    }

    /// The same with what moves: tickers, clocks (the second changing),
    /// timers, two graphics (the lower kept under the upper one coming in),
    /// maps with pulsing markers (drawn in two steps).
    #[test]
    fn cached_frames_move_as_drawn_whole() {
        let hd = cg_render::OutputFormat {
            width: 960,
            height: 540,
        };
        // 120 ms before a second: the clocks change during the frames.
        let t0 = 1_800_000_000_880.0;
        let mut rs = renderers();
        for (ex, tids) in [
            ("ex-02-info-continu", &["bandeau", "logo"][..]),
            ("ex-03-football", &["score", "goal"][..]),
            ("ex-16-quiz-tv", &["timer"][..]),
            ("ex-19-matinale", &["strip"][..]),
            // Pulsing dots over a map: the map under them is kept.
            ("ex-18-cyclisme", &["map"][..]),
            ("ex-13-documentaire", &["map"][..]),
            // Score boards (recorded: the same board, the same picture).
            ("ex-10-tennis", &["score"][..]),
            ("ex-17-basket", &["scorebug"][..]),
        ] {
            let d = all().find(|d| d.meta.id == ex).expect("example");
            let p = std::sync::Arc::new(d.project);
            let first = p.template(tids[0]).expect("template");
            let mut st = on_air(&p, first);
            for tid in &tids[1..] {
                let t = p.template(tid).expect("template");
                st.apply(&p, cg_core::Command::take(&t.zone, &t.id))
                    .expect("taken");
            }
            let mut frames = Vec::new();
            let mut ms = t0;
            for _ in 0..14 {
                frames.push((st.clone(), ms));
                st.tick(1.0);
                ms += 20.0;
            }
            let (reused, started) = same_with_cache(ex, &mut rs, &p, &frames, hd);
            // Given again, or drawn on the kept picture of what does not move.
            assert!(reused + started > 5, "{ex}: {reused} {started}");
        }
    }

    #[test]
    fn a_project_source_takes_the_new_projects_id() {
        let (p, sources) = project("ex-21-reseaux-sociaux", "p42").expect("example");
        let own = sources
            .iter()
            .find(|s| s.source.id == "social-p42")
            .expect("the project's social source");
        assert_eq!(own.source.project.as_deref(), Some("p42"));
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains(PROJECT), "no placeholder left");
        assert!(json.contains("\"source\":\"social-p42\""));
        // Shared sources stay shared.
        let (_, sources) = project("ex-20-live-shopping", "p42").expect("example");
        assert!(sources.iter().all(|s| s.source.project.is_none()));
    }

    #[test]
    fn every_example_loads_and_renders() {
        let examples: Vec<(String, ExampleFile)> = FILES
            .iter()
            .map(|j| {
                let d: ExampleFile = serde_json::from_str(j).unwrap_or_else(|e| {
                    let id = j.get(..200).unwrap_or(j);
                    panic!("an example parses: {e}\n{id}")
                });
                (d.meta.id.clone(), d)
            })
            .collect();
        assert_eq!(examples.len(), FILES.len());
        let mut ids: Vec<&str> = examples.iter().map(|(id, _)| id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), examples.len(), "example ids are unique");
        let mut renderer =
            cg_render::Renderer::new(std::env::temp_dir().join("strata-example-test"));
        for (id, d) in &examples {
            let p = &d.project;
            assert_eq!(&p.id, id, "the project id is the example id");
            let zones: Vec<&str> = p.zones.iter().map(|z| z.id.as_str()).collect();
            assert!(!p.templates.is_empty(), "{id}: no template");
            // The sources its fields and data sets read come with it.
            let sources: Vec<&str> = d.sources.iter().map(|s| s.source.id.as_str()).collect();
            for t in &p.templates {
                let used = t
                    .fields
                    .iter()
                    .filter_map(|f| f.binding.as_ref().map(|b| b.source.as_str()))
                    .chain(t.datasets.iter().map(|d| d.source.as_str()));
                for src in used {
                    assert!(
                        sources.contains(&src),
                        "{id}: {} reads the data source {src}, not in the example's sources",
                        t.name
                    );
                }
            }
            if let Some(c) = &d.meta.cover {
                assert!(p.template(c).is_some(), "{id}: cover {c} is not a template");
            }
            for r in &p.rundown {
                assert!(
                    p.template(&r.template_id).is_some(),
                    "{id}: item {} without template",
                    r.name
                );
                assert!(
                    zones.contains(&r.zone.as_str()),
                    "{id}: item {} in an unknown zone",
                    r.name
                );
                assert!(
                    r.playlist.is_empty() || p.playlists.iter().any(|pl| pl.id == r.playlist),
                    "{id}: item {} in an unknown playlist",
                    r.name
                );
            }
            for t in &p.templates {
                assert!(
                    zones.contains(&t.zone.as_str()),
                    "{id}: template {} in an unknown zone",
                    t.name
                );
                assert!(
                    t.markers.iter().any(|m| m.kind == cg_core::MarkerKind::Out),
                    "{id}: template {} has no on-air point",
                    t.name
                );
                // The graphics sit in their zone in every format (they are cut
                // to it on air).
                let zone = p.zones.iter().find(|z| z.id == t.zone).unwrap();
                let formats = std::iter::once((cg_core::MAIN_FORMAT, p.width, p.height))
                    .chain(p.formats.iter().map(|f| (f.id.as_str(), f.width, f.height)));
                for (f, fw, fh) in formats {
                    let area = zone.area(f, fh > fw);
                    for l in t.layers.iter().filter(|l| l.parent.is_none()) {
                        let over = l.overrides.get(f);
                        if over.and_then(|o| o.visible) == Some(false)
                            || (!l.visible && over.and_then(|o| o.visible) != Some(true))
                        {
                            continue;
                        }
                        let n = |prop: cg_core::Prop| {
                            over.and_then(|o| o.props.get(&prop))
                                .or(l.props.get(&prop))
                                .and_then(|v| v.as_number())
                                .unwrap_or(0.0)
                        };
                        let (x, y, w, h) = (
                            n(cg_core::Prop::X),
                            n(cg_core::Prop::Y),
                            n(cg_core::Prop::Width),
                            n(cg_core::Prop::Height),
                        );
                        let (ax, ay) = (area.x * fw as f64, area.y * fh as f64);
                        let (aw, ah) = (area.width * fw as f64, area.height * fh as f64);
                        assert!(
                            x >= ax - 2.0
                                && y >= ay - 2.0
                                && x + w <= ax + aw + 2.0
                                && y + h <= ay + ah + 2.0,
                            "{id}: {} / {} out of its zone {} ({f})",
                            t.name,
                            l.name,
                            zone.name
                        );
                    }
                }
                let data = t.default_data();
                let hold = t.hold_frame();
                for f in std::iter::once(cg_core::MAIN_FORMAT)
                    .chain(p.formats.iter().map(|f| f.id.as_str()))
                {
                    let frame = renderer.render_template(p, t, f, hold, &data);
                    let png = frame.to_png();
                    assert!(png.len() > 100, "{id}: {} renders", t.name);
                    // STRATA_EXAMPLES_PNG=dir: every template on air, to look at.
                    if let Some(dir) = std::env::var_os("STRATA_EXAMPLES_PNG") {
                        let dir = std::path::Path::new(&dir);
                        std::fs::create_dir_all(dir).unwrap();
                        std::fs::write(dir.join(format!("{id}__{}__{f}.png", t.id)), &png).unwrap();
                    }
                    // On air, the graphic shows something.
                    assert!(
                        coverage(&frame) > 0.0005,
                        "{id}: {} ({f}) is empty on air",
                        t.name
                    );
                }
            }
        }
    }
}
