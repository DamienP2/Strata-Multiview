//! Tally and UMD: what the switchers say of each source, routed to the
//! inputs the multiview shows (their frame, lamps and name bar).
//!
//! Sources: TSL UMD 3.1 and 5.0 (UDP, TCP listening or connecting: Ross,
//! Grass Valley, Sony, vMix, tally controllers…), Blackmagic ATEM
//! switchers, GPI (contacts reported by a GPI box calling the API). A route
//! ties a display of a source (TSL address or index, ATEM input, GPI pin)
//! to an input. The API can also set an input's tally and UMD directly.
//! Kept in `<data>/tally.json`, applied at once (the outputs go on).

pub mod api;
pub mod atem;
pub mod tsl;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use strata_mv_core::signal::Tally as InputTally;
use tokio::io::AsyncReadExt;

use crate::inputs::Inputs;

/// The lamp a tally lights.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lamp {
    /// On air (red).
    #[default]
    Program,
    /// Next (green).
    Preview,
    /// A third lamp (yellow).
    Aux,
}

/// The lamps of a display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Lamps {
    pub program: bool,
    pub preview: bool,
    pub aux: bool,
}

impl Lamps {
    pub fn light(&mut self, l: Lamp) {
        match l {
            Lamp::Program => self.program = true,
            Lamp::Preview => self.preview = true,
            Lamp::Aux => self.aux = true,
        }
    }
    fn any(&self) -> bool {
        self.program || self.preview || self.aux
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    #[default]
    Tsl31,
    Tsl5,
    Atem,
    /// Contacts reported through the API (`/api/tally/gpi/…`).
    Gpi,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    #[default]
    Udp,
    /// Strata waits for the switcher to connect.
    TcpListen,
    /// Strata connects to the switcher (`host`).
    TcpConnect,
}

fn yes() -> bool {
    true
}

fn tsl31_lamps() -> [Lamp; 4] {
    [Lamp::Program, Lamp::Preview, Lamp::Aux, Lamp::Aux]
}

/// A switcher or tally controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub protocol: Protocol,
    /// TSL: how it comes.
    #[serde(default)]
    pub transport: Transport,
    /// TSL: the port listened on, or connected to.
    #[serde(default)]
    pub port: u16,
    /// ATEM, TSL connecting: the switcher's address.
    #[serde(default)]
    pub host: String,
    /// TSL 5: only this screen's displays (None: every screen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<u16>,
    /// TSL 3.1: the lamp of tally 1 to 4.
    #[serde(default = "tsl31_lamps")]
    pub lamps: [Lamp; 4],
}

/// A display of a source shown by an input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub input: String,
    pub source: String,
    /// TSL address / index, ATEM input number, GPI pin.
    pub index: u32,
    /// GPI: the lamp its contact lights (default on air).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lamp: Option<Lamp>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TallyConfig {
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub routes: Vec<Route>,
}

impl TallyConfig {
    fn path(dir: &Path) -> PathBuf {
        dir.join("tally.json")
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let tmp = dir.join("tally.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, Self::path(dir))
    }

    pub fn validate(&mut self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        let mut ports = std::collections::HashSet::new();
        for s in &mut self.sources {
            s.id = s.id.trim().to_string();
            s.name = s.name.trim().to_string();
            s.host = s.host.trim().to_string();
            if s.id.is_empty() || !ids.insert(s.id.clone()) {
                return Err("tally sources need different ids".into());
            }
            if s.name.is_empty() {
                return Err("a tally source needs a name".into());
            }
            match s.protocol {
                Protocol::Tsl31 | Protocol::Tsl5 => {
                    if s.port == 0 {
                        return Err(format!("{}: a port is needed", s.name));
                    }
                    if s.transport == Transport::TcpConnect && s.host.is_empty() {
                        return Err(format!("{}: the switcher's address is needed", s.name));
                    }
                    let tcp = s.transport != Transport::Udp;
                    if s.enabled
                        && s.transport != Transport::TcpConnect
                        && !ports.insert((tcp, s.port))
                    {
                        return Err(format!(
                            "{}: port {} is already used by another source",
                            s.name, s.port
                        ));
                    }
                }
                Protocol::Atem if s.host.is_empty() => {
                    return Err(format!("{}: the switcher's address is needed", s.name));
                }
                _ => {}
            }
        }
        self.routes
            .retain(|r| !r.input.trim().is_empty() && ids.contains(&r.source));
        Ok(())
    }
}

/// How a source is doing, and what its displays say now.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceState {
    pub status: String,
    pub connected: bool,
    /// Last message, Unix ms.
    pub last_at: Option<i64>,
    /// By TSL address / index, ATEM input, GPI pin: lamps and text.
    pub displays: BTreeMap<u32, DisplayState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DisplayState {
    #[serde(flatten)]
    pub lamps: Lamps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// An input's tally set through the API (None: from the routes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Override {
    pub program: Option<bool>,
    pub preview: Option<bool>,
    pub aux: Option<bool>,
    pub umd: Option<String>,
}

/// The tally of the server: its settings, its sources running.
pub struct Tally {
    dir: PathBuf,
    config: RwLock<TallyConfig>,
    states: Mutex<BTreeMap<String, SourceState>>,
    overrides: Mutex<BTreeMap<String, Override>>,
    inputs: Arc<Inputs>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Tally {
    pub fn load(dir: &Path, inputs: Arc<Inputs>) -> Arc<Self> {
        Arc::new(Self {
            dir: dir.to_path_buf(),
            config: RwLock::new(TallyConfig::load(dir)),
            states: Default::default(),
            overrides: Default::default(),
            inputs,
            tasks: Default::default(),
        })
    }

    pub fn config(&self) -> TallyConfig {
        self.config
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn states(&self) -> BTreeMap<String, SourceState> {
        self.states
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn overrides(&self) -> BTreeMap<String, Override> {
        self.overrides
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// New settings: saved, the sources started again.
    pub fn set_config(self: &Arc<Self>, mut c: TallyConfig) -> Result<(), String> {
        c.validate()?;
        c.save(&self.dir).map_err(|e| e.to_string())?;
        *self.config.write().unwrap_or_else(|p| p.into_inner()) = c;
        self.start();
        Ok(())
    }

    /// Starts every source of the settings (those running stop first).
    pub fn start(self: &Arc<Self>) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|p| p.into_inner());
        for t in tasks.drain(..) {
            t.abort();
        }
        let c = self.config();
        {
            let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
            states.retain(|id, _| c.sources.iter().any(|s| &s.id == id && s.enabled));
            for s in c.sources.iter().filter(|s| s.enabled) {
                let st = states.entry(s.id.clone()).or_default();
                st.status = "Starting…".into();
                st.connected = false;
                if s.protocol == Protocol::Gpi {
                    st.status = "Waiting for the GPI box".into();
                }
            }
        }
        for s in c.sources.into_iter().filter(|s| s.enabled) {
            let me = self.clone();
            tasks.push(tokio::spawn(async move { me.run(s).await }));
        }
        drop(tasks);
        self.resolve();
    }

    pub fn stop(&self) {
        for t in self
            .tasks
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
        {
            t.abort();
        }
    }

    fn with_state(&self, id: &str, f: impl FnOnce(&mut SourceState)) {
        {
            let mut states = self.states.lock().unwrap_or_else(|p| p.into_inner());
            f(states.entry(id.to_string()).or_default());
        }
        self.resolve();
    }

    fn say(&self, id: &str, status: impl Into<String>, connected: bool) {
        let status = status.into();
        self.with_state(id, |s| {
            s.status = status;
            s.connected = connected;
        });
    }

    /// New displays of a source.
    fn displays(&self, id: &str, list: impl IntoIterator<Item = (u32, DisplayState)>) {
        let now = crate::state::now_ms();
        self.with_state(id, |s| {
            s.last_at = Some(now);
            for (i, d) in list {
                s.displays.insert(i, d);
            }
        });
    }

    /// A GPI contact closed or opened (`pin`, from 1).
    pub fn gpi(&self, source: &str, pin: u32, on: bool) -> Result<(), String> {
        let c = self.config();
        if !c
            .sources
            .iter()
            .any(|s| s.id == source && s.protocol == Protocol::Gpi && s.enabled)
        {
            return Err("no such GPI source".into());
        }
        self.say(source, "Receiving", true);
        self.displays(
            source,
            [(
                pin,
                DisplayState {
                    lamps: Lamps {
                        program: on,
                        ..Default::default()
                    },
                    text: String::new(),
                },
            )],
        );
        Ok(())
    }

    /// An input's tally set through the API (`None`: back to its routes).
    pub fn set_override(&self, input: &str, o: Option<Override>) {
        {
            let mut all = self.overrides.lock().unwrap_or_else(|p| p.into_inner());
            match o {
                Some(o) => all.insert(input.to_string(), o),
                None => all.remove(input),
            };
        }
        self.resolve();
    }

    /// The tally of every input, from the routes and the API.
    pub fn resolved(&self) -> BTreeMap<String, InputTally> {
        let c = self.config.read().unwrap_or_else(|p| p.into_inner());
        let states = self.states.lock().unwrap_or_else(|p| p.into_inner());
        let mut out: BTreeMap<String, InputTally> = BTreeMap::new();
        for r in &c.routes {
            let Some(d) = states.get(&r.source).and_then(|s| s.displays.get(&r.index)) else {
                continue;
            };
            let gpi = c
                .sources
                .iter()
                .any(|s| s.id == r.source && s.protocol == Protocol::Gpi);
            let t = out.entry(r.input.clone()).or_default();
            if gpi {
                // A contact lights the lamp of its route.
                if d.lamps.any() {
                    let mut l = Lamps::default();
                    l.light(r.lamp.unwrap_or_default());
                    t.program |= l.program;
                    t.preview |= l.preview;
                    t.aux |= l.aux;
                }
            } else {
                t.program |= d.lamps.program;
                t.preview |= d.lamps.preview;
                t.aux |= d.lamps.aux;
            }
            if t.umd.is_empty() && !d.text.is_empty() {
                t.umd = d.text.clone();
            }
        }
        for (input, o) in self
            .overrides
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            let t = out.entry(input.clone()).or_default();
            if let Some(v) = o.program {
                t.program = v;
            }
            if let Some(v) = o.preview {
                t.preview = v;
            }
            if let Some(v) = o.aux {
                t.aux = v;
            }
            if let Some(v) = &o.umd {
                t.umd = v.clone();
            }
        }
        out.retain(|_, t| *t != InputTally::default());
        out
    }

    /// The inputs' tally to the engine and the browsers.
    fn resolve(&self) {
        self.inputs.signals().set_tallies(self.resolved());
    }

    async fn run(self: Arc<Self>, s: SourceConfig) {
        match (s.protocol, s.transport) {
            (Protocol::Gpi, _) => {}
            (Protocol::Atem, _) => self.run_atem(&s).await,
            (_, Transport::Udp) => self.run_tsl_udp(&s).await,
            (_, Transport::TcpListen) => self.run_tsl_listen(&s).await,
            (_, Transport::TcpConnect) => self.run_tsl_connect(&s).await,
        }
    }

    /// Reads TSL bytes (a datagram, or what a TCP connection gave).
    fn tsl_bytes(&self, s: &SourceConfig, bytes: &[u8], rest: &mut Vec<u8>, tcp: bool) {
        let displays: Vec<tsl::Display> = match s.protocol {
            Protocol::Tsl31 => tsl::parse_v31(bytes, &s.lamps, rest),
            _ if tcp => tsl::unwrap_v5_tcp(bytes, rest)
                .iter()
                .flat_map(|p| tsl::parse_v5(p, s.screen))
                .collect(),
            _ => tsl::parse_v5(bytes, s.screen),
        };
        if !displays.is_empty() {
            self.displays(
                &s.id,
                displays.into_iter().map(|d| {
                    (
                        d.index,
                        DisplayState {
                            lamps: d.lamps,
                            text: d.text.unwrap_or_default(),
                        },
                    )
                }),
            );
        }
    }

    async fn run_tsl_udp(&self, s: &SourceConfig) {
        let sock = loop {
            match tokio::net::UdpSocket::bind(("0.0.0.0", s.port)).await {
                Ok(sock) => break sock,
                Err(e) => {
                    self.say(
                        &s.id,
                        format!("Cannot listen on UDP {}: {e}", s.port),
                        false,
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        };
        self.say(&s.id, format!("Listening on UDP {}", s.port), false);
        let mut buf = vec![0u8; 65536];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                continue;
            };
            let mut rest = Vec::new();
            self.tsl_bytes(s, &buf[..n], &mut rest, false);
            self.with_state(&s.id, |st| {
                if !st.connected {
                    st.connected = true;
                    st.status = format!("Receiving from {}", from.ip());
                }
            });
        }
    }

    async fn read_tcp(&self, s: &SourceConfig, mut stream: tokio::net::TcpStream) {
        let mut buf = vec![0u8; 16384];
        let mut rest = Vec::new();
        loop {
            match stream.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => self.tsl_bytes(s, &buf[..n], &mut rest, true),
            }
        }
    }

    async fn run_tsl_listen(self: &Arc<Self>, s: &SourceConfig) {
        let listener = loop {
            match tokio::net::TcpListener::bind(("0.0.0.0", s.port)).await {
                Ok(l) => break l,
                Err(e) => {
                    self.say(
                        &s.id,
                        format!("Cannot listen on TCP {}: {e}", s.port),
                        false,
                    );
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        };
        self.say(&s.id, format!("Listening on TCP {}", s.port), false);
        loop {
            let Ok((stream, from)) = listener.accept().await else {
                continue;
            };
            self.say(&s.id, format!("Connected from {}", from.ip()), true);
            let (me, s2) = (self.clone(), s.clone());
            tokio::spawn(async move {
                me.read_tcp(&s2, stream).await;
                me.say(&s2.id, format!("Listening on TCP {}", s2.port), false);
            });
        }
    }

    async fn run_tsl_connect(&self, s: &SourceConfig) {
        loop {
            match tokio::net::TcpStream::connect((s.host.as_str(), s.port)).await {
                Ok(stream) => {
                    self.say(&s.id, format!("Connected to {}:{}", s.host, s.port), true);
                    self.read_tcp(s, stream).await;
                    self.say(
                        &s.id,
                        format!("Lost {}:{} — trying again", s.host, s.port),
                        false,
                    );
                }
                Err(e) => {
                    self.say(
                        &s.id,
                        format!(
                            "Cannot connect to {}:{} ({e}) — trying again",
                            s.host, s.port
                        ),
                        false,
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn run_atem(&self, s: &SourceConfig) {
        let port = if s.port > 0 { s.port } else { atem::PORT };
        loop {
            let sock = match tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await {
                Ok(sock) => sock,
                Err(e) => {
                    self.say(&s.id, format!("Cannot open a socket: {e}"), false);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            };
            if let Err(e) = sock.connect((s.host.as_str(), port)).await {
                self.say(
                    &s.id,
                    format!("Cannot reach {} ({e}) — trying again", s.host),
                    false,
                );
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            self.say(&s.id, format!("Connecting to {}…", s.host), false);
            let mut session = atem::Session::default();
            let _ = sock.send(&session.hello()).await;
            let mut buf = vec![0u8; 2048];
            let mut connected = false;
            loop {
                let got = tokio::time::timeout(Duration::from_secs(5), sock.recv(&mut buf)).await;
                let n = match got {
                    Ok(Ok(n)) => n,
                    _ => break,
                };
                let (replies, events) = session.on_packet(&buf[..n]);
                for r in replies {
                    let _ = sock.send(&r).await;
                }
                if !connected {
                    connected = true;
                    self.say(&s.id, format!("Connected to {}", s.host), true);
                }
                for e in events {
                    match e {
                        atem::Event::Tally(list) => {
                            // Only the lamps change: the names stay.
                            let now = crate::state::now_ms();
                            self.with_state(&s.id, |st| {
                                st.last_at = Some(now);
                                for (input, pgm, pvw) in list {
                                    let d = st.displays.entry(input as u32).or_default();
                                    d.lamps.program = pgm;
                                    d.lamps.preview = pvw;
                                }
                            });
                        }
                        atem::Event::Name(input, name) => {
                            self.with_state(&s.id, |st| {
                                st.displays.entry(input as u32).or_default().text = name;
                            });
                        }
                    }
                }
            }
            self.say(
                &s.id,
                format!("No answer from {} — trying again", s.host),
                false,
            );
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally() -> Arc<Tally> {
        let dir = std::env::temp_dir().join(format!(
            "mv-tally-{}-{}",
            std::process::id(),
            strata_mv_core::new_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Tally::load(&dir, Arc::new(Inputs::default()))
    }

    fn source(id: &str, protocol: Protocol) -> SourceConfig {
        SourceConfig {
            id: id.into(),
            name: id.into(),
            enabled: true,
            protocol,
            transport: Transport::Udp,
            port: 0,
            host: String::new(),
            screen: None,
            lamps: tsl31_lamps(),
        }
    }

    #[test]
    fn routes_take_the_lamps_and_names_of_their_displays() {
        let t = tally();
        *t.config.write().unwrap() = TallyConfig {
            sources: vec![source("tsl", Protocol::Tsl5), source("gpi", Protocol::Gpi)],
            routes: vec![
                Route {
                    input: "cam1".into(),
                    source: "tsl".into(),
                    index: 1,
                    lamp: None,
                },
                Route {
                    input: "cam2".into(),
                    source: "tsl".into(),
                    index: 2,
                    lamp: None,
                },
                Route {
                    input: "cam2".into(),
                    source: "gpi".into(),
                    index: 7,
                    lamp: Some(Lamp::Aux),
                },
            ],
        };
        let red = Lamps {
            program: true,
            ..Default::default()
        };
        t.displays(
            "tsl",
            [
                (
                    1,
                    DisplayState {
                        lamps: red,
                        text: "Plateau".into(),
                    },
                ),
                (
                    2,
                    DisplayState {
                        lamps: Lamps::default(),
                        text: "Invité".into(),
                    },
                ),
            ],
        );
        t.gpi("gpi", 7, true).unwrap();
        let r = t.resolved();
        assert!(r["cam1"].program && r["cam1"].umd == "Plateau");
        assert!(r["cam2"].aux && !r["cam2"].program && r["cam2"].umd == "Invité");
        // The engine and the browsers have it.
        assert!(t.inputs.signals().tally("cam1").program);
        // The API has the last word.
        t.set_override(
            "cam1",
            Some(Override {
                program: Some(false),
                umd: Some("Régie".into()),
                ..Default::default()
            }),
        );
        let r = t.resolved();
        assert!(!r["cam1"].program && r["cam1"].umd == "Régie");
        t.set_override("cam1", None);
        assert!(t.resolved()["cam1"].program);
        assert!(t.gpi("tsl", 1, true).is_err());
    }

    #[test]
    fn settings_are_checked() {
        let mut c = TallyConfig {
            sources: vec![source("a", Protocol::Tsl31)],
            routes: vec![Route {
                input: "cam1".into(),
                source: "gone".into(),
                index: 1,
                lamp: None,
            }],
        };
        assert!(c.validate().is_err(), "a TSL source needs its port");
        c.sources[0].port = 8900;
        let mut b = source("b", Protocol::Tsl5);
        b.port = 8900;
        c.sources.push(b);
        assert!(c.validate().is_err(), "the same UDP port twice");
        c.sources[1].transport = Transport::TcpListen;
        c.validate().unwrap();
        assert!(c.routes.is_empty(), "a route to a source removed goes");
    }

    #[tokio::test]
    async fn a_tsl_datagram_lights_its_input() {
        let t = tally();
        let port = 41_000 + (std::process::id() % 1000) as u16;
        let mut s = source("tsl", Protocol::Tsl31);
        s.port = port;
        t.set_config(TallyConfig {
            sources: vec![s],
            routes: vec![Route {
                input: "cam3".into(),
                source: "tsl".into(),
                index: 3,
                lamp: None,
            }],
        })
        .unwrap();
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let msg = tsl::tests::v31(3, 0b10, "CAM 3");
        for _ in 0..50 {
            let _ = sock.send_to(&msg, ("127.0.0.1", port)).await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            if t.inputs.signals().tally("cam3").preview {
                break;
            }
        }
        let got = t.inputs.signals().tally("cam3");
        assert!(
            got.preview && got.umd == "CAM 3",
            "{got:?} {:?}",
            t.states()
        );
        t.stop();
    }
}
