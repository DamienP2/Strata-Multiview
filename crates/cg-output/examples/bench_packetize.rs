//! `cargo run --release -p cg-output --example bench_packetize`
use std::time::Instant;

use cg_output::st2110::{packetize, Essence};
use cg_render::Frame;

fn main() {
    let mut f = Frame::blank(1920, 1080);
    for (i, b) in f.data.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    let mut seq = 0;
    let n = 50;
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(packetize(&f, Essence::Fill, 96, &mut seq, 0));
    }
    println!(
        "packetize 1080p fill: {:.2} ms/frame",
        t.elapsed().as_secs_f64() * 1000.0 / n as f64
    );
}
