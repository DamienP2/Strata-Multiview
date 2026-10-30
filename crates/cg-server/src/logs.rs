//! The recent log in memory, for the Performance page: every event the
//! server logs also goes into a ring of the last few thousand lines.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

const KEEP: usize = 5000;

#[derive(Clone, Serialize)]
pub struct Entry {
    pub seq: u64,
    /// ms since 1970.
    pub at: i64,
    pub level: &'static str,
    pub target: String,
    pub message: String,
    /// The other fields, "key=value key=value".
    pub fields: String,
}

/// The log file, when the server writes one.
pub static FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

static RING: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
static SEQ: AtomicU64 = AtomicU64::new(0);

/// The tracing layer that fills the ring.
pub struct Ring;

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_str(&mut self, f: &Field, v: &str) {
        if f.name() == "message" {
            self.message = v.to_string();
        } else {
            let _ = write!(self.rest, "{}{}={v:?}", sep(&self.rest), f.name());
        }
    }
    fn record_debug(&mut self, f: &Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" {
            self.message = format!("{v:?}");
        } else {
            let _ = write!(self.rest, "{}{}={v:?}", sep(&self.rest), f.name());
        }
    }
}

fn sep(s: &str) -> &'static str {
    if s.is_empty() {
        ""
    } else {
        " "
    }
}

impl<S: Subscriber> Layer<S> for Ring {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut f = Fields::default();
        event.record(&mut f);
        let meta = event.metadata();
        let e = Entry {
            seq: SEQ.fetch_add(1, Ordering::Relaxed) + 1,
            at: crate::state::now_ms(),
            level: match *meta.level() {
                tracing::Level::ERROR => "ERROR",
                tracing::Level::WARN => "WARN",
                tracing::Level::INFO => "INFO",
                tracing::Level::DEBUG => "DEBUG",
                tracing::Level::TRACE => "TRACE",
            },
            target: meta.target().to_string(),
            message: f.message,
            fields: f.rest,
        };
        // Kept on disk too (System › Health).
        crate::health::record(&e);
        let mut ring = RING.lock().unwrap_or_else(|p| p.into_inner());
        if ring.len() == KEEP {
            ring.pop_front();
        }
        ring.push_back(e);
    }
}

fn rank(level: &str) -> u8 {
    match level {
        "ERROR" => 0,
        "WARN" => 1,
        "INFO" => 2,
        "DEBUG" => 3,
        _ => 4,
    }
}

/// Lines after `after`, at `level` or more serious, oldest first (at most `limit`, the latest ones).
pub fn since(after: u64, level: Option<&str>, limit: usize) -> (Vec<Entry>, u64) {
    let ring = RING.lock().unwrap_or_else(|p| p.into_inner());
    let max = level.map_or(4, |l| rank(&l.to_ascii_uppercase()));
    let mut v: Vec<Entry> = ring
        .iter()
        .filter(|e| e.seq > after && rank(e.level) <= max)
        .cloned()
        .collect();
    if v.len() > limit {
        v.drain(..v.len() - limit);
    }
    (v, SEQ.load(Ordering::Relaxed))
}

/// The whole ring as text, like the log file.
pub fn text() -> String {
    let ring = RING.lock().unwrap_or_else(|p| p.into_inner());
    let mut s = String::new();
    for e in ring.iter() {
        let at = jiff::Timestamp::from_millisecond(e.at)
            .map(|t| t.to_string())
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "{at} {:5} {}: {}{}{}",
            e.level,
            e.target,
            e.message,
            sep(&e.fields),
            e.fields
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn events_go_into_the_ring() {
        let sub = tracing_subscriber::registry().with(Ring);
        tracing::subscriber::with_default(sub, || {
            tracing::info!(output = "NEWS", n = 3, "frames dropped");
            tracing::debug!("details");
        });
        let (all, last) = since(0, None, 100);
        let e = all.iter().find(|e| e.message == "frames dropped").unwrap();
        assert_eq!(e.level, "INFO");
        assert_eq!(e.fields, "output=\"NEWS\" n=3");
        assert!(last >= e.seq);
        let (warn, _) = since(0, Some("warn"), 100);
        assert!(warn.iter().all(|e| e.level == "WARN" || e.level == "ERROR"));
        let (after, _) = since(e.seq, None, 100);
        assert!(after.iter().all(|x| x.seq > e.seq));
        assert!(text().contains("frames dropped output=\"NEWS\" n=3"));
    }
}
