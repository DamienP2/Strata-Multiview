//! Baseband outputs fed by the software renderer.
//!
//! The HTML5 output does not go through here: it is a browser page driven by
//! the server over WebSocket (see `cg-server`), rendering the same scene with
//! the web engine, which is what OBS / vMix / CasparCG browser sources expect.

pub mod colorspace;
pub mod decklink;
pub mod ffmpeg;
pub mod mov;
pub mod ndi;
pub mod sdi;
pub mod st2110;
pub mod stream;

use std::sync::Arc;
use std::time::SystemTime;

use cg_render::Frame;

#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("output not supported: {0}")]
    Unsupported(String),
    #[error("frame dropped (sender busy)")]
    Dropped,
    #[error("output closed")]
    Closed,
}

/// The rows of `new` that are not the same in `old` (true: changed), for an
/// output to convert only those again; None when all are to be (no picture
/// before, another size).
pub fn changed_rows(new: &Frame, old: Option<&Frame>) -> Option<Vec<bool>> {
    let old = old?;
    if (old.width, old.height) != (new.width, new.height) || old.data.len() != new.data.len() {
        return None;
    }
    let row = new.width as usize * 4;
    if row == 0 {
        return None;
    }
    Some(
        new.data
            .chunks_exact(row)
            .zip(old.data.chunks_exact(row))
            .map(|(a, b)| a != b)
            .collect(),
    )
}

/// Outputs are on unless switched off (serde default).
pub fn on() -> bool {
    true
}

/// A hardware frame clock (a DeckLink output locked to its reference).
pub trait FrameClock: Send + Sync {
    /// Waits for the next frame boundary; false on timeout.
    fn wait(&self, timeout: std::time::Duration) -> bool;
    /// Locked to the reference input (None: unknown).
    fn locked(&self) -> Option<bool>;
}

/// Anything that consumes rendered frames at the channel frame rate.
pub trait OutputSink: Send {
    fn name(&self) -> &str;
    /// One frame per tick of the channel clock; `at` is that tick (the same
    /// frame comes again, with a later `at`, when the next one is late).
    fn push(&mut self, frame: &Arc<Frame>, at: SystemTime) -> Result<(), OutputError>;
    /// The process doing the work, when it is another one (ffmpeg), for its load.
    fn process(&self) -> Option<u32> {
        None
    }
    /// Time its own threads have worked so far (not counting waits), for
    /// outputs that send from threads of their own.
    fn work_time(&self) -> Option<std::time::Duration> {
        None
    }
    /// The hardware clock this output runs on, for the channel to follow
    /// (an SDI output set to genlock).
    fn frame_clock(&self) -> Option<Arc<dyn FrameClock>> {
        None
    }
}
