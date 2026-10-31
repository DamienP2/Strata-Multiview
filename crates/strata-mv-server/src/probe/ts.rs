//! MPEG-TS analysis: the transport stream of an input as it comes (fed by
//! the relay of `inputs.rs`, or a segment of an HLS stream), read packet by
//! packet. Its tables (PAT, PMT, SDT, NIT, EIT now/next, TDT/TOT, SCTE-35),
//! its PIDs (rate, continuity, scrambling, PCR) and the TR 101 290 checks
//! that a monitor makes on it.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::time::Instant;

use serde::Serialize;

const PACKET: usize = 188;
const SYNC: u8 = 0x47;

/// A program event of the EPG (EIT present / following).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: u16,
    /// UTC, ISO 8601 ("2026-10-31T20:45:00Z"); "" when not given.
    pub start: String,
    pub duration_s: u32,
    pub name: String,
    pub text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub genre: String,
    /// The minimum age (DVB parental rating), 0 none.
    #[serde(skip_serializing_if = "is_zero")]
    pub rating: u8,
    /// Not running, starts in a few seconds, pausing, running, off air.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub running: String,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// A service (program) of the stream.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub number: u16,
    pub pmt_pid: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcr_pid: Option<u16>,
    pub name: String,
    pub provider: String,
    /// "TV numérique", "Radio numérique", "TV HD"… (DVB service type).
    pub kind: String,
    /// Under conditional access (CA descriptor, or the SDT's free_CA_mode).
    pub scrambled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ca_systems: Vec<String>,
    pub streams: Vec<Es>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now: Option<Event>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Event>,
}

/// An elementary stream of a program, as its PMT describes it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Es {
    pub pid: u16,
    pub stream_type: u8,
    /// "H.264", "AAC (ADTS)", "SCTE-35", "Télétexte", "SMPTE 2038 (ANC)"…
    pub kind: String,
    /// video, audio, subtitle, data.
    pub class: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// What its descriptors tell (audio type, teletext pages, captions…).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    /// "High@4.1", "Main 10@5.1 (High tier)": read from its SPS.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub profile: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PidView {
    pub pid: u16,
    /// What it carries ("PAT", "PMT 1", "H.264", "NULL"…).
    pub kind: String,
    pub packets: u64,
    /// Over the last seconds (or the whole segment).
    pub kbps: f64,
    pub cc_errors: u64,
    pub scrambled: bool,
    pub pcr: bool,
}

/// A splice point (SCTE-35): an ad break, a programme boundary.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Splice {
    pub pid: u16,
    /// "splice_insert", "time_signal", "splice_schedule"…
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u32>,
    /// Out of the network (the start of a break), or back into it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub immediate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<f64>,
    /// The splice time, seconds of the 90 kHz clock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pts_s: Option<f64>,
    /// The segmentation descriptors ("Provider Advertisement Start"…).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub segmentation: Vec<String>,
    pub cancel: bool,
    /// Seen how long ago (live) or at which second of the segment.
    pub at_s: f64,
}

/// One TR 101 290 check.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub priority: u8,
    /// "1.3 PAT_error".
    pub name: String,
    pub count: u64,
    /// Seconds since the last one (none yet: absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ago_s: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TsView {
    /// Read continuously (a live relay) or from one segment.
    pub live: bool,
    pub packets: u64,
    pub seconds: f64,
    pub kbps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_id: Option<u16>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub network: String,
    /// The time the stream gives (TDT / TOT), UTC.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub utc: String,
    pub tables: Vec<String>,
    /// Full EPG (EIT schedule) carried too.
    pub eit_schedule: bool,
    pub services: Vec<Service>,
    pub pids: Vec<PidView>,
    pub splices: Vec<Splice>,
    /// Heartbeats (splice_null) seen.
    pub splice_nulls: u64,
    pub checks: Vec<Check>,
}

#[derive(Default)]
struct Pid {
    packets: u64,
    cc: Option<u8>,
    cc_errors: u64,
    scrambled: bool,
    tei: u64,
    pcr: Option<u64>,
    has_pcr: bool,
}

/// A program as its PMT tells, before the SDT names it.
#[derive(Default, Clone)]
struct Pmt {
    pcr_pid: u16,
    ca: Vec<String>,
    streams: Vec<Es>,
}

#[derive(Default, Clone)]
struct SdtEntry {
    name: String,
    provider: String,
    kind: u8,
    free_ca: bool,
}

/// A counter of TR 101 290 errors.
#[derive(Default, Clone, Copy)]
struct Count {
    n: u64,
    last: Option<Instant>,
}

impl Count {
    fn hit(&mut self, now: Instant) {
        self.n += 1;
        self.last = Some(now);
    }
}

#[derive(Default)]
struct Errors {
    sync_loss: Count,
    sync_byte: Count,
    pat: Count,
    cc: Count,
    pmt: Count,
    pid: Count,
    transport: Count,
    crc: Count,
    pcr_repetition: Count,
    pcr_discontinuity: Count,
    sdt: Count,
    eit: Count,
}

/// Reads a transport stream as it comes.
pub struct TsMonitor {
    live: bool,
    started: Instant,
    carry: Vec<u8>,
    synced: bool,
    packets: u64,
    pids: BTreeMap<u16, Pid>,
    sections: HashMap<u16, Vec<u8>>,
    pat: BTreeMap<u16, u16>,
    ts_id: Option<u16>,
    network_id: Option<u16>,
    nit_pid: u16,
    pmts: BTreeMap<u16, Pmt>,
    sdt: BTreeMap<u16, SdtEntry>,
    network: String,
    eit: BTreeMap<u16, [Option<Event>; 2]>,
    eit_schedule: bool,
    utc: Option<(i64, Instant)>,
    splices: VecDeque<(Splice, Instant)>,
    splice_nulls: u64,
    tables: BTreeSet<&'static str>,
    errors: Errors,
    last_pat: Option<Instant>,
    last_pmt: HashMap<u16, Instant>,
    last_sdt: Option<Instant>,
    last_eit: Option<Instant>,
    /// PIDs a PMT names, and when each was last seen.
    seen_at: HashMap<u16, Instant>,
    /// The rate of each PID: packets at the start of the window.
    window: (Instant, HashMap<u16, u64>, u64),
    rates: HashMap<u16, f64>,
    total_rate: f64,
    /// PCR span (first, last) of the PCR PIDs: the segment's duration.
    pcr_span: HashMap<u16, (u64, u64)>,
    /// Video PIDs still looking for their SPS (tries left).
    sps_tries: HashMap<u16, u8>,
    profiles: HashMap<u16, String>,
}

impl TsMonitor {
    /// `live`: read as it comes (timed checks, rates over the last
    /// seconds); else one piece (a segment), timed by its PCR.
    pub fn new(live: bool) -> Self {
        let now = Instant::now();
        TsMonitor {
            live,
            started: now,
            carry: Vec::new(),
            synced: false,
            packets: 0,
            pids: BTreeMap::new(),
            sections: HashMap::new(),
            pat: BTreeMap::new(),
            ts_id: None,
            network_id: None,
            nit_pid: 0x10,
            pmts: BTreeMap::new(),
            sdt: BTreeMap::new(),
            network: String::new(),
            eit: BTreeMap::new(),
            eit_schedule: false,
            utc: None,
            splices: VecDeque::new(),
            splice_nulls: 0,
            tables: BTreeSet::new(),
            errors: Errors::default(),
            last_pat: None,
            last_pmt: HashMap::new(),
            last_sdt: None,
            last_eit: None,
            seen_at: HashMap::new(),
            window: (now, HashMap::new(), 0),
            rates: HashMap::new(),
            total_rate: 0.0,
            pcr_span: HashMap::new(),
            sps_tries: HashMap::new(),
            profiles: HashMap::new(),
        }
    }

    /// Looks like a transport stream: sync bytes every 188 bytes.
    pub fn looks_like_ts(data: &[u8]) -> bool {
        (0..PACKET.min(data.len())).any(|o| {
            let n = (data.len() - o) / PACKET;
            n >= 3 && (0..n.min(20)).all(|i| data[o + i * PACKET] == SYNC)
        })
    }

    /// Packets read so far.
    pub fn packets(&self) -> u64 {
        self.packets
    }

    /// Reads more of the stream.
    pub fn feed(&mut self, data: &[u8], now: Instant) {
        self.carry.extend_from_slice(data);
        let mut at = 0;
        let buf = std::mem::take(&mut self.carry);
        while buf.len() - at >= PACKET {
            if buf[at] != SYNC {
                if self.synced {
                    self.synced = false;
                    self.errors.sync_loss.hit(now);
                }
                self.errors.sync_byte.hit(now);
                // Find the next place with sync bytes 188 apart.
                match (at + 1..buf.len()).find(|&i| {
                    buf[i] == SYNC && (i + PACKET >= buf.len() || buf[i + PACKET] == SYNC)
                }) {
                    Some(i) => at = i,
                    None => {
                        at = buf.len();
                        break;
                    }
                }
                continue;
            }
            self.synced = true;
            let p: [u8; PACKET] = buf[at..at + PACKET].try_into().unwrap_or([0; PACKET]);
            self.packet(&p, now);
            at += PACKET;
        }
        self.carry = buf[at.min(buf.len())..].to_vec();
        if self.live {
            self.timed(now);
        }
    }

    fn packet(&mut self, p: &[u8; PACKET], now: Instant) {
        self.packets += 1;
        let tei = p[1] & 0x80 != 0;
        let pusi = p[1] & 0x40 != 0;
        let pid = u16::from(p[1] & 0x1F) << 8 | u16::from(p[2]);
        let scrambling = p[3] >> 6;
        let afc = (p[3] >> 4) & 3;
        let cc = p[3] & 0x0F;
        let e = self.pids.entry(pid).or_default();
        e.packets += 1;
        if tei {
            e.tei += 1;
            self.errors.transport.hit(now);
        }
        if scrambling != 0 {
            e.scrambled = true;
        }
        self.seen_at.insert(pid, now);
        let mut payload_at = 4;
        let mut discontinuity = false;
        if afc & 2 != 0 {
            let len = p[4] as usize;
            payload_at = 5 + len;
            if len > 0 {
                let flags = p[5];
                discontinuity = flags & 0x80 != 0;
                if flags & 0x10 != 0 && len >= 7 {
                    let base = u64::from(p[6]) << 25
                        | u64::from(p[7]) << 17
                        | u64::from(p[8]) << 9
                        | u64::from(p[9]) << 1
                        | u64::from(p[10]) >> 7;
                    let ext = u64::from(p[10] & 1) << 8 | u64::from(p[11]);
                    let pcr = base * 300 + ext;
                    e.has_pcr = true;
                    if let Some(prev) = e.pcr {
                        // 27 MHz: 40 ms = 1 080 000, 100 ms = 2 700 000.
                        let d = pcr as i64 - prev as i64;
                        if !discontinuity && !(0..=2_700_000).contains(&d) {
                            self.errors.pcr_discontinuity.hit(now);
                        } else if d > 1_080_000 {
                            self.errors.pcr_repetition.hit(now);
                        }
                    }
                    e.pcr = Some(pcr);
                    let span = self.pcr_span.entry(pid).or_insert((pcr, pcr));
                    span.1 = pcr;
                }
            }
        }
        // Continuity, on PIDs with a payload (not the null packets).
        if pid != 0x1FFF {
            let e = self.pids.entry(pid).or_default();
            if afc & 1 != 0 {
                if let Some(last) = e.cc {
                    if !discontinuity && cc != (last + 1) & 0x0F && cc != last {
                        e.cc_errors += 1;
                        self.errors.cc.hit(now);
                    }
                }
                e.cc = Some(cc);
            }
        }
        if afc & 1 == 0 || payload_at >= PACKET || scrambling != 0 {
            return;
        }
        let payload = &p[payload_at..];
        if self.is_psi(pid) {
            self.psi(pid, pusi, payload, now);
        } else if pusi && self.sps_tries.get(&pid).is_some_and(|t| *t > 0) {
            self.sps(pid, payload);
        }
    }

    fn is_psi(&self, pid: u16) -> bool {
        matches!(pid, 0x00 | 0x01 | 0x11 | 0x12 | 0x14)
            || pid == self.nit_pid
            || self.pat.values().any(|&v| v == pid)
            || self.pmts.values().any(|p| {
                p.streams
                    .iter()
                    .any(|s| s.pid == pid && s.stream_type == 0x86)
            })
    }

    /// Puts the sections of a PSI PID together.
    fn psi(&mut self, pid: u16, pusi: bool, payload: &[u8], now: Instant) {
        let mut done = Vec::new();
        {
            let buf = self.sections.entry(pid).or_default();
            let mut p = payload;
            if pusi {
                let Some((&ptr, rest)) = p.split_first() else {
                    return;
                };
                let ptr = ptr as usize;
                if ptr > rest.len() {
                    buf.clear();
                    return;
                }
                if !buf.is_empty() {
                    buf.extend_from_slice(&rest[..ptr]);
                    take_sections(buf, &mut done);
                }
                buf.clear();
                p = &rest[ptr..];
            } else if buf.is_empty() {
                return;
            }
            buf.extend_from_slice(p);
            take_sections(buf, &mut done);
            if buf.len() > 4400 {
                buf.clear();
            }
        }
        for s in done {
            self.section(pid, &s, now);
        }
    }

    fn section(&mut self, pid: u16, s: &[u8], now: Instant) {
        let table = s[0];
        let syntax = s[1] & 0x80 != 0;
        // The TDT has no CRC; every other one checked.
        if (syntax || table == 0x73 || table == 0xFC) && table != 0x70 && crc32(s) != 0 {
            self.errors.crc.hit(now);
            return;
        }
        match (pid, table) {
            (0, 0x00) => self.pat_section(s, now),
            (_, 0x02) => self.pmt_section(s, now),
            (_, 0x40) => self.nit_section(s),
            (0x11, 0x42) => self.sdt_section(s, now),
            (0x12, 0x4E) => self.eit_section(s, now),
            (0x12, 0x50..=0x5F) => {
                self.eit_schedule = true;
                self.tables.insert("EIT schedule");
            }
            (0x14, 0x70 | 0x73) => self.time_section(s, now),
            (_, 0xFC) => self.splice_section(pid, s, now),
            (0x01, 0x01) => {
                self.tables.insert("CAT");
            }
            _ => {}
        }
    }

    fn pat_section(&mut self, s: &[u8], now: Instant) {
        self.tables.insert("PAT");
        self.last_pat = Some(now);
        if s.len() < 12 {
            return;
        }
        self.ts_id = Some(u16::from_be_bytes([s[3], s[4]]));
        let end = s.len() - 4;
        let mut programs = BTreeMap::new();
        for e in s[8..end].as_chunks::<4>().0 {
            let number = u16::from_be_bytes([e[0], e[1]]);
            let pid = u16::from(e[2] & 0x1F) << 8 | u16::from(e[3]);
            if number == 0 {
                self.nit_pid = pid;
            } else {
                programs.insert(number, pid);
            }
        }
        self.pmts.retain(|n, _| programs.contains_key(n));
        self.pat = programs;
    }

    fn pmt_section(&mut self, s: &[u8], now: Instant) {
        if s.len() < 16 {
            return;
        }
        self.tables.insert("PMT");
        let number = u16::from_be_bytes([s[3], s[4]]);
        if let Some(&pmt_pid) = self.pat.get(&number) {
            self.last_pmt.insert(pmt_pid, now);
        }
        let pcr_pid = u16::from(s[8] & 0x1F) << 8 | u16::from(s[9]);
        let info = (usize::from(s[10] & 0x0F) << 8 | usize::from(s[11])).min(s.len() - 16);
        let end = s.len() - 4;
        let mut ca = Vec::new();
        descriptors(&s[12..12 + info], |tag, d| {
            if tag == 0x09 && d.len() >= 2 {
                ca.push(ca_system(u16::from_be_bytes([d[0], d[1]])));
            }
        });
        let mut at = 12 + info;
        let mut streams = Vec::new();
        while at + 5 <= end {
            let stream_type = s[at];
            let pid = u16::from(s[at + 1] & 0x1F) << 8 | u16::from(s[at + 2]);
            let len = usize::from(s[at + 3] & 0x0F) << 8 | usize::from(s[at + 4]);
            let d = &s[at + 5..(at + 5 + len).min(end)];
            streams.push(es(stream_type, pid, d, &mut ca));
            at += 5 + len;
        }
        for st in &streams {
            if matches!(st.stream_type, 0x1B | 0x24) && !self.profiles.contains_key(&st.pid) {
                self.sps_tries.entry(st.pid).or_insert(60);
            }
        }
        ca.sort();
        ca.dedup();
        self.pmts.insert(
            number,
            Pmt {
                pcr_pid,
                ca,
                streams,
            },
        );
    }

    fn nit_section(&mut self, s: &[u8]) {
        self.tables.insert("NIT");
        if s.len() < 12 {
            return;
        }
        self.network_id
            .get_or_insert(u16::from_be_bytes([s[3], s[4]]));
        let len = (usize::from(s[8] & 0x0F) << 8 | usize::from(s[9])).min(s.len() - 14);
        descriptors(&s[10..10 + len], |tag, d| {
            if tag == 0x40 {
                self.network = dvb_text(d);
            }
        });
    }

    fn sdt_section(&mut self, s: &[u8], now: Instant) {
        self.tables.insert("SDT");
        self.last_sdt = Some(now);
        if s.len() < 15 {
            return;
        }
        self.network_id = Some(u16::from_be_bytes([s[8], s[9]]));
        let end = s.len() - 4;
        let mut at = 11;
        while at + 5 <= end {
            let id = u16::from_be_bytes([s[at], s[at + 1]]);
            let free_ca = s[at + 3] & 0x10 != 0;
            let len = usize::from(s[at + 3] & 0x0F) << 8 | usize::from(s[at + 4]);
            let mut entry = SdtEntry {
                free_ca,
                ..Default::default()
            };
            descriptors(&s[at + 5..(at + 5 + len).min(end)], |tag, d| {
                if tag == 0x48 && d.len() >= 3 {
                    entry.kind = d[0];
                    let pl = d[1] as usize;
                    if 2 + pl < d.len() {
                        entry.provider = dvb_text(&d[2..2 + pl]);
                        let nl = d[2 + pl] as usize;
                        let from = 3 + pl;
                        entry.name = dvb_text(&d[from.min(d.len())..(from + nl).min(d.len())]);
                    }
                }
            });
            self.sdt.insert(id, entry);
            at += 5 + len;
        }
    }

    fn eit_section(&mut self, s: &[u8], now: Instant) {
        self.tables.insert("EIT p/f");
        self.last_eit = Some(now);
        if s.len() < 18 {
            return;
        }
        let service = u16::from_be_bytes([s[3], s[4]]);
        let which = s[6].min(1) as usize;
        let end = s.len() - 4;
        let at = 14;
        let slot = &mut self.eit.entry(service).or_default()[which];
        if at + 12 > end {
            *slot = None;
            return;
        }
        let e = &s[at..];
        let mut ev = Event {
            id: u16::from_be_bytes([e[0], e[1]]),
            start: dvb_time(&e[2..7]).unwrap_or_default(),
            duration_s: bcd_seconds(&e[7..10]),
            running: running(e[10] >> 5).to_string(),
            ..Default::default()
        };
        let len = usize::from(e[10] & 0x0F) << 8 | usize::from(e[11]);
        let d = &e[12..(12 + len).min(end - at)];
        let mut extended = String::new();
        descriptors(d, |tag, d| match tag {
            0x4D if d.len() >= 5 => {
                ev.language = lang(&d[..3]);
                let nl = d[3] as usize;
                if 4 + nl < d.len() {
                    ev.name = dvb_text(&d[4..4 + nl]);
                    let tl = d[4 + nl] as usize;
                    let from = 5 + nl;
                    ev.text = dvb_text(&d[from.min(d.len())..(from + tl).min(d.len())]);
                }
            }
            0x4E if d.len() >= 6 => {
                // The items first, then the text.
                let items = d[4] as usize;
                let at = 5 + items;
                if at < d.len() {
                    let tl = d[at] as usize;
                    extended.push_str(&dvb_text(&d[at + 1..(at + 1 + tl).min(d.len())]));
                }
            }
            0x54 if !d.is_empty() => ev.genre = genre(d[0]).to_string(),
            0x55 if d.len() >= 4 => {
                let r = d[3];
                if (1..=0x0F).contains(&r) {
                    ev.rating = r + 3;
                }
            }
            _ => {}
        });
        if !extended.is_empty() && ev.text.len() < extended.len() {
            ev.text = extended;
        }
        *slot = Some(ev);
    }

    fn time_section(&mut self, s: &[u8], now: Instant) {
        self.tables.insert(if s[0] == 0x70 { "TDT" } else { "TOT" });
        if s.len() >= 8 {
            if let Some(t) = dvb_seconds(&s[3..8]) {
                self.utc = Some((t, now));
            }
        }
    }

    fn splice_section(&mut self, pid: u16, s: &[u8], now: Instant) {
        self.tables.insert("SCTE-35");
        if s.len() < 18 {
            return;
        }
        let encrypted = s[4] & 0x80 != 0;
        let adjust = pts(&s[4..9]);
        let cmd_len = usize::from(s[11] & 0x0F) << 8 | usize::from(s[12]);
        let kind = s[13];
        let at_s = if self.live { 0.0 } else { self.packets as f64 };
        let mut sp = Splice {
            pid,
            command: match kind {
                0x00 => "splice_null",
                0x04 => "splice_schedule",
                0x05 => "splice_insert",
                0x06 => "time_signal",
                0x07 => "bandwidth_reservation",
                0xFF => "private_command",
                _ => "?",
            }
            .to_string(),
            at_s,
            ..Default::default()
        };
        if kind == 0x00 {
            self.splice_nulls += 1;
            return;
        }
        if encrypted {
            sp.command.push_str(" (chiffré)");
            self.push_splice(sp, now);
            return;
        }
        let c = &s[14..s.len() - 4];
        let time = |c: &[u8]| -> (Option<f64>, usize) {
            match c.first() {
                Some(b) if b & 0x80 != 0 && c.len() >= 5 => (
                    Some(((pts(&c[..5]) + adjust) & 0x1_FFFF_FFFF) as f64 / 90_000.0),
                    5,
                ),
                Some(_) => (None, 1),
                None => (None, 0),
            }
        };
        match kind {
            0x05 if c.len() >= 5 => {
                sp.event_id = Some(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
                sp.cancel = c[4] & 0x80 != 0;
                if !sp.cancel && c.len() >= 6 {
                    let f = c[5];
                    sp.out = Some(f & 0x80 != 0);
                    let program = f & 0x40 != 0;
                    let has_duration = f & 0x20 != 0;
                    sp.immediate = Some(f & 0x10 != 0);
                    let mut at = 6;
                    if program && f & 0x10 == 0 {
                        let (t, n) = time(&c[at..]);
                        sp.pts_s = t;
                        at += n;
                    } else if !program && at < c.len() {
                        // Component splice: their count, then each one.
                        let n = c[at] as usize;
                        at += 1 + n;
                    }
                    if has_duration && at + 5 <= c.len() {
                        sp.duration_s = Some(pts(&c[at..at + 5]) as f64 / 90_000.0);
                    }
                }
            }
            0x06 => {
                sp.pts_s = time(c).0;
            }
            _ => {}
        }
        // The descriptors after the command: segmentation (CUEI).
        let d_at = 14 + cmd_len;
        if d_at + 2 <= s.len() - 4 {
            let len = usize::from(s[d_at] & 0x0F) << 8 | usize::from(s[d_at + 1]);
            let d = &s[d_at + 2..(d_at + 2 + len).min(s.len() - 4)];
            descriptors(d, |tag, d| {
                if tag == 0x02 && d.len() >= 10 && &d[..4] == b"CUEI" {
                    let id = u32::from_be_bytes([d[4], d[5], d[6], d[7]]);
                    let cancel = d[8] & 0x80 != 0;
                    if cancel {
                        sp.segmentation.push(format!("#{id} annulé"));
                        return;
                    }
                    let f = d[9];
                    let program = f & 0x80 != 0;
                    let has_duration = f & 0x40 != 0;
                    let mut at = 10;
                    if !program && at < d.len() {
                        at += 1 + d[at] as usize * 6;
                    }
                    let mut dur = None;
                    if has_duration && at + 5 <= d.len() {
                        dur = Some(
                            (u64::from(d[at]) << 32
                                | u64::from(d[at + 1]) << 24
                                | u64::from(d[at + 2]) << 16
                                | u64::from(d[at + 3]) << 8
                                | u64::from(d[at + 4])) as f64
                                / 90_000.0,
                        );
                        at += 5;
                    }
                    // upid type, length, upid, then the type id.
                    if at + 2 <= d.len() {
                        let ul = d[at + 1] as usize;
                        at += 2 + ul;
                    }
                    if let Some(&t) = d.get(at) {
                        let mut text = format!("{} (#{id})", segmentation(t));
                        if let Some(dur) = dur {
                            text.push_str(&format!(", {dur:.1} s"));
                        }
                        sp.segmentation.push(text);
                    }
                }
            });
        }
        self.push_splice(sp, now);
    }

    fn push_splice(&mut self, sp: Splice, now: Instant) {
        self.splices.push_front((sp, now));
        self.splices.truncate(20);
    }

    /// The SPS of a video PID: its profile and level.
    fn sps(&mut self, pid: u16, payload: &[u8]) {
        let tries = self.sps_tries.entry(pid).or_insert(0);
        *tries = tries.saturating_sub(1);
        // PES header: 00 00 01, stream id, length, flags, header length.
        if payload.len() < 9 || payload[..3] != [0, 0, 1] {
            return;
        }
        let from = 9 + payload[8] as usize;
        let es = &payload[from.min(payload.len())..];
        let hevc = self
            .pmts
            .values()
            .flat_map(|p| p.streams.iter())
            .any(|s| s.pid == pid && s.stream_type == 0x24);
        let mut i = 0;
        while i + 4 < es.len() {
            if es[i] == 0 && es[i + 1] == 0 && es[i + 2] == 1 {
                let nal = &es[i + 3..];
                let found = if hevc {
                    ((nal[0] >> 1) & 0x3F == 33)
                        .then(|| hevc_profile(nal))
                        .flatten()
                } else {
                    (nal[0] & 0x1F == 7).then(|| avc_profile(nal)).flatten()
                };
                if let Some(p) = found {
                    self.profiles.insert(pid, p);
                    self.sps_tries.remove(&pid);
                    return;
                }
                i += 3;
            } else {
                i += 1;
            }
        }
    }

    /// The checks that need time: tables too far apart, PIDs gone, rates.
    fn timed(&mut self, now: Instant) {
        let over = |last: Option<Instant>, ms: u64| {
            last.is_some_and(|t| now.duration_since(t).as_millis() as u64 > ms)
        };
        // Once per gap: the last one moves to now.
        if over(self.last_pat, 500) {
            self.errors.pat.hit(now);
            self.last_pat = Some(now);
        }
        let late: Vec<u16> = self
            .last_pmt
            .iter()
            .filter(|(_, t)| now.duration_since(**t).as_millis() > 500)
            .map(|(p, _)| *p)
            .collect();
        for p in late {
            self.errors.pmt.hit(now);
            self.last_pmt.insert(p, now);
        }
        if over(self.last_sdt, 2000) {
            self.errors.sdt.hit(now);
            self.last_sdt = Some(now);
        }
        if over(self.last_eit, 2000) {
            self.errors.eit.hit(now);
            self.last_eit = Some(now);
        }
        // A picture or sound PID its PMT names, absent for 5 s (data such as
        // SCTE-35 comes only now and then).
        let named: Vec<u16> = self
            .pmts
            .values()
            .flat_map(|p| p.streams.iter())
            .filter(|s| s.class == "video" || s.class == "audio")
            .map(|s| s.pid)
            .collect();
        for pid in named {
            if let Some(t) = self.seen_at.get(&pid).copied() {
                if now.duration_since(t).as_secs() >= 5 {
                    self.errors.pid.hit(now);
                    self.seen_at.insert(pid, now);
                }
            }
        }
        let (since, counts, total) = &self.window;
        let dt = now.duration_since(*since).as_secs_f64();
        if dt >= 2.0 {
            self.rates = self
                .pids
                .iter()
                .map(|(pid, p)| {
                    let before = counts.get(pid).copied().unwrap_or(0);
                    (
                        *pid,
                        (p.packets - before) as f64 * 188.0 * 8.0 / dt / 1000.0,
                    )
                })
                .collect();
            self.total_rate = (self.packets - total) as f64 * 188.0 * 8.0 / dt / 1000.0;
            let counts = self.pids.iter().map(|(k, p)| (*k, p.packets)).collect();
            self.window = (now, counts, self.packets);
        }
    }

    /// What has been read, now.
    pub fn view(&self, now: Instant) -> TsView {
        // Live: over the last seconds; a segment: over its PCR span.
        let (seconds, rate_of): (f64, Box<dyn Fn(u16, u64) -> f64>) = if self.live {
            let rates = self.rates.clone();
            (
                now.duration_since(self.started).as_secs_f64(),
                Box::new(move |pid, _| rates.get(&pid).copied().unwrap_or(0.0)),
            )
        } else {
            let span = self
                .pcr_span
                .values()
                .map(|(a, b)| b.saturating_sub(*a))
                .max()
                .unwrap_or(0) as f64
                / 27_000_000.0;
            (
                span,
                Box::new(move |_, n| {
                    if span > 0.0 {
                        n as f64 * 188.0 * 8.0 / span / 1000.0
                    } else {
                        0.0
                    }
                }),
            )
        };
        let kbps = if self.live {
            self.total_rate
        } else {
            rate_of(0xFFFF, self.packets)
        };
        let mut kinds: HashMap<u16, String> = HashMap::new();
        kinds.insert(0, "PAT".into());
        kinds.insert(1, "CAT".into());
        kinds.insert(self.nit_pid, "NIT".into());
        kinds.insert(0x11, "SDT / BAT".into());
        kinds.insert(0x12, "EIT".into());
        kinds.insert(0x14, "TDT / TOT".into());
        kinds.insert(0x1FFF, "Bourrage (NULL)".into());
        for (n, pid) in &self.pat {
            kinds.insert(*pid, format!("PMT {n}"));
        }
        let mut services = Vec::new();
        for (number, pmt_pid) in &self.pat {
            let pmt = self.pmts.get(number).cloned().unwrap_or_default();
            let sdt = self.sdt.get(number).cloned().unwrap_or_default();
            let mut streams = pmt.streams.clone();
            for s in &mut streams {
                if let Some(p) = self.profiles.get(&s.pid) {
                    s.profile = p.clone();
                }
                kinds.insert(s.pid, s.kind.clone());
            }
            let scrambled = sdt.free_ca
                || !pmt.ca.is_empty()
                || streams
                    .iter()
                    .any(|s| self.pids.get(&s.pid).is_some_and(|p| p.scrambled));
            let [now_ev, next_ev] = self.eit.get(number).cloned().unwrap_or_default();
            services.push(Service {
                number: *number,
                pmt_pid: *pmt_pid,
                pcr_pid: self.pmts.get(number).map(|p| p.pcr_pid),
                name: sdt.name,
                provider: sdt.provider,
                kind: service_kind(sdt.kind).to_string(),
                scrambled,
                ca_systems: pmt.ca,
                streams,
                now: now_ev,
                next: next_ev,
            });
        }
        let pids = self
            .pids
            .iter()
            .map(|(pid, p)| PidView {
                pid: *pid,
                kind: kinds.get(pid).cloned().unwrap_or_else(|| "Inconnu".into()),
                packets: p.packets,
                kbps: rate_of(*pid, p.packets),
                cc_errors: p.cc_errors,
                scrambled: p.scrambled,
                pcr: p.has_pcr,
            })
            .collect();
        let ago = |c: Count| c.last.map(|t| now.duration_since(t).as_secs_f64());
        let e = &self.errors;
        let mut checks = vec![
            (1, "1.1 TS_sync_loss", e.sync_loss),
            (1, "1.2 Sync_byte_error", e.sync_byte),
            (1, "1.3 PAT_error", e.pat),
            (1, "1.4 Continuity_count_error", e.cc),
            (1, "1.5 PMT_error", e.pmt),
            (1, "1.6 PID_error", e.pid),
            (2, "2.1 Transport_error", e.transport),
            (2, "2.2 CRC_error", e.crc),
            (2, "2.3a PCR_repetition_error", e.pcr_repetition),
            (2, "2.3b PCR_discontinuity_error", e.pcr_discontinuity),
        ];
        if self.tables.contains("SDT") {
            checks.push((3, "3.5 SDT_error", e.sdt));
        }
        if self.tables.contains("EIT p/f") {
            checks.push((3, "3.6 EIT_error", e.eit));
        }
        let checks = checks
            .into_iter()
            // Timed ones mean nothing in one segment.
            .filter(|(_, n, _)| {
                self.live
                    || !(n.contains("PAT_")
                        || n.contains("PMT_")
                        || n.contains("PID_")
                        || n.contains("SDT_")
                        || n.contains("EIT_"))
            })
            .map(|(priority, name, c)| Check {
                priority,
                name: name.to_string(),
                count: c.n,
                ago_s: ago(c),
            })
            .collect();
        TsView {
            live: self.live,
            packets: self.packets,
            seconds,
            kbps,
            ts_id: self.ts_id,
            network_id: self.network_id,
            network: self.network.clone(),
            utc: self
                .utc
                .map(|(t, at)| {
                    let t = if self.live {
                        t + now.duration_since(at).as_secs() as i64
                    } else {
                        t
                    };
                    iso(t)
                })
                .unwrap_or_default(),
            tables: self.tables.iter().map(|s| s.to_string()).collect(),
            eit_schedule: self.eit_schedule,
            services,
            pids,
            splices: self
                .splices
                .iter()
                .map(|(s, at)| Splice {
                    at_s: if self.live {
                        now.duration_since(*at).as_secs_f64()
                    } else {
                        s.at_s
                    },
                    ..s.clone()
                })
                .collect(),
            splice_nulls: self.splice_nulls,
            checks,
        }
    }
}

/// Takes the whole sections at the front of `buf`.
fn take_sections(buf: &mut Vec<u8>, out: &mut Vec<Vec<u8>>) {
    loop {
        if buf.len() < 3 {
            return;
        }
        if buf[0] == 0xFF {
            // Stuffing: the rest of the packet.
            buf.clear();
            return;
        }
        let len = 3 + (usize::from(buf[1] & 0x0F) << 8 | usize::from(buf[2]));
        if buf.len() < len {
            return;
        }
        out.push(buf.drain(..len).collect());
    }
}

/// Calls `f(tag, data)` for each descriptor of a loop.
fn descriptors(mut d: &[u8], mut f: impl FnMut(u8, &[u8])) {
    while d.len() >= 2 {
        let (tag, len) = (d[0], d[1] as usize);
        if 2 + len > d.len() {
            return;
        }
        f(tag, &d[2..2 + len]);
        d = &d[2 + len..];
    }
}

/// An elementary stream from its PMT entry.
fn es(stream_type: u8, pid: u16, d: &[u8], ca: &mut Vec<String>) -> Es {
    let (mut kind, mut class) = match stream_type {
        0x01 => ("MPEG-1 vidéo", "video"),
        0x02 => ("MPEG-2 vidéo", "video"),
        0x03 => ("MPEG-1 audio", "audio"),
        0x04 => ("MPEG-2 audio", "audio"),
        0x05 => ("Sections privées", "data"),
        0x06 => ("PES privé", "data"),
        0x0B => ("DSM-CC (carrousel)", "data"),
        0x0D => ("DSM-CC", "data"),
        0x0F => ("AAC (ADTS)", "audio"),
        0x10 => ("MPEG-4 vidéo", "video"),
        0x11 => ("AAC (LATM)", "audio"),
        0x15 => ("Métadonnées PES", "data"),
        0x1B => ("H.264 / AVC", "video"),
        0x1C => ("MPEG-4 audio", "audio"),
        0x21 => ("JPEG 2000", "video"),
        0x24 => ("H.265 / HEVC", "video"),
        0x2D => ("MPEG-H 3D audio", "audio"),
        0x32 => ("JPEG XS", "video"),
        0x33 => ("H.266 / VVC", "video"),
        0x81 => ("AC-3", "audio"),
        0x86 => ("SCTE-35", "data"),
        0x87 => ("E-AC-3", "audio"),
        0xD1 => ("Dirac", "video"),
        0xEA => ("VC-1", "video"),
        _ => ("Inconnu", "data"),
    };
    let mut e = Es {
        pid,
        stream_type,
        ..Default::default()
    };
    let mut details = Vec::new();
    let mut own_kind: Option<String> = None;
    descriptors(d, |tag, d| match tag {
        0x0A if d.len() >= 4 => {
            e.language = lang(&d[..3]);
            match d[3] {
                1 => details.push("Sans dialogues (clean effects)".into()),
                2 => details.push("Malentendants".into()),
                3 => details.push("Audiodescription".into()),
                _ => {}
            }
        }
        0x05 if d.len() >= 4 => {
            let id = String::from_utf8_lossy(&d[..4]).to_string();
            match &d[..4] {
                b"AC-3" => own_kind = Some("AC-3".into()),
                b"EAC3" => own_kind = Some("E-AC-3".into()),
                b"Opus" => own_kind = Some("Opus".into()),
                b"VANC" => {
                    own_kind = Some("SMPTE 2038 (ANC)".into());
                }
                b"KLVA" => own_kind = Some("KLV (SMPTE 336)".into()),
                b"ID3 " => own_kind = Some("ID3 (métadonnées)".into()),
                b"CUEI" => own_kind = Some("SCTE-35".into()),
                b"BSSD" => own_kind = Some("AES3 (SMPTE 302M)".into()),
                b"HEVC" => {}
                _ => details.push(format!("Enregistrement « {} »", id.trim())),
            }
        }
        0x6A => own_kind = Some("AC-3".into()),
        0x7A => own_kind = Some("E-AC-3".into()),
        0x7B => own_kind = Some("DTS".into()),
        0x7C => own_kind = Some("AAC".into()),
        0x56 | 0x46 => {
            own_kind = Some(if tag == 0x56 {
                "Télétexte".into()
            } else {
                "Télétexte VBI".into()
            });
            for p in d.as_chunks::<5>().0 {
                if e.language.is_empty() {
                    e.language = lang(&p[..3]);
                }
                let t = p[3] >> 3;
                let mag = match p[3] & 7 {
                    0 => 8,
                    m => m,
                };
                let page = format!("{mag}{:02X}", p[4]);
                let what = match t {
                    1 => "page initiale",
                    2 => "sous-titres",
                    3 => "index",
                    4 => "horaires",
                    5 => "sous-titres malentendants",
                    _ => "page",
                };
                details.push(format!("{} {page} ({})", what, lang(&p[..3])));
            }
        }
        0x59 => {
            own_kind = Some("Sous-titres DVB".into());
            for p in d.as_chunks::<8>().0 {
                if e.language.is_empty() {
                    e.language = lang(&p[..3]);
                }
                let t = p[3];
                let hard = (0x20..=0x24).contains(&t);
                details.push(format!(
                    "{}{} (page {})",
                    lang(&p[..3]),
                    if hard { ", malentendants" } else { "" },
                    u16::from_be_bytes([p[4], p[5]])
                ));
            }
        }
        0x86 if !d.is_empty() => {
            // ATSC caption service descriptor: 608 / 708 services.
            let n = (d[0] & 0x1F) as usize;
            for s in d[1..].as_chunks::<6>().0.iter().take(n) {
                let digital = s[3] & 0x80 != 0;
                let what = if digital {
                    format!("CEA-708 service {}", s[3] & 0x3F)
                } else {
                    format!("CEA-608 CC{}", (s[3] & 1) + 1)
                };
                details.push(format!("Sous-titres codés {what} ({})", lang(&s[..3])));
            }
        }
        0x8A => own_kind = Some("SCTE-35".into()),
        0x09 if d.len() >= 2 => ca.push(ca_system(u16::from_be_bytes([d[0], d[1]]))),
        0x28 if d.len() >= 3 => {
            e.profile = format!("{}@{}", avc_name(d[0]), avc_level(d[2]));
        }
        0x38 if d.len() >= 13 => {
            e.profile = format!(
                "{}@{}{}",
                hevc_name(d[0] & 0x1F),
                hevc_level(d[11]),
                if d[0] & 0x20 != 0 { " (High tier)" } else { "" }
            );
        }
        0x0E if d.len() >= 3 => {
            let rate = (u32::from(d[0] & 0x3F) << 16 | u32::from(d[1]) << 8 | u32::from(d[2])) * 50;
            details.push(format!("Débit max {} kb/s", rate * 8 / 1000));
        }
        0x52 if !d.is_empty() => details.push(format!("Composant {}", d[0])),
        0x66 => details.push("Diffusion de données".into()),
        0x45 => own_kind = Some("Données VBI".into()),
        0x7F if !d.is_empty() => match d[0] {
            0x06 if d.len() >= 2 => {
                let mix = d[1] >> 2 & 0x1F;
                details.push(match mix {
                    1 => "Audiodescription".into(),
                    2 => "Malentendants".into(),
                    3 => "Malvoyants (son)".into(),
                    _ => "Audio supplémentaire".into(),
                });
                if d[1] & 1 != 0 && d.len() >= 5 {
                    e.language = lang(&d[2..5]);
                }
            }
            0x15 => own_kind = Some("AC-4".into()),
            _ => {}
        },
        _ => {}
    });
    if let Some(k) = own_kind {
        class = match k.as_str() {
            "AC-3" | "E-AC-3" | "Opus" | "DTS" | "AAC" | "AC-4" | "AES3 (SMPTE 302M)" => "audio",
            "Télétexte" | "Télétexte VBI" | "Sous-titres DVB" => "subtitle",
            _ => "data",
        };
        e.kind = k;
    } else {
        e.kind = kind.to_string();
    }
    if e.kind.is_empty() {
        kind = "Inconnu";
        e.kind = kind.into();
    }
    e.class = class.to_string();
    e.details = details;
    e
}

/// "fra", "eng"…
fn lang(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// A DVB string (EN 300 468 annex A): its character table first.
pub fn dvb_text(b: &[u8]) -> String {
    let Some(&first) = b.first() else {
        return String::new();
    };
    let (body, utf8, latin_n) = match first {
        0x15 => (&b[1..], true, 0),
        0x10 if b.len() >= 3 => (&b[3..], false, b[2]),
        0x01..=0x0B => (&b[1..], false, first + 4),
        0x00..=0x1F => (&b[1..], false, 0),
        _ => (b, false, 0),
    };
    let text: String = if utf8 {
        String::from_utf8_lossy(body).into_owned()
    } else {
        body.iter()
            .map(|&c| match c {
                // Control codes: emphasis on and off, line break.
                0x86 | 0x87 => '\0',
                0x8A => '\n',
                0x80..=0x9F => '\0',
                // ISO 6937 (the default) and ISO 8859-n: Latin-1 for the
                // usual letters (accents of 6937 are combining: kept as is).
                _ if latin_n == 0 && (0xC1..=0xCF).contains(&c) => '\0',
                _ => char::from(c),
            })
            .filter(|c| *c != '\0')
            .collect()
    };
    text.trim().to_string()
}

/// MJD + BCD UTC time (5 bytes) as seconds since 1970.
fn dvb_seconds(b: &[u8]) -> Option<i64> {
    if b.len() < 5 || b == [0xFF; 5] {
        return None;
    }
    let mjd = i64::from(u16::from_be_bytes([b[0], b[1]]));
    let bcd = |v: u8| i64::from(v >> 4) * 10 + i64::from(v & 0x0F);
    let secs = bcd(b[2]) * 3600 + bcd(b[3]) * 60 + bcd(b[4]);
    // MJD 40587 = 1970-01-01.
    Some((mjd - 40587) * 86_400 + secs)
}

fn dvb_time(b: &[u8]) -> Option<String> {
    dvb_seconds(b).map(iso)
}

/// Seconds since 1970 as "YYYY-MM-DDTHH:MM:SSZ".
fn iso(t: i64) -> String {
    jiff::Timestamp::from_second(t)
        .map(|ts| ts.strftime("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn bcd_seconds(b: &[u8]) -> u32 {
    let bcd = |v: u8| u32::from(v >> 4) * 10 + u32::from(v & 0x0F);
    bcd(b[0]) * 3600 + bcd(b[1]) * 60 + bcd(b[2])
}

/// The 33 bits of a PTS-like field (in 5 bytes, the top bit last of the first).
fn pts(b: &[u8]) -> u64 {
    (u64::from(b[0] & 1) << 32)
        | u64::from(b[1]) << 24
        | u64::from(b[2]) << 16
        | u64::from(b[3]) << 8
        | u64::from(b[4])
}

fn running(r: u8) -> &'static str {
    match r {
        1 => "pas en cours",
        2 => "commence bientôt",
        3 => "en pause",
        4 => "en cours",
        5 => "hors antenne",
        _ => "",
    }
}

fn service_kind(t: u8) -> &'static str {
    match t {
        0x00 => "",
        0x01 => "TV numérique",
        0x02 => "Radio numérique",
        0x03 => "Télétexte",
        0x0A => "Radio numérique (codage avancé)",
        0x0C => "Données",
        0x11 => "TV HD MPEG-2",
        0x16 => "TV SD (codage avancé)",
        0x19 => "TV HD (codage avancé)",
        0x1F => "TV UHD (HEVC)",
        0x20 => "TV UHD (HEVC, HDR)",
        _ => "Autre",
    }
}

fn genre(c: u8) -> &'static str {
    match c >> 4 {
        0x1 => "Film / fiction",
        0x2 => "Information",
        0x3 => "Divertissement / jeu",
        0x4 => "Sport",
        0x5 => "Jeunesse",
        0x6 => "Musique / danse",
        0x7 => "Arts / culture",
        0x8 => "Société / politique",
        0x9 => "Éducation / science",
        0xA => "Loisirs",
        0xB => "Spécial",
        _ => "",
    }
}

fn segmentation(t: u8) -> &'static str {
    match t {
        0x00 => "Non indiqué",
        0x01 => "Changement de contenu",
        0x10 => "Début de programme",
        0x11 => "Fin de programme",
        0x12 => "Fin anticipée de programme",
        0x13 => "Programme interrompu",
        0x14 => "Programme repris",
        0x17 => "Programme dépassé",
        0x20 => "Début de chapitre",
        0x21 => "Fin de chapitre",
        0x22 => "Début de coupure",
        0x23 => "Fin de coupure",
        0x30 => "Début de publicité (fournisseur)",
        0x31 => "Fin de publicité (fournisseur)",
        0x32 => "Début de publicité (distributeur)",
        0x33 => "Fin de publicité (distributeur)",
        0x34 => "Début d’opportunité (fournisseur)",
        0x35 => "Fin d’opportunité (fournisseur)",
        0x36 => "Début d’opportunité (distributeur)",
        0x37 => "Fin d’opportunité (distributeur)",
        0x40 => "Début d’indisponibilité",
        0x41 => "Fin d’indisponibilité",
        0x50 => "Début de blackout réseau",
        0x51 => "Fin de blackout réseau",
        _ => "Segmentation",
    }
}

fn ca_system(id: u16) -> String {
    let who = match id >> 8 {
        0x01 => "Seca / Mediaguard",
        0x05 => "Viaccess",
        0x06 => "Irdeto",
        0x09 => "NDS / VideoGuard",
        0x0B => "Conax",
        0x0D => "CryptoWorks",
        0x0E => "PowerVu",
        0x17 => "BetaCrypt",
        0x18 => "Nagravision",
        0x26 => "BISS",
        0x4A => "DRE / autres",
        _ => "",
    };
    if who.is_empty() {
        format!("CA 0x{id:04X}")
    } else {
        format!("{who} (0x{id:04X})")
    }
}

fn avc_name(p: u8) -> &'static str {
    match p {
        66 => "Baseline",
        77 => "Main",
        88 => "Extended",
        100 => "High",
        110 => "High 10",
        122 => "High 4:2:2",
        244 => "High 4:4:4",
        _ => "Profil ?",
    }
}

fn avc_level(l: u8) -> String {
    if l == 9 {
        "1b".into()
    } else if l.is_multiple_of(10) {
        format!("{}", l / 10)
    } else {
        format!("{}.{}", l / 10, l % 10)
    }
}

fn hevc_name(p: u8) -> &'static str {
    match p {
        1 => "Main",
        2 => "Main 10",
        3 => "Main Still",
        4 => "RExt",
        5 => "High Throughput",
        9 => "SCC",
        _ => "Profil ?",
    }
}

fn hevc_level(l: u8) -> String {
    let v = l as f32 / 30.0;
    if (v - v.round()).abs() < 0.01 {
        format!("{}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// The profile and level of an H.264 SPS (from its NAL header).
fn avc_profile(nal: &[u8]) -> Option<String> {
    (nal.len() >= 4).then(|| format!("{}@{}", avc_name(nal[1]), avc_level(nal[3])))
}

/// The profile, level and tier of an HEVC SPS (from its NAL header).
fn hevc_profile(nal: &[u8]) -> Option<String> {
    // Header (2), vps id / sub-layers (1), profile (1), flags (4 + 6), level.
    let nal = unescape(&nal[..nal.len().min(32)]);
    (nal.len() > 14).then(|| {
        format!(
            "{}@{}{}",
            hevc_name(nal[3] & 0x1F),
            hevc_level(nal[14]),
            if nal[3] & 0x20 != 0 {
                " (High tier)"
            } else {
                ""
            }
        )
    })
}

/// Without the emulation prevention bytes (00 00 03).
fn unescape(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut zeros = 0;
    for &v in b {
        if zeros >= 2 && v == 3 {
            zeros = 0;
            continue;
        }
        zeros = if v == 0 { zeros + 1 } else { 0 };
        out.push(v);
    }
    out
}

/// CRC-32/MPEG-2 of a section with its CRC: 0 when right.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section with its length and CRC.
    fn section(table: u8, id: u16, body: &[u8]) -> Vec<u8> {
        let len = 5 + body.len() + 4;
        let mut s = vec![
            table,
            0xB0 | (len >> 8) as u8,
            len as u8,
            (id >> 8) as u8,
            id as u8,
            0xC1,
            0,
            0,
        ];
        s.extend_from_slice(body);
        let crc = crc32(&s);
        s.extend_from_slice(&crc.to_be_bytes());
        s
    }

    /// The packets carrying a section on `pid`.
    fn packets(pid: u16, s: &[u8], cc: &mut u8) -> Vec<u8> {
        let mut out = Vec::new();
        let mut data = vec![0u8];
        data.extend_from_slice(s);
        for (i, chunk) in data.chunks(184).enumerate() {
            let mut p = vec![
                SYNC,
                (if i == 0 { 0x40 } else { 0 }) | (pid >> 8) as u8,
                pid as u8,
                0x10 | *cc,
            ];
            *cc = (*cc + 1) & 15;
            p.extend_from_slice(chunk);
            p.resize(PACKET, 0xFF);
            out.extend(p);
        }
        out
    }

    fn short_event(name: &str, text: &str) -> Vec<u8> {
        let mut d = vec![0x4D, 0, b'f', b'r', b'e', name.len() as u8];
        d.extend_from_slice(name.as_bytes());
        d.push(text.len() as u8);
        d.extend_from_slice(text.as_bytes());
        d[1] = (d.len() - 2) as u8;
        d
    }

    fn stream() -> Vec<u8> {
        let mut out = Vec::new();
        let mut cc = [0u8; 8];
        // PAT: program 1 on PMT 0x100.
        out.extend(packets(
            0,
            &section(0x00, 7, &[0, 1, 0xE1, 0x00]),
            &mut cc[0],
        ));
        // PMT: H.264 on 0x101 (PCR), AAC in French on 0x102, SCTE-35 on 0x103.
        let mut pmt = vec![0xE1, 0x01, 0xF0, 0x00];
        pmt.extend([0x1B, 0xE1, 0x01, 0xF0, 0x00]);
        pmt.extend([
            0x0F, 0xE1, 0x02, 0xF0, 0x06, 0x0A, 0x04, b'f', b'r', b'a', 0x00,
        ]);
        pmt.extend([0x86, 0xE1, 0x03, 0xF0, 0x00]);
        out.extend(packets(0x100, &section(0x02, 1, &pmt), &mut cc[1]));
        // SDT: service 1 "Canal Test" by "Strata".
        let mut sdt = vec![0x20, 0x01, 0xFF];
        let mut sd = vec![0x48, 0, 0x19, 6];
        sd.extend_from_slice(b"Strata");
        sd.push(10);
        sd.extend_from_slice(b"Canal Test");
        sd[1] = (sd.len() - 2) as u8;
        sdt.extend([0, 1, 0xFC, 0x80, sd.len() as u8]);
        sdt.extend(sd);
        out.extend(packets(0x11, &section(0x42, 7, &sdt), &mut cc[2]));
        // EIT present: "Le Journal", 2026-10-31 20:00 UTC, 45 min, running.
        let mut eit = vec![0, 7, 0x20, 0x01, 0, 0x4E];
        // MJD of 2026-10-31: (1793491200 / 86400) + 40587 = 61344.
        let mjd = 61344u16.to_be_bytes();
        let ev = short_event("Le Journal", "Les titres du soir");
        let mut e = vec![
            0x12, 0x34, mjd[0], mjd[1], 0x20, 0x00, 0x00, 0x00, 0x45, 0x00,
        ];
        e.push(0x80 | (ev.len() >> 8) as u8);
        e.push(ev.len() as u8);
        e.extend(ev);
        eit.extend(e);
        out.extend(packets(0x12, &section(0x4E, 1, &eit), &mut cc[3]));
        // SCTE-35 splice_insert: event 42, out of network, 30 s.
        let mut sc = vec![0xFC, 0x30, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xF0, 0, 0x05];
        // Event 42, out, program, with a duration; splice at PTS 0; 30 s
        // (2 700 000 ticks of 90 kHz); program 1, avail 0 of 0.
        let cmd = [
            0, 0, 0, 42, 0x7F, 0xEF, 0xFE, 0, 0, 0, 0, 0xFE, 0x00, 0x29, 0x32, 0xE0, 0x00, 0x01,
            0x00, 0x00,
        ];
        sc[12] = cmd.len() as u8;
        sc.extend(cmd);
        sc.extend([0, 0]);
        let len = sc.len() + 4 - 3;
        sc[1] = 0x30 | (len >> 8) as u8;
        sc[2] = len as u8;
        let crc = crc32(&sc);
        sc.extend(crc.to_be_bytes());
        out.extend(packets(0x103, &sc, &mut cc[4]));
        out
    }

    #[test]
    fn reads_the_tables_epg_and_splices() {
        let mut m = TsMonitor::new(false);
        let s = stream();
        // In odd pieces, as a relay hands them over.
        for c in s.chunks(1000) {
            m.feed(c, Instant::now());
        }
        let v = m.view(Instant::now());
        assert_eq!(v.ts_id, Some(7));
        assert!(v.tables.contains(&"PAT".to_string()));
        let svc = &v.services[0];
        assert_eq!(svc.number, 1);
        assert_eq!(svc.name, "Canal Test");
        assert_eq!(svc.provider, "Strata");
        assert_eq!(svc.kind, "TV HD (codage avancé)");
        assert_eq!(svc.pcr_pid, Some(0x101));
        assert_eq!(svc.streams.len(), 3);
        assert_eq!(svc.streams[0].kind, "H.264 / AVC");
        assert_eq!(svc.streams[1].language, "fra");
        assert_eq!(svc.streams[2].kind, "SCTE-35");
        let now = svc.now.as_ref().expect("EPG now");
        assert_eq!(now.name, "Le Journal");
        assert_eq!(now.text, "Les titres du soir");
        assert_eq!(now.start, "2026-10-31T20:00:00Z");
        assert_eq!(now.duration_s, 45 * 60);
        assert_eq!(now.running, "en cours");
        let sp = &v.splices[0];
        assert_eq!(sp.command, "splice_insert");
        assert_eq!(sp.event_id, Some(42));
        assert_eq!(sp.out, Some(true));
        assert!(
            (sp.duration_s.unwrap() - 30.0).abs() < 0.01,
            "{:?}",
            sp.duration_s
        );
        assert!(v.checks.iter().all(|c| c.count == 0), "{:?}", v.checks);
    }

    #[test]
    fn counts_continuity_errors_and_resyncs() {
        let mut m = TsMonitor::new(true);
        let mut s = stream();
        // A packet lost (the second of the PAT's PID… the PMT one), garbage.
        s.drain(PACKET..2 * PACKET);
        s.splice(0..0, [1, 2, 3]);
        m.feed(&s, Instant::now());
        let v = m.view(Instant::now());
        let sync = v
            .checks
            .iter()
            .find(|c| c.name.contains("Sync_byte"))
            .unwrap();
        assert!(sync.count >= 1);
        assert!(!v.services.is_empty());
    }

    #[test]
    fn dvb_strings_and_profiles() {
        assert_eq!(dvb_text(b"\x15Fran\xc3\xa7ais"), "Français");
        assert_eq!(dvb_text(b"\x05Caf\xe9"), "Café");
        assert_eq!(avc_profile(&[0x67, 100, 0, 41]).unwrap(), "High@4.1");
        let mut sps = vec![
            0x42, 0x01, 0x01, 0x22, 0x60, 0, 0, 0, 0x90, 0, 0, 3, 0, 0, 0, 153,
        ];
        sps.resize(20, 0);
        // 00 00 03 escaped: level at index 14 once unescaped.
        assert_eq!(hevc_profile(&sps).unwrap(), "Main 10@5.1 (High tier)");
        assert!(TsMonitor::looks_like_ts(&stream()));
        assert!(!TsMonitor::looks_like_ts(b"#EXTM3U\n"));
    }
}
