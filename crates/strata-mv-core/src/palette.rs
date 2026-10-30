//! Palette colours followed by the layers: a layer keeps the colour's value
//! (the renderers read it as any other); when the palette changes it, every
//! value equal to the old one is replaced by the new.

use serde_json::Value;

use crate::model::{Color, NamedColor, Project};

/// The colour as 8-bit RGBA, to compare values written differently
/// (`#fff`, `#FFFFFF`, `#ffffffff`, `[1, 1, 1, 1]`).
fn key(c: Color) -> [u8; 4] {
    c.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// A property value written as `[r, g, b, a]` (components 0..=1).
fn as_rgba(v: &Value) -> Option<Color> {
    let a = v.as_array().filter(|a| a.len() == 4)?;
    let mut c = [0.0f32; 4];
    for (o, x) in c.iter_mut().zip(a) {
        let x = x.as_f64().filter(|x| (0.0..=1.0).contains(x))?;
        *o = x as f32;
    }
    Some(Color(c))
}

/// Replaces `from` by `to` in `v`: hex strings anywhere, `[r, g, b, a]`
/// values of properties and keyframes. Returns how many were replaced.
pub fn replace(v: &mut Value, from: Color, to: Color) -> usize {
    walk(v, key(from), to, false)
}

fn walk(v: &mut Value, from: [u8; 4], to: Color, prop: bool) -> usize {
    if prop && as_rgba(v).is_some_and(|c| key(c) == from) {
        *v = serde_json::json!(to.0);
        return 1;
    }
    match v {
        Value::String(s) => match Color::from_hex(s) {
            Some(c) if s.trim().starts_with('#') && key(c) == from => {
                *s = to.to_hex();
                1
            }
            _ => 0,
        },
        Value::Array(items) => items.iter_mut().map(|x| walk(x, from, to, false)).sum(),
        Value::Object(map) => map
            .iter_mut()
            .map(|(k, x)| match k.as_str() {
                // A layer's properties: {name: value}.
                "props" => match x {
                    Value::Object(props) => {
                        props.values_mut().map(|p| walk(p, from, to, true)).sum()
                    }
                    _ => walk(x, from, to, false),
                },
                // A keyframe's value.
                "value" => walk(x, from, to, true),
                _ => walk(x, from, to, false),
            })
            .sum(),
        _ => 0,
    }
}

/// The colours whose value changed from `old` to `new` (same id): (old, new).
pub fn changes(old: &[NamedColor], new: &[NamedColor]) -> Vec<(Color, Color)> {
    new.iter()
        .filter_map(|n| {
            let o = old.iter().find(|o| o.id == n.id)?;
            let (a, b) = (Color::from_hex(&o.value)?, Color::from_hex(&n.value)?);
            (key(a) != key(b)).then_some((a, b))
        })
        .collect()
}

impl Project {
    /// Replaces the colours in the templates, drafts and rundown (the
    /// palette changed them). True when something changed.
    pub fn replace_colors(&mut self, changes: &[(Color, Color)]) -> bool {
        if changes.is_empty() {
            return false;
        }
        let Ok(mut v) = serde_json::to_value(&*self) else {
            return false;
        };
        let mut n = 0;
        for (from, to) in changes {
            for part in ["templates", "drafts", "rundown"] {
                if let Some(x) = v.get_mut(part) {
                    n += replace(x, *from, *to);
                }
            }
        }
        if n == 0 {
            return false;
        }
        match serde_json::from_value(v) {
            Ok(p) => {
                *self = p;
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replaces_hex_and_property_values() {
        let red = Color::from_hex("#e4002b").unwrap();
        let blue = Color::from_hex("#2f6fe4cc").unwrap();
        let mut v = json!({
            "layers": [{
                "props": { "fill": red.0, "x": 10.0, "opacity": [1.0, 1.0, 1.0, 1.0] },
                "tracks": { "fill": [{ "frame": 0, "value": red.0 }] },
                "config": { "bandColor": "#E4002B", "text": "#e4002b est rouge", "other": "#e4002c" }
            }]
        });
        assert_eq!(replace(&mut v, red, blue), 3);
        let l = &v["layers"][0];
        assert_eq!(as_rgba(&l["props"]["fill"]).map(key), Some(key(blue)));
        assert_eq!(
            as_rgba(&l["tracks"]["fill"][0]["value"]).map(key),
            Some(key(blue))
        );
        assert_eq!(l["config"]["bandColor"], "#2f6fe4cc");
        // Not a colour value, or another colour: untouched.
        assert_eq!(l["config"]["text"], "#e4002b est rouge");
        assert_eq!(l["config"]["other"], "#e4002c");
        assert_eq!(l["props"]["x"], 10.0);
    }

    #[test]
    fn changes_by_id() {
        let c = |id: &str, v: &str| NamedColor {
            id: id.into(),
            name: String::new(),
            value: v.into(),
        };
        let old = [c("a", "#ffffff"), c("b", "#000000")];
        let new = [c("a", "#FFF"), c("b", "#111111"), c("c", "#222222")];
        let ch = changes(&old, &new);
        assert_eq!(ch.len(), 1);
        assert_eq!(key(ch[0].1), [17, 17, 17, 255]);
    }
}
