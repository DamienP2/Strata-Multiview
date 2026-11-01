//! AMWA NMOS: the server as a Node of a broadcast IP plant.
//!
//! - IS-04 v1.3 (discovery and registration): the Node API under
//!   `/x-nmos/node/v1.3/` and, when a registry is set, registration and
//!   heartbeats. One device; for each ST 2110 output of a channel a source, a
//!   flow (video/raw) and a sender (RTP); for each UDP / RTP input a receiver
//!   (MPEG-TS).
//! - IS-05 v1.1 (connection management) under `/x-nmos/connection/v1.1/`:
//!   staged and active parameters, immediate or scheduled activation, bulk,
//!   transport files. Activating a sender changes its destination (or turns
//!   it on or off), a receiver the multicast group its input joins; both are
//!   saved to the settings, the engine applying them as for any settings
//!   change (the inputs keep running).
//!
//! The ids are UUIDs derived from a seed kept in `<data>/nmos-seed`: stable
//! from one start to the next, unique to this installation.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::inputs::InputKind;
use crate::state::{AppState, Shared};

/// NMOS settings (Settings › Network).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NmosConfig {
    pub enabled: bool,
    /// The registry: "http://registry.example.com:8010" ("": none, the Node
    /// API is still served for peer-to-peer control).
    pub registry: String,
    /// The address announced in the Node's endpoints ("": the first one of
    /// the machine).
    pub host: String,
}

/// Bumped at each start of the engine: the resources' version.
static CYCLE: AtomicU64 = AtomicU64::new(0);

/// A new engine cycle: new resource versions.
pub fn cycle_started() {
    CYCLE.store(now_ns(), Ordering::Relaxed);
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// "seconds:nanoseconds" (TAI: UTC + 37 s).
fn tai(ns: u64) -> String {
    let ns = ns + 37_000_000_000;
    format!("{}:{}", ns / 1_000_000_000, ns % 1_000_000_000)
}

fn version() -> String {
    let v = CYCLE.load(Ordering::Relaxed);
    tai(if v == 0 { now_ns() } else { v })
}

/// The installation's seed (made once).
fn seed(data_dir: &Path) -> Uuid {
    static SEED: LazyLock<Mutex<Option<Uuid>>> = LazyLock::new(|| Mutex::new(None));
    let mut s = SEED.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(u) = *s {
        return u;
    }
    let path = data_dir.join("nmos-seed");
    let u = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| Uuid::parse_str(t.trim()).ok())
        .unwrap_or_else(|| {
            let u = Uuid::new_v4();
            let _ = std::fs::write(&path, u.to_string());
            u
        });
    *s = Some(u);
    u
}

fn uid(seed: &Uuid, what: &str) -> String {
    Uuid::new_v5(seed, what.as_bytes()).to_string()
}

// ------------------------------------------------------------ the model --

/// What a sender stands for.
#[derive(Clone)]
struct SenderRef {
    channel: String,
    output: String,
    key: bool,
}

#[derive(Clone)]
struct ReceiverRef {
    input: String,
}

#[derive(Clone)]
struct Model {
    base: String,
    node: Value,
    device: Value,
    sources: Vec<Value>,
    flows: Vec<Value>,
    senders: Vec<(SenderRef, Value)>,
    receivers: Vec<(ReceiverRef, Value)>,
}

fn core(id: &str, label: &str, description: &str) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert("id".into(), id.into());
    m.insert("version".into(), version().into());
    m.insert("label".into(), label.into());
    m.insert("description".into(), description.into());
    m.insert("tags".into(), json!({}));
    m
}

fn with(mut m: serde_json::Map<String, Value>, extra: Value) -> Value {
    if let Value::Object(e) = extra {
        m.extend(e);
    }
    Value::Object(m)
}

/// The machine's interfaces: (name, IPv4, MAC as aa-bb-cc-dd-ee-ff).
fn interfaces() -> Vec<(String, Ipv4Addr, Option<String>)> {
    let mut out: Vec<(String, Ipv4Addr, Option<String>)> = Vec::new();
    for i in if_addrs::get_if_addrs().unwrap_or_default() {
        let std::net::IpAddr::V4(ip) = i.ip() else {
            continue;
        };
        if ip.is_loopback() || out.iter().any(|(n, _, _)| *n == i.name) {
            continue;
        }
        let mac = std::fs::read_to_string(format!("/sys/class/net/{}/address", i.name))
            .ok()
            .map(|m| m.trim().replace(':', "-").to_lowercase())
            .filter(|m| m.len() == 17 && m != "00-00-00-00-00-00");
        out.push((i.name, ip, mac));
    }
    out
}

/// The interface sending from (or receiving on) `ip`, else the first one.
fn binding(ifs: &[(String, Ipv4Addr, Option<String>)], ip: Option<Ipv4Addr>) -> Vec<String> {
    ip.and_then(|ip| ifs.iter().find(|(_, a, _)| *a == ip))
        .or(ifs.first())
        .map(|(n, _, _)| vec![n.clone()])
        .unwrap_or_default()
}

fn host_of(s: &AppState, ifs: &[(String, Ipv4Addr, Option<String>)]) -> String {
    let h = s.config.nmos.host.trim();
    if !h.is_empty() {
        return h.to_string();
    }
    if !s.bind.ip().is_unspecified() && !s.bind.ip().is_loopback() {
        return s.bind.ip().to_string();
    }
    ifs.first()
        .map(|(_, ip, _)| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".into())
}

fn multicast(ip: &Ipv4Addr) -> bool {
    ip.is_multicast()
}

/// An input's address: (multicast group or None, port, source filter, interface).
fn input_params(url: &str) -> (Option<Ipv4Addr>, u16, Option<Ipv4Addr>, Option<Ipv4Addr>) {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let (addr, query) = rest.split_once('?').unwrap_or((rest, ""));
    let addr = addr.trim_end_matches('/');
    let addr = addr.rsplit_once('@').map_or(addr, |(_, a)| a);
    let (host, port) = addr.rsplit_once(':').unwrap_or((addr, "5004"));
    let group = host.parse::<Ipv4Addr>().ok().filter(multicast);
    let port = port.parse().unwrap_or(5004);
    let q = |k: &str| {
        query
            .split('&')
            .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.parse::<Ipv4Addr>().ok())
    };
    (group, port, q("sources"), q("localaddr"))
}

fn input_transport(url: &str, group: Option<Ipv4Addr>) -> &'static str {
    match (url.starts_with("rtp://"), group.is_some()) {
        (true, true) => "urn:x-nmos:transport:rtp.mcast",
        (true, false) => "urn:x-nmos:transport:rtp.ucast",
        (false, true) => "urn:x-nmos:transport:udp.mcast",
        (false, false) => "urn:x-nmos:transport:udp.ucast",
    }
}

/// The NMOS resources of the server, built from its settings.
fn model(s: &AppState) -> Model {
    let seed = seed(&s.data_dir);
    let ifs = interfaces();
    let host = host_of(s, &ifs);
    let tls = s.config.tls.enabled;
    let protocol = if tls { "https" } else { "http" };
    let port = s.bind.port();
    let shown_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.clone()
    };
    let base = format!("{protocol}://{shown_host}:{port}");
    let hostname = gethostname::gethostname().to_string_lossy().into_owned();
    let node_id = uid(&seed, "node");
    let device_id = uid(&seed, "device");
    let name = if s.config.name.trim().is_empty() {
        "Strata Multiview".to_string()
    } else {
        s.config.name.clone()
    };

    // PTP when followed and locked, else the machine's clock.
    let sys = s.inputs.signals().system();
    let gmid = sys
        .ptp
        .grandmaster
        .to_lowercase()
        .replace([':', '.'], "-")
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || *c == '-')
        .collect::<String>();
    let gm_ok = gmid.len() == 23 && gmid.split('-').count() == 8;
    let clock = if sys.ptp.running && gm_ok {
        json!({ "name": "clk0", "ref_type": "ptp", "traceable": false, "version": "IEEE1588-2008", "gmid": gmid, "locked": sys.ptp.locked })
    } else {
        json!({ "name": "clk0", "ref_type": "internal" })
    };

    let node = with(
        core(&node_id, &name, "Strata Multiview"),
        json!({
            "href": format!("{base}/"),
            "hostname": hostname,
            "api": { "versions": ["v1.3"], "endpoints": [{ "host": host, "port": port, "protocol": protocol }] },
            "caps": {},
            "services": [],
            "clocks": [clock],
            "interfaces": ifs.iter().map(|(n, _, mac)| json!({
                "name": n,
                "chassis_id": mac,
                "port_id": mac.clone().unwrap_or_else(|| "00-00-00-00-00-00".into()),
            })).collect::<Vec<_>>(),
        }),
    );

    let mut sources = Vec::new();
    let mut flows = Vec::new();
    let mut senders = Vec::new();
    for c in &s.config.channels {
        for (_, width, height, outputs) in c.pictures() {
            for o in outputs.st2110.iter() {
                let streams = o.streams();
                for (n, st) in streams.iter().enumerate() {
                    let key = matches!(st.essence, strata_mv_output::st2110::Essence::Key);
                    let what = format!("{}/{}/{}", c.id, o.id, if key { "key" } else { "fill" });
                    let source_id = uid(&seed, &format!("source:{what}"));
                    let flow_id = uid(&seed, &format!("flow:{what}"));
                    let sender_id = uid(&seed, &format!("sender:{what}"));
                    let label = if streams.len() > 1 {
                        format!(
                            "{} — {} ({})",
                            c.name,
                            o.name,
                            if n == 0 { "fill" } else { "key" }
                        )
                    } else {
                        format!("{} — {}", c.name, o.name)
                    };
                    let rate =
                        json!({ "numerator": c.frame_rate.num, "denominator": c.frame_rate.den });
                    sources.push(with(
                        core(&source_id, &label, "Multiview picture"),
                        json!({ "format": "urn:x-nmos:format:video", "caps": {}, "device_id": device_id, "parents": [], "clock_name": "clk0", "grain_rate": rate }),
                    ));
                    let (w, h) = c.size_of(&o.id, (width, height));
                    let components = match st.essence {
                        strata_mv_output::st2110::Essence::Fill => json!([
                            { "name": "Y", "width": w, "height": h, "bit_depth": 10 },
                            { "name": "Cb", "width": w / 2, "height": h, "bit_depth": 10 },
                            { "name": "Cr", "width": w / 2, "height": h, "bit_depth": 10 },
                        ]),
                        strata_mv_output::st2110::Essence::Key => {
                            json!([{ "name": "A", "width": w, "height": h, "bit_depth": 10 }])
                        }
                        strata_mv_output::st2110::Essence::Rgba => json!([
                            { "name": "R", "width": w, "height": h, "bit_depth": 8 },
                            { "name": "G", "width": w, "height": h, "bit_depth": 8 },
                            { "name": "B", "width": w, "height": h, "bit_depth": 8 },
                            { "name": "A", "width": w, "height": h, "bit_depth": 8 },
                        ]),
                        strata_mv_output::st2110::Essence::Ycbcra => json!([
                            { "name": "Y", "width": w, "height": h, "bit_depth": 10 },
                            { "name": "Cb", "width": w / 2, "height": h, "bit_depth": 10 },
                            { "name": "Cr", "width": w / 2, "height": h, "bit_depth": 10 },
                            { "name": "A", "width": w, "height": h, "bit_depth": 10 },
                        ]),
                    };
                    flows.push(with(
                        core(&flow_id, &label, "Multiview picture"),
                        json!({
                            "format": "urn:x-nmos:format:video",
                            "media_type": "video/raw",
                            "source_id": source_id,
                            "device_id": device_id,
                            "parents": [],
                            "grain_rate": rate,
                            "frame_width": w,
                            "frame_height": h,
                            "interlace_mode": if c.interlaced { "interlaced_tff" } else { "progressive" },
                            "colorspace": "BT709",
                            "transfer_characteristic": "SDR",
                            "components": components,
                        }),
                    ));
                    let mut bindings = binding(&ifs, o.interface);
                    if let Some(b) = o.blue {
                        bindings.extend(binding(&ifs, b.interface));
                    }
                    let transport = if st.red.ip().is_multicast() {
                        "urn:x-nmos:transport:rtp.mcast"
                    } else {
                        "urn:x-nmos:transport:rtp.ucast"
                    };
                    senders.push((
                        SenderRef { channel: c.id.clone(), output: o.id.clone(), key },
                        with(
                            core(&sender_id, &label, "ST 2110-20"),
                            json!({
                                "flow_id": flow_id,
                                "transport": transport,
                                "device_id": device_id,
                                "manifest_href": format!("{base}/x-nmos/connection/v1.1/single/senders/{sender_id}/transportfile"),
                                "interface_bindings": bindings,
                                "subscription": { "receiver_id": null, "active": o.enabled },
                            }),
                        ),
                    ));
                }
            }
        }
    }

    let mut receivers = Vec::new();
    for i in s.config.inputs.iter().filter(|i| i.kind == InputKind::Udp) {
        let id = uid(&seed, &format!("receiver:{}", i.id));
        let (group, _, _, local) = input_params(&i.url);
        let sender = lock(&STATE).subscribed.get(&id).cloned();
        receivers.push((
            ReceiverRef {
                input: i.id.clone(),
            },
            with(
                core(&id, &i.name, "MPEG-TS input"),
                json!({
                    "device_id": device_id,
                    "transport": input_transport(&i.url, group),
                    "interface_bindings": binding(&ifs, local),
                    "subscription": { "sender_id": sender, "active": i.enabled },
                    "format": "urn:x-nmos:format:mux",
                    "caps": { "media_types": ["video/MP2T"] },
                }),
            ),
        ));
    }

    let device = with(
        core(&device_id, &name, "Multiviewer"),
        json!({
            "type": "urn:x-nmos:device:generic",
            "node_id": node_id,
            "senders": senders.iter().map(|(_, v)| v["id"].clone()).collect::<Vec<_>>(),
            "receivers": receivers.iter().map(|(_, v)| v["id"].clone()).collect::<Vec<_>>(),
            "controls": [{ "href": format!("{base}/x-nmos/connection/v1.1/"), "type": "urn:x-nmos:control:sr-ctrl/v1.1" }],
        }),
    );
    Model {
        base,
        node,
        device,
        sources,
        flows,
        senders,
        receivers,
    }
}

// ------------------------------------------------- IS-05 staged state --

#[derive(Default)]
struct ConnState {
    /// Staged parameters, by sender or receiver id.
    staged: HashMap<String, Value>,
    /// The NMOS sender a receiver was connected to.
    subscribed: HashMap<String, String>,
    /// The receiver a sender sends to (unicast).
    receiver_of: HashMap<String, String>,
    /// Transport files given to receivers.
    files: HashMap<String, String>,
}

static STATE: LazyLock<Mutex<ConnState>> = LazyLock::new(Default::default);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn ip_or_auto(ip: Option<Ipv4Addr>) -> Value {
    ip.map_or(json!("auto"), |i| json!(i.to_string()))
}

/// The active parameters of a sender, from the settings.
fn sender_active(s: &AppState, r: &SenderRef, id: &str) -> Option<Value> {
    let o = s
        .config
        .channels
        .iter()
        .find(|c| c.id == r.channel)?
        .outputs
        .st2110
        .iter()
        .find(|o| o.id == r.output)?;
    let st = o
        .streams()
        .into_iter()
        .find(|d| matches!(d.essence, strata_mv_output::st2110::Essence::Key) == r.key)?;
    let leg = |dest: SocketAddrV4, iface: Option<Ipv4Addr>| {
        json!({
            "source_ip": ip_or_auto(iface),
            "destination_ip": dest.ip().to_string(),
            "source_port": "auto",
            "destination_port": dest.port(),
            "rtp_enabled": o.enabled,
        })
    };
    let mut legs = vec![leg(st.red, o.interface)];
    if let Some(b) = st.blue {
        legs.push(leg(b, o.blue.and_then(|l| l.interface)));
    }
    Some(json!({
        "receiver_id": lock(&STATE).receiver_of.get(id).cloned(),
        "master_enable": o.enabled,
        "activation": { "mode": null, "requested_time": null, "activation_time": null },
        "transport_params": legs,
    }))
}

fn receiver_active(s: &AppState, r: &ReceiverRef, id: &str) -> Option<Value> {
    let i = s.config.inputs.iter().find(|i| i.id == r.input)?;
    let (group, port, source, local) = input_params(&i.url);
    let st = lock(&STATE);
    let file = st.files.get(id).cloned();
    Some(json!({
        "sender_id": st.subscribed.get(id).cloned(),
        "master_enable": i.enabled,
        "activation": { "mode": null, "requested_time": null, "activation_time": null },
        "transport_file": { "data": file, "type": file.as_ref().map(|_| "application/sdp") },
        "transport_params": [{
            "source_ip": source.map(|s| s.to_string()),
            "multicast_ip": group.map(|g| g.to_string()),
            "interface_ip": ip_or_auto(local),
            "destination_port": port,
            "rtp_enabled": i.enabled,
        }],
    }))
}

/// Merges a PATCH into the staged parameters (objects merged, arrays by leg).
fn merge(into: &mut Value, patch: &Value) {
    match (into, patch) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                match a.get_mut(k) {
                    Some(slot) if v.is_object() || v.is_array() => merge(slot, v),
                    _ => {
                        a.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (x, y) in a.iter_mut().zip(b) {
                merge(x, y);
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

fn parse_ip(v: &Value) -> Result<Option<Ipv4Addr>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s == "auto" => Ok(None),
        Value::String(s) => s
            .parse()
            .map(Some)
            .map_err(|_| format!("not an IPv4 address: {s}")),
        _ => Err("an IP address is a string".into()),
    }
}

fn parse_port(v: &Value, auto: u16) -> Result<u16, String> {
    match v {
        Value::String(s) if s == "auto" => Ok(auto),
        Value::Number(n) => n
            .as_u64()
            .filter(|p| (1..=65535).contains(p))
            .map(|p| p as u16)
            .ok_or_else(|| "port out of range".into()),
        Value::Null => Ok(auto),
        _ => Err("a port is a number or \"auto\"".into()),
    }
}

/// An SDP file given to a receiver: its group, port and source filter.
fn sdp_params(sdp: &str) -> (Option<Ipv4Addr>, Option<u16>, Option<Ipv4Addr>) {
    let mut group = None;
    let mut port = None;
    // The source filter, else the origin's address.
    let mut filter = None;
    let mut origin = None;
    for l in sdp.lines() {
        let l = l.trim();
        if let Some(c) = l.strip_prefix("c=IN IP4 ") {
            group = group.or_else(|| c.split('/').next().and_then(|a| a.trim().parse().ok()));
        } else if let Some(m) = l.strip_prefix("m=") {
            port = port.or_else(|| m.split_whitespace().nth(1).and_then(|p| p.parse().ok()));
        } else if let Some(f) = l.strip_prefix("a=source-filter:") {
            // a=source-filter: incl IN IP4 <dest> <source>
            filter = filter.or_else(|| f.split_whitespace().nth(4).and_then(|a| a.parse().ok()));
        } else if let Some(o) = l.strip_prefix("o=") {
            origin = origin.or_else(|| o.split_whitespace().nth(5).and_then(|a| a.parse().ok()));
        }
    }
    (group, port, filter.or(origin))
}

/// Applies staged parameters to the settings, then restarts the engine.
async fn apply(s: &Shared, kind: Kind, id: &str, staged: &Value) -> Result<(), String> {
    let m = model(s);
    let mut cfg = ServerConfig::load(&s.data_dir);
    let enable = staged.get("master_enable").and_then(Value::as_bool);
    let legs = staged
        .get("transport_params")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    match kind {
        Kind::Sender => {
            let (r, _) = m
                .senders
                .iter()
                .find(|(_, v)| v["id"] == id)
                .ok_or("no such sender")?;
            let ch = cfg
                .channels
                .iter_mut()
                .find(|c| c.id == r.channel)
                .ok_or("no such channel")?;
            let o = ch
                .outputs
                .st2110
                .iter_mut()
                .find(|o| o.id == r.output)
                .ok_or("no such output")?;
            if let Some(e) = enable {
                o.enabled = e;
            }
            let dest = |leg: &Value, was: SocketAddrV4| -> Result<SocketAddrV4, String> {
                let ip = parse_ip(leg.get("destination_ip").unwrap_or(&Value::Null))?
                    .unwrap_or(*was.ip());
                let port = parse_port(
                    leg.get("destination_port").unwrap_or(&Value::Null),
                    was.port(),
                )?;
                Ok(SocketAddrV4::new(ip, port))
            };
            let hardware = o.hardware.clone();
            let flow = cfg
                .hardware
                .st2110
                .flows
                .iter_mut()
                .find(|f| !hardware.is_empty() && f.id == hardware);
            match flow {
                // A flow of the Hardware settings: it is changed there.
                Some(f) => {
                    if let Some(l) = legs.first() {
                        if r.key {
                            f.key = Some(dest(l, f.key.unwrap_or(f.fill))?);
                        } else {
                            f.fill = dest(l, f.fill)?;
                        }
                    }
                    if let (Some(l), Some(b)) =
                        (legs.get(1), if r.key { f.blue_key } else { f.blue })
                    {
                        let d = dest(l, b)?;
                        if r.key {
                            f.blue_key = Some(d);
                        } else {
                            f.blue = Some(d);
                        }
                    }
                }
                None => {
                    if let Some(l) = legs.first() {
                        if let Some(ip) = parse_ip(l.get("source_ip").unwrap_or(&Value::Null))? {
                            o.interface = Some(ip);
                        }
                        if r.key {
                            o.key = Some(dest(l, o.key.unwrap_or(o.fill))?);
                        } else {
                            o.fill = dest(l, o.fill)?;
                        }
                    }
                    if let (Some(l), Some(b)) = (legs.get(1), o.blue.as_mut()) {
                        if r.key {
                            b.key = Some(dest(l, b.key.unwrap_or(b.fill))?);
                        } else {
                            b.fill = dest(l, b.fill)?;
                        }
                    }
                }
            }
            let mut st = lock(&STATE);
            match staged.get("receiver_id").and_then(Value::as_str) {
                Some(r) => st.receiver_of.insert(id.to_string(), r.to_string()),
                None => st.receiver_of.remove(id),
            };
        }
        Kind::Receiver => {
            let (r, _) = m
                .receivers
                .iter()
                .find(|(_, v)| v["id"] == id)
                .ok_or("no such receiver")?;
            let i = cfg
                .inputs
                .iter_mut()
                .find(|i| i.id == r.input)
                .ok_or("no such input")?;
            let (mut group, mut port, mut source, local) = input_params(&i.url);
            let file = staged
                .get("transport_file")
                .and_then(|f| f.get("data"))
                .and_then(Value::as_str)
                .map(String::from);
            let mut local = local;
            if let Some(l) = legs.first() {
                if let Some(v) = l.get("multicast_ip") {
                    group = parse_ip(v)?;
                }
                if let Some(v) = l.get("source_ip") {
                    source = parse_ip(v)?;
                }
                if let Some(v) = l.get("interface_ip") {
                    local = parse_ip(v)?;
                }
                if let Some(v) = l.get("destination_port") {
                    port = parse_port(v, 5004)?;
                }
            }
            let scheme = if i.url.starts_with("rtp://") {
                "rtp"
            } else {
                "udp"
            };
            let at = if scheme == "udp" { "@" } else { "" };
            let mut url = format!(
                "{scheme}://{at}{}:{port}",
                group.map(|g| g.to_string()).unwrap_or_default()
            );
            let mut q = Vec::new();
            if let Some(s) = source.filter(|_| group.is_some()) {
                q.push(format!("sources={s}"));
            }
            if let Some(l) = local {
                q.push(format!("localaddr={l}"));
            }
            if !q.is_empty() {
                url = format!("{url}?{}", q.join("&"));
            }
            i.url = url;
            if let Some(e) = enable {
                i.enabled = e;
            }
            let mut st = lock(&STATE);
            match staged.get("sender_id").and_then(Value::as_str) {
                Some(snd) => st.subscribed.insert(id.to_string(), snd.to_string()),
                None => st.subscribed.remove(id),
            };
            match file {
                Some(f) => st.files.insert(id.to_string(), f),
                None => st.files.remove(id),
            };
        }
    }
    cfg.normalize();
    cfg.validate()?;
    let dir = s.data_dir.clone();
    tokio::task::spawn_blocking(move || cfg.save(&dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    tracing::info!("NMOS: {} {id} activated", kind.word());
    s.changed("config");
    s.restart.notify_one();
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Sender,
    Receiver,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Sender => "sender",
            Kind::Receiver => "receiver",
        }
    }
}

// ------------------------------------------------------------- the API --

fn json_reply(code: StatusCode, v: Value) -> Response {
    (code, axum::Json(v)).into_response()
}

fn error(code: StatusCode, msg: impl Into<String>) -> Response {
    let msg = msg.into();
    json_reply(
        code,
        json!({ "code": code.as_u16(), "error": msg, "debug": null }),
    )
}

fn list(items: &[&str]) -> Response {
    json_reply(StatusCode::OK, json!(items))
}

/// `/x-nmos/`: the APIs served.
pub async fn handle_root(State(s): State<Shared>) -> Response {
    if !s.config.nmos.enabled {
        return error(StatusCode::NOT_FOUND, "NMOS is off (Settings › Network)");
    }
    list(&["node/", "connection/"])
}

/// Everything under `/x-nmos/`: the Node API and the Connection API.
pub async fn handle(
    State(s): State<Shared>,
    method: Method,
    UrlPath(rest): UrlPath<String>,
    body: Bytes,
) -> Response {
    if !s.config.nmos.enabled {
        return error(StatusCode::NOT_FOUND, "NMOS is off (Settings › Network)");
    }
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    match parts.first().copied() {
        None => list(&["node/", "connection/"]),
        Some("node") => node_api(&s, &method, &parts[1..]),
        Some("connection") => connection_api(&s, &method, &parts[1..], &body).await,
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

fn node_api(s: &AppState, method: &Method, p: &[&str]) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "the Node API is read-only here",
        );
    }
    if p.is_empty() {
        return list(&["v1.3/"]);
    }
    if p[0] != "v1.3" {
        return error(StatusCode::NOT_FOUND, "version not supported (v1.3)");
    }
    let m = model(s);
    let find = |all: Vec<Value>, id: Option<&&str>| match id {
        None => json_reply(StatusCode::OK, Value::Array(all)),
        Some(id) => all.into_iter().find(|v| v["id"] == **id).map_or_else(
            || error(StatusCode::NOT_FOUND, "no such resource"),
            |v| json_reply(StatusCode::OK, v),
        ),
    };
    match p.get(1).copied() {
        None => list(&[
            "self/",
            "sources/",
            "flows/",
            "devices/",
            "senders/",
            "receivers/",
        ]),
        Some("self") => json_reply(StatusCode::OK, m.node),
        Some("devices") => find(vec![m.device], p.get(2)),
        Some("sources") => find(m.sources, p.get(2)),
        Some("flows") => find(m.flows, p.get(2)),
        Some("senders") => find(m.senders.into_iter().map(|(_, v)| v).collect(), p.get(2)),
        Some("receivers") if p.get(3) == Some(&"target") => {
            error(StatusCode::NOT_IMPLEMENTED, "use IS-05")
        }
        Some("receivers") => find(m.receivers.into_iter().map(|(_, v)| v).collect(), p.get(2)),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn connection_api(s: &Shared, method: &Method, p: &[&str], body: &Bytes) -> Response {
    if p.is_empty() {
        return list(&["v1.1/"]);
    }
    if p[0] != "v1.1" {
        return error(StatusCode::NOT_FOUND, "version not supported (v1.1)");
    }
    match (p.get(1).copied(), p.get(2).copied()) {
        (None, _) => list(&["bulk/", "single/"]),
        (Some("single"), None) => list(&["senders/", "receivers/"]),
        (Some("single"), Some(k @ ("senders" | "receivers"))) => {
            let kind = if k == "senders" {
                Kind::Sender
            } else {
                Kind::Receiver
            };
            single(s, method, kind, &p[3..], body).await
        }
        (Some("bulk"), None) => list(&["senders/", "receivers/"]),
        (Some("bulk"), Some(k @ ("senders" | "receivers"))) => {
            if method != Method::POST {
                return error(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "POST an array of {id, params}",
                );
            }
            let kind = if k == "senders" {
                Kind::Sender
            } else {
                Kind::Receiver
            };
            let Ok(Value::Array(items)) = serde_json::from_slice::<Value>(body) else {
                return error(StatusCode::BAD_REQUEST, "an array of {id, params}");
            };
            let mut out = Vec::new();
            for it in items {
                let id = it["id"].as_str().unwrap_or_default().to_string();
                let r = patch_staged(s, kind, &id, &it["params"]).await;
                out.push(match r {
                    Ok((code, _)) => json!({ "id": id, "code": code.as_u16() }),
                    Err((code, e)) => json!({ "id": id, "code": code.as_u16(), "error": e }),
                });
            }
            json_reply(StatusCode::OK, Value::Array(out))
        }
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

fn exists(m: &Model, kind: Kind, id: &str) -> bool {
    match kind {
        Kind::Sender => m.senders.iter().any(|(_, v)| v["id"] == id),
        Kind::Receiver => m.receivers.iter().any(|(_, v)| v["id"] == id),
    }
}

fn active(s: &AppState, m: &Model, kind: Kind, id: &str) -> Option<Value> {
    match kind {
        Kind::Sender => m
            .senders
            .iter()
            .find(|(_, v)| v["id"] == id)
            .and_then(|(r, _)| sender_active(s, r, id)),
        Kind::Receiver => m
            .receivers
            .iter()
            .find(|(_, v)| v["id"] == id)
            .and_then(|(r, _)| receiver_active(s, r, id)),
    }
}

fn staged(s: &AppState, m: &Model, kind: Kind, id: &str) -> Option<Value> {
    if let Some(v) = lock(&STATE).staged.get(id) {
        return Some(v.clone());
    }
    active(s, m, kind, id)
}

async fn single(s: &Shared, method: &Method, kind: Kind, p: &[&str], body: &Bytes) -> Response {
    let m = model(s);
    let Some(id) = p.first().copied() else {
        let ids: Vec<String> = match kind {
            Kind::Sender => m
                .senders
                .iter()
                .map(|(_, v)| format!("{}/", v["id"].as_str().unwrap_or_default()))
                .collect(),
            Kind::Receiver => m
                .receivers
                .iter()
                .map(|(_, v)| format!("{}/", v["id"].as_str().unwrap_or_default()))
                .collect(),
        };
        return json_reply(StatusCode::OK, json!(ids));
    };
    if !exists(&m, kind, id) {
        return error(StatusCode::NOT_FOUND, "no such resource");
    }
    match (p.get(1).copied(), method.clone()) {
        (None, _) => match kind {
            Kind::Sender => list(&[
                "constraints/",
                "staged/",
                "active/",
                "transportfile/",
                "transporttype/",
            ]),
            Kind::Receiver => list(&["constraints/", "staged/", "active/", "transporttype/"]),
        },
        (Some("transporttype"), _) => {
            let t = match kind {
                Kind::Sender => "urn:x-nmos:transport:rtp",
                Kind::Receiver => {
                    let url = m
                        .receivers
                        .iter()
                        .find(|(_, v)| v["id"] == id)
                        .and_then(|(r, _)| s.config.inputs.iter().find(|i| i.id == r.input))
                        .map(|i| i.url.clone())
                        .unwrap_or_default();
                    if url.starts_with("rtp://") {
                        "urn:x-nmos:transport:rtp"
                    } else {
                        "urn:x-nmos:transport:udp"
                    }
                }
            };
            json_reply(StatusCode::OK, json!(t))
        }
        (Some("constraints"), _) => {
            let ips: Vec<String> = interfaces()
                .into_iter()
                .map(|(_, ip, _)| ip.to_string())
                .collect();
            let legs = active(s, &m, kind, id)
                .and_then(|a| a["transport_params"].as_array().map(Vec::len))
                .unwrap_or(1);
            let one = match kind {
                Kind::Sender => json!({
                    "source_ip": { "enum": ips },
                    "destination_ip": {},
                    "source_port": {},
                    "destination_port": {},
                    "rtp_enabled": {},
                }),
                Kind::Receiver => json!({
                    "source_ip": {},
                    "multicast_ip": {},
                    "interface_ip": { "enum": ips.iter().cloned().chain(["auto".to_string()]).collect::<Vec<_>>() },
                    "destination_port": {},
                    "rtp_enabled": {},
                }),
            };
            json_reply(StatusCode::OK, Value::Array(vec![one; legs]))
        }
        (Some("active"), _) => active(s, &m, kind, id).map_or_else(
            || error(StatusCode::NOT_FOUND, "no such resource"),
            |v| json_reply(StatusCode::OK, v),
        ),
        (Some("transportfile"), _) if kind == Kind::Sender => {
            let Some((r, _)) = m.senders.iter().find(|(_, v)| v["id"] == id) else {
                return error(StatusCode::NOT_FOUND, "no such sender");
            };
            let sdp = s
                .config
                .channels
                .iter()
                .find(|c| c.id == r.channel)
                .and_then(|c| {
                    c.pictures().into_iter().find_map(|(_, w, h, outs)| {
                        outs.st2110.iter().find(|o| o.id == r.output).and_then(|o| {
                            let (w, h) = c.size_of(&o.id, (w, h));
                            o.sdp(w, h, (c.frame_rate.num, c.frame_rate.den), r.key)
                        })
                    })
                });
            match sdp {
                Some(t) => ([(header::CONTENT_TYPE, "application/sdp")], t).into_response(),
                None => error(StatusCode::NOT_FOUND, "no transport file"),
            }
        }
        (Some("staged"), Method::GET) => staged(s, &m, kind, id).map_or_else(
            || error(StatusCode::NOT_FOUND, "no such resource"),
            |v| json_reply(StatusCode::OK, v),
        ),
        (Some("staged"), Method::PATCH) => {
            let Ok(patch) = serde_json::from_slice::<Value>(body) else {
                return error(StatusCode::BAD_REQUEST, "a JSON object");
            };
            match patch_staged(s, kind, id, &patch).await {
                Ok((code, v)) => json_reply(code, v),
                Err((code, e)) => error(code, e),
            }
        }
        (Some("staged"), _) => error(StatusCode::METHOD_NOT_ALLOWED, "GET or PATCH"),
        _ => error(StatusCode::NOT_FOUND, "not found"),
    }
}

/// A PATCH of the staged parameters, activating them when asked.
async fn patch_staged(
    s: &Shared,
    kind: Kind,
    id: &str,
    patch: &Value,
) -> Result<(StatusCode, Value), (StatusCode, String)> {
    let m = model(s);
    if !exists(&m, kind, id) {
        return Err((StatusCode::NOT_FOUND, "no such resource".into()));
    }
    if !patch.is_object() {
        return Err((StatusCode::BAD_REQUEST, "a JSON object".into()));
    }
    let mut st = staged(s, &m, kind, id).unwrap_or_else(|| json!({}));
    let pending = st["activation"]["mode"]
        .as_str()
        .is_some_and(|m| m.starts_with("activate_scheduled"));
    let mode = patch["activation"]["mode"].as_str().map(String::from);
    if pending && !(patch["activation"].is_object() && patch["activation"]["mode"].is_null()) {
        // A scheduled activation waits: only cancelling it is allowed.
        return Err((
            StatusCode::LOCKED,
            "a scheduled activation is pending".into(),
        ));
    }
    merge(&mut st, patch);
    // A transport file sets the parameters, those of the same PATCH aside.
    if let (Kind::Receiver, Some(sdp)) = (kind, patch["transport_file"]["data"].as_str()) {
        let (g, p, src) = sdp_params(sdp);
        let given = |k: &str| patch["transport_params"][0].get(k).is_some();
        let leg = &mut st["transport_params"][0];
        if leg.is_object() {
            if !given("multicast_ip") {
                leg["multicast_ip"] = json!(g.map(|g| g.to_string()));
            }
            if !given("source_ip") {
                leg["source_ip"] = json!(src.map(|s| s.to_string()));
            }
            if let (false, Some(p)) = (given("destination_port"), p) {
                leg["destination_port"] = json!(p);
            }
        }
    }
    let respond = |mut st: Value, mode: Option<&str>, requested: Value, at: Value| {
        st["activation"] =
            json!({ "mode": mode, "requested_time": requested, "activation_time": at });
        st
    };
    match mode.as_deref() {
        None => {
            let v = respond(st, None, Value::Null, Value::Null);
            lock(&STATE).staged.insert(id.to_string(), v.clone());
            Ok((StatusCode::OK, v))
        }
        Some("activate_immediate") => {
            apply(s, kind, id, &st)
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
            let v = respond(
                st,
                Some("activate_immediate"),
                Value::Null,
                json!(tai(now_ns())),
            );
            let mut idle = v.clone();
            idle["activation"] =
                json!({ "mode": null, "requested_time": null, "activation_time": null });
            lock(&STATE).staged.insert(id.to_string(), idle);
            Ok((StatusCode::OK, v))
        }
        Some(m @ ("activate_scheduled_relative" | "activate_scheduled_absolute")) => {
            let req = patch["activation"]["requested_time"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let (secs, nanos) = req
                .split_once(':')
                .and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)))
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "requested_time is \"seconds:nanoseconds\"".to_string(),
                ))?;
            let at_ns = if m.ends_with("relative") {
                now_ns() + secs * 1_000_000_000 + nanos
            } else {
                (secs * 1_000_000_000 + nanos).saturating_sub(37_000_000_000)
            };
            let v = respond(
                st.clone(),
                Some(m),
                json!(req),
                json!(tai(at_ns.max(now_ns()))),
            );
            lock(&STATE).staged.insert(id.to_string(), v.clone());
            let (s2, id2) = (s.clone(), id.to_string());
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_nanos(at_ns.saturating_sub(now_ns()))).await;
                // Cancelled meanwhile: nothing to do.
                let still = lock(&STATE).staged.get(&id2).cloned();
                let Some(mut st) = still.filter(|v| {
                    v["activation"]["mode"]
                        .as_str()
                        .is_some_and(|m| m.starts_with("activate_scheduled"))
                }) else {
                    return;
                };
                if let Err(e) = apply(&s2, kind, &id2, &st).await {
                    tracing::warn!("NMOS: scheduled activation of {id2}: {e}");
                }
                st["activation"] =
                    json!({ "mode": null, "requested_time": null, "activation_time": null });
                lock(&STATE).staged.insert(id2, st);
            });
            Ok((StatusCode::ACCEPTED, v))
        }
        Some(other) => Err((
            StatusCode::BAD_REQUEST,
            format!("unknown activation mode {other}"),
        )),
    }
}

// --------------------------------------------------------- registration --

/// Registers the node's resources with the registry and keeps them alive
/// (heartbeat every 5 s, registered again when the registry forgot them).
/// Ends with the engine cycle (`closing`).
pub fn spawn_registration(s: Shared) {
    let registry = s
        .config
        .nmos
        .registry
        .trim()
        .trim_end_matches('/')
        .to_string();
    if !s.config.nmos.enabled || registry.is_empty() {
        return;
    }
    let mut closing = s.closing.subscribe();
    tokio::spawn(async move {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(3)))
            .http_status_as_error(false)
            .build()
            .into();
        let base = format!("{registry}/x-nmos/registration/v1.3");
        let mut registered = false;
        loop {
            let m = model(&s);
            let node_id = m.node["id"].as_str().unwrap_or_default().to_string();
            let (agent2, base2) = (agent.clone(), base.clone());
            let ok = tokio::task::spawn_blocking(move || -> Result<bool, String> {
                if registered {
                    let r = agent2
                        .post(&format!("{base2}/health/nodes/{node_id}"))
                        .send_empty()
                        .map_err(|e| e.to_string())?;
                    if r.status().as_u16() == 200 {
                        return Ok(true);
                    }
                }
                // (Again) the whole node, in the order the registry needs.
                let mut all = vec![("node", m.node.clone()), ("device", m.device.clone())];
                all.extend(m.sources.iter().map(|v| ("source", v.clone())));
                all.extend(m.flows.iter().map(|v| ("flow", v.clone())));
                all.extend(m.senders.iter().map(|(_, v)| ("sender", v.clone())));
                all.extend(m.receivers.iter().map(|(_, v)| ("receiver", v.clone())));
                for (t, data) in all {
                    let r = agent2
                        .post(&format!("{base2}/resource"))
                        .header("content-type", "application/json")
                        .send(json!({ "type": t, "data": data }).to_string())
                        .map_err(|e| e.to_string())?;
                    let code = r.status().as_u16();
                    if code != 200 && code != 201 {
                        return Err(format!("registering a {t}: HTTP {code}"));
                    }
                }
                Ok(true)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let was = registered;
            registered = match ok {
                Ok(v) => v,
                Err(e) => {
                    if was {
                        tracing::warn!("NMOS registry {registry}: {e}");
                    } else {
                        tracing::debug!("NMOS registry {registry}: {e}");
                    }
                    false
                }
            };
            if registered && !was {
                tracing::info!("NMOS: registered with {registry} ({})", m.base);
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                _ = closing.wait_for(|c| *c) => break,
            }
        }
    });
}

/// `GET /api/nmos`: the node's id, addresses and resources (settings page).
pub async fn info(State(s): State<Shared>, _me: crate::auth::CurrentUser) -> axum::Json<Value> {
    axum::Json(summary(&s))
}

fn summary(s: &AppState) -> Value {
    let m = model(s);
    json!({
        "nodeId": m.node["id"],
        "nodeApi": format!("{}/x-nmos/node/v1.3/", m.base),
        "connectionApi": format!("{}/x-nmos/connection/v1.1/", m.base),
        "senders": m.senders.len(),
        "receivers": m.receivers.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_addresses() {
        assert_eq!(
            input_params("udp://@239.1.1.1:5000?sources=10.0.0.9&localaddr=192.168.1.2"),
            (
                Some("239.1.1.1".parse().unwrap()),
                5000,
                Some("10.0.0.9".parse().unwrap()),
                Some("192.168.1.2".parse().unwrap())
            )
        );
        assert_eq!(input_params("udp://@:5000"), (None, 5000, None, None));
        assert_eq!(
            input_params("rtp://239.2.2.2:20000"),
            (Some("239.2.2.2".parse().unwrap()), 20000, None, None)
        );
        assert_eq!(
            input_transport("rtp://239.2.2.2:20000", Some("239.2.2.2".parse().unwrap())),
            "urn:x-nmos:transport:rtp.mcast"
        );
    }

    #[test]
    fn sdp_of_a_sender() {
        let sdp = "v=0\r\no=- 1 1 IN IP4 10.0.0.9\r\ns=x\r\nt=0 0\r\nm=video 20000 RTP/AVP 33\r\nc=IN IP4 239.5.5.5/32\r\na=source-filter: incl IN IP4 239.5.5.5 10.0.0.7\r\n";
        assert_eq!(
            sdp_params(sdp),
            (
                Some("239.5.5.5".parse().unwrap()),
                Some(20000),
                Some("10.0.0.7".parse().unwrap())
            )
        );
    }

    #[test]
    fn staged_merge() {
        let mut a = json!({ "master_enable": false, "transport_params": [{ "destination_ip": "239.0.0.1", "destination_port": 5004 }, { "destination_ip": "239.0.1.1" }] });
        merge(
            &mut a,
            &json!({ "master_enable": true, "transport_params": [{ "destination_port": 6000 }, {}] }),
        );
        assert_eq!(a["master_enable"], true);
        assert_eq!(a["transport_params"][0]["destination_ip"], "239.0.0.1");
        assert_eq!(a["transport_params"][0]["destination_port"], 6000);
        assert_eq!(a["transport_params"][1]["destination_ip"], "239.0.1.1");
    }

    #[test]
    fn stable_ids() {
        let seed = Uuid::new_v4();
        assert_eq!(uid(&seed, "node"), uid(&seed, "node"));
        assert_ne!(uid(&seed, "node"), uid(&seed, "device"));
        assert_eq!(
            Uuid::parse_str(&uid(&seed, "x")).unwrap().get_version_num(),
            5
        );
        assert!(tai(0).starts_with("37:"));
    }
}
