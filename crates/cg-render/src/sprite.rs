//! Layers kept drawn (sprites), so that a layer whose picture does not
//! change is drawn once and laid on the frame after that.
//!
//! A step of a frame (a layer's own drawing, or a layer with its children
//! under a drop shadow) whose picture does not depend on time
//! (`cache::Motion`, not `Moving`) is drawn on a picture of its own, in
//! device pixels without the whole-pixel part of its position, and that
//! picture is laid on the frame at its position, with its opacity, through
//! its clip. The next frames lay the same picture again while nothing of
//! the layer changed (its values, text, data, the motion key…); a slide, a
//! fade, a graphic coming in under a static one: the layer is not drawn
//! again, only laid. At rest the fraction of its position is drawn into the
//! picture, so the frame is the one of drawing the layer straight; while
//! its geometry animates, the picture is laid at the fraction (bilinear)
//! and made at the output scale only, so one picture serves the whole
//! move. Frames are the same whichever renderer draws them: the choice
//! never depends on what a renderer kept, only on the scene.
//!
//! A layer drawing outside its box by more than the margin (the ring of the
//! picture touched) is drawn straight, as before, and remembered so.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tiny_skia::Pixmap;

/// Frames a sprite is kept unused.
const KEEP_FRAMES: u64 = 250;
/// Memory for the sprites of one renderer, beyond which the oldest go.
const MAX_BYTES: usize = 256 << 20;

pub(crate) struct Sprite {
    /// The picture; None: the layer draws out of its margin, drawn straight.
    pub pm: Option<Arc<Pixmap>>,
    /// Where the picture's (0, 0) is, in device pixels less the whole-pixel
    /// part of the layer's position.
    pub x: i32,
    pub y: i32,
    /// What it was drawn from.
    pub key: u64,
    used: u64,
}

/// The sprites of a renderer, by graphic on air and layer.
#[derive(Default)]
pub(crate) struct Sprites {
    map: HashMap<(usize, u64), Sprite>,
    bytes: usize,
    tick: u64,
    /// Layer definitions hashed (`Layer` as JSON), by address: made again
    /// now and then, in case an address serves another layer.
    defs: HashMap<usize, u64>,
    /// Sprites drawn and laid again (tests, measures).
    pub made: u64,
    pub found: u64,
}

impl Sprites {
    /// A frame begins: sprites unused for a while are dropped.
    pub fn frame(&mut self) {
        self.tick += 1;
        let tick = self.tick;
        if tick.is_multiple_of(100) {
            self.defs.clear();
        }
        if self.map.is_empty() {
            return;
        }
        let mut bytes = self.bytes;
        self.map.retain(|_, s| {
            let keep = tick - s.used <= KEEP_FRAMES;
            if !keep {
                bytes -= s.pm.as_ref().map_or(0, |p| p.data().len());
            }
            keep
        });
        self.bytes = bytes;
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.defs.clear();
        self.bytes = 0;
    }

    /// A layer's definition, hashed.
    pub fn def(&mut self, layer: &cg_core::Layer) -> u64 {
        let at = layer as *const cg_core::Layer as usize;
        *self.defs.entry(at).or_insert_with(|| {
            let mut k = Key::new();
            k.add(&serde_json::to_vec(layer).unwrap_or_default());
            k.finish()
        })
    }

    fn id(layer: &str) -> u64 {
        let mut k = Key::new();
        k.add(layer);
        k.finish()
    }

    /// The sprite of (`item`, `layer`) if it was made from `key`: its
    /// picture (None: the layer is drawn straight) and where it goes.
    pub fn get(
        &mut self,
        item: usize,
        layer: &str,
        key: u64,
    ) -> Option<(Option<Arc<Pixmap>>, i32, i32)> {
        let s = self.map.get_mut(&(item, Self::id(layer)))?;
        if s.key != key {
            return None;
        }
        s.used = self.tick;
        self.found += 1;
        Some((s.pm.clone(), s.x, s.y))
    }

    /// Keeps a sprite just made (or the knowledge that the layer is drawn straight).
    pub fn put(&mut self, item: usize, layer: &str, key: u64, pm: Option<Pixmap>, x: i32, y: i32) {
        let pm = pm.map(Arc::new);
        let k = (item, Self::id(layer));
        if let Some(old) = self.map.remove(&k) {
            self.bytes -= old.pm.as_ref().map_or(0, |p| p.data().len());
        }
        let len = pm.as_ref().map_or(0, |p| p.data().len());
        if self.bytes + len > MAX_BYTES {
            // The oldest go first until it fits (never this one).
            let mut by_age: Vec<((usize, u64), u64, usize)> = self
                .map
                .iter()
                .map(|(k, s)| (*k, s.used, s.pm.as_ref().map_or(0, |p| p.data().len())))
                .collect();
            by_age.sort_by_key(|(_, used, _)| *used);
            for (k, _, n) in by_age {
                if self.bytes + len <= MAX_BYTES {
                    break;
                }
                self.map.remove(&k);
                self.bytes -= n;
            }
        }
        self.bytes += len;
        self.made += 1;
        self.map.insert(
            k,
            Sprite {
                pm,
                x,
                y,
                key,
                used: self.tick,
            },
        );
    }
}

/// A hasher for the keys (what a sprite is drawn from).
pub(crate) struct Key(std::collections::hash_map::DefaultHasher);

impl Key {
    pub fn new() -> Key {
        Key(std::collections::hash_map::DefaultHasher::new())
    }
    pub fn add<T: Hash + ?Sized>(&mut self, v: &T) -> &mut Key {
        v.hash(&mut self.0);
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Key {
        self.add(&v.to_bits())
    }
    pub fn f64(&mut self, v: f64) -> &mut Key {
        self.add(&v.to_bits())
    }
    pub fn finish(&self) -> u64 {
        self.0.finish()
    }
}

/// True when a pixel of the outer ring of `pm` is not transparent: the
/// drawing reached the edge, so it may go on beyond it. `sides`: which of
/// left, top, right, bottom to look at (a side cut by the frame does not
/// matter).
pub(crate) fn touches_edge(pm: &Pixmap, sides: [bool; 4]) -> bool {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    if w < 3 || h < 3 {
        return true;
    }
    let d = pm.data();
    let row_has = |y: usize| {
        d[y * w * 4..(y + 1) * w * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] != 0)
    };
    if (sides[1] && row_has(0)) || (sides[3] && row_has(h - 1)) {
        return true;
    }
    (1..h - 1).any(|y| {
        (sides[0] && d[(y * w) * 4 + 3] != 0) || (sides[2] && d[(y * w + w - 1) * 4 + 3] != 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sprite_is_found_by_its_key_and_dropped_when_unused() {
        let mut s = Sprites::default();
        s.frame();
        s.put(0, "a", 7, Some(Pixmap::new(4, 4).unwrap()), 1, 2);
        assert!(s
            .get(0, "a", 7)
            .is_some_and(|(p, x, y)| p.is_some() && x == 1 && y == 2));
        assert!(s.get(0, "a", 8).is_none());
        for _ in 0..KEEP_FRAMES + 2 {
            s.frame();
        }
        assert!(s.get(0, "a", 7).is_none());
        assert_eq!(s.bytes, 0);
    }

    #[test]
    fn the_edge_ring_is_checked() {
        let mut pm = Pixmap::new(5, 5).unwrap();
        let all = [true; 4];
        assert!(!touches_edge(&pm, all));
        pm.pixels_mut()[2 * 5 + 2] =
            tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
        assert!(!touches_edge(&pm, all));
        pm.pixels_mut()[4 * 5 + 2] =
            tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
        assert!(touches_edge(&pm, all));
        assert!(!touches_edge(&pm, [true, true, true, false]));
    }
}
