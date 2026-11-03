//! The audio measures of an input, as broadcast meters make them:
//!
//! - **Sample peak** of each channel, 25 times a second (IEC 60268-18
//!   digital peak meter: no integration; the meters fall 20 dB in 1.7 s,
//!   signals.rs).
//! - **RMS** of each channel, AES17 convention (a sine reads the same as
//!   its peak: +3.01 dB over the plain mean power).
//! - **Loudness** of the programme (ITU-R BS.1770-4, EBU R128, Tech 3341
//!   and 3342): momentary 400 ms, short-term 3 s, integrated (gated, since
//!   the input started), loudness range; on the programme's channels only
//!   (a pair, a 5.1, a mono counted as dual mono), with their weights (the
//!   LFE out, the surrounds +1.5 dB).
//! - **True peak** of the programme (BS.1770-4 annex 2, 4 × oversampling):
//!   the highest of the last three seconds, and the highest since the start.
//!
//! The same code for every kind of input: the sound ffmpeg decodes, sent as
//! 32-bit float WAV on a local port ([`WavHeader`]), and NDI's.

use std::collections::VecDeque;

use ebur128::{Channel, EbuR128, Mode};
use strata_mv_core::signal::{db, Level, Loudness};

/// Measures a second (40 ms each).
pub const RATE: u32 = 25;
/// Measures over which the true peak is the highest (3 s).
const TP_WINDOWS: usize = 3 * RATE as usize;
/// AES17: the RMS of a full-scale sine reads 0 dBFS.
const AES17: f32 = 3.0103;

/// Which channels make the programme whose loudness is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Programme {
    /// A mono, a stereo, a 5.1 as they are; the first pair of more.
    #[default]
    Auto,
    /// Channels 2n-1 and 2n (n from 1).
    Pair(usize),
    /// A 5.1 (L R C LFE Ls Rs) from channel n (1 or 3…).
    Surround(usize),
    /// Every channel, each counted as a front one.
    All,
}

impl Programme {
    /// From the input's setting: "" (auto), "1-2", "3-4"…, "5.1", "5.1:3",
    /// "all".
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() || s == "auto" {
            return Programme::Auto;
        }
        if s == "all" {
            return Programme::All;
        }
        if let Some(rest) = s.strip_prefix("5.1") {
            let from = rest
                .trim_start_matches(':')
                .parse::<usize>()
                .unwrap_or(1)
                .max(1);
            return Programme::Surround(from);
        }
        match s.split_once('-').and_then(|(a, _)| a.parse::<usize>().ok()) {
            Some(a) if a >= 1 => Programme::Pair(a.div_ceil(2)),
            _ => Programme::Auto,
        }
    }

    /// The channels (0-based) of a track of `n` channels and their kinds.
    pub fn channels(&self, n: usize) -> Vec<(usize, Channel)> {
        let pair = |p: usize| -> Vec<(usize, Channel)> {
            let a = 2 * (p.max(1) - 1);
            if a + 1 < n {
                vec![(a, Channel::Left), (a + 1, Channel::Right)]
            } else if a < n {
                vec![(a, Channel::DualMono)]
            } else {
                Vec::new()
            }
        };
        let surround = |from: usize| -> Vec<(usize, Channel)> {
            let a = from.max(1) - 1;
            if a + 6 > n {
                return pair(1);
            }
            [
                Channel::Left,
                Channel::Right,
                Channel::Center,
                Channel::Unused,
                Channel::LeftSurround,
                Channel::RightSurround,
            ]
            .into_iter()
            .enumerate()
            .map(|(i, c)| (a + i, c))
            .collect()
        };
        match *self {
            Programme::Auto => match n {
                0 => Vec::new(),
                // A mono is heard on both sides: counted twice (R128 s1).
                1 => vec![(0, Channel::DualMono)],
                6 => surround(1),
                _ => pair(1),
            },
            Programme::Pair(p) => pair(p),
            Programme::Surround(from) => surround(from),
            Programme::All => (0..n).map(|i| (i, Channel::Center)).collect(),
        }
    }
}

/// One measure: the channels' levels, the programme's loudness, and the
/// samples measured since the start (its end).
#[derive(Debug, Clone)]
pub struct Measure {
    pub levels: Vec<Level>,
    pub loudness: Option<Loudness>,
    pub end: u64,
}

/// An input's meter.
pub struct Meter {
    channels: usize,
    rate: u32,
    prog: Vec<usize>,
    r128: Option<EbuR128>,
    /// Samples a measure (40 ms).
    window: usize,
    have: usize,
    peaks: Vec<f32>,
    sums: Vec<f64>,
    /// The programme's samples of the measure, interleaved.
    chunk: Vec<f32>,
    tp: VecDeque<f32>,
    tp_max: f32,
    samples: u64,
}

impl Meter {
    pub fn new(channels: usize, rate: u32, programme: Programme) -> Self {
        let prog = programme.channels(channels);
        let r128 = (!prog.is_empty() && rate > 0)
            .then(|| {
                let mut m = EbuR128::new(
                    prog.len() as u32,
                    rate,
                    // Histogram: the integrated loudness and the range cost
                    // the same after hours as after a second.
                    Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK | Mode::HISTOGRAM,
                )
                .ok()?;
                for (i, (_, c)) in prog.iter().enumerate() {
                    m.set_channel(i as u32, *c).ok()?;
                }
                Some(m)
            })
            .flatten();
        Meter {
            channels,
            rate,
            prog: prog.iter().map(|(i, _)| *i).collect(),
            r128,
            window: (rate / RATE).max(1) as usize,
            have: 0,
            peaks: vec![0.0; channels],
            sums: vec![0.0; channels],
            chunk: Vec::new(),
            tp: VecDeque::new(),
            tp_max: 0.0,
            samples: 0,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Interleaved samples (whole frames of `channels`).
    pub fn add_interleaved(&mut self, data: &[f32]) -> Vec<Measure> {
        let mut out = Vec::new();
        if self.channels == 0 {
            return out;
        }
        for frame in data.chunks_exact(self.channels) {
            self.frame(|c| frame[c], &mut out);
        }
        out
    }

    /// Planes, one a channel, of the same length.
    pub fn add_planar(&mut self, planes: &[&[f32]]) -> Vec<Measure> {
        let mut out = Vec::new();
        if planes.len() != self.channels || self.channels == 0 {
            return out;
        }
        let n = planes.iter().map(|p| p.len()).min().unwrap_or(0);
        // Sample i of every channel, one frame after the other.
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            self.frame(|c| planes[c][i], &mut out);
        }
        out
    }

    fn frame(&mut self, sample: impl Fn(usize) -> f32, out: &mut Vec<Measure>) {
        for c in 0..self.channels {
            let s = sample(c);
            let s = if s.is_finite() { s } else { 0.0 };
            self.peaks[c] = self.peaks[c].max(s.abs());
            self.sums[c] += (s as f64) * (s as f64);
        }
        for &c in &self.prog {
            self.chunk.push(sample(c));
        }
        self.have += 1;
        self.samples += 1;
        if self.have >= self.window {
            out.push(self.measure());
        }
    }

    /// The measure of the samples since the last one.
    fn measure(&mut self) -> Measure {
        let n = self.have.max(1) as f64;
        let levels = (0..self.channels)
            .map(|c| {
                let peak = db(self.peaks[c]);
                let rms = db((self.sums[c] / n).sqrt() as f32);
                Level {
                    peak,
                    // AES17: a square wave reads +3 dB over its peak.
                    rms: if rms > strata_mv_core::signal::SILENCE {
                        rms + AES17
                    } else {
                        rms
                    },
                    hold: peak,
                }
            })
            .collect();
        self.peaks.iter_mut().for_each(|p| *p = 0.0);
        self.sums.iter_mut().for_each(|s| *s = 0.0);
        self.have = 0;
        let mut loudness = None;
        if let Some(m) = self.r128.as_mut() {
            if m.add_frames_f32(&self.chunk).is_ok() {
                let tp = (0..self.prog.len() as u32)
                    .filter_map(|c| m.prev_true_peak(c).ok())
                    .fold(0.0f64, f64::max) as f32;
                self.tp.push_back(tp);
                while self.tp.len() > TP_WINDOWS {
                    self.tp.pop_front();
                }
                self.tp_max = self.tp_max.max(tp);
                let lufs = |r: Result<f64, ebur128::Error>| {
                    r.ok()
                        .filter(|v| v.is_finite())
                        .map_or(-70.0, |v| (v as f32).max(-70.0))
                };
                loudness = Some(Loudness {
                    momentary: lufs(m.loudness_momentary()),
                    short_term: lufs(m.loudness_shortterm()),
                    integrated: lufs(m.loudness_global()),
                    range: m
                        .loudness_range()
                        .ok()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0) as f32,
                    true_peak: db(self.tp.iter().copied().fold(0.0, f32::max)),
                });
            }
        }
        self.chunk.clear();
        Measure {
            levels,
            loudness,
            end: self.samples,
        }
    }

    /// The highest true peak since the start, dBTP.
    #[cfg(test)]
    pub fn true_peak_max(&self) -> f32 {
        db(self.tp_max)
    }
}

/// The header of the WAV ffmpeg writes on the meter's port: its format.
#[derive(Debug, Clone, PartialEq)]
pub struct WavHeader {
    pub channels: usize,
    pub rate: u32,
    /// Bytes of the header (the samples follow).
    pub len: usize,
}

impl WavHeader {
    /// None: not whole yet (or not 32-bit float WAV: Err).
    pub fn parse(b: &[u8]) -> Result<Option<WavHeader>, String> {
        if b.len() < 12 {
            return Ok(None);
        }
        if &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
            return Err("not WAV".into());
        }
        let mut i = 12;
        let mut fmt = None;
        loop {
            let Some(h) = b.get(i..i + 8) else {
                return Ok(None);
            };
            let id = &h[0..4];
            let size = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
            if id == b"data" {
                let Some((channels, rate, tag, bits)) = fmt else {
                    return Err("WAV without its format".into());
                };
                // 3: IEEE float; 0xFFFE: extensible (float, its subformat).
                if bits != 32 || !(tag == 3 || tag == 0xFFFE) {
                    return Err(format!("WAV of format {tag}, {bits} bits"));
                }
                return Ok(Some(WavHeader {
                    channels,
                    rate,
                    len: i + 8,
                }));
            }
            let Some(body) = b.get(i + 8..i + 8 + size) else {
                return Ok(None);
            };
            if id == b"fmt " && body.len() >= 16 {
                let tag = u16::from_le_bytes([body[0], body[1]]);
                let channels = u16::from_le_bytes([body[2], body[3]]) as usize;
                let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                fmt = Some((channels, rate, tag, bits));
            }
            // Chunks are padded to an even size.
            i += 8 + size + (size & 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` of a sine of `freq` Hz, `amp` (linear), phase `ph`, at 48 kHz.
    fn sine(freq: f64, amp: f64, ph: f64, secs: f64) -> Vec<f32> {
        (0..(secs * 48_000.0) as usize)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / 48_000.0 + ph).sin()) as f32
            })
            .collect()
    }

    fn amp(dbfs: f64) -> f64 {
        10f64.powf(dbfs / 20.0)
    }

    fn interleave(chs: &[Vec<f32>]) -> Vec<f32> {
        let n = chs[0].len();
        (0..n).flat_map(|i| chs.iter().map(move |c| c[i])).collect()
    }

    fn last(m: &mut Meter, data: &[f32]) -> Measure {
        m.add_interleaved(data).pop().expect("a measure")
    }

    #[test]
    fn tech_3341_stereo_sine() {
        // Case 1: 1 kHz, -23 dBFS on both channels: -23.0 LUFS (±0.1).
        let s = sine(1000.0, amp(-23.0), 0.0, 20.0);
        let mut m = Meter::new(2, 48_000, Programme::Auto);
        let r = last(&mut m, &interleave(&[s.clone(), s]));
        let l = r.loudness.unwrap();
        for v in [l.momentary, l.short_term, l.integrated] {
            assert!((v + 23.0).abs() <= 0.1, "{l:?}");
        }
        // Sample peak -23 dBFS; RMS (AES17) the same for a sine.
        assert!((r.levels[0].peak + 23.0).abs() < 0.05, "{:?}", r.levels[0]);
        assert!((r.levels[0].rms + 23.0).abs() < 0.05, "{:?}", r.levels[0]);
        // Case 2: -33 dBFS: -33.0 LUFS.
        let s = sine(1000.0, amp(-33.0), 0.0, 20.0);
        let mut m = Meter::new(2, 48_000, Programme::Auto);
        let l = last(&mut m, &interleave(&[s.clone(), s])).loudness.unwrap();
        assert!((l.integrated + 33.0).abs() <= 0.1, "{l:?}");
    }

    #[test]
    fn tech_3341_five_channels() {
        // Case 6: L R -28, C -24, Ls Rs -30 dBFS (LFE silent): -23.0 LUFS.
        let lr = sine(1000.0, amp(-28.0), 0.0, 20.0);
        let c = sine(1000.0, amp(-24.0), 0.0, 20.0);
        let s = sine(1000.0, amp(-30.0), 0.0, 20.0);
        let lfe = vec![0.0; lr.len()];
        let mut m = Meter::new(6, 48_000, Programme::Auto);
        let l = last(&mut m, &interleave(&[lr.clone(), lr, c, lfe, s.clone(), s]))
            .loudness
            .unwrap();
        assert!((l.integrated + 23.0).abs() <= 0.1, "{l:?}");
    }

    #[test]
    fn the_programme_of_a_multichannel_track() {
        // 8 channels: the programme on 1-2, other languages on 3-8. Only
        // the first pair is measured (else +4.8 dB).
        let p = sine(1000.0, amp(-23.0), 0.0, 10.0);
        let chs: Vec<Vec<f32>> = (0..8).map(|_| p.clone()).collect();
        let mut m = Meter::new(8, 48_000, Programme::Auto);
        let l = last(&mut m, &interleave(&chs)).loudness.unwrap();
        assert!((l.integrated + 23.0).abs() <= 0.1, "{l:?}");
        // Asked for: the third pair; all of them.
        assert_eq!(Programme::parse("5-6").channels(8)[0].0, 4);
        let mut all = Meter::new(8, 48_000, Programme::parse("all"));
        let l = last(&mut all, &interleave(&chs)).loudness.unwrap();
        assert!(
            (l.integrated - (-23.0 + 10.0 * 4f32.log10())).abs() <= 0.1,
            "{l:?}"
        );
        // A mono: counted twice (as heard on both sides).
        let mut mono = Meter::new(1, 48_000, Programme::Auto);
        let l = last(&mut mono, &p).loudness.unwrap();
        assert!((l.integrated + 23.0).abs() <= 0.1, "{l:?}");
        assert_eq!(Programme::parse("5.1:3"), Programme::Surround(3));
        assert_eq!(Programme::parse(""), Programme::Auto);
        // A pair the track has not: none.
        assert!(Programme::Pair(3).channels(2).is_empty());
    }

    #[test]
    fn tech_3341_true_peak() {
        // A quarter of the rate, 45° off the samples: the samples reach
        // -9.03 dBFS, the true peak -6.02 dBTP (+0.2 / -0.4 allowed).
        let s = sine(12_000.0, 0.5, std::f64::consts::FRAC_PI_4, 5.0);
        let mut m = Meter::new(2, 48_000, Programme::Auto);
        let r = last(&mut m, &interleave(&[s.clone(), s]));
        let tp = r.loudness.unwrap().true_peak;
        assert!((-6.42..=-5.82).contains(&tp), "true peak {tp}");
        assert!((r.levels[0].peak + 9.03).abs() < 0.05);
        // Three seconds later at -30 dBFS: the true peak follows (the
        // highest of the start kept apart).
        let q = sine(1000.0, amp(-30.0), 0.0, 4.0);
        let tp = last(&mut m, &interleave(&[q.clone(), q]))
            .loudness
            .unwrap()
            .true_peak;
        assert!((-30.5..=-29.5).contains(&tp), "true peak {tp}");
        assert!((m.true_peak_max() + 6.02).abs() < 0.4);
    }

    #[test]
    fn twenty_five_measures_a_second() {
        let s = vec![0.25f32; 2 * 48_000];
        let mut m = Meter::new(2, 48_000, Programme::Auto);
        let out = m.add_interleaved(&s);
        assert_eq!(out.len(), 25);
        assert_eq!(out[0].end, 1920);
        // Planar the same.
        let p = vec![0.25f32; 48_000];
        let mut m = Meter::new(2, 48_000, Programme::Auto);
        assert_eq!(m.add_planar(&[&p, &p]).len(), 25);
    }

    #[test]
    fn the_wav_header_ffmpeg_writes() {
        // RIFF, WAVE, fmt (extensible float, 6 channels), LIST, data.
        let mut b = Vec::new();
        b.extend(b"RIFF\xff\xff\xff\xffWAVE");
        b.extend(b"fmt ");
        b.extend(40u32.to_le_bytes());
        b.extend(0xFFFEu16.to_le_bytes());
        b.extend(6u16.to_le_bytes());
        b.extend(48_000u32.to_le_bytes());
        b.extend((48_000u32 * 24).to_le_bytes());
        b.extend(24u16.to_le_bytes());
        b.extend(32u16.to_le_bytes());
        b.extend([0u8; 24]);
        b.extend(b"LIST");
        b.extend(3u32.to_le_bytes());
        b.extend(b"abc\0");
        b.extend(b"data\xff\xff\xff\xff");
        let h = WavHeader::parse(&b).unwrap().unwrap();
        assert_eq!((h.channels, h.rate, h.len), (6, 48_000, b.len()));
        assert_eq!(WavHeader::parse(&b[..30]).unwrap(), None);
        assert!(WavHeader::parse(b"RIFX\0\0\0\0WAVE").is_err());
    }
}
