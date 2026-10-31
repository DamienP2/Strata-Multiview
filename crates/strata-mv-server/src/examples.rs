//! The example multiviews of a playout control room (final control,
//! sixteen channels, main and backup, off-air returns, audio, IP and sync,
//! contributions, the night shift, branding, UHD), embedded in the server.
//! Each one is `examples/ex-NN-slug.json`, `{"meta": ExampleInfo, "project":
//! Project}`; build.rs lists them. Their tiles read `{{input:N}}`, the N-th
//! input of this server (none when there are fewer).

use serde::{Deserialize, Serialize};
use strata_mv_core::{LayerKind, Project};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub description: String,
    /// One line for the example's card (the description is the long text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// Name and description in English (the main ones are in French).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<Translation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
struct ExampleFile {
    meta: ExampleInfo,
    project: Project,
}

const FILES: &[&str] = include!(concat!(env!("OUT_DIR"), "/examples.rs"));

fn all() -> impl Iterator<Item = ExampleFile> {
    FILES
        .iter()
        .filter_map(|j| match serde_json::from_str::<ExampleFile>(j) {
            Ok(d) => Some(d),
            Err(e) => {
                tracing::error!("bad example multiview: {e}");
                None
            }
        })
}

pub fn list() -> Vec<ExampleInfo> {
    all().map(|d| d.meta).collect()
}

/// An input of an example: `{{input:N}}`.
fn placeholder(n: usize) -> String {
    format!("{{{{input:{n}}}}}")
}

/// A copy of an example with its tiles on `inputs` (id, name) in order: the
/// N-th input, or none when there are fewer. A tile on an input takes its
/// name.
pub fn project(id: &str, inputs: &[(String, String)]) -> Option<Project> {
    let mut p = all().find(|d| d.meta.id == id)?.project;
    for t in &mut p.templates {
        for l in &mut t.layers {
            let input = match &mut l.kind {
                LayerKind::Source { x } => Some(&mut x.input),
                LayerKind::AudioMeter { x } => Some(&mut x.input),
                LayerKind::Loudness { x } => Some(&mut x.input),
                LayerKind::Timecode { x } => Some(&mut x.input),
                _ => None,
            };
            let Some(input) = input else {
                continue;
            };
            let Some(n) = input
                .strip_prefix("{{input:")
                .and_then(|r| r.strip_suffix("}}"))
                .and_then(|n| n.parse::<usize>().ok())
            else {
                continue;
            };
            debug_assert_eq!(*input, placeholder(n));
            let found = n.checked_sub(1).and_then(|i| inputs.get(i));
            *input = found.map(|(id, _)| id.clone()).unwrap_or_default();
            if let (Some((_, name)), LayerKind::Source { .. }) = (found, &l.kind) {
                l.name = name.clone();
            }
        }
    }
    Some(p)
}

/// The example as shown on its card: its tiles on no input (the bars).
pub fn cover(id: &str) -> Option<Project> {
    project(id, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_reads_and_takes_the_inputs_in_order() {
        let all = list();
        assert_eq!(all.len(), 10, "the ten control-room multiviews");
        let inputs = vec![
            ("cam1".to_string(), "CAM 1".to_string()),
            ("cam2".to_string(), "CAM 2".to_string()),
        ];
        for e in &all {
            let p = project(&e.id, &inputs).expect("example");
            assert_eq!(p.templates.len(), 1, "{}: one page", e.id);
            let json = serde_json::to_string(&p).unwrap();
            assert!(!json.contains("{{input:"), "{}: every input replaced", e.id);
            assert!(e.en.is_some(), "{}: in English too", e.id);
        }
        let p = project("ex-01-final", &inputs).unwrap();
        let tiles: Vec<_> = p.templates[0]
            .layers
            .iter()
            .filter_map(|l| match &l.kind {
                LayerKind::Source { x } => Some((l.name.as_str(), x.input.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(tiles[0], ("CAM 1", "cam1"));
        assert_eq!(tiles[1], ("CAM 2", "cam2"));
        assert_eq!(tiles[2].1, "", "no third input");
        assert_eq!(placeholder(3), "{{input:3}}");
    }
}
