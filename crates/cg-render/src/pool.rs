//! Frame buffers used again: a channel draws a new 8 MB picture (1080p)
//! every frame, and each one used to be a new allocation, freed once every
//! output had sent it. A dropped [`crate::Frame`] gives its buffer back
//! here and the next frame of that size takes it, so the memory in use
//! stays that of the few frames in flight; the spare ones are released once
//! nothing has been drawn for a moment ([`release_idle`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tiny_skia::{IntSize, Pixmap};

/// Spare buffers kept at most (a few frames in flight per picture).
const SPARE: usize = 6;
/// Smaller buffers are left to the allocator.
const MIN_LEN: usize = 1 << 20;

static FREE: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
/// When a buffer was last taken, and when spares were last released (ms).
static USED_AT: AtomicU64 = AtomicU64::new(0);
static RELEASED_AT: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn spare(len: usize) -> Option<Vec<u8>> {
    if len < MIN_LEN {
        return None;
    }
    USED_AT.store(now_ms(), Ordering::Relaxed);
    let mut free = FREE.lock().unwrap_or_else(|e| e.into_inner());
    let i = free.iter().position(|b| b.len() == len)?;
    Some(free.swap_remove(i))
}

/// A transparent picture (like `Pixmap::new`), on a spare buffer if any.
pub(crate) fn pixmap(width: u32, height: u32) -> Pixmap {
    let size = IntSize::from_wh(width.max(1), height.max(1)).expect("valid size");
    let len = size.width() as usize * size.height() as usize * 4;
    match spare(len) {
        Some(mut b) => {
            b.fill(0);
            Pixmap::from_vec(b, size).expect("buffer of the size")
        }
        None => Pixmap::new(size.width(), size.height()).expect("valid size"),
    }
}

/// A copy of `pm` (like `clone`), on a spare buffer if any.
pub(crate) fn copy(pm: &Pixmap) -> Pixmap {
    match spare(pm.data().len()) {
        Some(mut b) => {
            b.copy_from_slice(pm.data());
            let size = IntSize::from_wh(pm.width(), pm.height()).expect("valid size");
            Pixmap::from_vec(b, size).expect("buffer of the size")
        }
        None => pm.clone(),
    }
}

/// Takes back the buffer of a frame no longer used.
pub(crate) fn give(buf: Vec<u8>) {
    if buf.len() < MIN_LEN {
        return;
    }
    let mut free = FREE.lock().unwrap_or_else(|e| e.into_inner());
    if free.len() < SPARE {
        free.push(buf);
    }
}

/// Says that something drew big pictures without the pool (thumbnails…):
/// memory to give back once things are quiet again.
pub fn busy() {
    USED_AT.store(now_ms(), Ordering::Relaxed);
}

/// When nothing has been drawn for `quiet` since the last call that
/// released anything: frees the spare buffers and returns true (the caller
/// then hands freed memory back to the system, see cg-server).
pub fn release_idle(quiet: Duration) -> bool {
    let (used, released) = (
        USED_AT.load(Ordering::Relaxed),
        RELEASED_AT.load(Ordering::Relaxed),
    );
    let now = now_ms();
    if used <= released || now.saturating_sub(used) < quiet.as_millis() as u64 {
        return false;
    }
    RELEASED_AT.store(now, Ordering::Relaxed);
    FREE.lock().unwrap_or_else(|e| e.into_inner()).clear();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_give_their_buffer_back() {
        let pm = pixmap(1024, 512);
        let at = pm.data().as_ptr();
        give(pm.take());
        let again = pixmap(1024, 512);
        assert_eq!(again.data().as_ptr(), at);
        assert!(again.data().iter().all(|b| *b == 0));
        let copied = copy(&again);
        assert_eq!(copied.data(), again.data());
        // Small pictures stay with the allocator.
        give(vec![1; 16]);
        assert!(spare(16).is_none());
    }
}
