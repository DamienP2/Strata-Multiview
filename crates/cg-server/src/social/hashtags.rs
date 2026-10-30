//! Hashtags a project counts: every message of the connections it follows
//! (before moderation) carrying the tag, any case or accent, per minute
//! and per platform.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use super::rules::plain;
use super::Platform;

/// Minutes kept per hashtag (a show and more).
pub const MINUTES: usize = 360;
/// Minutes in the data source's `series`.
pub const SERIES: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hashtag {
    /// As written, with its "#": "#JT20H".
    pub tag: String,
    pub running: bool,
    /// Counting since (ms): added, or reset.
    pub since: i64,
    #[serde(default)]
    pub count: u64,
    /// Messages per platform ("x", "youtube"…).
    #[serde(default)]
    pub platforms: BTreeMap<String, u64>,
    /// Messages per minute: (minute since the epoch, count), oldest first.
    #[serde(default)]
    pub minutes: VecDeque<(i64, u32)>,
    /// When the messages of the last minute came (ms).
    #[serde(skip)]
    recent: VecDeque<i64>,
}

/// What templates read of a hashtag (`hashtags.<tag>` of the source).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub tag: String,
    pub count: u64,
    pub per_min: u32,
    /// Messages per minute, the last 60, oldest first.
    pub series: Vec<u32>,
    /// Share of each platform, in percent.
    pub platforms: BTreeMap<String, u32>,
}

/// A hashtag as the Social page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(flatten)]
    pub stats: Stats,
    pub running: bool,
    pub since: i64,
    /// Messages per minute since `since` (at most `MINUTES`), oldest first.
    pub history: Vec<u32>,
}

/// "jt20h", "#JT20H " → "#JT20H".
pub fn clean(tag: &str) -> String {
    let t: String = tag
        .trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    format!("#{t}")
}

/// The platform's id, as the API writes it ("youtube", "x"…).
pub fn platform_key(p: Platform) -> String {
    serde_json::to_value(p)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn minute(ms: i64) -> i64 {
    ms.div_euclid(60_000)
}

impl Hashtag {
    pub fn new(tag: &str, now: i64) -> Self {
        Self {
            tag: clean(tag),
            running: true,
            since: now,
            count: 0,
            platforms: BTreeMap::new(),
            minutes: VecDeque::new(),
            recent: VecDeque::new(),
        }
    }

    /// The tag without its "#": its key in the data source.
    pub fn key(&self) -> &str {
        self.tag.trim_start_matches('#')
    }

    /// `text` (lower case, no accents: `rules::plain`) carries the tag.
    pub fn in_text(&self, plain_text: &str) -> bool {
        let needle = format!("#{}", plain(self.key()));
        plain_text.match_indices(&needle).any(|(i, _)| {
            plain_text[i + needle.len()..]
                .chars()
                .next()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        })
    }

    /// Counts a message if it carries the tag; true when counted.
    pub fn count(&mut self, plain_text: &str, platform: Platform, at: i64, now: i64) -> bool {
        if !self.running || at < self.since || !self.in_text(plain_text) {
            return false;
        }
        self.count += 1;
        *self.platforms.entry(platform_key(platform)).or_default() += 1;
        let m = minute(at);
        match self.minutes.iter().position(|(x, _)| *x >= m) {
            Some(i) if self.minutes[i].0 == m => self.minutes[i].1 += 1,
            Some(i) => self.minutes.insert(i, (m, 1)),
            None => self.minutes.push_back((m, 1)),
        }
        while self
            .minutes
            .front()
            .is_some_and(|(x, _)| *x <= minute(now) - MINUTES as i64)
        {
            self.minutes.pop_front();
        }
        if now - at < 60_000 {
            self.recent.push_back(at);
        }
        true
    }

    pub fn reset(&mut self, now: i64) {
        *self = Self {
            running: self.running,
            ..Self::new(&self.tag, now)
        };
    }

    /// Messages in the last 60 seconds.
    pub fn per_min(&mut self, now: i64) -> u32 {
        while self.recent.front().is_some_and(|t| now - *t >= 60_000) {
            self.recent.pop_front();
        }
        self.recent.len() as u32
    }

    /// Messages per minute over the last `n` minutes (the current one
    /// last), oldest first.
    pub fn series(&self, now: i64, n: usize) -> Vec<u32> {
        let last = minute(now);
        let first = last - n as i64 + 1;
        let mut out = vec![0; n];
        for (m, c) in &self.minutes {
            if (first..=last).contains(m) {
                out[(m - first) as usize] = *c;
            }
        }
        out
    }

    /// Share of each platform, in percent.
    pub fn shares(&self) -> BTreeMap<String, u32> {
        let total: u64 = self.platforms.values().sum();
        self.platforms
            .iter()
            .map(|(k, n)| {
                (
                    k.clone(),
                    ((*n * 100) as f64 / total.max(1) as f64).round() as u32,
                )
            })
            .collect()
    }

    pub fn stats(&mut self, now: i64) -> Stats {
        Stats {
            tag: self.tag.clone(),
            count: self.count,
            per_min: self.per_min(now),
            series: self.series(now, SERIES),
            platforms: self.shares(),
        }
    }

    pub fn view(&mut self, now: i64) -> View {
        let span = (minute(now) - minute(self.since) + 1).clamp(1, MINUTES as i64) as usize;
        View {
            stats: self.stats(now),
            running: self.running,
            since: self.since,
            history: self.series(now, span),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_760_000_000_000;

    #[test]
    fn a_tag_is_found_any_case_or_accent_and_whole() {
        let h = Hashtag::new("jt20h", T0);
        assert_eq!(h.tag, "#jt20h");
        let h = Hashtag::new(" #Été2026 ", T0);
        assert_eq!(h.key(), "Été2026");
        assert!(h.in_text(&plain("Magnifique #ete2026 !")));
        assert!(h.in_text(&plain("#ÉTÉ2026")));
        assert!(!h.in_text(&plain("#ete20267 et #ete")));
        assert!(!h.in_text(&plain("ete2026 sans dièse")));
    }

    #[test]
    fn counts_per_minute_and_platform() {
        let mut h = Hashtag::new("#JT20H", T0);
        let text = plain("Bravo #JT20H");
        // 3 messages two minutes ago, 1 a minute ago, 2 now.
        for (at, p) in [
            (T0, Platform::X),
            (T0 + 1_000, Platform::X),
            (T0 + 2_000, Platform::Youtube),
            (T0 + 60_000, Platform::Facebook),
            (T0 + 120_000, Platform::X),
            (T0 + 125_000, Platform::Youtube),
        ] {
            assert!(h.count(&text, p, at, at));
        }
        assert!(!h.count(
            &plain("sans le tag"),
            Platform::X,
            T0 + 126_000,
            T0 + 126_000
        ));
        let now = T0 + 126_000;
        let s = h.stats(now);
        assert_eq!(s.count, 6);
        assert_eq!(s.series.len(), SERIES);
        let tail: Vec<u32> = s.series[SERIES - 3..].to_vec();
        let expect = {
            // The minutes the timestamps fall in.
            let mut v = vec![0u32; 3];
            for at in [
                T0,
                T0 + 1_000,
                T0 + 2_000,
                T0 + 60_000,
                T0 + 120_000,
                T0 + 125_000,
            ] {
                let i = (minute(at) - (minute(now) - 2)) as usize;
                v[i] += 1;
            }
            v
        };
        assert_eq!(tail, expect);
        assert_eq!(s.series.iter().sum::<u32>(), 6);
        assert_eq!(s.per_min, 2);
        assert_eq!(s.platforms["x"], 50);
        assert_eq!(s.platforms["youtube"], 33);
        assert_eq!(s.platforms["facebook"], 17);
        // Stopped: not counted; reset: from zero.
        h.running = false;
        assert!(!h.count(&text, Platform::X, now, now));
        h.reset(now);
        assert_eq!(
            (h.count, h.running, h.stats(now).series.iter().sum::<u32>()),
            (0, false, 0)
        );
        // Old minutes go.
        let mut h = Hashtag::new("#a", T0);
        h.count("#a", Platform::X, T0, T0);
        h.count("#a", Platform::X, T0 + 400 * 60_000, T0 + 400 * 60_000);
        assert_eq!(h.minutes.len(), 1);
    }
}
