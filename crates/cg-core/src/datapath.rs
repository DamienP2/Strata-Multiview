//! Values of data sources (JSON documents) and the paths that bind template
//! fields to them.
//!
//! Path syntax, from the root of the source's value:
//! - `title`, `match.home.score`: object keys (dots between keys);
//! - `items[0]`, `items[-1]`: array index (negative = from the end);
//! - `rows[{row}]`: `{key}` is replaced by the item's data (the operator
//!   picks the row, the player number…);
//! - `players[number=10]`, `players[number={num}]`: first element whose
//!   key equals the value;
//! - `items[*].title`: every element, one value per line (tickers, lists);
//! - `items.length`: number of elements (or characters of a text).

use std::collections::HashMap;

use serde_json::Value as Json;

use crate::anim::Data;
use crate::model::{
    CondOp, Condition, DataFilter, DataSet, DataSort, DataStep, FieldBinding, FilterOp, Template,
};

/// Latest value of each data source, by source id.
pub type Store = HashMap<String, Json>;

/// The text at `path` in `value`, with `{key}` taken from `data`.
pub fn lookup(value: &Json, path: &str, data: &Data) -> Option<String> {
    let path = substitute(path, data);
    let p = path.trim();
    // `….length`: how many elements (or characters).
    if let Some(base) = p
        .strip_suffix("length")
        .filter(|b| b.is_empty() || b.ends_with('.'))
    {
        let found = walk(value, base.trim_end_matches('.'));
        let n = match found.as_slice() {
            [] => return None,
            [one] if !base.contains("[*]") => match one {
                Json::Array(a) => a.len(),
                Json::Object(o) => o.len(),
                Json::String(s) => s.chars().count(),
                Json::Null => 0,
                _ => 1,
            },
            many => many.len(),
        };
        return Some(n.to_string());
    }
    let found = walk(value, p);
    if found.is_empty() {
        return None;
    }
    Some(found.into_iter().map(text).collect::<Vec<_>>().join("\n"))
}

fn walk<'a>(value: &'a Json, path: &str) -> Vec<&'a Json> {
    let mut cur: Vec<&Json> = vec![value];
    for seg in segments(path) {
        let mut next = Vec::new();
        for v in cur {
            match &seg {
                Seg::Key(k) => next.extend(v.get(k.as_str())),
                Seg::Index(i) => {
                    if let Some(a) = v.as_array() {
                        let i = if *i < 0 { a.len() as i64 + i } else { *i };
                        next.extend(usize::try_from(i).ok().and_then(|i| a.get(i)));
                    } else if let Some(o) = v.as_object() {
                        // Numbered keys ("1": …) behave like indexes.
                        next.extend(o.get(&i.to_string()));
                    }
                }
                Seg::All => match v {
                    Json::Array(a) => next.extend(a.iter()),
                    Json::Object(o) => next.extend(o.values()),
                    _ => {}
                },
                Seg::Filter(k, want) => {
                    if let Some(a) = v.as_array() {
                        next.extend(a.iter().find(|e| {
                            e.get(k.as_str()).map(text).as_deref() == Some(want.as_str())
                        }));
                    }
                }
            }
        }
        cur = next;
        if cur.is_empty() {
            break;
        }
    }
    cur
}

/// Fills the bound fields of `template` into `data`. Returns true when a
/// value changed. A source without a value at the path leaves the field as
/// it is (typed by the operator, or its default).
pub fn bind(template: &Template, data: &mut Data, store: &Store) -> bool {
    let mut changed = false;
    for f in &template.fields {
        let Some(b) = &f.binding else { continue };
        let Some(v) = store.get(&b.source) else {
            continue;
        };
        if is_manual(data, &f.key) {
            continue;
        }
        if b.list.is_some() || !b.steps.is_empty() {
            let Some(text) = read(v, b, data) else {
                continue;
            };
            if data.get(&f.key) != Some(&text) {
                data.insert(f.key.clone(), text);
                changed = true;
            }
            continue;
        }
        let sorted;
        let v = match &b.sort {
            Some(s) => {
                sorted = sort(v, s);
                &sorted
            }
            None => v,
        };
        if is_manual(data, &f.key) {
            continue;
        }
        let Some(text) = lookup_filtered(v, &b.path, &b.filters, data) else {
            continue;
        };
        if data.get(&f.key) != Some(&text) {
            data.insert(f.key.clone(), text);
            changed = true;
        }
    }
    changed
}

/// [`lookup`] then the filters; a missing value goes through them as an
/// empty text only when one of them gives a default.
pub fn lookup_filtered(
    value: &Json,
    path: &str,
    filters: &[DataFilter],
    data: &Data,
) -> Option<String> {
    let raw = match lookup(value, path, data) {
        Some(t) => t,
        None if filters
            .iter()
            .any(|f| matches!(f, DataFilter::Default { .. })) =>
        {
            String::new()
        }
        None => return None,
    };
    Some(filters.iter().fold(raw, |t, f| filter(&t, f)))
}

/// `value` with the list `s.list` sorted by `s.by`.
pub fn sort(value: &Json, s: &DataSort) -> Json {
    let mut out = value.clone();
    let mut cur = &mut out;
    for key in s.list.split('.').filter(|k| !k.is_empty()) {
        cur = match cur {
            Json::Object(o) => match o.get_mut(key) {
                Some(v) => v,
                None => return out,
            },
            Json::Array(a) => match key.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                Some(v) => v,
                None => return out,
            },
            _ => return out,
        };
    }
    if let Json::Array(items) = cur {
        let key = |e: &Json| e.get(s.by.as_str()).map(text).unwrap_or_default();
        let num = |e: &Json| {
            key(e)
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .unwrap_or(f64::NEG_INFINITY)
        };
        items.sort_by(|a, b| {
            let o = if s.numeric {
                num(a).total_cmp(&num(b))
            } else {
                key(a).to_lowercase().cmp(&key(b).to_lowercase())
            };
            if s.descending {
                o.reverse()
            } else {
                o
            }
        });
    }
    out
}

/// One filter step.
pub fn filter(text: &str, f: &DataFilter) -> String {
    let number = || text.trim().replace(',', ".").parse::<f64>().ok();
    match f {
        DataFilter::Upper => text.to_uppercase(),
        DataFilter::Lower => text.to_lowercase(),
        DataFilter::Capitalize => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for c in text.chars() {
                if start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    start = false;
                } else {
                    out.push(c);
                    if c.is_whitespace() || c == '-' {
                        start = true;
                    }
                }
            }
            out
        }
        DataFilter::Trim => text.trim().to_string(),
        DataFilter::Prefix { text: p } => format!("{p}{text}"),
        DataFilter::Suffix { text: s } => format!("{text}{s}"),
        DataFilter::Replace { find, with } if !find.is_empty() => text.replace(find.as_str(), with),
        DataFilter::Replace { .. } => text.to_string(),
        DataFilter::Truncate { max } => {
            if text.chars().count() > *max {
                let mut t: String = text.chars().take(*max).collect();
                t = t.trim_end().to_string();
                t.push('…');
                t
            } else {
                text.to_string()
            }
        }
        DataFilter::Lines { from, count } => {
            let lines: Vec<&str> = text.split('\n').collect();
            let n = lines.len() as i64;
            let start = if *from < 0 { n + from } else { from - 1 }.clamp(0, n) as usize;
            let end = if *count == 0 {
                lines.len()
            } else {
                (start + count).min(lines.len())
            };
            lines[start..end].join("\n")
        }
        DataFilter::Join { separator } => text
            .split('\n')
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join(separator),
        DataFilter::Number { decimals, grouping } => match number() {
            Some(v) => crate::widgets::format_number(v, *decimals, *grouping),
            None => text.to_string(),
        },
        DataFilter::Math { op, value } => match number() {
            Some(v) => {
                let r = match op {
                    FilterOp::Add => v + value,
                    FilterOp::Sub => v - value,
                    FilterOp::Mul => v * value,
                    FilterOp::Div if *value != 0.0 => v / value,
                    FilterOp::Div => v,
                };
                let r = (r * 1e6).round() / 1e6;
                if r.fract() == 0.0 && r.abs() < 1e15 {
                    format!("{}", r as i64)
                } else {
                    r.to_string()
                }
            }
            None => text.to_string(),
        },
        DataFilter::Default { text: d } if text.trim().is_empty() => d.clone(),
        DataFilter::Default { .. } => text.to_string(),
        DataFilter::Map { when, then } if text.trim() == when.trim() => then.clone(),
        DataFilter::Map { .. } => text.to_string(),
    }
}

// ------------------------------------------------------------- tables ----

/// Rows and named columns read from a source: what the Data canvas shows
/// in each node, and what the pipeline steps work on.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// The table as text: one line per row, its cells joined by a space.
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| c.trim())
                    .filter(|c| !c.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The value at dotted `list` ("" = the root) as a table: a list gives a
/// row per element; an object of objects a row per entry (its key in the
/// `#key` column, when keyed by numbers or 4 entries and more); anything
/// else one row. Nested objects become dotted
/// columns (`home.score`).
pub fn table(value: &Json, list: &str) -> Option<Table> {
    let mut cur = value;
    for k in list.split('.').filter(|k| !k.is_empty()) {
        cur = match cur {
            Json::Object(o) => o.get(k)?,
            Json::Array(a) => a.get(k.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    let mut cells: Vec<Vec<(String, String)>> = Vec::new();
    match cur {
        Json::Array(a) => {
            for e in a {
                let mut row = Vec::new();
                flatten(e, "", &mut row, 0);
                cells.push(row);
            }
        }
        // A collection keyed by ids ({"12": {…}, "15": {…}}), not a record
        // like {"home": {…}, "away": {…}}.
        Json::Object(o)
            if o.values().all(Json::is_object)
                && (o.len() >= 4
                    || (!o.is_empty() && o.keys().all(|k| k.parse::<f64>().is_ok()))) =>
        {
            for (k, e) in o {
                let mut row = vec![("#key".to_string(), k.clone())];
                flatten(e, "", &mut row, 0);
                cells.push(row);
            }
        }
        other => {
            let mut row = Vec::new();
            flatten(other, "", &mut row, 0);
            cells.push(row);
        }
    }
    let mut columns: Vec<String> = Vec::new();
    for row in &cells {
        for (k, _) in row {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let rows = cells
        .into_iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| {
                    row.iter()
                        .find(|(k, _)| k == c)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    Some(Table { columns, rows })
}

fn flatten(v: &Json, prefix: &str, out: &mut Vec<(String, String)>, depth: usize) {
    match v {
        Json::Object(o) if depth < 4 => {
            for (k, x) in o {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(x, &key, out, depth + 1);
            }
        }
        // A list of plain values in a cell: one per line.
        Json::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => out.push((
            col_name(prefix),
            a.iter().map(text).collect::<Vec<_>>().join("\n"),
        )),
        other => out.push((col_name(prefix), text(other))),
    }
}

fn col_name(prefix: &str) -> String {
    if prefix.is_empty() {
        "value".into()
    } else {
        prefix.into()
    }
}

/// One step of the pipeline.
pub fn step(t: Table, s: &DataStep, data: &Data) -> Table {
    match s {
        DataStep::Filter {
            conditions,
            any,
            from,
            count,
            columns,
        } => {
            let mut rows: Vec<Vec<String>> = t
                .rows
                .iter()
                .filter(|r| {
                    let ok = |c: &Condition| {
                        let cell = t.col(&c.column).map(|i| r[i].as_str()).unwrap_or("");
                        matches(cell, c.op, &substitute(&c.value, data))
                    };
                    if conditions.is_empty() {
                        true
                    } else if *any {
                        conditions.iter().any(ok)
                    } else {
                        conditions.iter().all(ok)
                    }
                })
                .cloned()
                .collect();
            let n = rows.len() as i64;
            let start = match *from {
                0 => 0,
                f if f < 0 => (n + f).max(0),
                f => (f - 1).min(n),
            } as usize;
            let end = if *count == 0 {
                rows.len()
            } else {
                start.saturating_add(*count).min(rows.len())
            };
            rows = rows.drain(start..end).collect();
            if columns.is_empty() {
                return Table {
                    columns: t.columns,
                    rows,
                };
            }
            let idx: Vec<Option<usize>> = columns.iter().map(|c| t.col(c)).collect();
            Table {
                columns: columns.clone(),
                rows: rows
                    .into_iter()
                    .map(|r| {
                        idx.iter()
                            .map(|i| i.map(|i| r[i].clone()).unwrap_or_default())
                            .collect()
                    })
                    .collect(),
            }
        }
        DataStep::Sort { keys } => {
            let mut t = t;
            let keys: Vec<_> = keys
                .iter()
                .filter_map(|k| t.col(&k.column).map(|i| (i, k)))
                .collect();
            t.rows.sort_by(|a, b| {
                for (i, k) in &keys {
                    let o = if k.numeric {
                        num(&a[*i]).total_cmp(&num(&b[*i]))
                    } else {
                        a[*i].to_lowercase().cmp(&b[*i].to_lowercase())
                    };
                    let o = if k.descending { o.reverse() } else { o };
                    if o.is_ne() {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
            t
        }
        DataStep::Transform { columns, ops } => {
            let mut t = t;
            for op in ops {
                match op {
                    DataFilter::Join { separator } => {
                        let joined = t
                            .text()
                            .split('\n')
                            .filter(|l| !l.trim().is_empty())
                            .collect::<Vec<_>>()
                            .join(separator);
                        t = Table {
                            columns: vec!["value".into()],
                            rows: vec![vec![joined]],
                        };
                    }
                    DataFilter::Default { text: d } if t.rows.is_empty() => {
                        let cols = if t.columns.is_empty() {
                            vec!["value".into()]
                        } else {
                            t.columns.clone()
                        };
                        let mut row = vec![String::new(); cols.len()];
                        row[0] = d.clone();
                        t = Table {
                            columns: cols,
                            rows: vec![row],
                        };
                    }
                    op => {
                        let on: Vec<bool> = t
                            .columns
                            .iter()
                            .map(|c| columns.is_empty() || columns.contains(c))
                            .collect();
                        for r in &mut t.rows {
                            for (i, cell) in r.iter_mut().enumerate() {
                                if on[i] {
                                    *cell = filter(cell, op);
                                }
                            }
                        }
                    }
                }
            }
            t
        }
    }
}

fn num(s: &str) -> f64 {
    s.trim()
        .replace(',', ".")
        .parse::<f64>()
        .unwrap_or(f64::NEG_INFINITY)
}

fn matches(cell: &str, op: CondOp, want: &str) -> bool {
    let (a, b) = (cell.trim(), want.trim());
    let nums = || {
        Some((
            a.replace(',', ".").parse::<f64>().ok()?,
            b.replace(',', ".").parse::<f64>().ok()?,
        ))
    };
    let cmp = || match nums() {
        Some((x, y)) => x.total_cmp(&y),
        None => a.to_lowercase().cmp(&b.to_lowercase()),
    };
    let eq = || match nums() {
        Some((x, y)) => x == y,
        None => a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase(),
    };
    match op {
        CondOp::Eq => eq(),
        CondOp::Ne => !eq(),
        CondOp::Contains => a.to_lowercase().contains(&b.to_lowercase()),
        CondOp::NotContains => !a.to_lowercase().contains(&b.to_lowercase()),
        CondOp::StartsWith => a.to_lowercase().starts_with(&b.to_lowercase()),
        CondOp::Gt => cmp().is_gt(),
        CondOp::Ge => cmp().is_ge(),
        CondOp::Lt => cmp().is_lt(),
        CondOp::Le => cmp().is_le(),
        CondOp::Empty => a.is_empty(),
        CondOp::NotEmpty => !a.is_empty(),
    }
}

/// The table after each step (the first is the source's table).
pub fn pipeline(value: &Json, list: &str, steps: &[DataStep], data: &Data) -> Option<Vec<Table>> {
    let mut out = vec![table(value, list)?];
    for s in steps {
        let next = step(out.last().unwrap().clone(), s, data);
        out.push(next);
    }
    Some(out)
}

/// What a table binding gives its field.
pub fn read(value: &Json, b: &FieldBinding, data: &Data) -> Option<String> {
    pipeline(value, b.list.as_deref().unwrap_or(""), &b.steps, data)
        .map(|t| t.last().unwrap().text())
}

/// Fills the columns of the current row of each data set of `template`
/// into `data` (the row number is in the data, 1 = first, kept within the
/// rows). Returns true when a value changed.
pub fn bind_datasets(template: &Template, data: &mut Data, store: &Store) -> bool {
    let mut changed = false;
    for ds in &template.datasets {
        changed |= bind_dataset(ds, data, store);
    }
    changed
}

fn bind_dataset(ds: &DataSet, data: &mut Data, store: &Store) -> bool {
    let Some(v) = store.get(&ds.source) else {
        return false;
    };
    let Some(t) = pipeline(v, &ds.list, &ds.steps, data).and_then(|mut s| s.pop()) else {
        return false;
    };
    let set = |k: String, v: String, data: &mut Data| {
        if data.get(&k) != Some(&v) {
            data.insert(k, v);
            true
        } else {
            false
        }
    };
    let t = with_added(t, data.get(&ds.added_key()).map(String::as_str));
    let n = t.rows.len();
    let row = data
        .get(&ds.row_key())
        .and_then(|r| r.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, n.max(1));
    let mut changed = set(ds.count_key(), n.to_string(), data);
    let all: Vec<&Vec<String>> = std::iter::once(&t.columns).chain(t.rows.iter()).collect();
    changed |= set(
        ds.table_key(),
        serde_json::to_string(&all).unwrap_or_default(),
        data,
    );
    let line = |r: &Vec<String>| row_line(&t.columns, r);
    // The chosen row only.
    let lines = t.rows.get(row - 1).map(line).unwrap_or_default();
    changed |= set(ds.lines_key(), lines, data);
    for (i, c) in t.columns.iter().enumerate() {
        let key = ds.column_key(c);
        // Set by hand: kept until the operator (or its time, or a trigger) lets go.
        if is_manual(data, &key) {
            continue;
        }
        let v = t
            .rows
            .get(row - 1)
            .and_then(|r| r.get(i))
            .cloned()
            .unwrap_or_default();
        changed |= set(key, v, data);
    }
    changed
}

/// A row as one line of text: its text / title / headline column, else its cells joined.
pub fn row_line(columns: &[String], row: &[String]) -> String {
    let named = ["text", "title", "headline"].iter().find_map(|n| {
        columns
            .iter()
            .position(|c| c.trim().eq_ignore_ascii_case(n))
    });
    match named {
        Some(i) => row.get(i).cloned().unwrap_or_default(),
        None => row
            .iter()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// A value the operator set by hand over the data (`<key>__manual` holds
/// how long: `removed`, `trigger:<name>`, `until:<unix ms>`).
pub fn is_manual(data: &Data, key: &str) -> bool {
    data.get(&format!("{key}__manual"))
        .is_some_and(|m| !m.trim().is_empty())
}

#[derive(serde::Deserialize)]
struct Added {
    #[serde(default)]
    at: Option<usize>,
    #[serde(default)]
    cells: std::collections::BTreeMap<String, Json>,
}

/// The table with the rows the operator added (JSON, see `DataSet::added_key`).
pub fn with_added(mut t: Table, added: Option<&str>) -> Table {
    let Some(list) = added.and_then(|a| serde_json::from_str::<Vec<Added>>(a).ok()) else {
        return t;
    };
    for a in list {
        for c in a.cells.keys() {
            if !t.columns.contains(c) {
                t.columns.push(c.clone());
                for r in &mut t.rows {
                    r.push(String::new());
                }
            }
        }
        let row = t
            .columns
            .iter()
            .map(|c| a.cells.get(c).map(text).unwrap_or_default())
            .collect();
        let at =
            a.at.map_or(t.rows.len(), |i| i.saturating_sub(1).min(t.rows.len()));
        t.rows.insert(at, row);
    }
    t
}

/// Sources a template reads.
pub fn sources(template: &Template) -> impl Iterator<Item = &str> {
    template
        .fields
        .iter()
        .filter_map(|f| f.binding.as_ref().map(|b| b.source.as_str()))
        .chain(template.datasets.iter().map(|d| d.source.as_str()))
}

/// A JSON value as display text.
pub fn text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn substitute(path: &str, data: &Data) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        match rest[i + 1..].find('}') {
            Some(j) => {
                let key = rest[i + 1..i + 1 + j].trim();
                out.push_str(data.get(key).map(|s| s.trim()).unwrap_or(""));
                rest = &rest[i + 2 + j..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, PartialEq)]
enum Seg {
    Key(String),
    Index(i64),
    All,
    Filter(String, String),
}

fn segments(path: &str) -> Vec<Seg> {
    let mut out = Vec::new();
    let path = path.trim().trim_start_matches('$').trim_start_matches('.');
    let mut key = String::new();
    let mut chars = path.chars().peekable();
    let flush = |key: &mut String, out: &mut Vec<Seg>| {
        if !key.trim().is_empty() {
            out.push(Seg::Key(key.trim().to_string()));
        }
        key.clear();
    };
    while let Some(c) = chars.next() {
        match c {
            '.' => flush(&mut key, &mut out),
            '[' => {
                flush(&mut key, &mut out);
                let mut inner = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    inner.push(c);
                }
                let inner = inner.trim().trim_matches(|c| c == '\'' || c == '"');
                out.push(if inner == "*" {
                    Seg::All
                } else if let Ok(i) = inner.parse::<i64>() {
                    Seg::Index(i)
                } else if let Some((k, v)) = inner.split_once('=') {
                    Seg::Filter(
                        k.trim().to_string(),
                        v.trim().trim_matches(|c| c == '\'' || c == '"').to_string(),
                    )
                } else {
                    Seg::Key(inner.to_string())
                });
            }
            c => key.push(c),
        }
    }
    flush(&mut key, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn d(pairs: &[(&str, &str)]) -> Data {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn paths() {
        let v = json!({
            "match": { "home": { "name": "PSG", "score": 2 }, "minute": 67.5 },
            "items": [{ "title": "One" }, { "title": "Two" }, { "title": "Three" }],
            "players": [{ "number": 10, "name": "Mbappé" }, { "number": 7, "name": "Dembélé" }],
            "live": true
        });
        let none = Data::new();
        assert_eq!(lookup(&v, "match.home.name", &none).as_deref(), Some("PSG"));
        assert_eq!(lookup(&v, "match.home.score", &none).as_deref(), Some("2"));
        assert_eq!(lookup(&v, "$.match.minute", &none).as_deref(), Some("67.5"));
        assert_eq!(lookup(&v, "items[1].title", &none).as_deref(), Some("Two"));
        assert_eq!(
            lookup(&v, "items[-1].title", &none).as_deref(),
            Some("Three")
        );
        assert_eq!(
            lookup(&v, "items[*].title", &none).as_deref(),
            Some("One\nTwo\nThree")
        );
        assert_eq!(lookup(&v, "items.length", &none).as_deref(), Some("3"));
        assert_eq!(
            lookup(&v, "players[number=7].name", &none).as_deref(),
            Some("Dembélé")
        );
        assert_eq!(
            lookup(&v, "players[number={n}].name", &d(&[("n", "10")])).as_deref(),
            Some("Mbappé")
        );
        assert_eq!(
            lookup(&v, "items[{row}].title", &d(&[("row", "0")])).as_deref(),
            Some("One")
        );
        assert_eq!(lookup(&v, "live", &none).as_deref(), Some("true"));
        assert_eq!(lookup(&v, "nope.x", &none), None);
        assert_eq!(lookup(&v, "items[9].title", &none), None);
    }

    #[test]
    fn binds_fields() {
        let mut t = crate::sample::score_bug();
        t.fields[2].binding = Some(crate::model::FieldBinding {
            source: "match".into(),
            path: "home.score".into(),
            filters: vec![],
            sort: None,
            list: None,
            steps: vec![],
            change: None,
            change_secs: None,
        });
        let mut store = Store::new();
        let mut data = d(&[("homeScore", "0")]);
        // No value yet: untouched.
        assert!(!bind(&t, &mut data, &store));
        store.insert("match".into(), json!({ "home": { "score": 3 } }));
        assert!(bind(&t, &mut data, &store));
        assert_eq!(data["homeScore"], "3");
        assert!(!bind(&t, &mut data, &store));
        assert_eq!(sources(&t).collect::<Vec<_>>(), vec!["match"]);
    }

    #[test]
    fn sorts_a_list_before_reading_it() {
        let v = json!({ "table": [{ "team": "Lyon", "pts": "52" }, { "team": "PSG", "pts": "74" }, { "team": "Lens", "pts": "9" }] });
        let s = DataSort {
            list: "table".into(),
            by: "pts".into(),
            descending: true,
            numeric: true,
        };
        assert_eq!(
            lookup(&sort(&v, &s), "table[*].team", &Data::new()).as_deref(),
            Some("PSG\nLyon\nLens")
        );
        let s = DataSort {
            list: "table".into(),
            by: "team".into(),
            descending: false,
            numeric: false,
        };
        assert_eq!(
            lookup(&sort(&v, &s), "table[0].team", &Data::new()).as_deref(),
            Some("Lens")
        );
    }

    #[test]
    fn filters_change_the_text() {
        use crate::model::{DataFilter as F, FilterOp};
        let v = json!({ "name": "  marie curie ", "items": ["a", "b", "c"], "n": "1234.5" });
        let none = Data::new();
        let run = |path: &str, fs: Vec<F>| lookup_filtered(&v, path, &fs, &none);
        assert_eq!(
            run("name", vec![F::Trim, F::Capitalize]).as_deref(),
            Some("Marie Curie")
        );
        assert_eq!(
            run(
                "name",
                vec![F::Trim, F::Upper, F::Prefix { text: "Dr ".into() }]
            )
            .as_deref(),
            Some("Dr MARIE CURIE")
        );
        assert_eq!(
            run("items[*]", vec![F::Lines { from: 2, count: 1 }]).as_deref(),
            Some("b")
        );
        assert_eq!(
            run(
                "items[*]",
                vec![
                    F::Lines { from: -2, count: 0 },
                    F::Join {
                        separator: " · ".into()
                    }
                ]
            )
            .as_deref(),
            Some("b · c")
        );
        assert_eq!(
            run(
                "n",
                vec![F::Math {
                    op: FilterOp::Mul,
                    value: 2.0
                }]
            )
            .as_deref(),
            Some("2469")
        );
        assert_eq!(
            run(
                "n",
                vec![F::Number {
                    decimals: 1,
                    grouping: true
                }]
            )
            .as_deref(),
            Some("1\u{202f}234,5")
        );
        assert_eq!(run("nope", vec![]), None);
        assert_eq!(
            run("nope", vec![F::Default { text: "—".into() }]).as_deref(),
            Some("—")
        );
        assert_eq!(
            run(
                "items[0]",
                vec![F::Map {
                    when: "a".into(),
                    then: "Alpha".into()
                }]
            )
            .as_deref(),
            Some("Alpha")
        );
        assert_eq!(
            run("name", vec![F::Trim, F::Truncate { max: 5 }]).as_deref(),
            Some("marie…")
        );
    }

    #[test]
    fn table_pipeline() {
        use crate::model::SortKey;
        let v = serde_json::json!({
            "standings": [
                {"team": "Lens", "pts": 30, "club": {"city": "Lens"}},
                {"team": "PSG", "pts": 41, "club": {"city": "Paris"}},
                {"team": "OM", "pts": 35, "club": {"city": "Marseille"}},
                {"team": "Lyon", "pts": 30, "club": {"city": "Lyon"}}
            ]
        });
        let t = table(&v, "standings").unwrap();
        assert_eq!(t.columns, ["team", "pts", "club.city"]);
        assert_eq!(t.rows.len(), 4);

        let sort = DataStep::Sort {
            keys: vec![
                SortKey {
                    column: "pts".into(),
                    descending: true,
                    numeric: true,
                },
                SortKey {
                    column: "team".into(),
                    descending: false,
                    numeric: false,
                },
            ],
        };
        let filter = DataStep::Filter {
            conditions: vec![Condition {
                column: "pts".into(),
                op: CondOp::Ge,
                value: "{min}".into(),
            }],
            any: false,
            from: 1,
            count: 3,
            columns: vec!["team".into(), "pts".into()],
        };
        let upper = DataStep::Transform {
            columns: vec!["team".into()],
            ops: vec![DataFilter::Upper],
        };
        let data: Data = [("min".to_string(), "30".to_string())].into();
        let stages = pipeline(&v, "standings", &[sort, filter, upper], &data).unwrap();
        assert_eq!(stages.len(), 4);
        assert_eq!(stages[1].rows[3][0], "Lyon");
        assert_eq!(stages[3].text(), "PSG 41\nOM 35\nLENS 30");

        // One value of an object: its row, one column.
        let one = DataStep::Filter {
            conditions: vec![],
            any: false,
            from: 0,
            count: 0,
            columns: vec!["home.score".into()],
        };
        let m = serde_json::json!({"home": {"score": 2}, "away": {"score": 1}});
        assert_eq!(
            pipeline(&m, "", &[one], &Data::new()).unwrap()[1].text(),
            "2"
        );

        // Join, and a default for an empty table.
        let none = DataStep::Filter {
            conditions: vec![Condition {
                column: "team".into(),
                op: CondOp::Eq,
                value: "Nice".into(),
            }],
            any: false,
            from: 0,
            count: 0,
            columns: vec![],
        };
        let def = DataStep::Transform {
            columns: vec![],
            ops: vec![DataFilter::Default { text: "—".into() }],
        };
        assert_eq!(
            pipeline(&v, "standings", &[none, def], &Data::new()).unwrap()[2].text(),
            "—"
        );
        let join = DataStep::Transform {
            columns: vec![],
            ops: vec![DataFilter::Join {
                separator: " · ".into(),
            }],
        };
        let only = DataStep::Filter {
            conditions: vec![],
            any: false,
            from: 0,
            count: 2,
            columns: vec!["team".into()],
        };
        assert_eq!(
            pipeline(&v, "standings", &[only, join], &Data::new()).unwrap()[2].text(),
            "Lens · PSG"
        );
    }

    #[test]
    fn dataset_rows() {
        use crate::model::{DataSet, RowChange};
        let mut t: Template =
            serde_json::from_value(serde_json::json!({"id": "t", "name": "t", "duration": 10}))
                .unwrap();
        t.datasets.push(DataSet {
            key: "standings".into(),
            name: "Standings".into(),
            source: "league".into(),
            list: "table".into(),
            steps: vec![],
            change: RowChange::OutIn,
        });
        let store: Store = [("league".to_string(), serde_json::json!({"table": [{"team": "PSG", "club.city": "Paris"}, {"team": "OM", "club.city": "Marseille"}]}))].into();
        let mut data = Data::new();
        assert!(bind_datasets(&t, &mut data, &store));
        assert_eq!(data["standings_team"], "PSG");
        assert_eq!(data["standings_club_city"], "Paris");
        assert_eq!(data["standings__count"], "2");
        // All the rows, for a table widget.
        assert_eq!(
            data["standings__table"],
            r#"[["team","club.city"],["PSG","Paris"],["OM","Marseille"]]"#
        );
        // A Text shows the chosen row only.
        assert_eq!(data["standings__lines"], "PSG · Paris");
        data.insert("standings__row".into(), "2".into());
        bind_datasets(&t, &mut data, &store);
        assert_eq!(data["standings_team"], "OM");
        assert_eq!(data["standings__lines"], "OM · Marseille");
        // Past the end: the last row.
        data.insert("standings__row".into(), "9".into());
        bind_datasets(&t, &mut data, &store);
        assert_eq!(data["standings_team"], "OM");
        assert!(!bind_datasets(&t, &mut data, &store));
        // A row added by hand, second; a value set by hand stays.
        data.insert(
            "standings__added".into(),
            r#"[{"at": 2, "cells": {"team": "Nice"}}]"#.into(),
        );
        data.insert("standings__row".into(), "2".into());
        bind_datasets(&t, &mut data, &store);
        assert_eq!(data["standings_team"], "Nice");
        assert_eq!(data["standings__count"], "3");
        data.insert("standings_team".into(), "PARIS".into());
        data.insert("standings_team__manual".into(), "removed".into());
        data.insert("standings__row".into(), "1".into());
        bind_datasets(&t, &mut data, &store);
        assert_eq!(data["standings_team"], "PARIS");
        assert_eq!(data["standings_club_city"], "Paris");
    }
}
