//! Template thumbnails, encoded once: the PNGs last made, by what they show
//! (the template, the project settings it is drawn with, the frame, the
//! data, the size), so lists opened again or by several people are not
//! rendered again. The least recently used go first past a byte budget.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

use axum::body::Bytes;
use serde::Serialize;

/// Encoded pictures kept at most (bytes).
const BUDGET: usize = 24 << 20;

#[derive(Default)]
pub struct Thumbs {
    inner: Mutex<Lru>,
}

#[derive(Default)]
struct Lru {
    map: HashMap<u64, (Bytes, u64)>,
    bytes: usize,
    tick: u64,
}

impl Thumbs {
    pub fn get(&self, key: u64) -> Option<Bytes> {
        let mut l = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        l.tick += 1;
        let tick = l.tick;
        l.map.get_mut(&key).map(|(b, used)| {
            *used = tick;
            b.clone()
        })
    }

    pub fn put(&self, key: u64, png: Bytes) {
        if png.len() > BUDGET / 8 {
            return;
        }
        let mut l = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        l.tick += 1;
        let tick = l.tick;
        l.bytes += png.len();
        if let Some((old, _)) = l.map.insert(key, (png, tick)) {
            l.bytes -= old.len();
        }
        while l.bytes > BUDGET {
            let Some(oldest) = l.map.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| *k) else {
                break;
            };
            if let Some((b, _)) = l.map.remove(&oldest) {
                l.bytes -= b.len();
            }
        }
    }

    /// Pictures or fonts changed: every thumbnail may look different.
    pub fn clear(&self) {
        let mut l = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        l.map.clear();
        l.bytes = 0;
    }
}

/// Feeds what serde writes into a hasher (no JSON text kept).
struct HashWriter<'a>(&'a mut DefaultHasher);

impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Builds the key of a picture from everything it is drawn from.
#[derive(Default)]
pub struct Key(DefaultHasher);

impl Key {
    pub fn value<T: Serialize + ?Sized>(mut self, v: &T) -> Self {
        let _ = serde_json::to_writer(HashWriter(&mut self.0), v);
        0xffu8.hash(&mut self.0);
        self
    }

    pub fn part<T: Hash + ?Sized>(mut self, v: &T) -> Self {
        v.hash(&mut self.0);
        self
    }

    /// A template of a project: the template and the project settings it
    /// is drawn with (size, rate, zones, formats, language).
    pub fn template(self, p: &cg_core::Project, t: &cg_core::Template) -> Self {
        self.part(&p.id)
            .part(&(p.width, p.height))
            .value(&p.frame_rate)
            .value(&p.zones)
            .value(&p.formats)
            .part(&p.language)
            .value(t)
    }

    pub fn finish(&self) -> u64 {
        self.0.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn least_recently_used_go_first() {
        let t = Thumbs::default();
        let big = Bytes::from(vec![0u8; BUDGET / 10]);
        for k in 0..10 {
            t.put(k, big.clone());
        }
        // 0 used again: 1 is the oldest now.
        assert!(t.get(0).is_some());
        t.put(10, big.clone());
        assert!(t.get(0).is_some() && t.get(1).is_none() && t.get(10).is_some());
        t.clear();
        assert!(t.get(0).is_none());
    }

    #[test]
    fn keys_follow_what_is_drawn() {
        let p = cg_core::sample::demo_project();
        let a = &p.templates[0];
        let mut b = a.clone();
        let k = |t: &cg_core::Template, w: u32| Key::default().template(&p, t).part(&w).finish();
        assert_eq!(k(a, 480), k(a, 480));
        assert_ne!(k(a, 480), k(a, 640));
        b.name.push('x');
        assert_ne!(k(a, 480), k(&b, 480));
    }
}
