//! Import of Motion Graphics templates (`.mogrt`): see [`super`].
//!
//! A `.mogrt` made in After Effects is a zip holding:
//!
//! - `definition.json`: the template's name, the composition it is built
//!   on (`sourceInfoLocalized`) and its Essential Graphics controls
//!   (`clientControls`: id, type, name `uiName`, value, range, menu
//!   items…);
//! - `project.aegraphic`: a zip of the After Effects project (`.aep`) and
//!   its media, read by [`super::aep`];
//! - `thumb.png` (the poster frame) and sometimes a preview movie.
//!
//! The controls are matched by id with the controllers the project itself
//! stores (`LIST:CIF3`, which says which layer property each one drives);
//! the definition gives their names and values as the template was saved.
//! The layout of `definition.json` is read loosely (localised strings as
//! `{"strDB": [{"localeString", "str"}]}`, a locale map or plain text;
//! types as After Effects' controller numbers or names): it is not
//! documented by Adobe.

use serde_json::Value as Json;

use super::aep::{self, Archive, Controller};
use super::{Converted, Ctx, Format, Note, Status};

pub fn analyze(bytes: &[u8], ctx: &Ctx) -> Result<Converted, String> {
    let zip = Archive::open(bytes)?;
    let names = zip.names().to_vec();
    let entry = |suffix: &str| {
        names
            .iter()
            .filter(|n| n.to_lowercase().ends_with(suffix) && !n.starts_with("__MACOSX/"))
            .min_by_key(|n| (n.matches('/').count(), n.len()))
            .cloned()
    };
    let def: Json = match entry("definition.json").and_then(|n| zip.read(&n)) {
        Some(b) => serde_json::from_slice(&b).map_err(|e| {
            format!("unreadable definition.json in the Motion Graphics template: {e}")
        })?,
        None => return Err("not a Motion Graphics template: no definition.json".into()),
    };
    let Some(graphic) = entry(".aegraphic") else {
        if entry(".prgraphic").is_some() {
            return Err("this Motion Graphics template was made in Premiere Pro: only templates made in After Effects (with a project.aegraphic) can be imported".into());
        }
        return Err(
            "no After Effects project (project.aegraphic) in this Motion Graphics template".into(),
        );
    };
    let inner = zip
        .read(&graphic)
        .ok_or("cannot read the project.aegraphic of the Motion Graphics template")?;
    // project.aegraphic is a zip of the .aep and its media.
    let (project, media) = if inner.starts_with(b"RIFX") {
        (inner.clone(), None)
    } else {
        let a = Archive::open(&inner)?;
        let aep_name = a
            .names()
            .iter()
            .filter(|n| n.to_lowercase().ends_with(".aep") && !n.starts_with("__MACOSX/"))
            .min_by_key(|n| (n.matches('/').count(), n.len()))
            .cloned()
            .ok_or("no After Effects project (.aep) in the project.aegraphic")?;
        let aep_bytes = a
            .read(&aep_name)
            .ok_or("cannot read the .aep of the Motion Graphics template")?;
        (aep_bytes, Some(a))
    };
    let lookup = |path: &str| {
        media
            .as_ref()
            .and_then(|m| m.find(path))
            .or_else(|| zip.find(path))
    };
    let source = source_name(&def);
    let title = loc(def.get("capsuleNameLocalized"))
        .or_else(|| loc(def.get("capsuleName")))
        .filter(|s| !s.trim().is_empty());
    let main_name = source.clone().or_else(|| title.clone());
    let mut p = aep::convert(&project, ctx, Some(&lookup), main_name.as_deref())?;

    // The composition the template is built on.
    let ids: Vec<String> = def
        .get("clientControls")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|c| str_of(c.get("id"))).collect())
        .unwrap_or_default();
    let main = main_name
        .as_ref()
        .and_then(|n| p.comps.iter().position(|c| &c.name == n))
        .or_else(|| {
            p.comps
                .iter()
                .position(|c| c.controllers.iter().any(|k| ids.contains(&k.id)))
        })
        .or_else(|| p.converted.comps.iter().position(|c| c.selected))
        .unwrap_or(0);
    if source.is_some() && main_name.as_ref() != Some(&p.comps[main].name) {
        p.converted.warnings.push(
            Note::new("Composition {name} not found", Status::Partial)
                .with("name", main_name.clone().unwrap_or_default()),
        );
    }
    for (i, c) in p.converted.comps.iter_mut().enumerate() {
        c.selected = i == main;
    }
    let controls = controls(&def, &p.comps[main].controllers);
    aep::apply_controls(&mut p.converted.comps[main], &p.comps[main], &controls);

    // The poster frame: the composition's picture in the list.
    let thumb = names
        .iter()
        .find(|n| {
            let f = n.rsplit('/').next().unwrap_or(n).to_lowercase();
            f == "thumb.png" || f == "thumb.jpg"
        })
        .cloned();
    if let Some(bytes) = thumb.and_then(|n| zip.read(&n)) {
        let ext = if bytes.starts_with(&[0xFF, 0xD8]) {
            "jpg"
        } else {
            "png"
        };
        let mut file = format!("mogrt-thumb.{ext}");
        let mut n = 2;
        while p.converted.files.iter().any(|(f, _)| *f == file) {
            file = format!("mogrt-thumb-{n}.{ext}");
            n += 1;
        }
        p.converted.comps[main].flat = Some(format!("{}{file}", ctx.prefix));
        p.converted.files.push((file, bytes));
    }

    let mut out = p.converted;
    out.format = Format::Mogrt;
    out.name = title.unwrap_or_default();
    Ok(out)
}

/// The composition named by the definition (`sourceInfoLocalized`).
fn source_name(def: &Json) -> Option<String> {
    let info = def.get("sourceInfoLocalized")?;
    let pick = |v: &Json| str_of(v.get("name")).filter(|s| !s.trim().is_empty());
    match info {
        Json::Object(m) => m
            .get("en_US")
            .and_then(pick)
            .or_else(|| m.values().find_map(pick))
            .or_else(|| pick(info)),
        Json::Array(a) => a.iter().find_map(pick),
        _ => None,
    }
}

/// The controls of the definition, completed by the project's controllers
/// with the same id (what they drive); the project's own when the
/// definition lists none.
fn controls(def: &Json, project: &[Controller]) -> Vec<Controller> {
    let Some(list) = def
        .get("clientControls")
        .and_then(Json::as_array)
        .filter(|a| !a.is_empty())
    else {
        return project.to_vec();
    };
    list.iter()
        .take(10_000)
        .filter_map(|c| {
            let id = str_of(c.get("id")).unwrap_or_default();
            let base = project
                .iter()
                .find(|p| !id.is_empty() && p.id == id)
                .cloned();
            // The project's type is the reliable one.
            let kind = base.as_ref().map(|b| b.kind).or(kind_of(c.get("type")))?;
            let mut out = base.unwrap_or(Controller {
                id,
                name: String::new(),
                kind,
                value: Vec::new(),
                min: None,
                max: None,
                items: Vec::new(),
                text: String::new(),
                layer: 0,
                path: Vec::new(),
            });
            if let Some(n) = loc(c.get("uiName")).or_else(|| loc(c.get("name"))) {
                out.name = n;
            }
            let value = c.get("value");
            match kind {
                6 => {
                    if let Some(t) = loc(value) {
                        out.text = t;
                    }
                }
                1 => {
                    if let Some(v) =
                        value.and_then(|v| v.as_bool().or(v.as_f64().map(|n| n != 0.0)))
                    {
                        out.value = vec![if v { 1.0 } else { 0.0 }];
                    }
                }
                4 => {
                    if let Some(c) = value.and_then(color) {
                        out.value = c.to_vec();
                    }
                }
                _ => {
                    if let Some(v) = value.and_then(number) {
                        out.value = vec![v];
                    }
                }
            }
            let range = c.get("range");
            let bound = |k: &str| {
                c.get(k)
                    .and_then(number)
                    .or_else(|| range.and_then(|r| r.get(k)).and_then(number))
            };
            if let Some(m) = bound("min") {
                out.min = Some(m);
            }
            if let Some(m) = bound("max") {
                out.max = Some(m);
            }
            let items = ["menuItems", "items", "options"]
                .iter()
                .find_map(|k| c.get(*k).and_then(Json::as_array))
                .map(|a| a.iter().filter_map(|i| loc(Some(i))).collect::<Vec<_>>());
            if let Some(items) = items.filter(|i| !i.is_empty()) {
                out.items = items;
            }
            Some(out)
        })
        .collect()
}

/// A controller type: After Effects' number, or a name.
fn kind_of(v: Option<&Json>) -> Option<u32> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return u32::try_from(n).ok();
    }
    let s = v.as_str()?.to_lowercase();
    Some(match s.as_str() {
        "checkbox" | "check box" | "bool" | "boolean" => 1,
        "slider" | "number" => 2,
        "angle" => 3,
        "color" | "colour" => 4,
        "point" => 5,
        "text" | "sourcetext" | "source text" => 6,
        "comment" => 8,
        "group" => 10,
        "dropdown" | "menu" | "popup" => 13,
        "media" | "mediareplacement" | "media replacement" | "image" => 14,
        _ => return None,
    })
}

/// A localised string: `{"strDB": [{"localeString": "en_US", "str": …}]}`,
/// a locale map, or plain text.
fn loc(v: Option<&Json>) -> Option<String> {
    let v = v?;
    match v {
        Json::String(s) => Some(s.clone()),
        Json::Object(m) => {
            if let Some(db) = m.get("strDB").and_then(Json::as_array) {
                let pick = |e: &&Json| str_of(e.get("str"));
                return db
                    .iter()
                    .find(|e| str_of(e.get("localeString")).as_deref() == Some("en_US"))
                    .and_then(|e| pick(&e))
                    .or_else(|| db.iter().find_map(|e| pick(&e)));
            }
            if let Some(s) = str_of(m.get("str")).or_else(|| str_of(m.get("value"))) {
                return Some(s);
            }
            m.get("en_US")
                .and_then(Json::as_str)
                .or_else(|| m.values().find_map(Json::as_str))
                .map(str::to_string)
        }
        _ => None,
    }
}

fn str_of(v: Option<&Json>) -> Option<String> {
    v.and_then(Json::as_str).map(str::to_string)
}

fn number(v: &Json) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .or_else(|| v.get("value").and_then(Json::as_f64))
        .filter(|n| n.is_finite())
}

/// RGBA 0..1 from `[r, g, b(, a)]` (0..1 or 0..255), `{"red", …}` /
/// `{"r", …}` or `#rrggbb`.
fn color(v: &Json) -> Option<[f64; 4]> {
    let c: Vec<f64> = match v {
        Json::Array(a) => a.iter().filter_map(Json::as_f64).collect(),
        Json::Object(m) => {
            let g = |a: &str, b: &str| m.get(a).or(m.get(b)).and_then(Json::as_f64);
            vec![
                g("red", "r")?,
                g("green", "g")?,
                g("blue", "b")?,
                g("alpha", "a").unwrap_or(1.0),
            ]
        }
        Json::String(s) => {
            let c = cg_core::Color::from_hex(s)?;
            return Some(c.0.map(|v| v as f64));
        }
        _ => return None,
    };
    if c.len() < 3 {
        return None;
    }
    let k = if c.iter().take(3).any(|v| *v > 1.0) {
        255.0
    } else {
        1.0
    };
    let a = c
        .get(3)
        .map_or(1.0, |a| if *a > 1.0 { a / 255.0 } else { *a });
    Some([c[0] / k, c[1] / k, c[2] / k, a])
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::{FieldKind, Prop};

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/tests/fixtures/import/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn ctx() -> Ctx {
        Ctx {
            prefix: "/staged/".into(),
            fonts: vec!["Inter".into()],
            zone: "lower".into(),
        }
    }

    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut out = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
        out.into_inner()
    }

    fn strdb(s: &str) -> Json {
        serde_json::json!({ "strDB": [{ "localeString": "en_US", "str": s }] })
    }

    /// A template as After Effects exports it: the definition, the
    /// project (zipped in project.aegraphic) and the poster frame. The
    /// project is a real one (py-aep sample); the definition is written
    /// here, its control ids being those the project stores.
    fn mogrt(aep: &str, def: Json) -> Vec<u8> {
        let project = zip(&[("project.aep", &fixture(aep))]);
        let def = serde_json::to_vec(&def).unwrap();
        zip(&[
            ("definition.json", &def),
            ("project.aegraphic", &project),
            ("thumb.png", b"\x89PNG\r\n\x1a\n"),
        ])
    }

    #[test]
    fn maps_definition_controls() {
        let def = serde_json::json!({
            "capsuleNameLocalized": strdb("Lower third"),
            "sourceInfoLocalized": { "en_US": { "name": "primary" } },
            "clientControls": [
                { "id": "548b4a79-0a1b-4242-80e4-6b1d54dbe236", "type": 2,
                  "uiName": strdb("Brightness"), "value": 10.0, "min": -100.0, "max": 100.0 },
                { "id": "0604860d-8095-4c24-a5ff-50df6e8d598f", "type": 2,
                  "uiName": strdb("Opacity"), "value": 80.0 },
                { "id": "be8441be-45e7-48d3-8308-319616b0441d", "type": 4,
                  "uiName": strdb("Background"), "value": [0.0, 0.0, 1.0, 1.0] },
                { "id": "00000000-aaaa-bbbb-cccc-000000000000", "type": 1,
                  "uiName": strdb("Show logo"), "value": true },
                { "id": "00000000-aaaa-bbbb-cccc-000000000001", "type": 13,
                  "uiName": strdb("Side"), "value": 2, "menuItems": ["Left", "Right"] }
            ]
        });
        let bytes = mogrt("aep_eg_controllers.aep", def);
        assert_eq!(super::super::detect("x.mogrt", &bytes), Some(Format::Mogrt));
        let c = analyze(&bytes, &ctx()).unwrap();
        assert_eq!(c.format, Format::Mogrt);
        assert_eq!(c.name, "Lower third");
        let sel: Vec<&str> = c
            .comps
            .iter()
            .filter(|c| c.selected)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(sel, ["primary"]);
        let p = c.comps.iter().find(|c| c.selected).unwrap();
        let v: Vec<(&str, FieldKind, Status)> = p
            .variables
            .iter()
            .map(|v| (v.name.as_str(), v.kind, v.status))
            .collect();
        assert_eq!(
            v,
            [
                ("Brightness", FieldKind::Number, Status::Partial),
                ("Opacity", FieldKind::Number, Status::Ok),
                ("Background", FieldKind::Color, Status::Partial),
                ("Show logo", FieldKind::Boolean, Status::Partial),
                ("Side", FieldKind::Text, Status::Partial)
            ]
        );
        let f = &p.template.fields;
        assert_eq!(f[0].default, "10");
        assert_eq!((f[0].min, f[0].max), (Some(-100.0), Some(100.0)));
        assert_eq!(f[1].default, "0.8");
        assert_eq!(f[2].default, "#0000ff");
        assert_eq!(f[3].default, "true");
        assert_eq!(f[4].default, "Right");
        assert_eq!(f[4].options.len(), 2);
        let l = &p.template.layers[0];
        assert_eq!(l.bindings[&Prop::Opacity], f[1].key);
        // The poster frame is the composition's picture.
        assert_eq!(p.flat.as_deref(), Some("/staged/mogrt-thumb.png"));
        assert!(c.files.iter().any(|(n, _)| n == "mogrt-thumb.png"));
    }

    #[test]
    fn binds_text_controls() {
        let def = serde_json::json!({
            "capsuleNameLocalized": strdb("Title"),
            "sourceInfoLocalized": { "en_US": { "name": "primary" } },
            "clientControls": [
                { "id": "ed8b9cc2-a22a-4b7a-84aa-27108984df50", "type": 6,
                  "uiName": strdb("Name"), "value": strdb("Jane Doe") }
            ]
        });
        let c = analyze(&mogrt("aep_eg_text.aep", def), &ctx()).unwrap();
        let p = c.comps.iter().find(|c| c.selected).unwrap();
        assert_eq!(p.name, "primary");
        assert_eq!(p.variables.len(), 1);
        assert_eq!(p.variables[0].status, Status::Ok);
        let f = &p.template.fields[0];
        assert_eq!((f.label.as_str(), f.default.as_str()), ("Name", "Jane Doe"));
        let cg_core::LayerKind::Text { text, .. } = &p.template.layers[0].kind else {
            panic!("not a text")
        };
        assert_eq!(*text, format!("{{{{{}}}}}", f.key));
    }

    #[test]
    fn falls_back_to_the_project_controllers() {
        // A definition without controls: the project's own are used.
        let def = serde_json::json!({ "sourceInfoLocalized": { "en_US": { "name": "primary" } } });
        let c = analyze(&mogrt("aep_eg_text.aep", def), &ctx()).unwrap();
        let p = c.comps.iter().find(|c| c.selected).unwrap();
        assert_eq!(p.variables[0].name, "Title Text");
        assert_eq!(c.name, "");
    }

    #[test]
    fn refuses_other_files() {
        let no_def = zip(&[("project.aegraphic", b"PK")]);
        assert!(analyze(&no_def, &ctx())
            .unwrap_err()
            .contains("definition.json"));
        let premiere = zip(&[("definition.json", b"{}"), ("x.prgraphic", b"x")]);
        assert!(analyze(&premiere, &ctx()).unwrap_err().contains("Premiere"));
        let bad = zip(&[
            ("definition.json", b"{}"),
            ("project.aegraphic", b"not a zip"),
        ]);
        assert!(analyze(&bad, &ctx()).is_err());
        assert!(analyze(b"nothing", &ctx()).is_err());
    }
}
