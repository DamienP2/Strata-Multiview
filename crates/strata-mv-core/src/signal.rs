//! What a source carries besides its picture, as the multiview shows it:
//! its tally lamps and UMD text (from the switcher: TSL, ATEM, GPI, the
//! API) and the levels of its audio channels. The server keeps them up to
//! date (strata-mv-server signals.rs); the Source widget draws them, in
//! the engine and in the browser (`web/src/engine/liveInputs.ts`).

use serde::{Deserialize, Serialize};

/// The level of silence, dBFS (no sample, or below the meters' scale).
pub const SILENCE: f32 = -120.0;

/// One audio channel, dBFS (0 = full scale; [`SILENCE`] when nothing).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// Highest sample of the last measure.
    pub peak: f32,
    /// Mean power of the last measure.
    pub rms: f32,
    /// Highest peak of the last seconds (the meter's hold mark).
    pub hold: f32,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            peak: SILENCE,
            rms: SILENCE,
            hold: SILENCE,
        }
    }
}

impl Level {
    /// The level of samples (-1…1): peak and mean power, dBFS.
    pub fn of(samples: impl Iterator<Item = f32>) -> Self {
        let (mut peak, mut sum, mut n) = (0f32, 0f64, 0usize);
        for s in samples {
            let a = s.abs();
            if a.is_finite() {
                peak = peak.max(a);
                sum += (s as f64) * (s as f64);
                n += 1;
            }
        }
        let rms = if n > 0 { (sum / n as f64).sqrt() as f32 } else { 0.0 };
        let peak = db(peak);
        Self {
            peak,
            rms: db(rms),
            hold: peak,
        }
    }
}

/// A linear amplitude (1 = full scale) in dBFS, [`SILENCE`] at most.
pub fn db(a: f32) -> f32 {
    if a > 0.0 && a.is_finite() {
        (20.0 * a.log10()).max(SILENCE)
    } else {
        SILENCE
    }
}

/// The tally lamps of a source and the text of its UMD.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tally {
    /// On air (red).
    pub program: bool,
    /// Next (green).
    pub preview: bool,
    /// A third lamp (yellow): an ISO record, an aux bus…
    pub aux: bool,
    /// The name the switcher gives it ("" = none received).
    pub umd: String,
}

/// Everything the Source widget shows of a live input.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceView {
    /// The input's name (Video inputs page).
    pub name: String,
    /// "1080i50" once a picture comes ("" until then).
    pub format: String,
    /// A picture came in the last two seconds.
    pub live: bool,
    pub tally: Tally,
    /// One per channel of its audio, in order (none: no audio).
    pub levels: Vec<Level>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_of_samples() {
        let l = Level::of([0.5f32, -0.5, 0.5, -0.5].into_iter());
        assert!((l.peak - -6.02).abs() < 0.01);
        assert!((l.rms - -6.02).abs() < 0.01);
        assert_eq!(Level::of(std::iter::empty()).peak, SILENCE);
        assert_eq!(db(0.0), SILENCE);
        assert_eq!(db(1.0), 0.0);
    }
}
