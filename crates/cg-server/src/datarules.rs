//! Validation rules of a data source, checked after every fetch or push.
//! Broken rules are only warnings: the value is kept and still goes on air,
//! the Data page and the Operator show what is wrong.
//!
//! Where a rule reads (`field`), dotted keys like a binding path:
//! - `list[].column`: `column` of every row of the list at `list` (`[]` alone,
//!   or `[].column`, is the value itself when it is a list);
//! - `path`: that value when the source's value has it (`match.minute`),
//!   else `path` in every row of the value's main list — the longest list,
//!   up to three levels down, the table the Data page shows (`score` for
//!   the CSV `{columns, rows}` or `[{candidat, score}, …]`).
//!
//! A calc rule sums `field` over the rows (or the numbers of a list).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// Most issues kept per source.
const MAX_ISSUES: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Where it reads (see the module's doc).
    pub field: String,
    #[serde(rename = "type")]
    pub kind: RuleKind,
    /// An empty value breaks it (else empty values are not checked).
    #[serde(default)]
    pub required: bool,
    /// text: length in characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// text: a regular expression it must contain (`^…$` for the whole text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// number: bounds, and most decimals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u32>,
    /// list: the values allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    /// date: `DD/MM/YYYY`, `YYYY-MM-DD HH:mm`… (or strftime `%d/%m/%Y`);
    /// none = ISO 8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// calc: the sum expected, give or take `tolerance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleKind {
    #[default]
    Text,
    Number,
    List,
    /// A media file of the project or the shared ones, or a web address.
    Image,
    Date,
    Calc,
}

/// A broken rule.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// The list of the row (dotted path, "" = the value itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<String>,
    /// Its row in that list (0 = first); none for a single value or a sum.
    pub row: Option<usize>,
    /// The column (in a row) or the path.
    pub field: String,
    pub value: String,
    /// What is wrong: required, minLength, maxLength, pattern, number, min,
    /// max, decimals, list, image, date or sum (the web app words it).
    pub message: &'static str,
    /// The limit it breaks: `limit`, or `expected` and `tolerance` for a sum.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<&'static str, Json>,
}

/// The number the Data page and the Operator show: rows with issues, else
/// issues.
pub fn warning_count(issues: &[Issue]) -> usize {
    let mut rows: Vec<(&Option<String>, usize)> = issues
        .iter()
        .filter_map(|i| i.row.map(|r| (&i.list, r)))
        .collect();
    rows.sort();
    rows.dedup();
    if rows.is_empty() {
        issues.len()
    } else {
        rows.len()
    }
}

/// Checks `value` against `rules`; `exists` tells whether a media path exists.
pub fn evaluate(rules: &[Rule], value: &Json, exists: &dyn Fn(&str) -> bool) -> Vec<Issue> {
    let mut out = Vec::new();
    for rule in rules.iter().filter(|r| !r.field.trim().is_empty()) {
        let field = rule.field.trim();
        match target(value, field) {
            Target::One(v) if rule.kind == RuleKind::Calc => {
                let nums: Vec<f64> = match v {
                    Some(Json::Array(a)) => a.iter().filter_map(number).collect(),
                    Some(v) => number(v).into_iter().collect(),
                    None => Vec::new(),
                };
                sum(rule, None, field, &nums, &mut out);
            }
            Target::One(v) => {
                if let Some(i) = check(rule, v, exists) {
                    out.push(Issue {
                        list: None,
                        row: None,
                        field: field.to_string(),
                        ..i
                    });
                }
            }
            Target::Rows { list, rows, column } if rule.kind == RuleKind::Calc => {
                let nums: Vec<f64> = rows
                    .iter()
                    .filter_map(|r| cell(r, &column).and_then(number))
                    .collect();
                sum(rule, Some(list), &column, &nums, &mut out);
            }
            Target::Rows { list, rows, column } => {
                for (n, r) in rows.iter().enumerate() {
                    if let Some(i) = check(rule, cell(r, &column), exists) {
                        out.push(Issue {
                            list: Some(list.clone()),
                            row: Some(n),
                            field: column.clone(),
                            ..i
                        });
                    }
                }
            }
        }
    }
    // Row by row, as they are read.
    out.sort_by_key(|i| (i.row.is_none(), i.row));
    out.truncate(MAX_ISSUES);
    out
}

enum Target<'a> {
    One(Option<&'a Json>),
    Rows {
        list: String,
        rows: &'a [Json],
        column: String,
    },
}

fn target<'a>(value: &'a Json, field: &str) -> Target<'a> {
    if let Some((list, column)) = field.split_once("[]") {
        let list = list.trim_end_matches('.');
        let rows = get(value, list)
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        return Target::Rows {
            list: list.to_string(),
            rows,
            column: column.trim_start_matches('.').to_string(),
        };
    }
    if value.is_object() {
        if let Some(v) = get(value, field) {
            return Target::One(Some(v));
        }
    }
    match main_list(value, String::new(), 0) {
        Some((list, rows)) => Target::Rows {
            list,
            rows,
            column: field.to_string(),
        },
        None => Target::One(None),
    }
}

/// The value at a dotted path ("" = itself); numbers index lists.
fn get<'a>(value: &'a Json, path: &str) -> Option<&'a Json> {
    path.split('.')
        .filter(|s| !s.is_empty())
        .try_fold(value, |v, k| match v {
            Json::Object(o) => o.get(k),
            Json::Array(a) => k.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        })
}

/// The longest list, up to three levels down, and where it is (as the
/// Data page's table).
fn main_list(v: &Json, path: String, depth: usize) -> Option<(String, &[Json])> {
    match v {
        Json::Array(a) => Some((path, a)),
        Json::Object(o) if depth <= 2 => {
            let mut best: Option<(String, &[Json])> = None;
            for (k, x) in o {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if let Some(found) = main_list(x, p, depth + 1) {
                    if best.as_ref().is_none_or(|b| found.1.len() > b.1.len()) {
                        best = Some(found);
                    }
                }
            }
            best
        }
        _ => None,
    }
}

/// A row's column ("" = the row itself).
fn cell<'a>(row: &'a Json, column: &str) -> Option<&'a Json> {
    get(row, column)
}

/// A number, also from text (`31,2`, `1 204.5`).
fn number(v: &Json) -> Option<f64> {
    match v {
        Json::Number(n) => n.as_f64(),
        Json::String(s) => normalized(s).parse().ok(),
        _ => None,
    }
}

fn normalized(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| !matches!(c, ' ' | '\u{a0}' | '\u{202f}'))
        .map(|c| if c == ',' { '.' } else { c })
        .collect()
}

/// Rounded for display (`191.7`, not `191.70000000000002`).
fn round(x: f64) -> Json {
    let r = (x * 1e6).round() / 1e6;
    serde_json::Number::from_f64(r).map_or(Json::Null, Json::Number)
}

fn issue(value: &str, message: &'static str, limit: Option<Json>) -> Issue {
    Issue {
        list: None,
        row: None,
        field: String::new(),
        value: value.to_string(),
        message,
        params: limit.into_iter().map(|l| ("limit", l)).collect(),
    }
}

fn sum(rule: &Rule, list: Option<String>, field: &str, nums: &[f64], out: &mut Vec<Issue>) {
    let Some(expected) = rule.equals else {
        return;
    };
    let total: f64 = nums.iter().sum();
    let tolerance = rule.tolerance.unwrap_or(0.0).abs();
    if (total - expected).abs() <= tolerance + 1e-9 {
        return;
    }
    let shown = round(total);
    out.push(Issue {
        list,
        row: None,
        field: field.to_string(),
        value: shown.to_string(),
        message: "sum",
        params: BTreeMap::from([
            ("expected", round(expected)),
            ("tolerance", round(tolerance)),
            ("sum", shown),
        ]),
    });
}

/// The broken rule of one value, if any (`field` and `row` left to fill).
fn check(rule: &Rule, v: Option<&Json>, exists: &dyn Fn(&str) -> bool) -> Option<Issue> {
    let text = v.map(cg_core::datapath::text).unwrap_or_default();
    let t = text.trim();
    if t.is_empty() {
        return rule.required.then(|| issue("", "required", None));
    }
    let n = |x: f64| Some(round(x));
    match rule.kind {
        RuleKind::Text => {
            let len = t.chars().count();
            if let Some(min) = rule.min_length.filter(|m| len < *m) {
                return Some(issue(t, "minLength", Some(min.into())));
            }
            if let Some(max) = rule.max_length.filter(|m| len > *m) {
                return Some(issue(t, "maxLength", Some(max.into())));
            }
            let pattern = rule.pattern.as_deref().filter(|p| !p.is_empty());
            // A pattern that does not compile checks nothing.
            if let Some(re) = pattern.and_then(|p| regex_automata::meta::Regex::new(p).ok()) {
                if !re.is_match(t) {
                    return Some(issue(t, "pattern", None));
                }
            }
        }
        RuleKind::Number => {
            let Some(x) = v.and_then(number) else {
                return Some(issue(t, "number", None));
            };
            if let Some(min) = rule.min.filter(|m| x < *m) {
                return Some(issue(t, "min", n(min)));
            }
            if let Some(max) = rule.max.filter(|m| x > *m) {
                return Some(issue(t, "max", n(max)));
            }
            let written = match v {
                Some(Json::Number(_)) => text.clone(),
                _ => normalized(t),
            };
            let decimals = written.split_once('.').map_or(0, |(_, d)| {
                d.chars().take_while(char::is_ascii_digit).count()
            });
            if let Some(max) = rule.decimals.filter(|m| decimals > *m as usize) {
                return Some(issue(t, "decimals", Some(max.into())));
            }
        }
        RuleKind::List => {
            if !rule.values.is_empty() && !rule.values.iter().any(|a| a.trim() == t) {
                return Some(issue(t, "list", None));
            }
        }
        RuleKind::Image => {
            let web = ["http://", "https://", "data:image/"]
                .iter()
                .any(|p| t.starts_with(p));
            if !web && !exists(t) {
                return Some(issue(t, "image", None));
            }
        }
        RuleKind::Date => {
            if !date_ok(t, rule.format.as_deref().filter(|f| !f.trim().is_empty())) {
                let shown = rule.format.clone().unwrap_or_default();
                return Some(issue(t, "date", Some(shown.into())));
            }
        }
        RuleKind::Calc => {}
    }
    None
}

/// `DD/MM/YYYY HH:mm` as strftime (`%d/%m/%Y %H:%M`); one with `%` as is.
fn strftime(format: &str) -> String {
    if format.contains('%') {
        return format.to_string();
    }
    let mut out = format.to_string();
    for (from, to) in [
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MM", "%m"),
        ("DD", "%d"),
        ("HH", "%H"),
        ("mm", "%M"),
        ("ss", "%S"),
    ] {
        out = out.replace(from, to);
    }
    out
}

fn date_ok(s: &str, format: Option<&str>) -> bool {
    let Some(format) = format else {
        return s.parse::<jiff::Timestamp>().is_ok()
            || s.parse::<jiff::civil::DateTime>().is_ok()
            || s.parse::<jiff::civil::Date>().is_ok();
    };
    let f = strftime(format);
    let Ok(t) = jiff::fmt::strtime::parse(&f, s) else {
        return false;
    };
    // A day that exists (no 31/02).
    let full_date = f.contains("%d") && f.contains("%m") && (f.contains("%Y") || f.contains("%y"));
    !full_date || t.to_date().is_ok()
}

/// Whether a media path of a template (`/assets/x.png`,
/// `/assets/projects/<id>/x.png`, or a bare `x.png`: the project's, else
/// the shared one) is a file under `assets`.
pub fn asset_exists(assets: &Path, project: Option<&str>, path: &str) -> bool {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let rel = path
        .strip_prefix("/assets/")
        .or_else(|| path.strip_prefix("assets/"))
        .unwrap_or(path)
        .trim_start_matches('/');
    let rel = rel.replace("%20", " ");
    if rel.is_empty() || rel.split('/').any(|s| s == ".." || s.is_empty()) {
        return false;
    }
    let bare = !rel.contains('/');
    let in_project = project
        .filter(|_| bare)
        .is_some_and(|p| assets.join("projects").join(p).join(&rel).is_file());
    in_project || assets.join(&rel).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(field: &str, kind: RuleKind) -> Rule {
        Rule {
            field: field.into(),
            kind,
            ..Default::default()
        }
    }

    fn run(rules: &[Rule], v: &Json) -> Vec<Issue> {
        evaluate(rules, v, &|p| p == "/assets/ok.png")
    }

    fn messages(issues: &[Issue]) -> Vec<(Option<usize>, &str, &str)> {
        issues
            .iter()
            .map(|i| (i.row, i.field.as_str(), i.message))
            .collect()
    }

    fn election() -> Json {
        json!({"rows": [
            {"candidat": "M. Durand", "parti": "REN", "score": 31.2},
            {"candidat": "C. Petit", "parti": "RN ?", "score": "27,9"},
            {"candidat": "A. Morel", "parti": "LR", "score": 118},
            {"candidat": "J. Garnier", "parti": "PS", "score": 14.6},
        ]})
    }

    #[test]
    fn datarules_text() {
        let mut r = rule("name", RuleKind::Text);
        r.required = true;
        r.min_length = Some(2);
        r.max_length = Some(5);
        r.pattern = Some("^[A-Z]".into());
        assert!(run(&[r.clone()], &json!({"name": "Abc"})).is_empty());
        let m = |v: Json| run(&[r.clone()], &json!({ "name": v }))[0].message;
        assert_eq!(m(json!("")), "required");
        assert_eq!(m(json!(null)), "required");
        assert_eq!(m(json!("A")), "minLength");
        assert_eq!(m(json!("Abcdef")), "maxLength");
        assert_eq!(m(json!("abc")), "pattern");
        // Missing: required only when asked.
        assert_eq!(run(&[r.clone()], &json!({}))[0].message, "required");
        r.required = false;
        assert!(run(&[r], &json!({ "name": "  " })).is_empty());
        // A pattern that does not compile is ignored.
        let mut bad = rule("name", RuleKind::Text);
        bad.pattern = Some("(".into());
        assert!(run(&[bad], &json!({"name": "x"})).is_empty());
    }

    #[test]
    fn datarules_number() {
        let mut r = rule("score", RuleKind::Number);
        r.min = Some(0.0);
        r.max = Some(100.0);
        r.decimals = Some(1);
        let m = |v: Json| {
            run(&[r.clone()], &json!({ "score": v }))
                .first()
                .map(|i| i.message)
        };
        assert_eq!(m(json!(31.2)), None);
        assert_eq!(m(json!("27,9")), None);
        assert_eq!(m(json!("1 00")), None);
        assert_eq!(m(json!(100)), None);
        assert_eq!(m(json!(118)), Some("max"));
        assert_eq!(m(json!(-1)), Some("min"));
        assert_eq!(m(json!(12.25)), Some("decimals"));
        assert_eq!(m(json!("12,25")), Some("decimals"));
        assert_eq!(m(json!("douze")), Some("number"));
        let i = &run(&[r.clone()], &json!({"score": 118}))[0];
        assert_eq!(i.params["limit"], json!(100.0));
        assert_eq!(i.value, "118");
    }

    #[test]
    fn datarules_list_image_date() {
        let mut l = rule("parti", RuleKind::List);
        l.values = vec!["LR".into(), "PS".into(), "RN".into()];
        assert!(run(&[l.clone()], &json!({"parti": "RN"})).is_empty());
        assert_eq!(run(&[l], &json!({"parti": "RN ?"}))[0].message, "list");

        let img = rule("photo", RuleKind::Image);
        for ok in ["/assets/ok.png", "https://cdn/x.jpg", ""] {
            assert!(
                run(std::slice::from_ref(&img), &json!({ "photo": ok })).is_empty(),
                "{ok}"
            );
        }
        assert_eq!(
            run(&[img], &json!({"photo": "/assets/missing.png"}))[0].message,
            "image"
        );

        let mut d = rule("date", RuleKind::Date);
        assert!(run(&[d.clone()], &json!({"date": "2026-10-24"})).is_empty());
        assert!(run(&[d.clone()], &json!({"date": "2026-10-24T20:00:00Z"})).is_empty());
        assert_eq!(
            run(&[d.clone()], &json!({"date": "24/10/2026"}))[0].message,
            "date"
        );
        d.format = Some("DD/MM/YYYY".into());
        assert!(run(&[d.clone()], &json!({"date": "24/10/2026"})).is_empty());
        assert_eq!(
            run(&[d.clone()], &json!({"date": "31/02/2026"}))[0].message,
            "date"
        );
        assert_eq!(
            run(&[d.clone()], &json!({"date": "2026-10-24"}))[0].message,
            "date"
        );
        d.format = Some("HH:mm".into());
        assert!(run(&[d.clone()], &json!({"date": "20:45"})).is_empty());
        assert_eq!(run(&[d], &json!({"date": "25:61"}))[0].message, "date");
    }

    #[test]
    fn datarules_field_paths() {
        let mut r = rule("score", RuleKind::Number);
        r.max = Some(100.0);
        // A plain name on a table: every row of its main list.
        let issues = run(&[r.clone()], &election());
        assert_eq!(messages(&issues), vec![(Some(2), "score", "max")]);
        assert_eq!(issues[0].list.as_deref(), Some("rows"));
        // `list[].column`: the same, explicitly.
        r.field = "rows[].score".into();
        assert_eq!(run(&[r.clone()], &election()), issues);
        // A root list.
        let root = election()["rows"].clone();
        r.field = "score".into();
        let i = run(&[r.clone()], &root);
        assert_eq!((i[0].row, i[0].list.as_deref()), (Some(2), Some("")));
        r.field = "[].score".into();
        assert_eq!(run(&[r.clone()], &root)[0].row, Some(2));
        // An object that has the path: that one value, not the rows.
        let v = json!({"score": 150, "rows": [{"score": 1}]});
        r.field = "score".into();
        assert_eq!(
            messages(&run(&[r.clone()], &v)),
            vec![(None, "score", "max")]
        );
        r.field = "match.home.score".into();
        let nested = json!({"match": {"home": {"score": 101}}});
        assert_eq!(run(&[r], &nested)[0].row, None);
        // Main list down a level, nested column.
        let mut c = rule("team.name", RuleKind::Text);
        c.required = true;
        let v = json!({"data": {"items": [{"team": {"name": "PSG"}}, {"team": {}}]}});
        let i = run(&[c], &v);
        assert_eq!(messages(&i), vec![(Some(1), "team.name", "required")]);
        assert_eq!(i[0].list.as_deref(), Some("data.items"));
    }

    #[test]
    fn datarules_calc_sum() {
        let mut r = rule("score", RuleKind::Calc);
        r.equals = Some(100.0);
        r.tolerance = Some(0.5);
        let i = run(&[r.clone()], &election());
        assert_eq!(messages(&i), vec![(None, "score", "sum")]);
        assert_eq!(i[0].params["sum"], json!(191.7));
        assert_eq!(i[0].value, "191.7");
        let ok = json!([{"score": 50.2}, {"score": "49,6"}]);
        assert!(run(&[r.clone()], &ok).is_empty());
        let off = json!([{"score": 50.2}, {"score": 50.4}]);
        assert_eq!(run(&[r.clone()], &off).len(), 1);
        // Without a tolerance: exact.
        r.tolerance = None;
        assert_eq!(
            run(&[r.clone()], &json!([{"score": 60}, {"score": 40}])),
            vec![]
        );
        // Numbers of a list.
        r.field = "shares".into();
        assert!(run(&[r], &json!({"shares": [25, 75]})).is_empty());
    }

    #[test]
    fn datarules_warning_count_and_order() {
        let mut parti = rule("parti", RuleKind::List);
        parti.values = vec!["REN".into(), "LR".into(), "PS".into()];
        let mut score = rule("score", RuleKind::Number);
        score.max = Some(100.0);
        let mut sum = rule("score", RuleKind::Calc);
        sum.equals = Some(100.0);
        let i = run(&[sum, score, parti], &election());
        assert_eq!(
            messages(&i),
            vec![
                (Some(1), "parti", "list"),
                (Some(2), "score", "max"),
                (None, "score", "sum")
            ]
        );
        assert_eq!(warning_count(&i), 2);
        assert_eq!(warning_count(&i[2..]), 1);
    }

    #[test]
    fn datarules_assets() {
        let dir = std::env::temp_dir().join(format!("cg-datarules-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("projects/p1")).unwrap();
        std::fs::write(dir.join("logo.png"), b"x").unwrap();
        std::fs::write(dir.join("projects/p1/face.png"), b"x").unwrap();
        assert!(asset_exists(&dir, None, "/assets/logo.png"));
        assert!(asset_exists(&dir, None, "logo.png?v=2"));
        assert!(asset_exists(&dir, Some("p1"), "face.png"));
        assert!(asset_exists(&dir, None, "/assets/projects/p1/face.png"));
        assert!(!asset_exists(&dir, None, "face.png"));
        assert!(!asset_exists(&dir, Some("p1"), "../p1/face.png"));
        assert!(!asset_exists(&dir, None, "/assets/"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
