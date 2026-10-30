//! SMPTE ST 2110-20 (uncompressed video over RTP, RFC 4175) sender.
//!
//! Fill and key are sent as two separate essence streams (one thread and
//! socket each), as broadcast keyers expect. This is a pure software implementation:
//! - packets are paced evenly across the frame period (close to the 2110-21
//!   "wide" sender model, gapped: sent within 90 % of it), in bursts of
//!   [`BURST`] packets: one system call each on Linux (`sendmmsg`, and UDP
//!   segmentation offload when the kernel has it), one per packet elsewhere;
//! - packets are as large as ST 2110-10's standard UDP size limit allows,
//!   each line split evenly in whole pixel groups (fewer packets per frame);
//! - each stream is packed by one thread and sent on each network by one
//!   thread per leg;
//! - RTP timestamps derive from the system clock. For production use the host
//!   must be PTP-disciplined (ptp4l + phc2sys) so system time follows the
//!   facility grandmaster; they count TAI (system time + `tai_offset`), as
//!   ST 2110-10 asks. Kernel-bypass NICs (DPDK / Rivermax) are on the
//!   roadmap for tighter pacing at UHD rates;
//! - SMPTE 2022-7: with a blue leg, every packet goes out on both networks
//!   (same sequence numbers and timestamps), so a receiver loses nothing
//!   when one network fails.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cg_render::Frame;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};

use crate::colorspace::{pack_line_4224_10, pack_line_422_10, pack_line_rgba8};
use crate::{OutputError, OutputSink};

/// RTP payload bytes per packet of [`packetize`]. 1200 = 240 pgroups (480 px),
/// fits a 1500 MTU with RTP + RFC 4175 headers and divides 1920/1280/3840
/// line widths.
const PAYLOAD_BYTES: usize = 1200;
/// Largest UDP payload a sender uses: ST 2110-10's standard UDP size limit
/// (fits a 1500 bytes MTU with the IP and UDP headers). Less when the
/// network's MTU is smaller ([`path_mtu`]): packets are never fragmented.
const MAX_UDP_BYTES: usize = 1460;
/// IPv4 and UDP headers.
const IP_UDP_BYTES: usize = 28;

/// How [`packetize_into`] cuts lines into packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Payloads of this many bytes (whole pgroups), the last of a line shorter.
    Fixed(usize),
    /// As few packets per line as UDP payloads of `max_udp` bytes allow, all
    /// of the same size (whole pgroups).
    Even { max_udp: usize },
}
/// Packets sent at once before pacing (one `sendmmsg` on Linux).
const BURST: usize = 64;
/// RTP header (12 bytes) and RFC 4175 payload header (8, one line segment).
const HEADER_BYTES: usize = 20;
const RTP_CLOCK: f64 = 90_000.0;

/// How the picture and its alpha travel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum St2110Encoding {
    /// Two ST 2110-20 streams: YCbCr 4:2:2 10-bit fill and its key.
    #[default]
    FillKey,
    /// One stream, RGBA 4:4:4:4 8-bit.
    Rgba,
    /// One stream, YCbCr + alpha 4:2:2:4 10-bit.
    Ycbcra,
}

/// One RTP stream of a sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Essence {
    Fill,
    Key,
    Rgba,
    Ycbcra,
}

impl Essence {
    /// (bytes, pixels) of one pgroup.
    fn pgroup(self) -> (usize, usize) {
        match self {
            Essence::Fill | Essence::Key => (5, 2),
            Essence::Rgba => (4, 1),
            Essence::Ycbcra => (15, 4),
        }
    }

    fn pack(self, rgba: &[u8], out: &mut Vec<u8>) {
        match self {
            Essence::Fill => pack_line_422_10(rgba, out, false),
            Essence::Key => pack_line_422_10(rgba, out, true),
            Essence::Rgba => pack_line_rgba8(rgba, out),
            Essence::Ycbcra => pack_line_4224_10(rgba, out),
        }
    }

    /// SDP `sampling` and `depth`.
    fn sampling(self) -> (&'static str, u8) {
        match self {
            Essence::Fill => ("YCbCr-4:2:2", 10),
            Essence::Key => ("KEY", 10),
            Essence::Rgba => ("RGBA", 8),
            Essence::Ycbcra => ("YCbCrA-4:2:2:4", 10),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Essence::Fill => "fill",
            Essence::Key => "key",
            Essence::Rgba => "rgba",
            Essence::Ycbcra => "ycbcra",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct St2110Config {
    /// Stable id (layout, status, SDP links).
    #[serde(default)]
    pub id: String,
    /// Off: kept in the settings, not started.
    #[serde(default = "crate::on")]
    pub enabled: bool,
    pub name: String,
    /// The flow of the Hardware settings this sends ("" = none): the server
    /// fills the fields below from it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hardware: String,
    #[serde(default)]
    pub encoding: St2110Encoding,
    /// Multicast group (or unicast) for the fill stream.
    pub fill: SocketAddrV4,
    /// Key (alpha) stream of the fill + key encoding.
    #[serde(default)]
    pub key: Option<SocketAddrV4>,
    /// Local interface address to send from.
    #[serde(default)]
    pub interface: Option<Ipv4Addr>,
    /// SMPTE 2022-7 copy on the secondary (blue) network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blue: Option<Leg>,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    #[serde(default = "default_pt")]
    pub payload_type: u8,
    /// TAI − UTC in seconds: RTP timestamps count TAI.
    #[serde(default = "default_tai")]
    pub tai_offset: u32,
}

/// The blue (2022-7 secondary) destinations of a sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leg {
    pub fill: SocketAddrV4,
    #[serde(default)]
    pub key: Option<SocketAddrV4>,
    #[serde(default)]
    pub interface: Option<Ipv4Addr>,
}

fn default_ttl() -> u32 {
    32
}
fn default_pt() -> u8 {
    96
}
fn default_tai() -> u32 {
    37
}

/// One RTP stream of a sender and where it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamDest {
    pub essence: Essence,
    pub red: SocketAddrV4,
    /// Its 2022-7 copy.
    pub blue: Option<SocketAddrV4>,
}

impl St2110Config {
    /// The streams this sender sends, with their destinations.
    pub fn streams(&self) -> Vec<StreamDest> {
        let blue = |key: bool| {
            self.blue
                .and_then(|l| if key { l.key } else { Some(l.fill) })
        };
        let one = |essence| StreamDest {
            essence,
            red: self.fill,
            blue: blue(false),
        };
        match self.encoding {
            St2110Encoding::FillKey => std::iter::once(one(Essence::Fill))
                .chain(self.key.map(|k| StreamDest {
                    essence: Essence::Key,
                    red: k,
                    blue: blue(true),
                }))
                .collect(),
            St2110Encoding::Rgba => vec![one(Essence::Rgba)],
            St2110Encoding::Ycbcra => vec![one(Essence::Ycbcra)],
        }
    }

    /// Session description for one stream (`key = true`: the key of a
    /// fill + key pair; otherwise the main stream). With a blue leg it
    /// describes both paths (RFC 7104 duplication, as 2022-7 receivers expect).
    pub fn sdp(&self, width: u32, height: u32, rate: (u32, u32), key: bool) -> Option<String> {
        let d = self
            .streams()
            .into_iter()
            .find(|d| (d.essence == Essence::Key) == key)?;
        let rate = if rate.1 == 1 {
            rate.0.to_string()
        } else {
            format!("{}/{}", rate.0, rate.1)
        };
        let (sampling, depth) = d.essence.sampling();
        let red_src = self.interface.unwrap_or(Ipv4Addr::UNSPECIFIED);
        let media = |dest: SocketAddrV4, src: Ipv4Addr, mid: Option<&str>| {
            format!(
                "m=video {port} RTP/AVP {pt}\r\n\
                 c=IN IP4 {ip}/{ttl}\r\n\
                 a=source-filter: incl IN IP4 {ip} {src}\r\n\
                 a=rtpmap:{pt} raw/90000\r\n\
                 a=fmtp:{pt} sampling={sampling}; width={width}; height={height}; exactframerate={rate}; depth={depth}; TCS=SDR; colorimetry=BT709; PM=2110GPM; SSN=ST2110-20:2017; TP=2110TPW; \r\n\
                 a=ts-refclk:ptp=IEEE1588-2008:traceable\r\n\
                 a=mediaclk:direct=0\r\n{mid}",
                port = dest.port(),
                pt = self.payload_type,
                ip = dest.ip(),
                ttl = self.ttl,
                mid = mid.map_or(String::new(), |m| format!("a=mid:{m}\r\n")),
            )
        };
        let mut sdp = format!(
            "v=0\r\n\
             o=- {sess} 0 IN IP4 {red_src}\r\n\
             s={name} ({label})\r\n\
             t=0 0\r\n",
            sess = u32::from(d.red.ip().to_owned()) ^ d.red.port() as u32,
            name = self.name,
            label = d.essence.label(),
        );
        match d.blue {
            Some(blue) => {
                let blue_src = self
                    .blue
                    .and_then(|l| l.interface)
                    .unwrap_or(Ipv4Addr::UNSPECIFIED);
                sdp.push_str("a=group:DUP primary secondary\r\n");
                sdp.push_str(&media(d.red, red_src, Some("primary")));
                sdp.push_str(&media(blue, blue_src, Some("secondary")));
            }
            None => sdp.push_str(&media(d.red, red_src, None)),
        }
        Some(sdp)
    }
}

/// RTP packets of one frame, stored back to back in a single buffer.
#[derive(Default, Clone)]
pub struct Packets {
    buf: Vec<u8>,
    ends: Vec<usize>,
}

impl Packets {
    pub fn len(&self) -> usize {
        self.ends.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
    pub fn get(&self, i: usize) -> &[u8] {
        let start = if i == 0 { 0 } else { self.ends[i - 1] };
        &self.buf[start..self.ends[i]]
    }
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        (0..self.len()).map(|i| self.get(i))
    }
    /// Where packet `i` starts in the buffer.
    #[cfg(target_os = "linux")]
    fn start(&self, i: usize) -> usize {
        if i == 0 {
            0
        } else {
            self.ends[i - 1]
        }
    }
}

/// RTP payload bytes per packet for lines of `line_bytes` in pgroups of
/// `pg_bytes`: the line split in as few packets as UDP payloads of `max_udp`
/// bytes allow, all of the same size (in whole pgroups, ST 2110-20).
pub fn even_payload(line_bytes: usize, pg_bytes: usize, max_udp: usize) -> usize {
    let max = (max_udp.saturating_sub(HEADER_BYTES) / pg_bytes * pg_bytes).max(pg_bytes);
    let groups = line_bytes.div_ceil(pg_bytes);
    if groups == 0 {
        return max;
    }
    let n = (groups * pg_bytes).div_ceil(max);
    groups.div_ceil(n) * pg_bytes
}

/// Splits a frame into RFC 4175 RTP packets (single line segment per
/// packet), lines cut as `split` says, in whole pgroups.
#[allow(clippy::too_many_arguments)]
pub fn packetize_into(
    frame: &Frame,
    essence: Essence,
    pt: u8,
    seq: &mut u32,
    timestamp: u32,
    split: Split,
    out: &mut Packets,
    line: &mut Vec<u8>,
) {
    let (w, h) = (frame.width as usize, frame.height as usize);
    out.buf.clear();
    out.ends.clear();
    for y in 0..h {
        essence.pack(&frame.data[y * w * 4..(y + 1) * w * 4], line);
        let (pg_bytes, pg_px) = essence.pgroup();
        let most = match split {
            Split::Even { max_udp } => even_payload(line.len(), pg_bytes, max_udp),
            Split::Fixed(payload) => (payload / pg_bytes * pg_bytes).max(pg_bytes),
        };
        let mut offset_bytes = 0;
        while offset_bytes < line.len() {
            let len = most.min(line.len() - offset_bytes);
            let last = y == h - 1 && offset_bytes + len >= line.len();
            let p = &mut out.buf;
            // RTP header
            p.push(0x80);
            p.push(if last { 0x80 | pt } else { pt });
            p.extend_from_slice(&(*seq as u16).to_be_bytes());
            p.extend_from_slice(&timestamp.to_be_bytes());
            p.extend_from_slice(&0x2110_0000u32.to_be_bytes()); // SSRC
                                                                // RFC 4175 payload header
            p.extend_from_slice(&((*seq >> 16) as u16).to_be_bytes()); // extended seq
            p.extend_from_slice(&(len as u16).to_be_bytes());
            p.extend_from_slice(&((y as u16) & 0x7fff).to_be_bytes()); // F=0, line
            let pixel_offset = (offset_bytes / pg_bytes * pg_px) as u16;
            p.extend_from_slice(&(pixel_offset & 0x7fff).to_be_bytes()); // C=0
            p.extend_from_slice(&line[offset_bytes..offset_bytes + len]);
            out.ends.push(p.len());
            *seq = seq.wrapping_add(1);
            offset_bytes += len;
        }
    }
}

/// The packets of the picture sent last (`out`) made the packets of `frame`
/// (same size), whose rows `rows` changed: those lines packed again, in
/// place, then every packet restamped. False when the layout differs
/// (`packetize_into` is then needed).
pub fn repacketize(
    frame: &Frame,
    essence: Essence,
    rows: &[bool],
    seq: &mut u32,
    timestamp: u32,
    out: &mut Packets,
    line: &mut Vec<u8>,
) -> bool {
    let (w, h) = (frame.width as usize, frame.height as usize);
    if rows.len() != h || h == 0 || !out.ends.len().is_multiple_of(h) {
        return false;
    }
    let per_line = out.ends.len() / h;
    for (y, _) in rows.iter().enumerate().filter(|(_, c)| **c) {
        essence.pack(&frame.data[y * w * 4..(y + 1) * w * 4], line);
        let mut offset = 0;
        for k in y * per_line..(y + 1) * per_line {
            let start = if k == 0 { 0 } else { out.ends[k - 1] } + HEADER_BYTES;
            let end = out.ends[k];
            let len = end - start;
            if offset + len > line.len() {
                return false;
            }
            out.buf[start..end].copy_from_slice(&line[offset..offset + len]);
            offset += len;
        }
        if offset != line.len() {
            return false;
        }
    }
    restamp(out, seq, timestamp);
    true
}

/// The packets of the picture sent last, for the same picture again: only
/// their sequence numbers and timestamp change (what `packetize_into` would
/// make, without converting the picture again).
pub fn restamp(out: &mut Packets, seq: &mut u32, timestamp: u32) {
    let mut start = 0;
    for &end in &out.ends {
        let p = &mut out.buf[start..end];
        p[2..4].copy_from_slice(&(*seq as u16).to_be_bytes());
        p[4..8].copy_from_slice(&timestamp.to_be_bytes());
        p[12..14].copy_from_slice(&((*seq >> 16) as u16).to_be_bytes());
        *seq = seq.wrapping_add(1);
        start = end;
    }
}

pub fn packetize(
    frame: &Frame,
    essence: Essence,
    pt: u8,
    seq: &mut u32,
    timestamp: u32,
) -> Packets {
    let mut out = Packets::default();
    packetize_into(
        frame,
        essence,
        pt,
        seq,
        timestamp,
        Split::Fixed(PAYLOAD_BYTES),
        &mut out,
        &mut Vec::new(),
    );
    out
}

fn open_socket(ttl: u32, interface: Option<Ipv4Addr>) -> std::io::Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_multicast_ttl_v4(ttl)?;
    if let Some(ip) = interface {
        s.set_multicast_if_v4(&ip)?;
        s.bind(&SocketAddrV4::new(ip, 0).into())?;
    }
    s.set_send_buffer_size(8 << 20).ok();
    Ok(s.into())
}

/// The MTU of the route to `dest` from `interface`, when the system tells
/// (Linux: `IP_MTU` of a socket connected there).
#[cfg(target_os = "linux")]
fn path_mtu(dest: SocketAddrV4, interface: Option<Ipv4Addr>) -> Option<usize> {
    use std::os::fd::AsRawFd;
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).ok()?;
    if let Some(ip) = interface {
        s.set_multicast_if_v4(&ip).ok()?;
        s.bind(&SocketAddrV4::new(ip, 0).into()).ok()?;
    }
    s.connect(&dest.into()).ok()?;
    let mut mtu: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: an int option into an int.
    let r = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_MTU,
            &mut mtu as *mut libc::c_int as *mut libc::c_void,
            &mut len,
        )
    };
    (r == 0 && mtu > 0).then_some(mtu as usize)
}

#[cfg(not(target_os = "linux"))]
fn path_mtu(_dest: SocketAddrV4, _interface: Option<Ipv4Addr>) -> Option<usize> {
    None
}

/// The largest UDP payload for packets going to `dests` (destination,
/// interface): the standard UDP size limit, less on a network whose MTU is
/// smaller (no IP fragments).
fn max_udp(dests: &[(SocketAddrV4, Option<Ipv4Addr>)]) -> usize {
    dests
        .iter()
        .filter_map(|(d, i)| path_mtu(*d, *i))
        .map(|mtu| mtu.saturating_sub(IP_UDP_BYTES))
        .fold(MAX_UDP_BYTES, usize::min)
        .max(HEADER_BYTES + 64)
}

/// One stream = one packing thread, and one sending (pacing) thread per
/// network leg.
struct Stream {
    tx: SyncSender<(Arc<Frame>, SystemTime)>,
}

pub struct St2110Sender {
    essences: Vec<Stream>,
    config: St2110Config,
    /// µs its threads spent packetizing and sending (not pacing).
    busy_us: Arc<AtomicU64>,
}

impl St2110Sender {
    pub fn start(config: St2110Config, frame_period: Duration) -> Result<Self, OutputError> {
        let mut essences = Vec::new();
        let busy_us = Arc::new(AtomicU64::new(0));
        // UDP segmentation offload, unless switched off (STRATA_ST2110_GSO=0).
        let gso = std::env::var("STRATA_ST2110_GSO").map_or(true, |v| v.trim() != "0");
        for d in config.streams() {
            let mut legs = vec![(open_socket(config.ttl, config.interface)?, d.red, "red")];
            let mut dests = vec![(d.red, config.interface)];
            if let Some(blue) = d.blue {
                let interface = config.blue.and_then(|l| l.interface);
                legs.push((open_socket(config.ttl, interface)?, blue, "blue"));
                dests.push((blue, interface));
            }
            let max_udp = max_udp(&dests);
            tracing::debug!(max_udp, "ST 2110 packet size");
            let essence = d.essence;
            // One thread per leg: the 2022-7 copies go out side by side.
            let mut senders = Vec::new();
            for (socket, dest, leg) in legs {
                let (tx, rx) = sync_channel::<Job>(1);
                let busy = busy_us.clone();
                std::thread::Builder::new()
                    .name(format!("st2110-{}-{leg}", essence.label()))
                    .spawn(move || send_leg(socket, dest, gso, rx, &busy))
                    .map_err(OutputError::Io)?;
                senders.push(tx);
            }
            let (tx, rx) = sync_channel::<(Arc<Frame>, SystemTime)>(2);
            let send = Sending {
                name: config.name.clone(),
                legs: senders,
                max_udp,
                essence,
                pt: config.payload_type,
                tai: Duration::from_secs(config.tai_offset.into()),
                period: frame_period,
            };
            let busy = busy_us.clone();
            std::thread::Builder::new()
                .name(format!("st2110-{}-{}", config.name, essence.label()))
                .spawn(move || run(send, rx, &busy))
                .map_err(OutputError::Io)?;
            essences.push(Stream { tx });
        }
        tracing::info!(fill = %config.fill, key = ?config.key, blue = ?config.blue, encoding = ?config.encoding, "ST 2110-20 sender started");
        Ok(Self {
            essences,
            config,
            busy_us,
        })
    }

    pub fn config(&self) -> &St2110Config {
        &self.config
    }
}

/// What one stream's packing thread packs, and the legs it hands it to.
struct Sending {
    name: String,
    /// Red, then blue (2022-7).
    legs: Vec<SyncSender<Job>>,
    /// UDP payload bytes at most.
    max_udp: usize,
    essence: Essence,
    pt: u8,
    /// TAI − UTC.
    tai: Duration,
    period: Duration,
}

/// The packets of a frame for a leg to send, paced over `budget` from `start`.
struct Job {
    packets: Arc<Packets>,
    start: Instant,
    budget: Duration,
}

fn run(send: Sending, rx: Receiver<(Arc<Frame>, SystemTime)>, busy: &AtomicU64) {
    let Sending {
        name,
        legs,
        max_udp,
        essence,
        pt,
        tai,
        period,
    } = send;
    let mut seq = 0u32;
    let (mut packets, mut line) = (Arc::new(Packets::default()), Vec::new());
    let mut last: Option<Arc<Frame>> = None;
    let (mut frames, mut stats_at) = (0u32, Instant::now());
    while let Ok((frame, at)) = rx.recv() {
        let start = Instant::now();
        frames += 1;
        if stats_at.elapsed() >= Duration::from_secs(5) {
            tracing::debug!(
                name,
                stream = essence.label(),
                fps = frames as f64 / stats_at.elapsed().as_secs_f64(),
                "ST 2110 send rate"
            );
            (frames, stats_at) = (0, Instant::now());
        }
        // Fill and key threads derive the same timestamp from the frame's
        // capture time so receivers can pair them.
        let ts = rtp_timestamp(frame_time(at) + tai);
        // The legs are done with the packets of the frame before (else they
        // are copied: a leg still sending keeps its own).
        let pk = Arc::make_mut(&mut packets);
        // The same picture as the last one (nothing moved): its packets
        // again; else only the lines that changed are packed again.
        if last.as_ref().is_some_and(|l| Arc::ptr_eq(l, &frame)) {
            restamp(pk, &mut seq, ts);
        } else {
            let again = crate::changed_rows(&frame, last.as_deref()).is_some_and(|rows| {
                let before = seq;
                let done = repacketize(&frame, essence, &rows, &mut seq, ts, pk, &mut line);
                if !done {
                    seq = before;
                }
                done
            });
            if !again {
                packetize_into(
                    &frame,
                    essence,
                    pt,
                    &mut seq,
                    ts,
                    Split::Even { max_udp },
                    pk,
                    &mut line,
                );
            }
            last = Some(frame.clone());
        }
        // Paced across what is left of ~90% of the frame period.
        let job = |packets: &Arc<Packets>| Job {
            packets: packets.clone(),
            start: Instant::now(),
            budget: period.mul_f64(0.9).saturating_sub(start.elapsed()),
        };
        busy.fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
        for leg in &legs {
            // A leg still sending the frame before holds this one back
            // (frames queue, then are dropped, in `push`).
            if leg.send(job(&packets)).is_err() {
                return;
            }
        }
    }
    tracing::info!(name, stream = essence.label(), "ST 2110 sender stopped");
}

/// One network leg: sends the packets of each frame in bursts, paced.
fn send_leg(socket: UdpSocket, dest: SocketAddrV4, gso: bool, rx: Receiver<Job>, busy: &AtomicU64) {
    let mut out = batch::Out::new(socket, dest, gso);
    while let Ok(job) = rx.recv() {
        let send_start = Instant::now();
        let packets = &job.packets;
        let n = packets.len();
        let mut paced = Duration::ZERO;
        let mut i = 0;
        while i < n {
            let end = (i + BURST).min(n);
            out.send(packets, i..end);
            i = end;
            if i < n {
                let due = job.budget.mul_f64(i as f64 / n as f64);
                let elapsed = job.start.elapsed();
                if due > elapsed {
                    std::thread::sleep(due - elapsed);
                    paced += due - elapsed;
                }
            }
        }
        drop(job);
        let worked = send_start.elapsed().saturating_sub(paced);
        busy.fetch_add(worked.as_micros() as u64, Ordering::Relaxed);
    }
}

/// Sending packets in bursts: `sendmmsg` (one system call for many
/// packets), with UDP segmentation offload (`UDP_SEGMENT`: one packet
/// through the stack for many on the wire) when the kernel has it.
#[cfg(target_os = "linux")]
mod batch {
    use std::net::{SocketAddrV4, UdpSocket};
    use std::ops::Range;
    use std::os::fd::AsRawFd;

    use super::Packets;

    /// Segments of one offloaded send at most (the kernel's limit).
    const MAX_SEGMENTS: usize = 64;
    /// Bytes of one offloaded send at most (an IPv4 datagram).
    const MAX_GSO_BYTES: usize = 65_507;

    /// Room for one control message carrying a u16 (`CMSG_SPACE(2)`).
    #[repr(C, align(8))]
    #[derive(Clone, Copy)]
    struct Cmsg([u8; 32]);

    pub struct Out {
        socket: UdpSocket,
        addr: libc::sockaddr_in,
        /// Segmentation offload (off once the kernel refused it).
        gso: bool,
        hdrs: Vec<libc::mmsghdr>,
        iovs: Vec<libc::iovec>,
        cmsgs: Vec<Cmsg>,
        /// First packet of each message.
        firsts: Vec<usize>,
        /// Packets of each message (first, end, size).
        groups: Vec<(usize, usize, usize)>,
    }

    impl Out {
        pub fn new(socket: UdpSocket, dest: SocketAddrV4, gso: bool) -> Out {
            // SAFETY: a plain C struct, all zeroes is valid.
            let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
            addr.sin_family = libc::AF_INET as libc::sa_family_t;
            addr.sin_port = dest.port().to_be();
            addr.sin_addr = libc::in_addr {
                s_addr: u32::from(*dest.ip()).to_be(),
            };
            Out {
                socket,
                addr,
                gso,
                hdrs: Vec::new(),
                iovs: Vec::new(),
                cmsgs: Vec::new(),
                firsts: Vec::new(),
                groups: Vec::new(),
            }
        }

        /// Sends packets `range` of `p` (a packet the network refuses is
        /// skipped, as `send_to` errors are). Returns the system calls made.
        pub fn send(&mut self, p: &Packets, range: Range<usize>) -> usize {
            let mut calls = 0;
            let mut from = range.start;
            while from < range.end {
                let n = self.messages(p, from..range.end);
                let mut done = 0;
                while done < n {
                    calls += 1;
                    // SAFETY: the headers point at `self.addr`, `self.iovs`,
                    // `self.cmsgs` and into `p.buf`, all alive and unmoved
                    // until the call returns.
                    let sent = unsafe {
                        libc::sendmmsg(
                            self.socket.as_raw_fd(),
                            self.hdrs.as_mut_ptr().add(done),
                            (n - done) as libc::c_uint,
                            0,
                        )
                    };
                    if sent > 0 {
                        done += sent as usize;
                        continue;
                    }
                    let err = std::io::Error::last_os_error();
                    match err.raw_os_error() {
                        Some(libc::EINTR) => {}
                        Some(libc::EIO)
                        | Some(libc::EINVAL)
                        | Some(libc::ENOPROTOOPT)
                        | Some(libc::EMSGSIZE)
                            if self.gso && self.hdrs[done].msg_hdr.msg_controllen > 0 =>
                        {
                            // No segmentation offload here: plain packets
                            // from this message on.
                            tracing::info!(error = %err, "ST 2110: no UDP segmentation offload, packets sent one by one");
                            self.gso = false;
                            break;
                        }
                        // This message is lost; the next ones go.
                        _ => done += 1,
                    }
                }
                from = if done < n {
                    self.firsts[done]
                } else {
                    range.end
                };
            }
            calls
        }

        /// The messages for packets `range`, in `self.hdrs`; how many.
        fn messages(&mut self, p: &Packets, range: Range<usize>) -> usize {
            self.hdrs.clear();
            self.iovs.clear();
            self.cmsgs.clear();
            self.firsts.clear();
            let len = |i: usize| p.ends[i] - p.start(i);
            let mut i = range.start;
            // Messages first (iovs and cmsgs must not move once pointed at).
            let mut groups = std::mem::take(&mut self.groups);
            groups.clear();
            while i < range.end {
                let size = len(i);
                let mut j = i + 1;
                if self.gso {
                    let most = MAX_SEGMENTS.min(MAX_GSO_BYTES / size.max(1));
                    while j < range.end && j - i < most && len(j) == size {
                        j += 1;
                    }
                    // The last segment may be shorter.
                    if j < range.end && j - i < most && len(j) < size {
                        j += 1;
                    }
                }
                groups.push((i, j, size));
                i = j;
            }
            for &(a, b, _) in &groups {
                self.iovs.push(libc::iovec {
                    iov_base: p.buf[p.start(a)..].as_ptr() as *mut libc::c_void,
                    iov_len: p.ends[b - 1] - p.start(a),
                });
                self.cmsgs.push(Cmsg([0; 32]));
                self.firsts.push(a);
            }
            for (k, &(a, b, size)) in groups.iter().enumerate() {
                // SAFETY: plain C struct.
                let mut h: libc::msghdr = unsafe { std::mem::zeroed() };
                h.msg_name = &mut self.addr as *mut libc::sockaddr_in as *mut libc::c_void;
                h.msg_namelen = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
                h.msg_iov = &mut self.iovs[k];
                h.msg_iovlen = 1;
                if b - a > 1 {
                    let c = &mut self.cmsgs[k];
                    h.msg_control = c.0.as_mut_ptr() as *mut libc::c_void;
                    // SAFETY: CMSG_SPACE(2) fits in the 32 bytes of `c`,
                    // which is aligned for a cmsghdr.
                    unsafe {
                        h.msg_controllen = libc::CMSG_SPACE(2) as _;
                        let cm = libc::CMSG_FIRSTHDR(&h);
                        (*cm).cmsg_level = libc::SOL_UDP;
                        (*cm).cmsg_type = libc::UDP_SEGMENT;
                        (*cm).cmsg_len = libc::CMSG_LEN(2) as _;
                        std::ptr::write_unaligned(libc::CMSG_DATA(cm) as *mut u16, size as u16);
                    }
                }
                self.hdrs.push(libc::mmsghdr {
                    msg_hdr: h,
                    msg_len: 0,
                });
            }
            self.groups = groups;
            self.hdrs.len()
        }
    }
}

/// Sending packets one by one (`send_to`), where `sendmmsg` is not.
#[cfg(not(target_os = "linux"))]
mod batch {
    use std::net::{SocketAddrV4, UdpSocket};
    use std::ops::Range;

    use super::Packets;

    pub struct Out {
        socket: UdpSocket,
        dest: SocketAddrV4,
    }

    impl Out {
        pub fn new(socket: UdpSocket, dest: SocketAddrV4, _gso: bool) -> Out {
            Out { socket, dest }
        }

        /// Sends packets `range` of `p`; the system calls made.
        pub fn send(&mut self, p: &Packets, range: Range<usize>) -> usize {
            for i in range.clone() {
                let _ = self.socket.send_to(p.get(i), self.dest);
            }
            range.len()
        }
    }
}

/// 90 kHz ticks since the epoch, modulo 2³² (ST 2110-10).
fn rtp_timestamp(since_epoch: Duration) -> u32 {
    let ticks = since_epoch.as_secs() as u128 * RTP_CLOCK as u128
        + since_epoch.subsec_nanos() as u128 * RTP_CLOCK as u128 / 1_000_000_000;
    ticks as u32
}

fn frame_time(at: SystemTime) -> Duration {
    at.duration_since(UNIX_EPOCH).unwrap_or_default()
}

impl OutputSink for St2110Sender {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn work_time(&self) -> Option<Duration> {
        Some(Duration::from_micros(self.busy_us.load(Ordering::Relaxed)))
    }

    fn push(&mut self, frame: &Arc<Frame>, at: SystemTime) -> Result<(), OutputError> {
        let mut result = Ok(());
        for e in &self.essences {
            match e.tx.try_send((frame.clone(), at)) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => result = Err(OutputError::Dropped),
                Err(TrySendError::Disconnected(_)) => return Err(OutputError::Closed),
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_count_and_marker() {
        let f = Frame::blank(1920, 1080);
        let mut seq = 0;
        let pkts = packetize(&f, Essence::Fill, 96, &mut seq, 1234);
        // 1920 px * 2.5 B = 4800 B per line = 4 packets.
        assert_eq!(pkts.len(), 1080 * 4);
        assert_eq!(seq, 1080 * 4);
        let last = pkts.get(pkts.len() - 1);
        assert_eq!(last[1], 0x80 | 96);
        assert_eq!(pkts.get(0)[1], 96);
        assert_eq!(pkts.get(0).len(), 12 + 8 + 1200);
        // Second packet of line 0 starts at pixel 480.
        assert_eq!(u16::from_be_bytes([pkts.get(1)[18], pkts.get(1)[19]]), 480);
    }

    #[test]
    fn changed_lines_only_are_packed_again() {
        let mut old = Frame::blank(64, 6);
        for (i, b) in old.data.iter_mut().enumerate() {
            *b = (i * 7 % 251) as u8;
        }
        let mut new = old.clone();
        for b in &mut new.data[64 * 4 * 2..64 * 4 * 4] {
            *b = b.wrapping_add(3);
        }
        let rows = crate::changed_rows(&new, Some(&old)).unwrap();
        assert_eq!(rows, [false, false, true, true, false, false]);
        for essence in [Essence::Fill, Essence::Key, Essence::Rgba, Essence::Ycbcra] {
            let mut seq = 0xfffe;
            let mut line = Vec::new();
            let mut again = packetize(&old, essence, 96, &mut seq, 1234);
            let mut seq2 = seq;
            assert!(repacketize(
                &new, essence, &rows, &mut seq2, 5678, &mut again, &mut line
            ));
            let want = packetize(&new, essence, 96, &mut seq, 5678);
            assert_eq!(seq, seq2);
            assert_eq!(again.buf, want.buf);
            assert_eq!(again.ends, want.ends);
        }
    }

    #[test]
    fn the_same_picture_again_is_only_restamped() {
        let mut f = Frame::blank(64, 4);
        for (i, b) in f.data.iter_mut().enumerate() {
            *b = (i * 7 % 251) as u8;
        }
        for essence in [Essence::Fill, Essence::Rgba, Essence::Ycbcra] {
            let mut seq = 0xfffe;
            let mut again = packetize(&f, essence, 96, &mut seq, 1234);
            let mut seq2 = seq;
            restamp(&mut again, &mut seq2, 5678);
            let want = packetize(&f, essence, 96, &mut seq, 5678);
            assert_eq!(seq, seq2);
            assert_eq!(again.buf, want.buf);
            assert_eq!(again.ends, want.ends);
        }
    }

    #[test]
    fn sdp_contains_format() {
        let cfg = St2110Config {
            enabled: true,
            id: "a".into(),
            hardware: String::new(),
            encoding: St2110Encoding::FillKey,
            name: "CG1".into(),
            fill: "239.10.0.1:5004".parse().unwrap(),
            key: Some("239.10.0.2:5004".parse().unwrap()),
            interface: Some("10.0.0.5".parse().unwrap()),
            blue: None,
            ttl: 16,
            payload_type: 96,
            tai_offset: 37,
        };
        let sdp = cfg.sdp(1920, 1080, (60000, 1001), true).unwrap();
        assert!(sdp.contains("c=IN IP4 239.10.0.2/16"));
        assert!(sdp.contains("exactframerate=60000/1001"));
        assert!(sdp.contains("sampling=KEY"));
        let one = St2110Config {
            enabled: true,
            encoding: St2110Encoding::Ycbcra,
            ..cfg
        };
        assert_eq!(one.streams().len(), 1);
        let sdp = one.sdp(1920, 1080, (50, 1), false).unwrap();
        assert!(sdp.contains("sampling=YCbCrA-4:2:2:4") && sdp.contains("depth=10"));
        assert!(one.sdp(1920, 1080, (50, 1), true).is_none());
    }

    #[test]
    fn a_blue_leg_doubles_every_stream() {
        let cfg = St2110Config {
            id: "a".into(),
            enabled: true,
            name: "CG".into(),
            hardware: String::new(),
            encoding: St2110Encoding::FillKey,
            fill: "239.1.0.1:5004".parse().unwrap(),
            key: Some("239.1.0.2:5004".parse().unwrap()),
            interface: Some("10.1.0.5".parse().unwrap()),
            blue: Some(Leg {
                fill: "239.2.0.1:5004".parse().unwrap(),
                key: Some("239.2.0.2:5004".parse().unwrap()),
                interface: Some("10.2.0.5".parse().unwrap()),
            }),
            ttl: 32,
            payload_type: 96,
            tai_offset: 37,
        };
        let s = cfg.streams();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].blue, Some("239.2.0.2:5004".parse().unwrap()));
        let sdp = cfg.sdp(1920, 1080, (50, 1), true).unwrap();
        assert!(sdp.contains("a=group:DUP primary secondary"));
        assert!(sdp.contains("c=IN IP4 239.1.0.2/32") && sdp.contains("c=IN IP4 239.2.0.2/32"));
        assert!(sdp.contains("incl IN IP4 239.2.0.2 10.2.0.5"));
        assert!(sdp.contains("a=mid:primary") && sdp.contains("a=mid:secondary"));
        assert_eq!(sdp.matches("m=video").count(), 2);
    }

    #[test]
    fn rtp_timestamps_count_90khz_and_wrap() {
        assert_eq!(rtp_timestamp(Duration::from_secs(1)), 90_000);
        assert_eq!(rtp_timestamp(Duration::from_millis(20)), 1_800);
        // 2³² ticks later it wraps to the same value.
        let wrap = Duration::from_secs_f64(4_294_967_296.0 / 90_000.0);
        let t = Duration::from_secs(1_700_000_000);
        assert!(rtp_timestamp(t + wrap).abs_diff(rtp_timestamp(t)) <= 1);
    }

    fn pattern(w: u32, h: u32) -> Frame {
        let mut f = Frame::blank(w, h);
        for (i, b) in f.data.iter_mut().enumerate() {
            *b = ((i * 7 + i / 4096) % 251) as u8;
        }
        f
    }

    fn fnv(h: &mut u64, b: &[u8]) {
        for x in b {
            *h = (*h ^ *x as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// `packetize` makes the packets it made before batched sending and the
    /// largest payloads (hashes of the packets of the sender before them).
    #[test]
    fn packetization_is_unchanged() {
        let want = [
            (
                (1920, 1080),
                Essence::Fill,
                4320,
                69840,
                0x66cbfe9d11fb9c77u64,
            ),
            ((1920, 1080), Essence::Key, 4320, 69840, 0xe74b7a3295e0f101),
            ((1920, 1080), Essence::Rgba, 7560, 73080, 0xbd31a48ac084fcfc),
            (
                (1920, 1080),
                Essence::Ycbcra,
                6480,
                72000,
                0x1c2ca95d64280d8e,
            ),
            ((1280, 720), Essence::Fill, 2160, 67680, 0xbc38e64c759945c7),
            ((1280, 720), Essence::Rgba, 3600, 69120, 0xb64ad22c5e9e908d),
            (
                (1280, 720),
                Essence::Ycbcra,
                2880,
                68400,
                0x45f3b0d971277421,
            ),
            ((722, 6), Essence::Fill, 12, 65532, 0xdcec30a040a1aebb),
            ((722, 6), Essence::Ycbcra, 18, 65538, 0x71a83fd515e96993),
        ];
        for ((w, h), e, n, end, hash) in want {
            let f = pattern(w, h);
            let mut seq = 0xfff0;
            let p = packetize(&f, e, 96, &mut seq, 0x1234_5678);
            let mut got = 0xcbf2_9ce4_8422_2325u64;
            for k in p.iter() {
                fnv(&mut got, &(k.len() as u64).to_le_bytes());
                fnv(&mut got, k);
            }
            assert_eq!((p.len(), seq, got), (n, end, hash), "{w}x{h} {e:?}");
        }
    }

    /// The payload of each line, in order, from the packets.
    fn lines(p: &Packets, h: usize) -> Vec<Vec<u8>> {
        let mut out = vec![Vec::new(); h];
        for k in p.iter() {
            let y = u16::from_be_bytes([k[16], k[17]]) as usize & 0x7fff;
            out[y].extend_from_slice(&k[HEADER_BYTES..]);
        }
        out
    }

    /// The largest payloads: within the UDP size limit, whole pgroups, lines
    /// split evenly, the same picture bytes as before in fewer packets.
    #[test]
    fn largest_payloads_carry_the_same_lines() {
        for (w, h) in [(1920u32, 1080u32), (1280, 720), (3840, 4), (722, 6)] {
            let f = pattern(w, h);
            for e in [Essence::Fill, Essence::Key, Essence::Rgba, Essence::Ycbcra] {
                let (pg_bytes, pg_px) = e.pgroup();
                let mut seq = 7;
                let old = packetize(&f, e, 96, &mut seq, 1);
                let (mut big, mut line) = (Packets::default(), Vec::new());
                let mut seq2 = 7;
                packetize_into(
                    &f,
                    e,
                    96,
                    &mut seq2,
                    1,
                    Split::Even {
                        max_udp: MAX_UDP_BYTES,
                    },
                    &mut big,
                    &mut line,
                );
                assert!(big.len() <= old.len(), "{w} {e:?}");
                assert_eq!(seq2, 7 + big.len() as u32);
                assert_eq!(lines(&big, h as usize), lines(&old, h as usize));
                let per_line = big.len() / h as usize;
                for (i, k) in big.iter().enumerate() {
                    assert!(k.len() <= MAX_UDP_BYTES, "{w} {e:?} {}", k.len());
                    let len = u16::from_be_bytes([k[14], k[15]]) as usize;
                    assert_eq!(len, k.len() - HEADER_BYTES);
                    let at = u16::from_be_bytes([k[18], k[19]]) as usize;
                    // Each packet starts on a pgroup, the line split evenly.
                    assert_eq!(at % pg_px, 0);
                    if i % per_line + 1 < per_line {
                        assert_eq!(len % pg_bytes, 0);
                        assert_eq!(len, big.get(i - i % per_line).len() - HEADER_BYTES);
                    }
                    // The marker on the last packet only.
                    assert_eq!(k[1] & 0x80 != 0, i == big.len() - 1);
                }
            }
        }
        // 1080p: 5 packets a line in YCbCrA (6 before), 6 in RGBA (7).
        let f = pattern(1920, 2);
        let (mut p, mut line, mut seq) = (Packets::default(), Vec::new(), 0);
        packetize_into(
            &f,
            Essence::Ycbcra,
            96,
            &mut seq,
            0,
            Split::Even {
                max_udp: MAX_UDP_BYTES,
            },
            &mut p,
            &mut line,
        );
        assert_eq!((p.len(), p.get(0).len()), (10, 1460));
        packetize_into(
            &f,
            Essence::Rgba,
            96,
            &mut seq,
            0,
            Split::Even {
                max_udp: MAX_UDP_BYTES,
            },
            &mut p,
            &mut line,
        );
        assert_eq!((p.len(), p.get(0).len()), (12, 1300));
        packetize_into(
            &f,
            Essence::Fill,
            96,
            &mut seq,
            0,
            Split::Even {
                max_udp: MAX_UDP_BYTES,
            },
            &mut p,
            &mut line,
        );
        assert_eq!((p.len(), p.get(0).len()), (8, 1220));
    }

    fn receiver() -> (UdpSocket, SocketAddrV4) {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket2::SockRef::from(&rx)
            .set_recv_buffer_size(16 << 20)
            .ok();
        rx.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let std::net::SocketAddr::V4(a) = rx.local_addr().unwrap() else {
            unreachable!()
        };
        (rx, a)
    }

    fn received(rx: &UdpSocket, n: usize) -> Vec<Vec<u8>> {
        let mut buf = [0u8; 65536];
        (0..n)
            .map_while(|_| rx.recv(&mut buf).ok().map(|l| buf[..l].to_vec()))
            .collect()
    }

    /// Sent in bursts (one system call each on Linux, segmented or not),
    /// the packets arrive as packetized, one datagram each.
    #[test]
    fn bursts_send_every_packet_as_it_is() {
        // 722 px: a shorter packet at the end of each line.
        let f = pattern(722, 40);
        let mut seq = 0;
        let (mut p, mut line) = (Packets::default(), Vec::new());
        packetize_into(
            &f,
            Essence::Ycbcra,
            96,
            &mut seq,
            0,
            Split::Even {
                max_udp: MAX_UDP_BYTES,
            },
            &mut p,
            &mut line,
        );
        for gso in [true, false] {
            let (rx, to) = receiver();
            let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
            let mut out = batch::Out::new(tx, to, gso);
            let mut calls = 0;
            let mut i = 0;
            while i < p.len() {
                let end = (i + BURST).min(p.len());
                calls += out.send(&p, i..end);
                i = end;
            }
            let got = received(&rx, p.len());
            assert_eq!(got.len(), p.len(), "gso {gso}");
            for (k, g) in got.iter().enumerate() {
                assert_eq!(g.as_slice(), p.get(k), "gso {gso} packet {k}");
            }
            if cfg!(target_os = "linux") {
                assert!(calls <= p.len().div_ceil(BURST) + 1, "{calls} calls");
            }
        }
    }

    /// A sender with a blue leg: every packet of a frame on both networks,
    /// the same packets, each leg sending from its own thread.
    #[test]
    fn both_legs_get_every_packet() {
        let (red, red_at) = receiver();
        let (blue, blue_at) = receiver();
        let cfg = St2110Config {
            id: "t".into(),
            enabled: true,
            name: "T".into(),
            hardware: String::new(),
            encoding: St2110Encoding::Rgba,
            fill: red_at,
            key: None,
            interface: None,
            blue: Some(Leg {
                fill: blue_at,
                key: None,
                interface: None,
            }),
            ttl: 1,
            payload_type: 96,
            tai_offset: 37,
        };
        let mut s = St2110Sender::start(cfg, Duration::from_millis(20)).unwrap();
        let f = Arc::new(pattern(320, 60));
        s.push(&f, SystemTime::now()).unwrap();
        let mut seq = 0;
        let (mut want, mut line) = (Packets::default(), Vec::new());
        packetize_into(
            &f,
            Essence::Rgba,
            96,
            &mut seq,
            0,
            Split::Even {
                max_udp: MAX_UDP_BYTES,
            },
            &mut want,
            &mut line,
        );
        let r = received(&red, want.len());
        let b = received(&blue, want.len());
        assert_eq!(r.len(), want.len());
        assert_eq!(r, b);
        for (k, g) in r.iter().enumerate() {
            // Same packets but the timestamp.
            assert_eq!(g[..4], want.get(k)[..4]);
            assert_eq!(g[8..], want.get(k)[8..]);
        }
    }

    /// Packets fit the network: the standard size limit on loopback (MTU
    /// 64 KiB), less where the MTU is smaller.
    #[test]
    fn packets_fit_the_route() {
        let lo = "127.0.0.1:5004".parse().unwrap();
        assert_eq!(max_udp(&[(lo, None)]), MAX_UDP_BYTES);
        if let Some(mtu) = path_mtu("192.0.2.1:5004".parse().unwrap(), None) {
            assert_eq!(
                max_udp(&[(lo, None), ("192.0.2.1:5004".parse().unwrap(), None)]),
                MAX_UDP_BYTES.min(mtu - IP_UDP_BYTES)
            );
        }
    }

    #[test]
    fn one_stream_pgroups() {
        let f = Frame::blank(1920, 1080);
        let mut seq = 0;
        // RGBA: 7680 B per line, 1200 B per packet.
        let p = packetize(&f, Essence::Rgba, 96, &mut seq, 0);
        assert_eq!(p.len(), 1080 * 7);
        assert_eq!(u16::from_be_bytes([p.get(1)[18], p.get(1)[19]]), 300);
        // YCbCrA: 480 pgroups of 15 B = 7200 B per line, 80 pgroups a packet.
        let p = packetize(&f, Essence::Ycbcra, 96, &mut seq, 0);
        assert_eq!(p.len(), 1080 * 6);
        assert_eq!(u16::from_be_bytes([p.get(1)[18], p.get(1)[19]]), 320);
    }
}
