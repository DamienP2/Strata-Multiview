//! Receiving SRT with libsrt itself (loaded at run time, the library ffmpeg
//! uses), for its statistics: round trip time, rate, packets lost, sent
//! again and dropped, the receiver's buffer. When libsrt is not found, the
//! inputs fall back to ffmpeg receiving SRT (no statistics).
//!
//! The bytes received go on to the decoder as a relay's would.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::Read;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

type Sock = c_int;
const INVALID: Sock = -1;

// srt.h, SRT_SOCKOPT.
const SRTO_RCVSYN: c_int = 2;
const SRTO_RCVTIMEO: c_int = 14;
const SRTO_LATENCY: c_int = 23;
const SRTO_PASSPHRASE: c_int = 26;
const SRTO_PBKEYLEN: c_int = 27;
const SRTO_CONNTIMEO: c_int = 36;
const SRTO_STREAMID: c_int = 46;

/// The beginning of SRT_TRACEBSTATS (srt.h, 1.4 / 1.5), padded: the
/// library writes the whole of its own, longer one. Checked against
/// libsrt 1.5 (`a_live_srt_stream_is_measured` and a dump of a live one).
#[repr(C)]
#[derive(Clone, Copy)]
struct Bstats {
    ms_time_stamp: i64,
    pkt_sent_total: i64,
    pkt_recv_total: i64,
    pkt_snd_loss_total: c_int,
    pkt_rcv_loss_total: c_int,
    pkt_retrans_total: c_int,
    pkt_sent_ack_total: c_int,
    pkt_recv_ack_total: c_int,
    pkt_sent_nak_total: c_int,
    pkt_recv_nak_total: c_int,
    us_snd_duration_total: i64,
    pkt_snd_drop_total: c_int,
    pkt_rcv_drop_total: c_int,
    pkt_rcv_undecrypt_total: c_int,
    byte_sent_total: u64,
    byte_recv_total: u64,
    byte_rcv_loss_total: u64,
    byte_retrans_total: u64,
    byte_snd_drop_total: u64,
    byte_rcv_drop_total: u64,
    byte_rcv_undecrypt_total: u64,
    pkt_sent: i64,
    pkt_recv: i64,
    pkt_snd_loss: c_int,
    pkt_rcv_loss: c_int,
    pkt_retrans: c_int,
    pkt_rcv_retrans: c_int,
    pkt_sent_ack: c_int,
    pkt_recv_ack: c_int,
    pkt_sent_nak: c_int,
    pkt_recv_nak: c_int,
    mbps_send_rate: f64,
    mbps_recv_rate: f64,
    us_snd_duration: i64,
    pkt_reorder_distance: c_int,
    pkt_rcv_avg_belated_time: f64,
    pkt_rcv_belated: i64,
    pkt_snd_drop: c_int,
    pkt_rcv_drop: c_int,
    pkt_rcv_undecrypt: c_int,
    byte_sent: u64,
    byte_recv: u64,
    byte_rcv_loss: u64,
    byte_retrans: u64,
    byte_snd_drop: u64,
    byte_rcv_drop: u64,
    byte_rcv_undecrypt: u64,
    us_pkt_snd_period: f64,
    pkt_flow_window: c_int,
    pkt_congestion_window: c_int,
    pkt_flight_size: c_int,
    ms_rtt: f64,
    mbps_bandwidth: f64,
    byte_avail_snd_buf: c_int,
    byte_avail_rcv_buf: c_int,
    mbps_max_bw: f64,
    byte_mss: c_int,
    pkt_snd_buf: c_int,
    byte_snd_buf: c_int,
    ms_snd_buf: c_int,
    ms_snd_tsbpd_delay: c_int,
    pkt_rcv_buf: c_int,
    byte_rcv_buf: c_int,
    ms_rcv_buf: c_int,
    ms_rcv_tsbpd_delay: c_int,
    _rest: [u8; 1024],
}

struct Lib {
    _lib: libloading::Library,
    create: unsafe extern "C" fn() -> Sock,
    setflag: unsafe extern "C" fn(Sock, c_int, *const c_void, c_int) -> c_int,
    bind: unsafe extern "C" fn(Sock, *const libc::sockaddr, c_int) -> c_int,
    listen: unsafe extern "C" fn(Sock, c_int) -> c_int,
    accept: unsafe extern "C" fn(Sock, *mut libc::sockaddr, *mut c_int) -> Sock,
    connect: unsafe extern "C" fn(Sock, *const libc::sockaddr, c_int) -> c_int,
    recvmsg: unsafe extern "C" fn(Sock, *mut c_char, c_int) -> c_int,
    close: unsafe extern "C" fn(Sock) -> c_int,
    bstats: unsafe extern "C" fn(Sock, *mut Bstats, c_int) -> c_int,
    error: unsafe extern "C" fn() -> *const c_char,
}

const NAMES: &[&str] = &[
    "libsrt.so.1.5",
    "libsrt-gnutls.so.1.5",
    "libsrt-openssl.so.1.5",
    "libsrt.so.1.4",
    "libsrt-gnutls.so.1.4",
    "libsrt-openssl.so.1.4",
    "libsrt.so",
    "libsrt.1.5.dylib",
    "libsrt.dylib",
    "/opt/homebrew/lib/libsrt.dylib",
    "/usr/local/lib/libsrt.dylib",
    "srt.dll",
    "libsrt.dll",
];

fn lib() -> Option<&'static Lib> {
    static LIB: OnceLock<Option<Lib>> = OnceLock::new();
    LIB.get_or_init(|| {
        if std::env::var("STRATA_SRT_NATIVE").is_ok_and(|v| v.trim() == "0") {
            return None;
        }
        for name in NAMES {
            // SAFETY: libsrt's own initialisation only.
            let Ok(l) = (unsafe { libloading::Library::new(name) }) else {
                continue;
            };
            // SAFETY: the symbols of srt.h, with their C signatures.
            let lib = unsafe {
                let startup: unsafe extern "C" fn() -> c_int = match l.get(b"srt_startup\0") {
                    Ok(s) => *s,
                    Err(_) => continue,
                };
                macro_rules! sym {
                    ($n:literal) => {
                        match l.get($n) {
                            Ok(s) => *s,
                            Err(_) => continue,
                        }
                    };
                }
                let lib = Lib {
                    create: sym!(b"srt_create_socket\0"),
                    setflag: sym!(b"srt_setsockflag\0"),
                    bind: sym!(b"srt_bind\0"),
                    listen: sym!(b"srt_listen\0"),
                    accept: sym!(b"srt_accept\0"),
                    connect: sym!(b"srt_connect\0"),
                    recvmsg: sym!(b"srt_recvmsg\0"),
                    close: sym!(b"srt_close\0"),
                    bstats: sym!(b"srt_bstats\0"),
                    error: sym!(b"srt_getlasterror_str\0"),
                    _lib: l,
                };
                if startup() < 0 {
                    continue;
                }
                // Its own log: critical only (an accept that waits logs an
                // error every try).
                if let Ok(f) = lib
                    ._lib
                    .get::<unsafe extern "C" fn(c_int)>(b"srt_setloglevel\0")
                {
                    f(2);
                }
                lib
            };
            tracing::info!("SRT inputs received by {name} (with their statistics)");
            return Some(lib);
        }
        None
    })
    .as_ref()
}

/// libsrt can be used.
pub fn available() -> bool {
    lib().is_some()
}

fn err(l: &Lib) -> String {
    // SAFETY: a static string of libsrt (thread local).
    unsafe { CStr::from_ptr((l.error)()) }
        .to_string_lossy()
        .into_owned()
}

/// What an SRT address asks for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub listener: bool,
    pub latency_ms: u32,
    pub passphrase: String,
    pub pbkeylen: u32,
    pub streamid: String,
}

/// `srt://host:port?mode=listener&latency=120000&passphrase=…&streamid=…`
/// (ffmpeg's form: latency in microseconds).
pub fn parse(url: &str) -> Option<Target> {
    let rest = url.trim().strip_prefix("srt://")?;
    let (addr, query) = rest.split_once('?').unwrap_or((rest, ""));
    let addr = addr.trim_end_matches('/');
    let (host, port) = addr.rsplit_once(':')?;
    let mut t = Target {
        host: host.trim_matches(['[', ']']).to_string(),
        port: port.parse().ok()?,
        latency_ms: 120,
        ..Default::default()
    };
    for kv in query.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let v = percent_decode(v);
        match k {
            "mode" => t.listener = v == "listener" || v == "server",
            "latency" | "rcvlatency" => {
                t.latency_ms = v.parse::<u64>().map_or(120, |us| (us / 1000).max(1) as u32)
            }
            "passphrase" => t.passphrase = v,
            "pbkeylen" => t.pbkeylen = v.parse().unwrap_or(0),
            "streamid" => t.streamid = v,
            _ => {}
        }
    }
    if t.host.is_empty() {
        t.host = "0.0.0.0".into();
        t.listener = true;
    }
    Some(t)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The statistics of an SRT input, as the pages and Prometheus get them.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub connected: bool,
    pub peer: String,
    pub rtt_ms: f64,
    pub rate_mbps: f64,
    /// Since connected: packets received, lost (then mostly sent again),
    /// sent again, dropped (too late: missing from the picture).
    pub received: i64,
    pub lost: i64,
    pub retransmitted: i64,
    pub dropped: i64,
    /// In the last second.
    pub lost_per_s: i64,
    pub dropped_per_s: i64,
    /// In the last ten seconds: lost (%) and dropped.
    pub loss_pct_10s: f64,
    pub dropped_10s: i64,
    /// What the receiver holds, ms (its latency).
    pub buffer_ms: i64,
    pub latency_ms: i64,
}

fn sockaddr(a: &SocketAddr) -> (libc::sockaddr_storage, c_int) {
    // SAFETY: zeroed sockaddr_storage, then filled as the family says.
    let mut st: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let len = match a {
        SocketAddr::V4(v4) => {
            let sin = libc::sockaddr_in {
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
                sin_len: std::mem::size_of::<libc::sockaddr_in>() as u8,
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr {
                    s_addr: u32::from_ne_bytes(v4.ip().octets()),
                },
                sin_zero: [0; 8],
            };
            // SAFETY: sockaddr_in fits in sockaddr_storage.
            unsafe { std::ptr::write(&mut st as *mut _ as *mut libc::sockaddr_in, sin) };
            std::mem::size_of::<libc::sockaddr_in>()
        }
        SocketAddr::V6(v6) => {
            // SAFETY: zeroed, then its fields set.
            let mut sin6: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
            {
                sin6.sin6_len = std::mem::size_of::<libc::sockaddr_in6>() as u8;
            }
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_addr = libc::in6_addr {
                s6_addr: v6.ip().octets(),
            };
            // SAFETY: sockaddr_in6 fits in sockaddr_storage.
            unsafe { std::ptr::write(&mut st as *mut _ as *mut libc::sockaddr_in6, sin6) };
            std::mem::size_of::<libc::sockaddr_in6>()
        }
    };
    (st, len as c_int)
}

fn set_int(l: &Lib, s: Sock, opt: c_int, v: c_int) {
    // SAFETY: an int option of libsrt.
    unsafe {
        (l.setflag)(
            s,
            opt,
            &v as *const c_int as *const c_void,
            std::mem::size_of::<c_int>() as c_int,
        )
    };
}

fn set_str(l: &Lib, s: Sock, opt: c_int, v: &str) -> Result<(), String> {
    let c = CString::new(v).map_err(|e| e.to_string())?;
    // SAFETY: a string option of libsrt (its bytes, without the NUL).
    let r = unsafe { (l.setflag)(s, opt, c.as_ptr() as *const c_void, v.len() as c_int) };
    if r < 0 {
        return Err(err(l));
    }
    Ok(())
}

fn configure(l: &Lib, s: Sock, t: &Target) -> Result<(), String> {
    set_int(l, s, SRTO_LATENCY, t.latency_ms as c_int);
    set_int(l, s, SRTO_CONNTIMEO, 3000);
    if !t.passphrase.is_empty() {
        set_str(l, s, SRTO_PASSPHRASE, &t.passphrase)?;
        if t.pbkeylen > 0 {
            set_int(l, s, SRTO_PBKEYLEN, t.pbkeylen as c_int);
        }
    }
    if !t.streamid.is_empty() {
        set_str(l, s, SRTO_STREAMID, &t.streamid)?;
    }
    Ok(())
}

/// A connected SRT socket: its bytes, and its statistics kept up to date.
pub struct Receiver {
    sock: Sock,
    stats: Arc<Mutex<Stats>>,
    last: Option<(Instant, Bstats)>,
    peer: String,
    /// (received, lost, dropped) totals of the last ten seconds.
    window: std::collections::VecDeque<(i64, i64, i64)>,
}

impl Receiver {
    /// Connects (caller) or waits for the sender (listener) until `stop`.
    pub fn open(
        t: &Target,
        stats: Arc<Mutex<Stats>>,
        stop: impl Fn() -> bool,
    ) -> Result<Self, String> {
        let l = lib().ok_or("libsrt not found")?;
        let addr = (t.host.as_str(), t.port)
            .to_socket_addrs()
            .map_err(|e| format!("{}: {e}", t.host))?
            .next()
            .ok_or_else(|| format!("{}: no address", t.host))?;
        // SAFETY: libsrt calls on sockets it made; closed on every way out.
        unsafe {
            let s = (l.create)();
            if s == INVALID {
                return Err(err(l));
            }
            if let Err(e) = configure(l, s, t) {
                (l.close)(s);
                return Err(e);
            }
            let (sa, len) = sockaddr(&addr);
            if t.listener {
                if (l.bind)(s, &sa as *const _ as *const libc::sockaddr, len) < 0
                    || (l.listen)(s, 1) < 0
                {
                    let e = err(l);
                    (l.close)(s);
                    return Err(e);
                }
                // Accept without blocking, to see `stop`.
                set_int(l, s, SRTO_RCVSYN, 0);
                loop {
                    let mut peer: libc::sockaddr_storage = std::mem::zeroed();
                    let mut plen = std::mem::size_of::<libc::sockaddr_storage>() as c_int;
                    let c = (l.accept)(s, &mut peer as *mut _ as *mut libc::sockaddr, &mut plen);
                    if c != INVALID {
                        (l.close)(s);
                        set_int(l, c, SRTO_RCVSYN, 1);
                        set_int(l, c, SRTO_RCVTIMEO, 500);
                        return Ok(Self::new(c, stats, peer_name(&peer)));
                    }
                    if stop() {
                        (l.close)(s);
                        return Err("stopped".into());
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if (l.connect)(s, &sa as *const _ as *const libc::sockaddr, len) < 0 {
                let e = err(l);
                (l.close)(s);
                return Err(e);
            }
            set_int(l, s, SRTO_RCVTIMEO, 500);
            Ok(Self::new(s, stats, addr.to_string()))
        }
    }

    fn new(sock: Sock, stats: Arc<Mutex<Stats>>, peer: String) -> Self {
        let mut r = Self {
            sock,
            stats,
            last: None,
            peer,
            window: Default::default(),
        };
        r.measure();
        r
    }

    /// Reads the statistics again (each second).
    fn measure(&mut self) {
        let Some(l) = lib() else { return };
        // SAFETY: zeroed, then filled by libsrt (at most its own size, which
        // the padding covers).
        let mut b: Bstats = unsafe { std::mem::zeroed() };
        if unsafe { (l.bstats)(self.sock, &mut b, 0) } < 0 {
            return;
        }
        let now = Instant::now();
        let (lost_s, dropped_s) = match self.last {
            Some((_, p)) => (
                (b.pkt_rcv_loss_total - p.pkt_rcv_loss_total).max(0) as i64,
                (b.pkt_rcv_drop_total - p.pkt_rcv_drop_total).max(0) as i64,
            ),
            None => (0, 0),
        };
        let r1 = |v: f64| (v * 10.0).round() / 10.0;
        self.window.push_back((
            b.pkt_recv_total,
            b.pkt_rcv_loss_total as i64,
            b.pkt_rcv_drop_total as i64,
        ));
        while self.window.len() > 11 {
            self.window.pop_front();
        }
        let (first, last) = (
            self.window.front().copied().unwrap_or_default(),
            self.window.back().copied().unwrap_or_default(),
        );
        let (recv10, lost10, drop10) = (last.0 - first.0, last.1 - first.1, last.2 - first.2);
        *self.stats.lock().unwrap_or_else(|p| p.into_inner()) = Stats {
            connected: true,
            peer: self.peer.clone(),
            rtt_ms: r1(b.ms_rtt),
            rate_mbps: (b.mbps_recv_rate * 100.0).round() / 100.0,
            received: b.pkt_recv_total,
            lost: b.pkt_rcv_loss_total as i64,
            retransmitted: b.pkt_rcv_retrans as i64,
            dropped: b.pkt_rcv_drop_total as i64,
            lost_per_s: lost_s,
            dropped_per_s: dropped_s,
            loss_pct_10s: if recv10 + lost10 > 0 {
                (lost10 as f64 * 1000.0 / (recv10 + lost10) as f64).round() / 10.0
            } else {
                0.0
            },
            dropped_10s: drop10.max(0),
            buffer_ms: b.ms_rcv_buf as i64,
            latency_ms: b.ms_rcv_tsbpd_delay as i64,
        };
        self.last = Some((now, b));
    }
}

fn peer_name(st: &libc::sockaddr_storage) -> String {
    // SAFETY: read as the family it says.
    unsafe {
        match st.ss_family as c_int {
            libc::AF_INET => {
                let a = &*(st as *const _ as *const libc::sockaddr_in);
                let ip = std::net::Ipv4Addr::from(u32::from_be(a.sin_addr.s_addr));
                format!("{ip}:{}", u16::from_be(a.sin_port))
            }
            libc::AF_INET6 => {
                let a = &*(st as *const _ as *const libc::sockaddr_in6);
                format!(
                    "[{}]:{}",
                    std::net::Ipv6Addr::from(a.sin6_addr.s6_addr),
                    u16::from_be(a.sin6_port)
                )
            }
            _ => String::new(),
        }
    }
}

impl Read for Receiver {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let l = lib().ok_or_else(|| std::io::Error::other("libsrt"))?;
        if self
            .last
            .is_none_or(|(t, _)| t.elapsed() >= Duration::from_secs(1))
        {
            self.measure();
        }
        loop {
            // SAFETY: up to `buf.len()` bytes written by libsrt.
            let n = unsafe {
                (l.recvmsg)(
                    self.sock,
                    buf.as_mut_ptr() as *mut c_char,
                    buf.len().min(i32::MAX as usize) as c_int,
                )
            };
            if n > 0 {
                return Ok(n as usize);
            }
            let e = err(l);
            // Nothing for 500 ms (the sender pauses): try again, measured.
            if e.contains("timeout") || e.contains("Timeout") || e.contains("again") {
                self.measure();
                continue;
            }
            return Ok(0);
        }
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        if let Some(l) = lib() {
            // SAFETY: our socket, closed once.
            unsafe { (l.close)(self.sock) };
        }
        self.stats
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .connected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        let t = parse("srt://0.0.0.0:10001?mode=listener&latency=120000").unwrap();
        assert!(t.listener);
        assert_eq!(
            (t.host.as_str(), t.port, t.latency_ms),
            ("0.0.0.0", 10001, 120)
        );
        let t = parse(
            "srt://enc.example.com:9000?passphrase=a%20b%26c&pbkeylen=16&streamid=#!::r=live",
        )
        .unwrap();
        assert!(!t.listener);
        assert_eq!(t.passphrase, "a b&c");
        assert_eq!(t.pbkeylen, 16);
        assert_eq!(t.streamid, "#!::r=live");
        assert!(parse("udp://x:1").is_none());
        let t = parse("srt://:5000").unwrap();
        assert!(t.listener);
    }

    #[test]
    fn stats_layout() {
        // Where libsrt 1.5 writes them (a dump of a live stream, x86-64).
        let b: Bstats = unsafe { std::mem::zeroed() };
        let at = |p: *const u8| p as usize - &b as *const _ as usize;
        assert_eq!(at(&b.pkt_recv_total as *const _ as _), 16);
        assert_eq!(at(&b.byte_recv_total as *const _ as _), 88);
        assert_eq!(at(&b.pkt_recv as *const _ as _), 144);
        assert_eq!(at(&b.mbps_recv_rate as *const _ as _), 192);
        assert_eq!(at(&b.byte_recv as *const _ as _), 256);
        assert_eq!(at(&b.ms_rtt as *const _ as _), 328);
        assert_eq!(at(&b.ms_rcv_buf as *const _ as _), 388);
        assert_eq!(at(&b.ms_rcv_tsbpd_delay as *const _ as _), 392);
    }
}
