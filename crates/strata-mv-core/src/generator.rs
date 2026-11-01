//! The test signal generator of a channel: what its outputs send instead of
//! the multiview — colour bars, test patterns, a still picture, a lip sync
//! test — with or without sound (1 kHz tone, EBU stereo ident, a beep in
//! time with the flash). The pictures are drawn by
//! `strata-mv-render/src/generator.rs`; the sound is made here, frame by
//! frame, to the sample (so it stays in time with the pictures on every
//! output that carries both).

use serde::{Deserialize, Serialize};

/// What the picture shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    /// SMPTE RP 219 / ARIB STD-B28 HD colour bars.
    #[default]
    SmpteHd,
    /// EBU 100/0/75/0 colour bars.
    Ebu75,
    /// 100 % colour bars.
    Bars100,
    /// A luma ramp over an 11-step grey scale.
    GreyScale,
    /// PLUGE: black level (−2 %, 0, +2 %, +4 %) and white references.
    Pluge,
    /// Crosshatch, centre cross, circle and safe areas (geometry, scaling).
    Grid,
    /// A circular zone plate (frequencies sweeping: scaling, aliasing).
    ZonePlate,
    /// One colour.
    Solid,
    /// A still picture of the library.
    Image,
    /// Lip sync: a flash and a beep at the start of each second, a sweep to
    /// read the offset frame by frame.
    Lipsync,
}

/// What the sound is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// No sound sent.
    None,
    /// Sound sent, silent.
    Silence,
    /// 1 kHz on both channels.
    #[default]
    Tone,
    /// EBU Tech 3304 stereo ident: 1 kHz, the left channel cut for 250 ms
    /// every 3 s.
    EbuIdent,
    /// A 1 kHz beep at the start of each second (with the lip sync flash).
    Beep,
}

/// The generator of a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneratorConfig {
    /// The outputs send it instead of the multiview.
    pub enabled: bool,
    pub pattern: Pattern,
    /// The colour of `Solid` (#rrggbb).
    pub color: String,
    /// The picture of `Image` (a library path).
    pub image: String,
    /// Words in a box over the picture ("" none): the channel's name…
    pub ident: String,
    /// The time of day and frame (HH:MM:SS:FF) burnt in.
    pub clock: bool,
    /// A small box moving across the picture: it is live, not frozen.
    pub motion: bool,
    pub tone: Tone,
    /// The tone's level, dBFS (EBU R68: −18; SMPTE RP 155: −20).
    pub level: f32,
    /// The tone's frequency, Hz.
    pub frequency: f32,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern: Pattern::SmpteHd,
            color: "#000000".into(),
            image: String::new(),
            ident: String::new(),
            clock: false,
            motion: false,
            tone: Tone::Tone,
            level: -18.0,
            frequency: 1000.0,
        }
    }
}

/// The sample rate of the sound.
pub const RATE: u32 = 48_000;

/// The first sample of frame `n` at `num`/`den` frames a second (to the
/// sample: 29.97 alternates 1601 and 1602 samples).
pub fn first_sample(n: u64, num: u32, den: u32) -> u64 {
    (n as u128 * RATE as u128 * den.max(1) as u128 / num.max(1) as u128) as u64
}

/// A second begins during frame `n` (the flash: its picture and sound
/// both start with the frame).
pub fn starts_second(n: u64, num: u32, den: u32) -> bool {
    let fps = num.max(1) as u128;
    let den = den.max(1) as u128;
    // Frame n covers [n·den/num, (n+1)·den/num) seconds.
    let a = n as u128 * den;
    let b = (n as u128 + 1) * den;
    // A whole second s with a ≤ s·num < b.
    let s = a.div_ceil(fps);
    s * fps < b
}

/// How long the lip sync flash and beep last, frames (about 40 ms, one
/// frame at least).
pub fn flash_frames(num: u32, den: u32) -> u64 {
    ((num as f64 / den.max(1) as f64) * 0.04).round().max(1.0) as u64
}

/// The frames since the last second began (0: the flash).
pub fn frames_into_second(n: u64, num: u32, den: u32) -> u64 {
    let mut k = 0;
    while k < n && !starts_second(n - k, num, den) {
        k += 1;
        if k > 400 {
            break;
        }
    }
    k
}

/// The sound of frame `n` (interleaved stereo, 16-bit), none when `None`.
pub fn sound(g: &GeneratorConfig, n: u64, num: u32, den: u32) -> Option<Vec<i16>> {
    if g.tone == Tone::None {
        return None;
    }
    let from = first_sample(n, num, den);
    let to = first_sample(n + 1, num, den);
    let amp = 10f32.powf(g.level.clamp(-60.0, 0.0) / 20.0) * 32767.0;
    let f = g.frequency.clamp(20.0, 20_000.0) as f64;
    // The beep: the flash's frames, from the first sample of the second.
    let beeping = frames_into_second(n, num, den) < flash_frames(num, den);
    let mut out = Vec::with_capacity(((to - from) * 2) as usize);
    for s in from..to {
        let t = s as f64 / RATE as f64;
        let v = ((t * f * std::f64::consts::TAU).sin() as f32 * amp) as i16;
        let (l, r) = match g.tone {
            Tone::Silence | Tone::None => (0, 0),
            Tone::Tone => (v, v),
            Tone::EbuIdent => {
                // The left cut for 250 ms at the start of each 3 s.
                let cut = (s % (3 * RATE as u64)) < (RATE as u64 / 4);
                (if cut { 0 } else { v }, v)
            }
            Tone::Beep => {
                if beeping {
                    (v, v)
                } else {
                    (0, 0)
                }
            }
        };
        out.push(l);
        out.push(r);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sample_once_at_any_rate() {
        // 29.97: 1601.6 samples a frame on average, none lost.
        let total: u64 = (0..30_000)
            .map(|n| first_sample(n + 1, 30_000, 1001) - first_sample(n, 30_000, 1001))
            .sum();
        assert_eq!(total, first_sample(30_000, 30_000, 1001));
        assert_eq!(first_sample(25, 25, 1), 48_000);
        let g = GeneratorConfig::default();
        assert_eq!(sound(&g, 0, 25, 1).unwrap().len(), 1920 * 2);
    }

    #[test]
    fn the_flash_begins_each_second() {
        let flashes: Vec<u64> = (0..100).filter(|n| starts_second(*n, 25, 1)).collect();
        assert_eq!(flashes, vec![0, 25, 50, 75]);
        // 29.97: second 1 begins during frame 29 (0.968 s to 1.001 s).
        let f: Vec<u64> = (0..65)
            .filter(|n| starts_second(*n, 30_000, 1001))
            .collect();
        assert_eq!(f, vec![0, 29, 59]);
        assert_eq!(frames_into_second(27, 25, 1), 2);
        assert_eq!(flash_frames(25, 1), 1);
        assert_eq!(flash_frames(50, 1), 2);
    }

    #[test]
    fn the_beep_goes_with_the_flash() {
        let g = GeneratorConfig {
            tone: Tone::Beep,
            ..Default::default()
        };
        let loud = |n: u64| sound(&g, n, 25, 1).unwrap().iter().any(|v| v.abs() > 1000);
        assert!(loud(0) && loud(25));
        assert!(!loud(1) && !loud(24));
        // −18 dBFS: a peak of about 4 125.
        let peak = sound(&GeneratorConfig::default(), 3, 25, 1)
            .unwrap()
            .iter()
            .map(|v| v.abs())
            .max()
            .unwrap();
        assert!((4000..4200).contains(&peak), "{peak}");
    }

    #[test]
    fn the_ebu_ident_cuts_the_left() {
        let g = GeneratorConfig {
            tone: Tone::EbuIdent,
            ..Default::default()
        };
        let s = sound(&g, 2, 25, 1).unwrap();
        // 80 ms in: left cut, right on.
        assert!(s.iter().step_by(2).all(|v| *v == 0));
        assert!(s.iter().skip(1).step_by(2).any(|v| v.abs() > 1000));
        let s = sound(&g, 20, 25, 1).unwrap();
        assert!(s.iter().step_by(2).any(|v| v.abs() > 1000));
    }
}
