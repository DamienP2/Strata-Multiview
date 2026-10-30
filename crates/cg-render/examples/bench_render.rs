//! `cargo run --release -p cg-render --example bench_render`
use std::time::Instant;

use cg_core::{sample, ChannelState, Command};
use cg_render::{OutputFormat, Renderer};

const OUT: OutputFormat = OutputFormat {
    width: 1920,
    height: 1080,
};

fn main() {
    let project = sample::demo_project();
    let mut state = ChannelState::default();
    for (layer, id) in [(10, "lower-third"), (20, "score-bug")] {
        state
            .apply(
                &project,
                Command::Take {
                    zone: None,
                    layer: Some(layer),
                    template_id: id.into(),
                    data: None,
                    rundown_id: None,
                },
            )
            .unwrap();
    }
    for _ in 0..15 {
        state.tick(1.0); // mid-animation: masks and partial opacity active
    }
    let mut r = Renderer::new(std::env::temp_dir().join("cg-bench-assets"));
    r.render_channel(&project, &state, OUT, "UTC"); // warm caches
    let n = 50;
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(r.render_channel(&project, &state, OUT, "UTC"));
    }
    println!(
        "render 1080p (2 graphics): {:.2} ms/frame",
        t.elapsed().as_secs_f64() * 1000.0 / n as f64
    );
}
