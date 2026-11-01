//! Ember+ provider (Lawo's control protocol: VSM, Lawo, Cerebrum, Ross
//! panels…): S101 framing over TCP, Glow DTD 2.31 in BER.
//!
//! The tree:
//! - `1` strata: `info` (name, version, uptime, alarms on now, critical,
//!   acknowledgeAll), `channels` (per channel: the multiview on air, chosen
//!   from the library; its name), `inputs` (per input: name, live, status,
//!   alarms, loudness ×10).
//!
//! A consumer that opened a node gets the changes of its parameters pushed
//! (every 500 ms at most).

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use strata_mv_core::signal::Severity;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::state::Shared;

/// Ember+ settings (Settings › Network).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EmberConfig {
    pub enabled: bool,
    pub port: u16,
}

impl Default for EmberConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 9000,
        }
    }
}

// ----------------------------------------------------------------- S101 --

const BOF: u8 = 0xfe;
const EOF: u8 = 0xff;
const CE: u8 = 0xfd;
const XOR: u8 = 0x20;
const INV: u8 = 0xf8;

fn crc16(data: &[u8]) -> u16 {
    // CRC-16/X-25 (reflected 0x1021), as S101 uses it.
    let mut crc: u16 = 0xffff;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn escape_into(out: &mut Vec<u8>, b: u8) {
    if b >= INV {
        out.extend([CE, b ^ XOR]);
    } else {
        out.push(b);
    }
}

fn frame(body: &[u8]) -> Vec<u8> {
    let crc = !crc16(body);
    let mut out = vec![BOF];
    for &b in body.iter().chain(&crc.to_le_bytes()) {
        escape_into(&mut out, b);
    }
    out.push(EOF);
    out
}

/// A Glow packet as S101 frames (several above 1 kB).
pub fn ember_frames(ber: &[u8]) -> Vec<u8> {
    let chunks: Vec<&[u8]> = if ber.is_empty() {
        vec![&[][..]]
    } else {
        ber.chunks(1024).collect()
    };
    let mut out = Vec::new();
    let n = chunks.len();
    for (i, c) in chunks.into_iter().enumerate() {
        let flags = match (i == 0, i == n - 1) {
            (true, true) => 0xc0,
            (true, false) => 0x80,
            (false, true) => 0x40,
            (false, false) => 0x00,
        };
        // slot, message (EmBER), command (packet), version, flags, DTD (Glow),
        // 2 app bytes: Glow 2.31.
        let mut body = vec![0x00, 0x0e, 0x00, 0x01, flags, 0x01, 0x02, 0x1f, 0x02];
        body.extend_from_slice(c);
        out.extend(frame(&body));
    }
    out
}

fn keepalive_response() -> Vec<u8> {
    frame(&[0x00, 0x0e, 0x02, 0x01])
}

/// What a connection receives, frame by frame.
#[derive(Default)]
struct S101Reader {
    buf: Vec<u8>,
    escaped: bool,
    inside: bool,
    packet: Vec<u8>,
}

enum Incoming {
    KeepaliveRequest,
    Glow(Vec<u8>),
}

impl S101Reader {
    fn feed(&mut self, data: &[u8], out: &mut Vec<Incoming>) {
        for &b in data {
            match b {
                BOF => {
                    self.buf.clear();
                    self.escaped = false;
                    self.inside = true;
                }
                EOF if self.inside => {
                    self.inside = false;
                    let f = std::mem::take(&mut self.buf);
                    self.frame(&f, out);
                }
                CE => self.escaped = true,
                _ if self.inside => {
                    self.buf.push(if self.escaped { b ^ XOR } else { b });
                    self.escaped = false;
                }
                _ => {}
            }
        }
    }

    fn frame(&mut self, f: &[u8], out: &mut Vec<Incoming>) {
        if f.len() < 6 || crc16(f) != 0xf0b8 || f[0] != 0x00 || f[1] != 0x0e {
            return;
        }
        let body = &f[..f.len() - 2];
        match body[2] {
            0x01 => out.push(Incoming::KeepaliveRequest),
            0x00 if body.len() >= 8 => {
                let flags = body[4];
                let app = body[6] as usize;
                let Some(payload) = body.get(7 + app..) else {
                    return;
                };
                if flags & 0x80 != 0 {
                    self.packet.clear();
                }
                if flags & 0x20 == 0 {
                    self.packet.extend_from_slice(payload);
                }
                if flags & 0x40 != 0 {
                    out.push(Incoming::Glow(std::mem::take(&mut self.packet)));
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ BER --

fn len(out: &mut Vec<u8>, n: usize) {
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let b = (n as u32).to_be_bytes();
        let skip = b.iter().take_while(|&&x| x == 0).count();
        out.push(0x80 | (4 - skip) as u8);
        out.extend_from_slice(&b[skip..]);
    }
}

fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    len(&mut out, body.len());
    out.extend_from_slice(body);
    out
}

const fn app(n: u8) -> u8 {
    0x60 | n
}
const fn ctx(n: u8) -> u8 {
    0xa0 | n
}

fn int(v: i64) -> Vec<u8> {
    let b = v.to_be_bytes();
    let mut i = 0;
    while i < 7 && ((b[i] == 0 && b[i + 1] & 0x80 == 0) || (b[i] == 0xff && b[i + 1] & 0x80 != 0)) {
        i += 1;
    }
    tlv(0x02, &b[i..])
}

fn utf8(s: &str) -> Vec<u8> {
    tlv(0x0c, s.as_bytes())
}

fn boolean(v: bool) -> Vec<u8> {
    tlv(0x01, &[if v { 0xff } else { 0 }])
}

fn rel_oid(path: &[u32]) -> Vec<u8> {
    let mut body = Vec::new();
    for &a in path {
        let mut tmp = vec![(a & 0x7f) as u8];
        let mut v = a >> 7;
        while v > 0 {
            tmp.push(0x80 | (v & 0x7f) as u8);
            v >>= 7;
        }
        tmp.reverse();
        body.extend(tmp);
    }
    tlv(0x0d, &body)
}

fn cat(parts: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    parts.into_iter().flatten().collect()
}

/// A decoded TLV (single-byte tags: all Glow needs).
#[derive(Debug, Clone)]
enum Ber {
    Prim(u8, Vec<u8>),
    Cons(u8, Vec<Ber>),
}

impl Ber {
    fn tag(&self) -> u8 {
        match self {
            Ber::Prim(t, _) | Ber::Cons(t, _) => *t,
        }
    }
    fn children(&self) -> &[Ber] {
        match self {
            Ber::Cons(_, c) => c,
            Ber::Prim(..) => &[],
        }
    }
    fn child(&self, tag: u8) -> Option<&Ber> {
        self.children().iter().find(|c| c.tag() == tag)
    }
    /// The value inside an explicit context tag.
    fn inner(&self) -> Option<&Ber> {
        self.children().first()
    }
    fn as_int(&self) -> Option<i64> {
        match self {
            Ber::Prim(0x02, d) if !d.is_empty() && d.len() <= 8 => {
                let mut v: i64 = if d[0] & 0x80 != 0 { -1 } else { 0 };
                for &b in d {
                    v = (v << 8) | b as i64;
                }
                Some(v)
            }
            _ => None,
        }
    }
    fn as_bool(&self) -> Option<bool> {
        match self {
            Ber::Prim(0x01, d) => d.first().map(|&b| b != 0),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<String> {
        match self {
            Ber::Prim(0x0c, d) => Some(String::from_utf8_lossy(d).into_owned()),
            _ => None,
        }
    }
    fn as_oid(&self) -> Option<Vec<u32>> {
        match self {
            Ber::Prim(0x0d, d) => {
                let mut out = Vec::new();
                let mut v: u32 = 0;
                for &b in d {
                    v = (v << 7) | (b & 0x7f) as u32;
                    if b & 0x80 == 0 {
                        out.push(v);
                        v = 0;
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
}

/// One TLV from `buf` and the bytes it took (definite and indefinite lengths).
fn parse(buf: &[u8]) -> Option<(Ber, usize)> {
    let tag = *buf.first()?;
    if tag & 0x1f == 0x1f {
        return None; // multi-byte tags: not in Glow
    }
    let first = *buf.get(1)? as usize;
    let constructed = tag & 0x20 != 0;
    let (length, mut at) = if first == 0x80 {
        (None, 2)
    } else if first & 0x80 != 0 {
        let n = first & 0x7f;
        if n > 4 {
            return None;
        }
        let mut l = 0usize;
        for i in 0..n {
            l = (l << 8) | *buf.get(2 + i)? as usize;
        }
        (Some(l), 2 + n)
    } else {
        (Some(first), 2)
    };
    match (constructed, length) {
        (false, Some(l)) => Some((Ber::Prim(tag, buf.get(at..at + l)?.to_vec()), at + l)),
        (false, None) => None,
        (true, Some(l)) => {
            let end = at + l;
            let body = buf.get(at..end)?;
            let mut kids = Vec::new();
            let mut i = 0;
            while i < body.len() {
                let (k, n) = parse(&body[i..])?;
                kids.push(k);
                i += n;
            }
            Some((Ber::Cons(tag, kids), end))
        }
        (true, None) => {
            let mut kids = Vec::new();
            loop {
                if buf.get(at..at + 2)? == [0, 0] {
                    return Some((Ber::Cons(tag, kids), at + 2));
                }
                let (k, n) = parse(&buf[at..])?;
                kids.push(k);
                at += n;
            }
        }
    }
}

// ----------------------------------------------------------- the tree --

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Int(i64),
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone)]
enum Item {
    Node {
        number: u32,
        identifier: String,
        description: String,
        children: Vec<Item>,
    },
    Param {
        number: u32,
        identifier: String,
        description: String,
        value: Val,
        writable: bool,
        /// 1 integer, 3 string, 4 boolean, 6 enum.
        kind: u8,
        enumeration: Option<String>,
        factor: Option<i64>,
        format: Option<String>,
    },
}

impl Item {
    fn number(&self) -> u32 {
        match self {
            Item::Node { number, .. } | Item::Param { number, .. } => *number,
        }
    }
}

fn node(number: u32, identifier: &str, description: &str, children: Vec<Item>) -> Item {
    Item::Node {
        number,
        identifier: identifier.into(),
        description: description.into(),
        children,
    }
}

fn param(number: u32, identifier: &str, description: &str, value: Val) -> Item {
    let kind = match value {
        Val::Int(_) => 1,
        Val::Str(_) => 3,
        Val::Bool(_) => 4,
    };
    Item::Param {
        number,
        identifier: identifier.into(),
        description: description.into(),
        value,
        writable: false,
        kind,
        enumeration: None,
        factor: None,
        format: None,
    }
}

/// The multiviews to choose from: (project id, name), by name.
async fn library(s: &Shared) -> Vec<(String, String)> {
    let lib = s.library.read().await;
    let mut v: Vec<(String, String)> = lib.list().map(|p| (p.id.clone(), p.name.clone())).collect();
    v.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    v
}

async fn tree(s: &Shared) -> Item {
    let lib = library(s).await;
    let enumeration = std::iter::once("(aucun)".to_string())
        .chain(lib.iter().map(|(_, n)| n.replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n");
    let signals = s.inputs.signals();
    let (_, journal) = signals.journal();
    let open: Vec<_> = journal.iter().filter(|e| e.ended.is_none()).collect();
    let critical = open
        .iter()
        .filter(|e| e.severity == Severity::Critical)
        .count();

    let mut info = vec![
        param(1, "name", "Server name", Val::Str(s.config.name.clone())),
        param(
            2,
            "version",
            "Version",
            Val::Str(env!("CARGO_PKG_VERSION").into()),
        ),
        param(
            3,
            "uptime",
            "Seconds since the start",
            Val::Int(
                s.started_at
                    .elapsed()
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            ),
        ),
        param(4, "alarms", "Faults on now", Val::Int(open.len() as i64)),
        param(
            5,
            "critical",
            "Critical faults on now",
            Val::Int(critical as i64),
        ),
    ];
    let mut ack = param(
        6,
        "acknowledgeAll",
        "Acknowledge every fault (write true)",
        Val::Bool(false),
    );
    if let Item::Param { writable, .. } = &mut ack {
        *writable = true;
    }
    info.push(ack);

    let mut channels = Vec::new();
    for (i, c) in s.config.channels.iter().enumerate() {
        let loaded = match s.channel(&c.id) {
            Some(ch) => ch.project_id.read().await.clone(),
            None => None,
        };
        let index = loaded
            .and_then(|pid| lib.iter().position(|(id, _)| *id == pid))
            .map_or(0, |p| p + 1);
        let mv = Item::Param {
            number: 1,
            identifier: "multiview".into(),
            description: "Multiview on air".into(),
            value: Val::Int(index as i64),
            writable: true,
            kind: 6,
            enumeration: Some(enumeration.clone()),
            factor: None,
            format: None,
        };
        channels.push(node(
            i as u32 + 1,
            &c.id,
            &c.name,
            vec![mv, param(2, "name", "Name", Val::Str(c.name.clone()))],
        ));
    }

    let mut inputs = Vec::new();
    for (j, i) in s.config.inputs.iter().enumerate() {
        let alarms = signals.alarms(&i.id);
        let status = if alarms.iter().any(|a| a.severity == Severity::Critical) {
            2
        } else if alarms.is_empty() {
            0
        } else {
            1
        };
        let text = alarms
            .iter()
            .map(|a| a.kind.text("fr"))
            .collect::<Vec<_>>()
            .join(", ");
        let live = s.inputs.status().iter().any(|st| st.id == i.id && st.live);
        let lufs = signals
            .loudness_of(&i.id)
            .0
            .map(|l| (l.short_term * 10.0).round() as i64)
            .unwrap_or(-990);
        let st = Item::Param {
            number: 3,
            identifier: "status".into(),
            description: "Status".into(),
            value: Val::Int(status),
            writable: false,
            kind: 6,
            enumeration: Some("ok\nwarning\ncritical".into()),
            factor: None,
            format: None,
        };
        let loud = Item::Param {
            number: 5,
            identifier: "loudness".into(),
            description: "Short-term loudness".into(),
            value: Val::Int(lufs),
            writable: false,
            kind: 1,
            enumeration: None,
            factor: Some(10),
            format: Some("%.1f LUFS".into()),
        };
        inputs.push(node(
            j as u32 + 1,
            &i.id,
            &i.name,
            vec![
                param(1, "name", "Name", Val::Str(i.name.clone())),
                param(2, "live", "Pictures coming in", Val::Bool(live)),
                st,
                param(4, "alarms", "Faults on now", Val::Str(text)),
                loud,
            ],
        ));
    }

    node(
        1,
        "strata",
        "Strata Multiview",
        vec![
            node(1, "info", "Server", info),
            node(2, "channels", "Channels", channels),
            node(3, "inputs", "Inputs", inputs),
        ],
    )
}

fn find<'a>(root: &'a Item, path: &[u32]) -> Option<&'a Item> {
    let (first, rest) = path.split_first()?;
    if root.number() != *first {
        return None;
    }
    let mut at = root;
    for n in rest {
        let Item::Node { children, .. } = at else {
            return None;
        };
        at = children.iter().find(|c| c.number() == *n)?;
    }
    Some(at)
}

/// Every parameter's value, by path.
fn values(it: &Item, path: &mut Vec<u32>, out: &mut BTreeMap<Vec<u32>, Val>) {
    path.push(it.number());
    match it {
        Item::Node { children, .. } => {
            for c in children {
                values(c, path, out);
            }
        }
        Item::Param { value, .. } => {
            out.insert(path.clone(), value.clone());
        }
    }
    path.pop();
}

fn val_ber(v: &Val) -> Vec<u8> {
    match v {
        Val::Int(i) => int(*i),
        Val::Bool(b) => boolean(*b),
        Val::Str(s) => utf8(s),
    }
}

/// The contents of an element (the SET).
fn contents(it: &Item, value_only: bool) -> Vec<u8> {
    match it {
        Item::Node {
            identifier,
            description,
            ..
        } => tlv(
            0x31,
            &cat([
                tlv(ctx(0), &utf8(identifier)),
                tlv(ctx(1), &utf8(description)),
                tlv(ctx(3), &boolean(true)),
            ]),
        ),
        Item::Param {
            identifier,
            description,
            value,
            writable,
            kind,
            enumeration,
            factor,
            format,
            ..
        } => {
            if value_only {
                return tlv(0x31, &tlv(ctx(2), &val_ber(value)));
            }
            let mut p = vec![
                tlv(ctx(0), &utf8(identifier)),
                tlv(ctx(1), &utf8(description)),
                tlv(ctx(2), &val_ber(value)),
            ];
            p.push(tlv(ctx(5), &int(if *writable { 3 } else { 1 })));
            if let Some(f) = format {
                p.push(tlv(ctx(6), &utf8(f)));
            }
            if let Some(e) = enumeration {
                p.push(tlv(ctx(7), &utf8(e)));
            }
            if let Some(f) = factor {
                p.push(tlv(ctx(8), &int(*f)));
            }
            p.push(tlv(ctx(9), &boolean(true)));
            p.push(tlv(ctx(13), &int(*kind as i64)));
            tlv(0x31, &cat(p))
        }
    }
}

/// A numbered element with its contents (no children).
fn numbered(it: &Item) -> Vec<u8> {
    let tag = match it {
        Item::Node { .. } => app(3),
        Item::Param { .. } => app(1),
    };
    tlv(
        tag,
        &cat([
            tlv(ctx(0), &int(it.number() as i64)),
            tlv(ctx(1), &contents(it, false)),
        ]),
    )
}

fn root(elements: Vec<Vec<u8>>) -> Vec<u8> {
    let coll = cat(elements.into_iter().map(|e| tlv(ctx(0), &e)));
    tlv(app(0), &tlv(app(11), &coll))
}

/// The answer to a GetDirectory on `path` (the root when empty).
fn directory(t: &Item, path: &[u32]) -> Option<Vec<u8>> {
    if path.is_empty() {
        return Some(root(vec![numbered(t)]));
    }
    let it = find(t, path)?;
    Some(match it {
        Item::Node { children, .. } => {
            let kids = cat(children.iter().map(|c| tlv(ctx(0), &numbered(c))));
            let q = cat([
                tlv(ctx(0), &rel_oid(path)),
                tlv(ctx(1), &contents(it, false)),
                tlv(ctx(2), &tlv(app(4), &kids)),
            ]);
            root(vec![tlv(app(10), &q)])
        }
        Item::Param { .. } => {
            let q = cat([
                tlv(ctx(0), &rel_oid(path)),
                tlv(ctx(1), &contents(it, false)),
            ]);
            root(vec![tlv(app(9), &q)])
        }
    })
}

fn value_update(t: &Item, path: &[u32]) -> Option<Vec<u8>> {
    let it = find(t, path)?;
    Some(tlv(
        app(9),
        &cat([
            tlv(ctx(0), &rel_oid(path)),
            tlv(ctx(1), &contents(it, true)),
        ]),
    ))
}

// ------------------------------------------------------- the requests --

enum Request {
    Directory(Vec<u32>),
    Set(Vec<u32>, Val),
}

/// What a Glow packet asks.
fn requests(packet: &[u8]) -> Vec<Request> {
    let mut out = Vec::new();
    let Some((r, _)) = parse(packet) else {
        return out;
    };
    if r.tag() != app(0) {
        return out;
    }
    for coll in r.children().iter().filter(|c| c.tag() == app(11)) {
        for el in coll.children() {
            if let Some(e) = el.inner() {
                walk(e, &[], &mut out);
            }
        }
    }
    out
}

fn walk(e: &Ber, parent: &[u32], out: &mut Vec<Request>) {
    let tag = e.tag();
    // Command at this level: getDirectory (32) of the parent.
    if tag == app(2) {
        if e.child(ctx(0)).and_then(Ber::inner).and_then(Ber::as_int) == Some(32) {
            out.push(Request::Directory(parent.to_vec()));
        }
        return;
    }
    let path: Vec<u32> = match tag {
        t if t == app(9) || t == app(10) => {
            match e.child(ctx(0)).and_then(Ber::inner).and_then(Ber::as_oid) {
                Some(p) => p,
                None => return,
            }
        }
        t if t == app(1) || t == app(3) => {
            match e.child(ctx(0)).and_then(Ber::inner).and_then(Ber::as_int) {
                Some(n) => parent.iter().copied().chain([n as u32]).collect(),
                None => return,
            }
        }
        _ => return,
    };
    // A value written to a parameter.
    if tag == app(9) || tag == app(1) {
        if let Some(v) = e
            .child(ctx(1))
            .and_then(Ber::inner)
            .and_then(|set| set.child(ctx(2)))
            .and_then(Ber::inner)
        {
            let val = v
                .as_int()
                .map(Val::Int)
                .or_else(|| v.as_bool().map(Val::Bool))
                .or_else(|| v.as_str().map(Val::Str));
            if let Some(val) = val {
                out.push(Request::Set(path.clone(), val));
            }
        }
    }
    if let Some(kids) = e.child(ctx(2)) {
        for coll in kids.children() {
            for k in coll.children() {
                if let Some(inner) = k.inner() {
                    walk(inner, &path, out);
                } else {
                    walk(k, &path, out);
                }
            }
        }
    }
}

/// Applies a value written by a consumer.
async fn set(s: &Shared, t: &Item, path: &[u32], v: &Val) -> Result<(), String> {
    let Some(Item::Param {
        identifier,
        writable: true,
        ..
    }) = find(t, path)
    else {
        return Err("read-only".into());
    };
    match (path, identifier.as_str(), v) {
        ([1, 1, 6], "acknowledgeAll", Val::Bool(true))
        | ([1, 1, 6], "acknowledgeAll", Val::Int(1)) => {
            let ev = s.inputs.signals().ack(None, None, "Ember+");
            tracing::info!("Ember+: {} fault(s) acknowledged", ev.len());
            s.alarms.dispatch(ev);
            Ok(())
        }
        ([1, 2, ch, 1], "multiview", Val::Int(i)) => {
            let c = s
                .config
                .channels
                .get(*ch as usize - 1)
                .ok_or("no such channel")?;
            let lib = library(s).await;
            let pid = match *i {
                0 => None,
                n => Some(
                    lib.get(n as usize - 1)
                        .ok_or("no such multiview")?
                        .0
                        .clone(),
                ),
            };
            tracing::info!("Ember+: channel {} shows {:?}", c.id, pid);
            s.load_project(&c.id, pid).await
        }
        _ => Ok(()),
    }
}

// --------------------------------------------------------- the server --

/// Serves Ember+ on its port while the engine cycle runs.
pub fn spawn(s: Shared) {
    let c = s.config.ember.clone();
    if !c.enabled {
        return;
    }
    let mut closing = s.closing.subscribe();
    tokio::spawn(async move {
        let listener = match TcpListener::bind(("0.0.0.0", c.port)).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Ember+: cannot listen on port {}: {e}", c.port);
                return;
            }
        };
        tracing::info!("Ember+ provider on port {}", c.port);
        loop {
            tokio::select! {
                r = listener.accept() => {
                    if let Ok((sock, peer)) = r {
                        tracing::info!("Ember+: consumer {peer}");
                        tokio::spawn(connection(s.clone(), sock, s.closing.subscribe()));
                    }
                }
                _ = closing.wait_for(|c| *c) => break,
            }
        }
    });
}

async fn connection(
    s: Shared,
    mut sock: TcpStream,
    mut closing: tokio::sync::watch::Receiver<bool>,
) {
    let stop = async move {
        let _ = closing.wait_for(|c| *c).await;
    };
    tokio::pin!(stop);
    let _ = sock.set_nodelay(true);
    let mut reader = S101Reader::default();
    let mut buf = vec![0u8; 16384];
    // Nodes the consumer opened: their parameters' changes are pushed.
    let mut opened: HashSet<Vec<u32>> = HashSet::new();
    let mut sent: BTreeMap<Vec<u32>, Val> = BTreeMap::new();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
        let mut out = Vec::new();
        tokio::select! {
            r = sock.read(&mut buf) => {
                let n = match r {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let mut msgs = Vec::new();
                reader.feed(&buf[..n], &mut msgs);
                for m in msgs {
                    match m {
                        Incoming::KeepaliveRequest => out.extend(keepalive_response()),
                        Incoming::Glow(p) => {
                            let t = tree(&s).await;
                            for req in requests(&p) {
                                match req {
                                    Request::Directory(path) => {
                                        if let Some(b) = directory(&t, &path) {
                                            out.extend(ember_frames(&b));
                                        }
                                        let mut all = BTreeMap::new();
                                        values(&t, &mut Vec::new(), &mut all);
                                        for (k, v) in all {
                                            if k.len() == path.len() + 1 && k.starts_with(&path) {
                                                sent.insert(k, v);
                                            }
                                        }
                                        opened.insert(path);
                                    }
                                    Request::Set(path, v) => {
                                        if let Err(e) = set(&s, &t, &path, &v).await {
                                            tracing::debug!("Ember+: set {path:?}: {e}");
                                        }
                                        // The value as it now is, written or not.
                                        let t = tree(&s).await;
                                        if let Some(u) = value_update(&t, &path) {
                                            out.extend(ember_frames(&root(vec![u])));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ = tick.tick() => {
                if opened.is_empty() {
                    continue;
                }
                let t = tree(&s).await;
                let mut all = BTreeMap::new();
                values(&t, &mut Vec::new(), &mut all);
                let mut updates = Vec::new();
                for (k, v) in all {
                    let seen = opened.contains(&k[..k.len() - 1]);
                    if seen && sent.get(&k) != Some(&v) {
                        if let Some(u) = value_update(&t, &k) {
                            updates.push(u);
                        }
                        sent.insert(k, v);
                    }
                }
                if !updates.is_empty() {
                    out.extend(ember_frames(&root(updates)));
                }
            }
            _ = &mut stop => break,
        }
        if !out.is_empty() && sock.write_all(&out).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s101_round_trip() {
        let ber = vec![0x60, 0x03, 0x6b, 0x01, 0xff];
        let wire = ember_frames(&ber);
        assert_eq!(wire[0], BOF);
        assert_eq!(*wire.last().unwrap(), EOF);
        let mut r = S101Reader::default();
        let mut got = Vec::new();
        r.feed(&wire, &mut got);
        assert!(matches!(&got[..], [Incoming::Glow(p)] if *p == ber));
        // Large packets go in several frames.
        let big = vec![0x42u8; 3000];
        let mut got = Vec::new();
        r.feed(&ember_frames(&big), &mut got);
        assert!(matches!(&got[..], [Incoming::Glow(p)] if *p == big));
    }

    #[test]
    fn ber_definite_and_indefinite() {
        let def = tlv(
            app(3),
            &cat([
                tlv(ctx(0), &int(300)),
                tlv(ctx(1), &tlv(0x31, &tlv(ctx(0), &utf8("é")))),
            ]),
        );
        let (b, n) = parse(&def).unwrap();
        assert_eq!(n, def.len());
        assert_eq!(
            b.child(ctx(0)).and_then(Ber::inner).and_then(Ber::as_int),
            Some(300)
        );
        // Indefinite: 63 80 A0 80 02 01 05 00 00 00 00
        let ind = [0x63, 0x80, 0xa0, 0x80, 0x02, 0x01, 0x05, 0, 0, 0, 0];
        let (b, n) = parse(&ind).unwrap();
        assert_eq!(n, ind.len());
        assert_eq!(
            b.child(ctx(0)).and_then(Ber::inner).and_then(Ber::as_int),
            Some(5)
        );
        assert_eq!(rel_oid(&[1, 2, 300]), [0x0d, 4, 1, 2, 0x82, 0x2c]);
        let (o, _) = parse(&rel_oid(&[1, 2, 300])).unwrap();
        assert_eq!(o.as_oid(), Some(vec![1, 2, 300]));
    }

    #[test]
    fn requests_are_read() {
        // Root { elements { [0] Command getDirectory } }
        let cmd = tlv(app(2), &tlv(ctx(0), &int(32)));
        let p = root(vec![cmd.clone()]);
        assert!(matches!(&requests(&p)[..], [Request::Directory(v)] if v.is_empty()));
        // QualifiedNode 1.2 { children { Command getDirectory } }
        let q = tlv(
            app(10),
            &cat([
                tlv(ctx(0), &rel_oid(&[1, 2])),
                tlv(ctx(2), &tlv(app(4), &tlv(ctx(0), &cmd))),
            ]),
        );
        assert!(
            matches!(&requests(&root(vec![q]))[..], [Request::Directory(v)] if *v == vec![1, 2])
        );
        // QualifiedParameter 1.2.1.1 { contents { value 3 } }
        let set = tlv(
            app(9),
            &cat([
                tlv(ctx(0), &rel_oid(&[1, 2, 1, 1])),
                tlv(ctx(1), &tlv(0x31, &tlv(ctx(2), &int(3)))),
            ]),
        );
        assert!(
            matches!(&requests(&root(vec![set]))[..], [Request::Set(p, Val::Int(3))] if *p == vec![1, 2, 1, 1])
        );
        // Nested: Node 1 { Node 3 { Command } }
        let nested = tlv(
            app(3),
            &cat([
                tlv(ctx(0), &int(1)),
                tlv(
                    ctx(2),
                    &tlv(
                        app(4),
                        &tlv(
                            ctx(0),
                            &tlv(
                                app(3),
                                &cat([
                                    tlv(ctx(0), &int(3)),
                                    tlv(ctx(2), &tlv(app(4), &tlv(ctx(0), &cmd))),
                                ]),
                            ),
                        ),
                    ),
                ),
            ]),
        );
        assert!(
            matches!(&requests(&root(vec![nested]))[..], [Request::Directory(v)] if *v == vec![1, 3])
        );
    }

    #[test]
    fn crc_matches_s101() {
        // The S101 check: the CRC of a frame with its CRC is 0xF0B8.
        let body = [0x00, 0x0e, 0x02, 0x01];
        let crc = !crc16(&body);
        let mut all = body.to_vec();
        all.extend(crc.to_le_bytes());
        assert_eq!(crc16(&all), 0xf0b8);
    }
}
