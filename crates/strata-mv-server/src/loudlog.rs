//! Loudness logs and the EBU R128 report.
//!
//! Every second, the loudness of each input heard is written to
//! `<data>/loudness/<input>/<YYYY-MM-DD>.csv` (UTC days): time (ms since
//! 1970), momentary, short-term, integrated (LUFS), range (LU), true peak
//! (dBTP). Kept 90 days.
//!
//! The report of a period recomputes from those lines: integrated loudness
//! (EBU R128 gating on the momentary values: −70 LUFS absolute, −10 LU
//! relative), loudness range (EBU Tech 3342 on the short-term values: −70
//! absolute, −20 relative, 10th to 95th percentile), highest true peak,
//! time above the target, and the curve minute by minute. The values are
//! taken once a second (the standard samples ten times a second): the
//! integrated loudness agrees within a few tenths of LU on a programme.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::api::{bad, ApiResult};
use crate::auth::CurrentUser;
use crate::state::Shared;

const DAYS_KEPT: i64 = 90;
const DAY_MS: i64 = 86_400_000;

fn dir_of(data: &Path) -> PathBuf {
    data.join("loudness")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// "2026-11-01" of a time (UTC).
fn day(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| t.strftime("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// An input id fit for a folder name.
fn folder(id: &str) -> Option<String> {
    let ok = !id.is_empty()
        && id.len() <= 80
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !id.starts_with('.');
    ok.then(|| id.to_string())
}

/// Writes the inputs' loudness every second, for as long as the server runs.
pub fn spawn(data: PathBuf, inputs: Arc<crate::inputs::Inputs>) {
    let _ = std::thread::Builder::new()
        .name("mv-loudness-log".into())
        .spawn(move || {
            let root = dir_of(&data);
            tracing::info!("loudness log in {}", root.display());
            let mut open: HashMap<String, (String, BufWriter<File>)> = HashMap::new();
            let mut n: u64 = 0;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                n += 1;
                let at = now_ms();
                let today = day(at);
                let signals = inputs.signals();
                for st in inputs.status() {
                    let Some(name) = folder(&st.id) else { continue };
                    let (Some(l), _) = signals.loudness_of(&st.id) else {
                        continue;
                    };
                    if !st.live {
                        continue;
                    }
                    let stale = open.get(&st.id).is_none_or(|(d, _)| *d != today);
                    if stale {
                        let d = root.join(&name);
                        let _ = std::fs::create_dir_all(&d);
                        match OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(d.join(format!("{today}.csv")))
                        {
                            Ok(f) => {
                                open.insert(st.id.clone(), (today.clone(), BufWriter::new(f)));
                            }
                            Err(e) => {
                                tracing::warn!("loudness log of {}: {e}", st.id);
                                continue;
                            }
                        }
                    }
                    if let Some((_, w)) = open.get_mut(&st.id) {
                        let r = |v: f32| {
                            if v.is_finite() {
                                format!("{:.1}", v)
                            } else {
                                String::new()
                            }
                        };
                        let _ = writeln!(
                            w,
                            "{at},{},{},{},{},{}",
                            r(l.momentary),
                            r(l.short_term),
                            r(l.integrated),
                            r(l.range),
                            r(l.true_peak)
                        );
                    }
                }
                if n.is_multiple_of(10) {
                    for (_, w) in open.values_mut() {
                        let _ = w.flush();
                    }
                    // Inputs gone: their files closed.
                    let ids: Vec<String> = inputs.status().into_iter().map(|s| s.id).collect();
                    open.retain(|id, _| ids.contains(id));
                }
                if n % 3600 == 5 {
                    prune(&root, at);
                }
            }
        });
}

/// Removes the days older than DAYS_KEPT.
fn prune(root: &Path, now: i64) {
    let oldest = day(now - DAYS_KEPT * DAY_MS);
    let Ok(dirs) = std::fs::read_dir(root) else {
        return;
    };
    for d in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(d.path()) else {
            continue;
        };
        for f in files.flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if name.ends_with(".csv") && name.len() == 14 && name[..10] < *oldest {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }
}

/// One line of the log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub at: i64,
    pub momentary: Option<f32>,
    pub short_term: Option<f32>,
    pub true_peak: Option<f32>,
}

fn parse(line: &str) -> Option<Sample> {
    let mut f = line.split(',');
    let at = f.next()?.parse().ok()?;
    let v = |s: Option<&str>| {
        s.and_then(|x| x.parse::<f32>().ok())
            .filter(|x| x.is_finite())
    };
    let momentary = v(f.next());
    let short_term = v(f.next());
    let _integrated = f.next();
    let _range = f.next();
    let true_peak = v(f.next());
    Some(Sample {
        at,
        momentary,
        short_term,
        true_peak,
    })
}

/// The lines of an input from `from` to `to` (ms).
pub fn read(data: &Path, id: &str, from: i64, to: i64) -> Vec<Sample> {
    let Some(name) = folder(id) else {
        return Vec::new();
    };
    let d = dir_of(data).join(name);
    let mut out = Vec::new();
    let mut t = from - from.rem_euclid(DAY_MS);
    while t <= to {
        if let Ok(f) = File::open(d.join(format!("{}.csv", day(t)))) {
            for l in BufReader::new(f).lines().map_while(Result::ok) {
                if let Some(s) = parse(&l).filter(|s| s.at >= from && s.at <= to) {
                    out.push(s);
                }
            }
        }
        t += DAY_MS;
    }
    out
}

fn power(lufs: f32) -> f64 {
    10f64.powf((lufs as f64 + 0.691) / 10.0)
}
fn lufs(p: f64) -> f32 {
    (-0.691 + 10.0 * p.log10()) as f32
}

/// Integrated loudness of momentary values (EBU R128 gating).
pub fn integrated(momentary: &[f32]) -> Option<f32> {
    let gated: Vec<f32> = momentary.iter().copied().filter(|v| *v > -70.0).collect();
    if gated.is_empty() {
        return None;
    }
    let mean = |v: &[f32]| v.iter().map(|x| power(*x)).sum::<f64>() / v.len() as f64;
    let relative = lufs(mean(&gated)) - 10.0;
    let kept: Vec<f32> = gated.into_iter().filter(|v| *v > relative).collect();
    (!kept.is_empty()).then(|| lufs(mean(&kept)))
}

/// Loudness range of short-term values (EBU Tech 3342).
pub fn range(short: &[f32]) -> Option<f32> {
    let gated: Vec<f32> = short.iter().copied().filter(|v| *v > -70.0).collect();
    if gated.len() < 2 {
        return None;
    }
    let mean = gated.iter().map(|x| power(*x)).sum::<f64>() / gated.len() as f64;
    let rel = lufs(mean) - 20.0;
    let mut kept: Vec<f32> = gated.into_iter().filter(|v| *v > rel).collect();
    if kept.len() < 2 {
        return None;
    }
    kept.sort_by(|a, b| a.total_cmp(b));
    let at = |q: f32| kept[((kept.len() - 1) as f32 * q).round() as usize];
    Some(at(0.95) - at(0.10))
}

/// A minute of the curve: start (ms), short-term mean, short-term max, true peak max.
pub type Minute = (i64, Option<f32>, Option<f32>, Option<f32>);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub input: String,
    pub name: String,
    pub from: i64,
    pub to: i64,
    /// Seconds measured.
    pub seconds: usize,
    pub integrated: Option<f32>,
    pub range: Option<f32>,
    pub max_true_peak: Option<f32>,
    pub max_momentary: Option<f32>,
    pub max_short_term: Option<f32>,
    /// The standard measured against (the input's alarm profile's).
    pub standard: String,
    pub target: f32,
    pub tolerance: f32,
    pub max_true_peak_allowed: f32,
    /// Seconds with the short-term loudness above target + 3 LU.
    pub loud_seconds: usize,
    /// Seconds with the true peak above the standard's.
    pub peak_seconds: usize,
    /// Integrated within the tolerance of the target, the true peak at
    /// most the standard's.
    pub compliant: Option<bool>,
    /// Minute by minute: [start ms, short-term mean, short-term max, true peak max].
    pub minutes: Vec<Minute>,
}

pub fn report(
    input: &str,
    name: &str,
    from: i64,
    to: i64,
    s: &[Sample],
    norm: &strata_mv_core::signal::LoudnessNorm,
) -> Report {
    let target = norm.target;
    let m: Vec<f32> = s.iter().filter_map(|x| x.momentary).collect();
    let st: Vec<f32> = s.iter().filter_map(|x| x.short_term).collect();
    let tp: Vec<f32> = s.iter().filter_map(|x| x.true_peak).collect();
    let max = |v: &[f32]| v.iter().copied().reduce(f32::max);
    let integrated = integrated(&m);
    let max_tp = max(&tp);
    let mut minutes: Vec<Minute> = Vec::new();
    let mut bucket: Vec<&Sample> = Vec::new();
    let flush = |b: &mut Vec<&Sample>, out: &mut Vec<_>| {
        if let Some(first) = b.first() {
            let start = first.at - first.at.rem_euclid(60_000);
            let st: Vec<f32> = b.iter().filter_map(|x| x.short_term).collect();
            let mean = (!st.is_empty())
                .then(|| lufs(st.iter().map(|x| power(*x)).sum::<f64>() / st.len() as f64));
            let r1 = |v: Option<f32>| v.map(|x| (x * 10.0).round() / 10.0);
            out.push((
                start,
                r1(mean),
                r1(max(&st)),
                r1(max(&b
                    .iter()
                    .filter_map(|x| x.true_peak)
                    .collect::<Vec<_>>())),
            ));
        }
        b.clear();
    };
    for x in s {
        if bucket
            .first()
            .is_some_and(|f| f.at / 60_000 != x.at / 60_000)
        {
            flush(&mut bucket, &mut minutes);
        }
        bucket.push(x);
    }
    flush(&mut bucket, &mut minutes);
    let r1 = |v: Option<f32>| v.map(|x| (x * 10.0).round() / 10.0);
    Report {
        input: input.into(),
        name: name.into(),
        from,
        to,
        seconds: s.len(),
        integrated: r1(integrated),
        range: r1(range(&st)),
        max_true_peak: r1(max_tp),
        max_momentary: r1(max(&m)),
        max_short_term: r1(max(&st)),
        standard: norm.standard.clone(),
        target,
        tolerance: norm.tolerance,
        max_true_peak_allowed: norm.max_true_peak,
        loud_seconds: st.iter().filter(|v| **v > target + 3.0).count(),
        peak_seconds: tp.iter().filter(|v| **v > norm.max_true_peak).count(),
        compliant: integrated.map(|i| {
            (i - target).abs() <= norm.tolerance && max_tp.is_none_or(|p| p <= norm.max_true_peak)
        }),
        minutes,
    }
}

#[derive(Deserialize)]
pub struct ReportQuery {
    input: String,
    /// ms since 1970 (default: an hour ago).
    from: Option<i64>,
    to: Option<i64>,
    #[serde(default)]
    csv: bool,
}

/// `GET /api/loudness/report?input=&from=&to=[&csv=true]`.
pub async fn get_report(
    State(s): State<Shared>,
    CurrentUser(_me): CurrentUser,
    Query(q): Query<ReportQuery>,
) -> ApiResult<Response> {
    let to = q.to.unwrap_or_else(now_ms);
    let from = q.from.unwrap_or(to - 3_600_000);
    if to <= from || to - from > 31 * DAY_MS {
        return Err(bad("the period is from 1 s to 31 days"));
    }
    let name = s
        .config
        .inputs
        .iter()
        .find(|i| i.id == q.input)
        .map(|i| i.name.clone())
        .unwrap_or_else(|| q.input.clone());
    let data = s.data_dir.clone();
    let input = q.input.clone();
    let samples = tokio::task::spawn_blocking(move || read(&data, &input, from, to))
        .await
        .map_err(crate::api::internal)?;
    if q.csv {
        let mut out =
            String::from("\u{feff}Heure;Momentané (LUFS);Court terme (LUFS);True peak (dBTP)\n");
        let tz: jiff::tz::TimeZone =
            jiff::tz::TimeZone::get(&s.config.timezone).unwrap_or(jiff::tz::TimeZone::UTC);
        let v = |x: Option<f32>| {
            x.map(|x| format!("{x:.1}").replace('.', ","))
                .unwrap_or_default()
        };
        for x in &samples {
            let t = jiff::Timestamp::from_millisecond(x.at)
                .map(|t| {
                    t.to_zoned(tz.clone())
                        .strftime("%Y-%m-%d %H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default();
            out.push_str(&format!(
                "{t};{};{};{}\n",
                v(x.momentary),
                v(x.short_term),
                v(x.true_peak)
            ));
        }
        let file = format!("loudness-{}-{}.csv", q.input, day(from));
        return Ok((
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{file}\""),
                ),
            ],
            out,
        )
            .into_response());
    }
    let norm = s.inputs.signals().norm_of(&q.input);
    Ok(axum::Json(report(&q.input, &name, from, to, &samples, &norm)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gating_and_range() {
        // A steady −23 LUFS with silence between: the silence is gated out.
        let mut m = vec![-23.0f32; 60];
        m.extend(vec![-90.0; 60]);
        assert!((integrated(&m).unwrap() + 23.0).abs() < 0.01);
        // Two levels −20 and −26: mean power, then the relative gate keeps both.
        let two: Vec<f32> = (0..100)
            .map(|i| if i % 2 == 0 { -20.0 } else { -26.0 })
            .collect();
        let i = integrated(&two).unwrap();
        assert!((i - -22.03).abs() < 0.05, "{i}");
        // Range: 10th to 95th percentile.
        let st: Vec<f32> = (0..=100).map(|i| -30.0 + i as f32 * 0.1).collect();
        assert!((range(&st).unwrap() - 8.5).abs() < 0.11);
        assert_eq!(range(&[-23.0]), None);
    }

    #[test]
    fn log_lines_and_report() {
        assert_eq!(
            parse("1000,-23.0,-22.5,-23.1,4.0,-3.2"),
            Some(Sample {
                at: 1000,
                momentary: Some(-23.0),
                short_term: Some(-22.5),
                true_peak: Some(-3.2)
            })
        );
        assert_eq!(parse("1000,,,,,").unwrap().momentary, None);
        let s: Vec<Sample> = (0..180)
            .map(|i| Sample {
                at: i * 1000,
                momentary: Some(-23.0),
                short_term: Some(if i < 10 { -18.0 } else { -23.0 }),
                true_peak: Some(if i == 5 { -0.5 } else { -6.0 }),
            })
            .collect();
        let r = report("cam1", "Caméra 1", 0, 180_000, &s, &Default::default());
        assert_eq!(r.seconds, 180);
        assert_eq!(r.integrated, Some(-23.0));
        assert_eq!(r.loud_seconds, 10);
        assert_eq!(r.peak_seconds, 1);
        assert_eq!(r.compliant, Some(false)); // the true peak
        assert_eq!(r.minutes.len(), 3);
        assert_eq!(r.max_true_peak, Some(-0.5));
    }

    #[test]
    fn days_and_folders() {
        assert_eq!(day(0), "1970-01-01");
        assert_eq!(folder("cam-1"), Some("cam-1".into()));
        assert_eq!(folder("../x"), None);
    }
}
