//! Renders a graphic to a video file with alpha, for post-production (the
//! Premiere Pro panel, or any client of `POST /api/render`).
//!
//! The clip plays the template like on air: animation in up to the hold
//! point, `hold` seconds on air (clocks, tickers… keep running), then the
//! out animation. Frames are rendered at the requested size and frame rate;
//! the project format closest to that shape (16:9, 9:16…) is used.
//!
//! With a [`ClipTiming`] (the Premiere panel's clip timeline), the clip
//! lasts exactly its duration and leaves each stop of the template (pauses,
//! on-air point) at the second given: an animation that has more time than
//! it needs plays at its speed then holds on the stop, one that has less is
//! played faster to arrive in time.

use std::sync::mpsc;

use cg_core::{ChannelState, Command, Data, MarkerKind, Project, Template};
use cg_render::{OutputFormat, Renderer};
use serde::Deserialize;

/// Longest clip accepted.
pub const MAX_SECONDS: f64 = 300.0;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderRequest {
    pub project_id: String,
    pub template_id: String,
    #[serde(default)]
    pub data: Data,
    pub width: u32,
    pub height: u32,
    /// Output frame rate (the sequence's).
    pub frame_rate: cg_core::FrameRate,
    /// Seconds on air between the in and out animations.
    #[serde(default = "default_hold")]
    pub hold: f64,
    /// Play the out animation at the end.
    #[serde(default = "yes")]
    pub include_out: bool,
    /// The clip's own timeline; `hold` and `includeOut` are then not used.
    #[serde(default)]
    pub timing: Option<ClipTiming>,
}

/// When a clip leaves each stop of its template, in seconds of the clip.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipTiming {
    /// One per stop of the template, in order (see [`stops`]).
    pub stops: Vec<f64>,
    /// Length of the clip.
    pub duration: f64,
}

/// The stops of a template, in frames and in order: its pauses and its
/// on-air point (one stop per frame).
pub fn stops(t: &Template) -> Vec<f64> {
    let mut v: Vec<f64> = t
        .markers
        .iter()
        .filter(|m| m.kind == MarkerKind::Pause)
        .map(|m| m.frame.clamp(0.0, t.duration))
        .chain(std::iter::once(t.hold_frame()))
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    v
}

impl ClipTiming {
    /// Within the clip and in order; as many stops as the template has.
    fn checked(&self, count: usize) -> Result<ClipTiming, String> {
        if self.stops.len() != count {
            return Err(format!(
                "the clip timing has {} stops, the template {count}",
                self.stops.len()
            ));
        }
        if !self.duration.is_finite() || self.duration <= 0.0 || self.duration > MAX_SECONDS {
            return Err("invalid clip duration".into());
        }
        let mut prev = 0.0;
        let stops = self
            .stops
            .iter()
            .map(|&s| {
                prev = if s.is_finite() { s } else { prev }.clamp(prev, self.duration);
                prev
            })
            .collect();
        Ok(ClipTiming {
            stops,
            duration: self.duration,
        })
    }

    /// The template frame shown `t` seconds into the clip (`stops` of the
    /// template in frames, `fps` its frame rate); `None` once it has gone out.
    fn frame_at(&self, stops: &[f64], end: f64, fps: f64, t: f64) -> Option<f64> {
        let (mut from_f, mut from_t) = (0.0, 0.0);
        for k in 0..=stops.len() {
            let last = k == stops.len();
            let (to_f, to_t) = if last {
                (end, self.duration)
            } else {
                (stops[k], self.stops[k])
            };
            if last || t < to_t {
                let anim = (to_f - from_f) / fps;
                let len = to_t - from_t;
                let d = (t - from_t).max(0.0);
                let f = if len >= anim {
                    from_f + d.min(anim) * fps
                } else if len > 0.0 {
                    from_f + (d / len).min(1.0) * (to_f - from_f)
                } else {
                    to_f
                };
                return (!last || f < end - 1e-6).then_some(f);
            }
            (from_f, from_t) = (to_f, to_t);
        }
        None
    }
}

fn default_hold() -> f64 {
    5.0
}
fn yes() -> bool {
    true
}

/// Renders the clip and returns a QuickTime file (PNG frames with alpha).
pub fn render_clip(
    project: &Project,
    req: &RenderRequest,
    assets: std::path::PathBuf,
    tz: &str,
) -> Result<Vec<u8>, String> {
    let template = project
        .template(&req.template_id)
        .ok_or("no such template")?;
    if !(16..=7680).contains(&req.width) || !(16..=7680).contains(&req.height) {
        return Err("invalid size".into());
    }
    let out_fps = req.frame_rate.as_f64();
    if !(1.0..=240.0).contains(&out_fps) {
        return Err("invalid frame rate".into());
    }
    let max_frames = (MAX_SECONDS * out_fps) as usize;
    // Project frames advanced per output frame.
    let step = project.frame_rate.as_f64().max(1.0) / out_fps;
    let hold_frames = (req.hold.clamp(0.0, MAX_SECONDS) * out_fps).round() as usize;

    let mut state = ChannelState::default();
    state
        .apply(
            project,
            Command::Take {
                zone: None,
                layer: Some(1),
                template_id: template.id.clone(),
                data: Some(req.data.clone()),
                rundown_id: None,
            },
        )
        .map_err(|e| e.to_string())?;
    let out = OutputFormat {
        width: req.width,
        height: req.height,
    };

    // Encode PNGs on every core while frames are rendered in order.
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(1, 8);
    let (job_tx, job_rx) = mpsc::sync_channel::<(usize, cg_render::Frame)>(workers * 2);
    let job_rx = std::sync::Arc::new(std::sync::Mutex::new(job_rx));
    let (done_tx, done_rx) = mpsc::channel::<(usize, Vec<u8>)>();
    let handles: Vec<_> = (0..workers)
        .map(|_| {
            let rx = job_rx.clone();
            let tx = done_tx.clone();
            std::thread::spawn(move || loop {
                let job = rx.lock().ok().and_then(|r| r.recv().ok());
                let Some((i, frame)) = job else { break };
                let _ = tx.send((i, frame.to_png()));
            })
        })
        .collect();
    drop(done_tx);

    let mut renderer = Renderer::new(assets);
    let mut n = 0usize;
    let mut held = 0usize;
    let mut out_started = false;
    if let Some(timing) = &req.timing {
        let stops = stops(template);
        let timing = timing.checked(stops.len())?;
        let fps = project.frame_rate.as_f64().max(1.0);
        let total = ((timing.duration * out_fps).round() as usize).clamp(1, max_frames);
        let start_ms = cg_core::playout::now_ms() as f64;
        for i in 0..total {
            let t = i as f64 / out_fps;
            match timing.frame_at(&stops, template.duration, fps, t) {
                Some(f) => {
                    if let Some(item) = state.layers.get_mut(&1) {
                        item.frame = f;
                        item.age = t * fps;
                    }
                }
                None => {
                    state.layers.remove(&1);
                }
            }
            let frame = renderer.render_channel_at(project, &state, out, tz, start_ms + t * 1000.0);
            if job_tx.send((i, frame)).is_err() {
                break;
            }
            n += 1;
        }
    } else {
        loop {
            if n >= max_frames {
                break;
            }
            let frame = renderer.render_channel(project, &state, out, tz);
            if job_tx.send((n, frame)).is_err() {
                break;
            }
            n += 1;
            // A clip plays through the pauses: it only holds on the on-air point.
            let at_pause = state.layers.get(&1).is_some_and(|i| {
                i.phase == cg_core::Phase::OutHold
                    || (i.phase == cg_core::Phase::Holding
                        && project
                            .template(&i.template_id)
                            .is_some_and(|t| i.frame < t.hold_frame() - 1e-6))
            });
            if at_pause {
                let _ = state.apply(
                    project,
                    Command::Continue {
                        zone: None,
                        layer: Some(1),
                    },
                );
            }
            let item = state.layers.get(&1);
            match item {
                None => break,
                Some(i) if i.phase == cg_core::Phase::Holding && !out_started => {
                    held += 1;
                    if held > hold_frames {
                        if !req.include_out {
                            break;
                        }
                        out_started = true;
                        state
                            .apply(
                                project,
                                Command::TakeOut {
                                    zone: None,
                                    layer: Some(1),
                                },
                            )
                            .map_err(|e| e.to_string())?;
                        if state.layers.is_empty() {
                            break;
                        }
                    }
                }
                // Holding at the very end of a template without out animation.
                Some(i) if i.phase == cg_core::Phase::Holding && out_started => break,
                _ => {}
            }
            state.tick(step);
        }
    }
    drop(job_tx);
    let mut frames: Vec<Option<Vec<u8>>> = vec![None; n];
    for (i, png) in done_rx {
        if i < frames.len() {
            frames[i] = Some(png);
        }
    }
    for h in handles {
        let _ = h.join();
    }
    let frames: Vec<Vec<u8>> = frames.into_iter().map(|f| f.unwrap_or_default()).collect();
    if frames.is_empty() || frames.iter().any(|f| f.is_empty()) {
        return Err("rendering failed".into());
    }
    Ok(cg_output::mov::write_png_mov(
        &frames,
        req.width,
        req.height,
        (req.frame_rate.num, req.frame_rate.den),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_in_hold_and_out() {
        let p = cg_core::sample::demo_project();
        let req = RenderRequest {
            project_id: p.id.clone(),
            template_id: "lower-third".into(),
            data: Data::new(),
            width: 192,
            height: 108,
            frame_rate: cg_core::FrameRate::new(25, 1),
            hold: 1.0,
            include_out: true,
            timing: None,
        };
        let mov = render_clip(&p, &req, std::env::temp_dir(), "UTC").unwrap();
        let t = p.template("lower-third").unwrap();
        // In + 1 s hold + out, at 25 fps from a 50 fps project.
        let expected = (t.hold_frame() / 2.0) + 25.0 + (t.duration - t.out_frame().unwrap()) / 2.0;
        let count = u32::from_be_bytes(
            mov[mov.windows(4).position(|w| w == b"stsz").unwrap() + 12..][..4]
                .try_into()
                .unwrap(),
        ) as f64;
        assert!(
            (count - expected).abs() <= 3.0,
            "{count} frames, expected about {expected}"
        );
    }

    #[test]
    fn clip_timing_holds_or_speeds_up() {
        // In: 25 frames to the on-air point, out: 25 more, at 25 fps.
        let at = |stop: f64, duration: f64, t: f64| {
            ClipTiming {
                stops: vec![stop],
                duration,
            }
            .frame_at(&[25.0], 50.0, 25.0, t)
        };
        // Time to spare: plays at its speed, holds on air, goes out at 3 s.
        assert_eq!(at(3.0, 5.0, 0.5), Some(12.5));
        assert_eq!(at(3.0, 5.0, 2.0), Some(25.0));
        assert_eq!(at(3.0, 5.0, 3.5), Some(37.5));
        assert_eq!(at(3.0, 5.0, 4.5), None);
        // Half the time: twice as fast.
        assert_eq!(at(0.5, 1.0, 0.25), Some(12.5));
        assert_eq!(at(0.5, 1.0, 0.75), Some(37.5));
    }

    #[test]
    fn clip_timing_lasts_its_duration() {
        let p = cg_core::sample::demo_project();
        let t = p.template("lower-third").unwrap();
        let n = stops(t).len();
        let req = RenderRequest {
            project_id: p.id.clone(),
            template_id: "lower-third".into(),
            data: Data::new(),
            width: 192,
            height: 108,
            frame_rate: cg_core::FrameRate::new(25, 1),
            hold: 5.0,
            include_out: true,
            timing: Some(ClipTiming {
                stops: (1..=n).map(|i| i as f64 * 0.8).collect(),
                duration: 3.0,
            }),
        };
        let mov = render_clip(&p, &req, std::env::temp_dir(), "UTC").unwrap();
        let count = u32::from_be_bytes(
            mov[mov.windows(4).position(|w| w == b"stsz").unwrap() + 12..][..4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(count, 75);
        // Stops that do not match the template are refused.
        let bad = RenderRequest {
            timing: Some(ClipTiming {
                stops: vec![1.0; n + 1],
                duration: 3.0,
            }),
            ..req
        };
        assert!(render_clip(&p, &bad, std::env::temp_dir(), "UTC").is_err());
    }
}
