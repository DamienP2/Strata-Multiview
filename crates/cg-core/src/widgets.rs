//! Prefabricated widgets: clocks, tickers and rolls.
//!
//! Everything here is a pure function of (wall-clock time, time on air) so
//! the Rust renderer and the HTML5 renderer (`web/src/engine/widgets.ts`,
//! keep in sync) draw the same thing.

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

use crate::anim::{interpolate_text, Data};
use crate::model::ClockMode;

/// Time inputs of a widget.
#[derive(Debug, Clone, Copy)]
pub struct TimeCtx<'a> {
    /// Wall-clock time, Unix milliseconds.
    pub now_ms: i64,
    /// Seconds since the graphic was taken on air (or the designer playhead).
    pub age_secs: f64,
    /// Zone used when a widget does not set one ("" = system zone).
    pub default_tz: &'a str,
    /// Frames per second of the channel (a crawl moves a whole number of
    /// pixels per frame; 0: unknown).
    pub fps: f64,
}

fn zone(name: &str, fallback: &str) -> TimeZone {
    let name = if name.trim().is_empty() {
        fallback
    } else {
        name
    };
    if name.trim().is_empty() {
        return TimeZone::system();
    }
    TimeZone::get(name.trim()).unwrap_or_else(|_| TimeZone::system())
}

/// Calendar / clock fields used by [`format_tokens`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Fields {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u64,
    pub minute: u32,
    pub second: u32,
    pub tenth: u32,
    /// Durations: hours are not wrapped to 0..24 and there is no AM/PM.
    pub duration: bool,
}

/// Formats with tokens yyyy yy MM M dd d HH H hh h mm m ss s f A; any other
/// character is copied as is.
pub fn format_tokens(fmt: &str, f: &Fields) -> String {
    const TOKENS: [&str; 16] = [
        "yyyy", "yy", "MM", "M", "dd", "d", "HH", "H", "hh", "h", "mm", "m", "ss", "s", "f", "A",
    ];
    let h12 = match f.hour % 12 {
        0 => 12,
        h => h,
    };
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for t in TOKENS {
            if let Some(r) = rest.strip_prefix(t) {
                let v = match t {
                    "yyyy" => format!("{:04}", f.year),
                    "yy" => format!("{:02}", f.year.rem_euclid(100)),
                    "MM" => format!("{:02}", f.month),
                    "M" => f.month.to_string(),
                    "dd" => format!("{:02}", f.day),
                    "d" => f.day.to_string(),
                    "HH" => format!("{:02}", f.hour),
                    "H" => f.hour.to_string(),
                    "hh" if !f.duration => format!("{h12:02}"),
                    "h" if !f.duration => h12.to_string(),
                    "hh" => format!("{:02}", f.hour),
                    "h" => f.hour.to_string(),
                    "mm" => format!("{:02}", f.minute),
                    "m" => f.minute.to_string(),
                    "ss" => format!("{:02}", f.second),
                    "s" => f.second.to_string(),
                    "f" => f.tenth.to_string(),
                    "A" if f.duration => String::new(),
                    "A" => (if f.hour < 12 { "AM" } else { "PM" }).to_string(),
                    _ => unreachable!(),
                };
                out.push_str(&v);
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn zoned_fields(z: &Zoned, ms: i64) -> Fields {
    Fields {
        year: z.year() as i32,
        month: z.month() as u32,
        day: z.day() as u32,
        hour: z.hour() as u64,
        minute: z.minute() as u32,
        second: z.second() as u32,
        tenth: (ms.rem_euclid(1000) / 100) as u32,
        duration: false,
    }
}

pub fn duration_fields(secs: f64) -> Fields {
    let secs = secs.max(0.0);
    let whole = secs.floor() as u64;
    Fields {
        hour: whole / 3600,
        minute: ((whole / 60) % 60) as u32,
        second: (whole % 60) as u32,
        tenth: ((secs - secs.floor()) * 10.0).floor() as u32,
        duration: true,
        ..Fields::default()
    }
}

/// "[HH:]mm:ss" or plain seconds.
pub fn parse_duration(s: &str) -> Option<f64> {
    let parts: Vec<f64> = s
        .trim()
        .split(':')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    Some(match parts.as_slice() {
        [s] => *s,
        [m, s] => m * 60.0 + s,
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        _ => return None,
    })
}

/// Countdown target: "HH:mm[:ss]" today in `tz`, or "yyyy-MM-ddTHH:mm[:ss]".
fn target_ms(target: &str, tz: &TimeZone, now: &Zoned) -> Option<i64> {
    let t = target.trim();
    if t.contains('T') || t.matches('-').count() >= 2 {
        let dt: DateTime = t.replace(' ', "T").parse().ok()?;
        return Some(dt.to_zoned(tz.clone()).ok()?.timestamp().as_millisecond());
    }
    // Time of day: "HH:mm" or "HH:mm:ss" (unlike durations, two parts are h:m).
    let parts: Vec<f64> = t
        .split(':')
        .map(|p| p.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    let secs = match parts.as_slice() {
        [h, m] => h * 3600.0 + m * 60.0,
        [h, m, s] => h * 3600.0 + m * 60.0 + s,
        _ => return None,
    };
    let midnight = now.start_of_day().ok()?;
    Some(midnight.timestamp().as_millisecond() + (secs * 1000.0) as i64)
}

pub fn clock_text(
    mode: ClockMode,
    format: &str,
    timezone: &str,
    target: &str,
    data: &Data,
    ctx: &TimeCtx,
) -> String {
    let fmt = if format.trim().is_empty() {
        "HH:mm:ss"
    } else {
        format
    };
    let tz = zone(timezone, ctx.default_tz);
    let Ok(ts) = Timestamp::from_millisecond(ctx.now_ms) else {
        return String::new();
    };
    let now = ts.to_zoned(tz.clone());
    match mode {
        ClockMode::Time => format_tokens(fmt, &zoned_fields(&now, ctx.now_ms)),
        ClockMode::Elapsed => format_tokens(fmt, &duration_fields(ctx.age_secs)),
        ClockMode::Timer => {
            let total = parse_duration(&interpolate_text(target, data)).unwrap_or(0.0);
            format_tokens(fmt, &duration_fields(total - ctx.age_secs))
        }
        ClockMode::Countdown => {
            let remaining = target_ms(&interpolate_text(target, data), &tz, &now)
                .map(|t| (t - ctx.now_ms) as f64 / 1000.0)
                .unwrap_or(0.0);
            // Count down to the next whole second, like broadcast clocks.
            format_tokens(fmt, &duration_fields(remaining.max(0.0).ceil()))
        }
    }
}

/// Start positions (along the scroll axis) of the copies of a scrolling
/// strip of length `cycle` in a box of length `len`, `travelled` pixels
/// after it started entering from the far edge.
/// A crawl's speed on air: the nearest whole number of pixels per frame (at
/// least one), so every frame moves the text by the same whole step and its
/// glyphs are drawn the same each time — no shimmer, no uneven steps.
pub fn frame_speed(px_s: f64, fps: f64) -> f64 {
    if fps.is_nan() || fps < 1.0 || !px_s.is_finite() {
        return px_s;
    }
    (px_s / fps).round().max(1.0) * fps
}

pub fn scroll_positions(len: f64, cycle: f64, travelled: f64, looping: bool) -> Vec<f64> {
    if cycle <= 0.0 || len <= 0.0 {
        return Vec::new();
    }
    let first = len - travelled;
    if first >= 0.0 || !looping {
        return if first + cycle > 0.0 {
            vec![first]
        } else {
            Vec::new()
        };
    }
    let mut x = -((travelled - len).rem_euclid(cycle));
    let mut out = Vec::new();
    while x < len {
        out.push(x);
        x += cycle;
    }
    out
}

fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Progress 0..1 of a grow-in animation of `duration` seconds.
pub fn grow(duration: f64, age: f64) -> f64 {
    if duration <= 0.0 {
        1.0
    } else {
        ease_out(age / duration)
    }
}

fn number(raw: &str, data: &Data) -> f64 {
    interpolate_text(raw, data)
        .trim()
        .replace(',', ".")
        .replace(' ', "")
        .parse()
        .unwrap_or(0.0)
}

/// Formats `v` with `decimals` and an optional thin-space thousands separator.
pub fn format_number(v: f64, decimals: u8, grouping: bool) -> String {
    let s = format!("{:.*}", decimals as usize, v.abs());
    let (int, frac) = s
        .split_once('.')
        .map(|(a, b)| (a.to_string(), Some(b.to_string())))
        .unwrap_or((s.clone(), None));
    let int = if grouping {
        let digits: Vec<char> = int.chars().collect();
        let mut out = String::new();
        for (i, c) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push('\u{202f}');
            }
            out.push(*c);
        }
        out
    } else {
        int
    };
    let sign = if v < 0.0 && v.abs() >= 0.5 * 10f64.powi(-(decimals as i32)) {
        "-"
    } else {
        ""
    };
    match frac {
        Some(f) => format!("{sign}{int},{f}"),
        None => format!("{sign}{int}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn counter_text(
    from: &str,
    to: &str,
    duration: f64,
    decimals: u8,
    prefix: &str,
    suffix: &str,
    grouping: bool,
    data: &Data,
    age: f64,
) -> String {
    let (a, b) = (number(from, data), number(to, data));
    let v = a + (b - a) * grow(duration, age);
    format!(
        "{}{}{}",
        interpolate_text(prefix, data),
        format_number(v, decimals, grouping),
        interpolate_text(suffix, data)
    )
}

/// Filled fraction of a progress bar.
pub fn progress_ratio(value: &str, max: &str, duration: f64, data: &Data, age: f64) -> f64 {
    let max = number(max, data);
    let ratio = if max > 0.0 {
        (number(value, data) / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ratio * grow(duration, age)
}

/// Non-empty lines of a multi-line widget text after data substitution.
pub fn lines(text: &str, data: &Data) -> Vec<String> {
    interpolate_text(text, data)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// (index of the visible item, its opacity) for a carousel of `n` items.
pub fn rotator_state(n: usize, interval: f64, fade: f64, age: f64) -> (usize, f64) {
    if n <= 1 || interval <= 0.0 {
        return (0, 1.0);
    }
    let idx = (age / interval).floor().max(0.0) as usize % n;
    let t = age.rem_euclid(interval);
    let fade = fade.clamp(0.0, interval / 2.0);
    let opacity = if fade <= 0.0 {
        1.0
    } else {
        (t / fade).min((interval - t) / fade).clamp(0.0, 1.0)
    };
    // The very first item appears without fading in.
    let opacity = if age < interval && t < fade {
        1.0
    } else {
        opacity
    };
    (idx, opacity)
}

/// (current image, next image, mix 0..1 towards next) of a slideshow.
pub fn slideshow_state(n: usize, interval: f64, fade: f64, age: f64) -> (usize, usize, f64) {
    if n <= 1 || interval <= 0.0 {
        return (0, 0, 0.0);
    }
    let idx = (age / interval).floor().max(0.0) as usize % n;
    let t = age.rem_euclid(interval);
    let fade = fade.clamp(0.0, interval);
    let mix = if fade > 0.0 && t > interval - fade {
        (t - (interval - fade)) / fade
    } else {
        0.0
    };
    (idx, (idx + 1) % n, mix)
}

/// Text revealed so far, with a blinking cursor.
pub fn typewriter_text(text: &str, speed: f64, cursor: bool, data: &Data, age: f64) -> String {
    let full = interpolate_text(text, data);
    let total = full.chars().count();
    let shown = ((age * speed).floor().max(0.0) as usize).min(total);
    let mut out: String = full.chars().take(shown).collect();
    if cursor && (shown < total || (age * 2.0).floor() as i64 % 2 == 0) {
        out.push('|');
    }
    out
}

/// "Label: value" lines of a bar chart.
pub fn chart_rows(text: &str, data: &Data) -> Vec<(String, f64)> {
    lines(text, data)
        .into_iter()
        .map(|l| match l.rsplit_once(':') {
            Some((label, v)) => (
                label.trim().to_string(),
                v.trim().replace(',', ".").parse().unwrap_or(0.0),
            ),
            None => (l, 0.0),
        })
        .collect()
}

/// Hand angles in degrees (hour, minute, second), clockwise from 12.
pub fn clock_hands(timezone: &str, ctx: &TimeCtx) -> (f64, f64, f64) {
    let Ok(ts) = Timestamp::from_millisecond(ctx.now_ms) else {
        return (0.0, 0.0, 0.0);
    };
    let z = ts.to_zoned(zone(timezone, ctx.default_tz));
    let (h, m, s) = (z.hour() as f64, z.minute() as f64, z.second() as f64);
    (
        ((h % 12.0) + m / 60.0) * 30.0,
        (m + s / 60.0) * 6.0,
        s * 6.0,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_crawl_moves_whole_pixels_per_frame() {
        use super::frame_speed;
        // 110 px/s at 50 fps = 2.2 px per frame: 2 px per frame, 100 px/s.
        assert_eq!(frame_speed(110.0, 50.0), 100.0);
        assert_eq!(frame_speed(130.0, 50.0), 150.0);
        // At least one pixel a frame; 25 fps: steps of 25 px/s.
        assert_eq!(frame_speed(10.0, 50.0), 50.0);
        assert_eq!(frame_speed(110.0, 25.0), 100.0);
        // No frame rate known: the speed as set.
        assert_eq!(frame_speed(110.0, 0.0), 110.0);
    }

    use super::*;

    fn ctx(now_ms: i64, age: f64) -> TimeCtx<'static> {
        TimeCtx {
            now_ms,
            age_secs: age,
            default_tz: "UTC",
            fps: 50.0,
        }
    }

    #[test]
    fn time_of_day_in_zone() {
        // 2026-10-16T08:05:09.3Z
        let ms = 1_792_137_909_300;
        let d = Data::new();
        assert_eq!(
            clock_text(ClockMode::Time, "HH:mm:ss", "", "", &d, &ctx(ms, 0.0)),
            "08:05:09"
        );
        assert_eq!(
            clock_text(
                ClockMode::Time,
                "HH:mm",
                "Europe/Paris",
                "",
                &d,
                &ctx(ms, 0.0)
            ),
            "10:05"
        );
        assert_eq!(
            clock_text(
                ClockMode::Time,
                "dd/MM/yyyy h:mm A",
                "",
                "",
                &d,
                &ctx(ms, 0.0)
            ),
            "16/10/2026 8:05 AM"
        );
        assert_eq!(
            clock_text(ClockMode::Time, "ss.f", "", "", &d, &ctx(ms, 0.0)),
            "09.3"
        );
    }

    #[test]
    fn countdown_and_timers() {
        let ms = 1_792_137_909_300; // 08:05:09.3 UTC
        let d = Data::new();
        assert_eq!(
            clock_text(
                ClockMode::Countdown,
                "HH:mm:ss",
                "",
                "08:10",
                &d,
                &ctx(ms, 0.0)
            ),
            "00:04:51"
        );
        assert_eq!(
            clock_text(
                ClockMode::Countdown,
                "HH:mm:ss",
                "",
                "08:00",
                &d,
                &ctx(ms, 0.0)
            ),
            "00:00:00"
        );
        assert_eq!(
            clock_text(
                ClockMode::Countdown,
                "H:mm:ss",
                "",
                "2026-10-17T08:05:10",
                &d,
                &ctx(ms, 0.0)
            ),
            "24:00:01"
        );
        assert_eq!(
            clock_text(ClockMode::Elapsed, "mm:ss", "", "", &d, &ctx(ms, 75.4)),
            "01:15"
        );
        assert_eq!(
            clock_text(ClockMode::Timer, "m:ss", "", "2:00", &d, &ctx(ms, 30.2)),
            "1:29"
        );
        assert_eq!(
            clock_text(ClockMode::Timer, "m:ss", "", "2:00", &d, &ctx(ms, 500.0)),
            "0:00"
        );
    }

    #[test]
    fn animated_values() {
        let d = Data::new();
        assert_eq!(
            counter_text("0", "1234567", 2.0, 0, "", " €", true, &d, 5.0),
            "1\u{202f}234\u{202f}567 €"
        );
        assert_eq!(
            counter_text("0", "50", 2.0, 1, "", "%", false, &d, 0.0),
            "0,0%"
        );
        assert!((progress_ratio("50", "200", 1.0, &d, 3.0) - 0.25).abs() < 1e-9);
        assert_eq!(rotator_state(3, 4.0, 0.5, 9.0), (2, 1.0));
        assert_eq!(rotator_state(3, 4.0, 0.5, 7.75).1, 0.5);
        assert_eq!(slideshow_state(2, 5.0, 1.0, 4.5), (0, 1, 0.5));
        assert_eq!(typewriter_text("Hello", 2.0, false, &d, 1.6), "Hel");
        assert_eq!(
            chart_rows("Yes: 60\nNo: 40,5\nBlank", &d),
            vec![
                ("Yes".into(), 60.0),
                ("No".into(), 40.5),
                ("Blank".into(), 0.0)
            ]
        );
        let (h, m, sec) = clock_hands("", &ctx(1_792_137_909_300, 0.0));
        assert_eq!(
            (h, m, sec),
            ((8.0 + 5.0 / 60.0) * 30.0, (5.0 + 9.0 / 60.0) * 6.0, 54.0)
        );
    }

    #[test]
    fn scrolling() {
        // Entering from the right.
        assert_eq!(scroll_positions(100.0, 300.0, 40.0, true), vec![60.0]);
        // Looping: copies tile the box.
        assert_eq!(
            scroll_positions(100.0, 60.0, 130.0, true),
            vec![-30.0, 30.0, 90.0]
        );
        // Non-looping roll has gone past.
        assert!(scroll_positions(100.0, 50.0, 200.0, false).is_empty());
    }
}
