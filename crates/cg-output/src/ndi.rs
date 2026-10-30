//! NDI output (alpha included) through the NDI runtime.
//!
//! The NDI SDK is distributed by Vizrt under its own licence, so it is not
//! linked at build time: the runtime library (installed with NDI Tools, or
//! the SDK redistributable) is loaded when the first NDI output opens. Without
//! it, opening an output returns `OutputError::Unsupported` with where it was
//! looked for. The server settings (or `NDI_LIB_PATH`) point to the library
//! explicitly; otherwise the runtime shipped next to the server by the Strata
//! installers is used, then the NDI SDK / NDI Tools install folders.

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Mutex, OnceLock};

use cg_render::Frame;
use serde::{Deserialize, Serialize};

use crate::{OutputError, OutputSink};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NdiConfig {
    #[serde(default)]
    pub id: String,
    /// Off: kept in the settings, not started.
    #[serde(default = "crate::on")]
    pub enabled: bool,
    /// Source name seen by receivers (shown as "HOST (name)").
    pub name: String,
    /// Comma-separated NDI groups ("" = public).
    #[serde(default)]
    pub groups: String,
    /// Send the alpha channel (RGBA) or an opaque picture (RGBX).
    #[serde(default = "yes")]
    pub alpha: bool,
}

fn yes() -> bool {
    true
}

#[repr(C)]
struct SendCreate {
    p_ndi_name: *const c_char,
    p_groups: *const c_char,
    clock_video: bool,
    clock_audio: bool,
}

#[repr(C)]
struct VideoFrameV2 {
    xres: i32,
    yres: i32,
    four_cc: u32,
    frame_rate_n: i32,
    frame_rate_d: i32,
    picture_aspect_ratio: f32,
    frame_format_type: i32,
    timecode: i64,
    p_data: *const u8,
    line_stride_in_bytes: i32,
    p_metadata: *const c_char,
    timestamp: i64,
}

const fn fourcc(s: &[u8; 4]) -> u32 {
    s[0] as u32 | (s[1] as u32) << 8 | (s[2] as u32) << 16 | (s[3] as u32) << 24
}
const FOURCC_RGBA: u32 = fourcc(b"RGBA");
const FOURCC_RGBX: u32 = fourcc(b"RGBX");
const PROGRESSIVE: i32 = 1;
const TIMECODE_SYNTHESIZE: i64 = i64::MAX;

type Instance = *mut c_void;

struct Lib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn(*const SendCreate) -> Instance,
    send: unsafe extern "C" fn(Instance, *const VideoFrameV2),
    destroy: unsafe extern "C" fn(Instance),
}

// The NDI functions are thread-safe; the library handle is only read.
unsafe impl Send for Lib {}
unsafe impl Sync for Lib {}

/// Runtime path set in the server settings (tried first).
static CONFIGURED: Mutex<String> = Mutex::new(String::new());

/// Sets the runtime path from the server settings ("" = search for it).
pub fn set_library_path(path: &str) {
    *CONFIGURED.lock().unwrap_or_else(|e| e.into_inner()) = path.trim().to_string();
}

fn candidates() -> Vec<String> {
    let mut c: Vec<String> = Vec::new();
    let configured = CONFIGURED.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !configured.is_empty() {
        c.push(configured);
    }
    c.extend(std::env::var("NDI_LIB_PATH"));
    // The runtime shipped with Strata, next to the server.
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
    {
        let names: &[&str] = if cfg!(target_os = "windows") {
            &["Processing.NDI.Lib.x64.dll"]
        } else if cfg!(target_os = "macos") {
            &["lib/libndi.dylib", "libndi.dylib"]
        } else {
            &["lib/libndi.so.6", "libndi.so.6"]
        };
        c.extend(
            names
                .iter()
                .map(|n| dir.join(n).to_string_lossy().into_owned()),
        );
    }
    let runtime_dirs: Vec<String> = [
        "NDI_RUNTIME_DIR_V6",
        "NDI_RUNTIME_DIR_V5",
        "NDI_RUNTIME_DIR_V4",
    ]
    .iter()
    .filter_map(|v| std::env::var(v).ok())
    .collect();
    if cfg!(target_os = "windows") {
        for dir in &runtime_dirs {
            c.push(format!("{dir}\\Processing.NDI.Lib.x64.dll"));
        }
        for v in ["6", "5"] {
            c.push(format!(
                "C:\\Program Files\\NDI\\NDI {v} Runtime\\v{v}\\Processing.NDI.Lib.x64.dll"
            ));
            c.push(format!(
                "C:\\Program Files\\NDI\\NDI {v} Tools\\Runtime\\Processing.NDI.Lib.x64.dll"
            ));
        }
        c.push("Processing.NDI.Lib.x64.dll".into());
    } else if cfg!(target_os = "macos") {
        for dir in &runtime_dirs {
            c.push(format!("{dir}/libndi.dylib"));
        }
        c.extend(
            [
                "/usr/local/lib/libndi.dylib",
                "/opt/homebrew/lib/libndi.dylib",
                "/Library/NDI SDK for Apple/lib/macOS/libndi.dylib",
                "/Library/NDI SDK for Apple/lib/macOS/libndi_advanced.dylib",
                "/Library/NDI Advanced SDK for Apple/lib/macOS/libndi_advanced.dylib",
                "libndi.dylib",
            ]
            .map(String::from),
        );
    } else {
        for dir in &runtime_dirs {
            c.push(format!("{dir}/libndi.so.6"));
            c.push(format!("{dir}/libndi.so.5"));
        }
        c.extend([
            "libndi.so.6".into(),
            "libndi.so.5".into(),
            "libndi.so".into(),
            "/usr/local/lib/libndi.so".into(),
        ]);
    }
    let mut seen = std::collections::HashSet::new();
    c.retain(|p| seen.insert(p.clone()));
    c
}

fn load() -> Result<Lib, String> {
    let tried = candidates();
    let mut failures = Vec::new();
    for path in &tried {
        // Only report the paths that exist but would not load (a bare name
        // is searched in the system folders).
        let bare = !path.contains('/') && !path.contains('\\');
        // SAFETY: loading the NDI runtime runs its initialisers, which is
        // what the SDK expects of its users.
        let lib = match unsafe { libloading::Library::new(path) } {
            Ok(l) => l,
            Err(e) => {
                if !bare && std::path::Path::new(path).exists() {
                    failures.push(format!("{path}: {e}"));
                }
                continue;
            }
        };
        unsafe {
            let init: libloading::Symbol<unsafe extern "C" fn() -> bool> =
                lib.get(b"NDIlib_initialize\0").map_err(|e| e.to_string())?;
            if !init() {
                return Err("the NDI runtime refused to start (unsupported CPU?)".into());
            }
            let create = *lib
                .get(b"NDIlib_send_create\0")
                .map_err(|e| e.to_string())?;
            let send = *lib
                .get(b"NDIlib_send_send_video_v2\0")
                .map_err(|e| e.to_string())?;
            let destroy = *lib
                .get(b"NDIlib_send_destroy\0")
                .map_err(|e| e.to_string())?;
            tracing::info!(path, "NDI runtime loaded");
            *LOADED.lock().unwrap_or_else(|e| e.into_inner()) = Some(path.clone());
            return Ok(Lib {
                _lib: lib,
                create,
                send,
                destroy,
            });
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "NDI runtime found but not loadable ({})",
            failures.join("; ")
        ));
    }
    Err(format!(
        "NDI runtime not found (reinstall Strata, or install the NDI SDK; looked for {})",
        tried.join(", ")
    ))
}

/// The runtime, loaded once it is found; looked for again at each output
/// start until then (it may be installed or configured meanwhile).
fn lib() -> Result<&'static Lib, OutputError> {
    static LIB: OnceLock<Lib> = OnceLock::new();
    static LOADING: Mutex<()> = Mutex::new(());
    if let Some(l) = LIB.get() {
        return Ok(l);
    }
    let _guard = LOADING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(l) = LIB.get() {
        return Ok(l);
    }
    let l = load().map_err(OutputError::Unsupported)?;
    Ok(LIB.get_or_init(|| l))
}

/// Path of the loaded runtime, if any.
pub fn loaded_path() -> Option<String> {
    LOADED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

static LOADED: Mutex<Option<String>> = Mutex::new(None);

pub struct NdiSender {
    config: NdiConfig,
    lib: &'static Lib,
    instance: Instance,
    rate: (i32, i32),
    buf: Arc<Vec<u8>>,
    /// The picture `buf` was made from (the same again: not converted again).
    last: Option<Arc<Frame>>,
}

// The instance is used from the render thread that owns the sender only.
unsafe impl Send for NdiSender {}

impl NdiSender {
    pub fn open(config: NdiConfig, rate: (u32, u32)) -> Result<Self, OutputError> {
        let lib = lib()?;
        let name = CString::new(config.name.clone())
            .map_err(|_| OutputError::Unsupported("invalid NDI name".into()))?;
        let groups = CString::new(config.groups.clone())
            .map_err(|_| OutputError::Unsupported("invalid NDI groups".into()))?;
        let create = SendCreate {
            p_ndi_name: name.as_ptr(),
            p_groups: if config.groups.trim().is_empty() {
                std::ptr::null()
            } else {
                groups.as_ptr()
            },
            // The playout clock paces the frames.
            clock_video: false,
            clock_audio: false,
        };
        // SAFETY: the strings outlive the call; NDI copies them.
        let instance = unsafe { (lib.create)(&create) };
        if instance.is_null() {
            return Err(OutputError::Unsupported(
                "NDI could not create the source".into(),
            ));
        }
        tracing::info!(name = config.name, "NDI source started");
        Ok(Self {
            config,
            lib,
            instance,
            rate: (rate.0 as i32, rate.1 as i32),
            buf: Arc::new(Vec::new()),
            last: None,
        })
    }
}

/// NDI takes straight (not premultiplied) alpha.
pub fn unpremultiply(rgba: &[u8], out: &mut Vec<u8>, alpha: bool) {
    out.resize(rgba.len(), 0);
    unpremultiply_into(rgba, out, alpha);
}

/// The same into a buffer of the same length.
fn unpremultiply_into(rgba: &[u8], out: &mut [u8], alpha: bool) {
    for (p, o) in rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<4>().0)
    {
        let a = p[3] as u32;
        *o = if !alpha || a == 255 || a == 0 {
            [p[0], p[1], p[2], if alpha { p[3] } else { 255 }]
        } else {
            let f = |c: u8| ((c as u32 * 255 + a / 2) / a).min(255) as u8;
            [f(p[0]), f(p[1]), f(p[2]), p[3]]
        };
    }
}

impl OutputSink for NdiSender {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn push(&mut self, frame: &Arc<Frame>, at: std::time::SystemTime) -> Result<(), OutputError> {
        // The same picture again (nothing moved): converted already; else
        // only the rows that changed since the last one.
        if !self.last.as_ref().is_some_and(|l| Arc::ptr_eq(l, frame)) {
            let buf = Arc::make_mut(&mut self.buf);
            match crate::changed_rows(frame, self.last.as_deref()) {
                Some(rows) if buf.len() == frame.data.len() => {
                    let w = frame.width as usize * 4;
                    for (y, _) in rows.iter().enumerate().filter(|(_, c)| **c) {
                        let r = y * w..(y + 1) * w;
                        unpremultiply_into(&frame.data[r.clone()], &mut buf[r], self.config.alpha);
                    }
                }
                _ => unpremultiply(&frame.data, buf, self.config.alpha),
            }
            self.last = Some(frame.clone());
        }
        let buf = &self.buf;
        let video = VideoFrameV2 {
            xres: frame.width as i32,
            yres: frame.height as i32,
            four_cc: if self.config.alpha {
                FOURCC_RGBA
            } else {
                FOURCC_RGBX
            },
            frame_rate_n: self.rate.0,
            frame_rate_d: self.rate.1,
            picture_aspect_ratio: 0.0,
            frame_format_type: PROGRESSIVE,
            timecode: TIMECODE_SYNTHESIZE,
            p_data: buf.as_ptr(),
            line_stride_in_bytes: frame.width as i32 * 4,
            p_metadata: std::ptr::null(),
            // When the channel shows the frame (100 ns since the Unix epoch):
            // receivers pace on it, evenly, not on when the send happened.
            timestamp: at
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| (d.as_nanos() / 100) as i64),
        };
        // SAFETY: synchronous send: NDI has finished with the buffer on return.
        unsafe { (self.lib.send)(self.instance, &video) };
        Ok(())
    }
}

impl Drop for NdiSender {
    fn drop(&mut self) {
        // SAFETY: created by this sender, destroyed once.
        unsafe { (self.lib.destroy)(self.instance) };
    }
}

// ------------------------------------------------------------- receive --

#[repr(C)]
struct Source {
    p_ndi_name: *const c_char,
    p_url_address: *const c_char,
}

#[repr(C)]
struct FindCreate {
    show_local_sources: bool,
    p_groups: *const c_char,
    p_extra_ips: *const c_char,
}

#[repr(C)]
struct RecvCreateV3 {
    source_to_connect_to: Source,
    color_format: i32,
    bandwidth: i32,
    allow_video_fields: bool,
    p_ndi_recv_name: *const c_char,
}

const RECV_COLOR_RGBX_RGBA: i32 = 2;
const RECV_BANDWIDTH_HIGHEST: i32 = 100;
const FRAME_TYPE_VIDEO: i32 = 1;
const FRAME_TYPE_ERROR: i32 = 4;

/// The receiving side of the runtime (looked up once the runtime is loaded).
struct RecvLib {
    find_create: unsafe extern "C" fn(*const FindCreate) -> Instance,
    find_wait: unsafe extern "C" fn(Instance, u32) -> bool,
    find_sources: unsafe extern "C" fn(Instance, *mut u32) -> *const Source,
    recv_create: unsafe extern "C" fn(*const RecvCreateV3) -> Instance,
    recv_capture:
        unsafe extern "C" fn(Instance, *mut VideoFrameV2, *mut c_void, *mut c_void, u32) -> i32,
    recv_free_video: unsafe extern "C" fn(Instance, *const VideoFrameV2),
    recv_destroy: unsafe extern "C" fn(Instance),
}

unsafe impl Send for RecvLib {}
unsafe impl Sync for RecvLib {}

fn recv_lib() -> Result<&'static RecvLib, String> {
    static RECV: OnceLock<Result<RecvLib, String>> = OnceLock::new();
    let lib = lib().map_err(|e| e.to_string())?;
    RECV.get_or_init(|| unsafe {
        let l = &lib._lib;
        let why = |e: libloading::Error| format!("this NDI runtime cannot receive ({e})");
        Ok(RecvLib {
            find_create: *l.get(b"NDIlib_find_create_v2\0").map_err(why)?,
            find_wait: *l.get(b"NDIlib_find_wait_for_sources\0").map_err(why)?,
            find_sources: *l.get(b"NDIlib_find_get_current_sources\0").map_err(why)?,
            recv_create: *l.get(b"NDIlib_recv_create_v3\0").map_err(why)?,
            recv_capture: *l.get(b"NDIlib_recv_capture_v2\0").map_err(why)?,
            recv_free_video: *l.get(b"NDIlib_recv_free_video_v2\0").map_err(why)?,
            recv_destroy: *l.get(b"NDIlib_recv_destroy\0").map_err(why)?,
        })
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// The finder of the server, kept for the life of the process: NDI
/// announces sources over several seconds (mDNS, Discovery Server), so a
/// finder made for each search sees only the first ones.
struct Finder(Instance);
// Used under the mutex only.
unsafe impl Send for Finder {}
static FINDER: std::sync::Mutex<Option<Finder>> = std::sync::Mutex::new(None);

/// The NDI sources seen on the network. The first search waits (at most
/// `wait_ms`) until the list stops changing for a second; later ones read
/// the list the finder keeps up to date, after a short look for news.
pub fn sources(wait_ms: u32) -> Result<Vec<String>, String> {
    let r = recv_lib()?;
    let mut finder = FINDER.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = finder.is_none();
    if fresh {
        let create = FindCreate {
            show_local_sources: true,
            p_groups: std::ptr::null(),
            p_extra_ips: std::ptr::null(),
        };
        // SAFETY: plain C call; the instance lives as long as the process.
        let find = unsafe { (r.find_create)(&create) };
        if find.is_null() {
            return Err("NDI cannot look for sources".into());
        }
        *finder = Some(Finder(find));
    }
    let find = finder.as_ref().expect("finder").0;
    let start = std::time::Instant::now();
    let budget =
        std::time::Duration::from_millis(if fresh { wait_ms.max(1000) } else { 1500 } as u64);
    // The first sources, then until nothing new for a second.
    let mut quiet = if fresh { wait_ms.max(1000) } else { 400 };
    loop {
        let left = budget.saturating_sub(start.elapsed()).as_millis() as u32;
        if left == 0 {
            break;
        }
        // SAFETY: the finder is alive (never destroyed).
        let changed = unsafe { (r.find_wait)(find, quiet.min(left)) };
        if !changed {
            break;
        }
        quiet = 1000;
    }
    let mut n = 0u32;
    // SAFETY: the list stays valid until the next call on this finder (under the lock).
    let mut out = Vec::new();
    unsafe {
        let list = (r.find_sources)(find, &mut n);
        for i in 0..n as usize {
            let s = &*list.add(i);
            if !s.p_ndi_name.is_null() {
                out.push(
                    std::ffi::CStr::from_ptr(s.p_ndi_name)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// A picture received over NDI: straight-alpha RGBA rows of `stride` bytes.
pub struct Received<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub alpha: bool,
    pub data: &'a [u8],
    /// Pictures per second as sent (frames, not fields); (0, 0) unknown.
    pub rate: (u32, u32),
    /// The sender's picture is made of two fields.
    pub interlaced: bool,
}

/// Receives the video of one NDI source.
pub struct NdiReceiver {
    lib: &'static RecvLib,
    instance: Instance,
}

// Used by the input thread that owns it only.
unsafe impl Send for NdiReceiver {}

impl NdiReceiver {
    pub fn open(source: &str) -> Result<Self, String> {
        let lib = recv_lib()?;
        let name = CString::new(source).map_err(|_| "invalid NDI source name".to_string())?;
        let recv_name = CString::new("Strata").unwrap_or_default();
        let create = RecvCreateV3 {
            source_to_connect_to: Source {
                p_ndi_name: name.as_ptr(),
                p_url_address: std::ptr::null(),
            },
            color_format: RECV_COLOR_RGBX_RGBA,
            bandwidth: RECV_BANDWIDTH_HIGHEST,
            allow_video_fields: false,
            p_ndi_recv_name: recv_name.as_ptr(),
        };
        // SAFETY: the strings outlive the call; NDI copies them.
        let instance = unsafe { (lib.recv_create)(&create) };
        if instance.is_null() {
            return Err("NDI could not create the receiver".into());
        }
        Ok(Self { lib, instance })
    }

    /// Waits up to `timeout_ms` for a picture and hands it to `f`. Ok(false):
    /// nothing came; Err: the connection failed.
    pub fn capture(&mut self, timeout_ms: u32, f: impl FnOnce(Received)) -> Result<bool, String> {
        let mut v = VideoFrameV2 {
            xres: 0,
            yres: 0,
            four_cc: 0,
            frame_rate_n: 0,
            frame_rate_d: 0,
            picture_aspect_ratio: 0.0,
            frame_format_type: 0,
            timecode: 0,
            p_data: std::ptr::null(),
            line_stride_in_bytes: 0,
            p_metadata: std::ptr::null(),
            timestamp: 0,
        };
        // SAFETY: the frame is freed with the runtime's own function after use.
        unsafe {
            let kind = (self.lib.recv_capture)(
                self.instance,
                &mut v,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                timeout_ms,
            );
            if kind == FRAME_TYPE_ERROR {
                return Err("the NDI connection was lost".into());
            }
            if kind != FRAME_TYPE_VIDEO {
                return Ok(false);
            }
            let ok = !v.p_data.is_null()
                && v.xres > 0
                && v.yres > 0
                && (v.four_cc == FOURCC_RGBA || v.four_cc == FOURCC_RGBX);
            if ok {
                let stride = v.line_stride_in_bytes.max(v.xres * 4) as usize;
                let data = std::slice::from_raw_parts(v.p_data, stride * v.yres as usize);
                f(Received {
                    width: v.xres as u32,
                    height: v.yres as u32,
                    stride,
                    alpha: v.four_cc == FOURCC_RGBA,
                    data,
                    rate: (v.frame_rate_n.max(0) as u32, v.frame_rate_d.max(0) as u32),
                    interlaced: v.frame_format_type != PROGRESSIVE,
                });
            }
            (self.lib.recv_free_video)(self.instance, &v);
            Ok(ok)
        }
    }
}

impl Drop for NdiReceiver {
    fn drop(&mut self) {
        // SAFETY: created by this receiver, destroyed once.
        unsafe { (self.lib.recv_destroy)(self.instance) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha() {
        let mut out = Vec::new();
        unpremultiply(&[64, 32, 0, 128, 10, 10, 10, 0], &mut out, true);
        assert_eq!(out, vec![128, 64, 0, 128, 10, 10, 10, 0]);
        unpremultiply(&[64, 32, 0, 128], &mut out, false);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn fourcc_codes() {
        assert_eq!(FOURCC_RGBA, 0x4142_4752);
    }
}
