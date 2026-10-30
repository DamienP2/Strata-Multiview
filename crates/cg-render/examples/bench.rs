//! Render bench: every example project on air at 1080p, ms per frame.
//! cargo run --release -p cg-render --example bench -- crates/cg-server/examples [filter] [frames] [renderers]
use cg_core::playout::{ChannelState, Command};
use cg_core::Project;
use cg_render::FrameCache;
use cg_render::{OutputFormat, Renderer};
use std::sync::Arc;
use std::time::Instant;

#[derive(serde::Deserialize)]
struct File {
    project: Project,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("crates/cg-server/examples");
    let filter = args.get(2).cloned().unwrap_or_default();
    let frames: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(150);
    let nr: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let solo = std::env::var_os("SOLO").is_some();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| p.to_string_lossy().contains(&filter))
        .collect();
    files.sort();
    let out = OutputFormat {
        width: 1920,
        height: 1080,
    };
    for f in files {
        let d: File = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let p = Arc::new(d.project);
        let fps = p.frame_rate.as_f64();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        // Everything of the rundown on air at once (one per zone/layer), as a busy moment.
        let items: Vec<(String, String)> = {
            let mut v: Vec<(String, String)> = Vec::new();
            for r in &p.rundown {
                if !v.iter().any(|(z, _)| *z == r.zone) {
                    v.push((r.zone.clone(), r.template_id.clone()));
                }
            }
            if v.is_empty() {
                for t in &p.templates {
                    if !v.iter().any(|(z, _)| *z == t.zone) {
                        v.push((t.zone.clone(), t.id.clone()));
                    }
                }
            }
            v
        };
        let sets: Vec<Vec<(String, String)>> = if solo {
            p.templates
                .iter()
                .map(|t| vec![(t.zone.clone(), t.id.clone())])
                .collect()
        } else {
            vec![items]
        };
        for set in sets {
            let mut st = ChannelState::default();
            for (zone, tid) in &set {
                let _ = st.apply(
                    &p,
                    Command::Take {
                        zone: Some(zone.clone()),
                        layer: None,
                        template_id: tid.clone(),
                        data: None,
                        rundown_id: None,
                    },
                );
            }
            let cache = FrameCache::new();
            let mut renderers: Vec<Renderer> = (0..nr)
                .map(|_| Renderer::new(std::env::temp_dir().join("bench-assets")))
                .collect();
            if std::env::var_os("NOSPRITES").is_some() {
                renderers.iter_mut().for_each(|r| r.set_sprites(false));
            }
            let mut times = Vec::with_capacity(frames);
            let (mut same, mut prev): (usize, Option<Arc<cg_render::Frame>>) = (0, None);
            for i in 0..frames {
                st.tick(1.0);
                st.auto_resume(&p);
                let r = &mut renderers[i % nr];
                let t = Instant::now();
                let fr = r.render_channel_cached(
                    &p,
                    &st,
                    out,
                    "Europe/Paris",
                    i as f64 * 1000.0 / fps,
                    &cache,
                );
                times.push(t.elapsed().as_secs_f64() * 1000.0);
                if prev.as_ref().is_some_and(|q| Arc::ptr_eq(q, &fr)) {
                    same += 1;
                }
                prev = Some(fr);
            }
            let avg = times.iter().sum::<f64>() / times.len() as f64;
            let max = times.iter().cloned().fold(0.0, f64::max);
            let mut sorted = times.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = sorted[sorted.len() / 2];
            let p90 = sorted[sorted.len() * 9 / 10];
            let label = if solo {
                let t = p.template(&set[0].1).unwrap();
                format!("{name} / {} [{}]", t.name, t.layers.len())
            } else {
                format!("{name} ({} on air)", set.len())
            };
            let (made, found) = renderers.iter().fold((0, 0), |(a, b), r| {
                let (m, f) = r.sprite_counts();
                (a + m, b + f)
            });
            println!(
                "{label}: avg {avg:.1} ms  p50 {p50:.1}  p90 {p90:.1}  max {max:.1}  same {same}/{frames}  sprites {made} made {found} laid  ({:.0}% of {:.1} ms)",
                avg / (1000.0 / fps) * 100.0,
                1000.0 / fps
            );
        }
    }
}
