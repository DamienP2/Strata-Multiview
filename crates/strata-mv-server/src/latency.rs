//! End-to-end latency, measured with the test generator's lip sync flash.
//!
//! A channel showing the generator's lip sync pattern sends a white flash on
//! the tick that starts each second; the time it leaves is kept here. An
//! input receiving it back (the output looped to an input, through a
//! router, an encoder, a network…) sees the middle of its picture turn
//! white: the time between the two is the latency of the whole chain,
//! decoding included. The last 20 measures of each input are kept.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::auth::CurrentUser;
use crate::state::Shared;

/// Flashes sent: (channel, when it left).
static SENT: LazyLock<Mutex<VecDeque<(String, SystemTime)>>> = LazyLock::new(Default::default);

#[derive(Default)]
struct Seen {
    bright: bool,
    /// (latency ms, channel, when measured ms since 1970).
    measures: VecDeque<(f64, String, i64)>,
}

static SEEN: LazyLock<Mutex<HashMap<String, Seen>>> = LazyLock::new(Default::default);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A channel sent the lip sync flash at `at`.
pub fn sent(channel: &str, at: SystemTime) {
    let mut s = lock(&SENT);
    s.push_back((channel.to_string(), at));
    while s.len() > 40 {
        s.pop_front();
    }
}

/// The flash of the lip sync pattern in a 16 × 9 luma print: the middle
/// white, the side dark (the flash, not a white picture).
fn flash_in(print: &[u8]) -> Option<bool> {
    if print.len() < 144 {
        return None;
    }
    let cell = |x: usize, y: usize| print[y * 16 + x] as u32;
    let mid: u32 = (2..=4)
        .flat_map(|y| (6..=9).map(move |x| (x, y)))
        .map(|(x, y)| cell(x, y))
        .sum::<u32>()
        / 12;
    let side = (cell(1, 3) + cell(14, 3)) / 2;
    Some(mid > 190 && side < 110)
}

/// An input's picture came (its luma print): a flash coming back is measured.
pub fn picture(input: &str, print: &[u8], now: SystemTime) {
    let Some(bright) = flash_in(print) else {
        return;
    };
    let mut seen = lock(&SEEN);
    let s = seen.entry(input.to_string()).or_default();
    let rising = bright && !s.bright;
    s.bright = bright;
    if !rising {
        return;
    }
    // The latest flash sent before it, less than 3 s ago.
    let found = lock(&SENT).iter().rev().find_map(|(ch, at)| {
        now.duration_since(*at)
            .ok()
            .filter(|d| *d < Duration::from_secs(3))
            .map(|d| (ch.clone(), d))
    });
    let Some((ch, d)) = found else { return };
    let ms = d.as_secs_f64() * 1000.0;
    let at = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    s.measures.push_back(((ms * 10.0).round() / 10.0, ch, at));
    while s.measures.len() > 20 {
        s.measures.pop_front();
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Latency {
    pub input: String,
    pub channel: String,
    pub last_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub count: usize,
    /// When last measured, ms since 1970.
    pub at: i64,
}

/// The inputs' latencies measured.
pub fn all() -> Vec<Latency> {
    let seen = lock(&SEEN);
    let mut out: Vec<Latency> = seen
        .iter()
        .filter_map(|(id, s)| {
            let (last, ch, at) = s.measures.back()?.clone();
            let v: Vec<f64> = s.measures.iter().map(|m| m.0).collect();
            let avg = v.iter().sum::<f64>() / v.len() as f64;
            Some(Latency {
                input: id.clone(),
                channel: ch,
                last_ms: last,
                avg_ms: (avg * 10.0).round() / 10.0,
                min_ms: v.iter().copied().fold(f64::MAX, f64::min),
                max_ms: v.iter().copied().fold(0.0, f64::max),
                count: v.len(),
                at,
            })
        })
        .collect();
    out.sort_by(|a, b| a.input.cmp(&b.input));
    out
}

/// `GET /api/latency`.
pub async fn get(State(_s): State<Shared>, CurrentUser(_me): CurrentUser) -> Json<Vec<Latency>> {
    Json(all())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print(mid: u8, side: u8) -> Vec<u8> {
        let mut p = vec![side; 144];
        for y in 2..=4 {
            for x in 6..=9 {
                p[y * 16 + x] = mid;
            }
        }
        p
    }

    #[test]
    fn a_flash_coming_back_is_measured() {
        let t0 = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        sent("test-ch", t0);
        picture("loop-in", &print(38, 38), t0 + Duration::from_millis(100));
        // White everywhere: not the flash.
        picture("loop-in", &print(250, 250), t0 + Duration::from_millis(150));
        picture("loop-in", &print(38, 38), t0 + Duration::from_millis(160));
        picture("loop-in", &print(255, 38), t0 + Duration::from_millis(183));
        // Still bright: one measure only.
        picture("loop-in", &print(255, 38), t0 + Duration::from_millis(223));
        let l = all().into_iter().find(|l| l.input == "loop-in").unwrap();
        assert_eq!(l.count, 1);
        assert_eq!(l.last_ms, 183.0);
        assert_eq!(l.channel, "test-ch");
    }
}
