//! The video hardware of the server (Hardware page): DeckLink cards and
//! what each SDI connector does, and the ST 2110 flows with their multicast
//! addresses. The Video streams destinations (and SDI / ST 2110 inputs)
//! name one of these by id; `ServerConfig::normalize` copies the actual
//! parameters into them, so the engine opens them as before.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddrV4};

use cg_output::st2110::St2110Encoding;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareConfig {
    #[serde(default)]
    pub sdi: SdiHardware,
    #[serde(default)]
    pub st2110: St2110Hardware,
    #[serde(default)]
    pub network: NetworkRoles,
    #[serde(default)]
    pub sync: SyncConfig,
}

/// Which network interface does what, by name (`en0`, `eth1`…: names stay
/// when addresses change). "" = no particular one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRoles {
    /// The web interface and the API listen on this one only (and on the
    /// loopback, so the machine itself always gets in).
    #[serde(default)]
    pub control: String,
    /// NDI sends and looks for sources on this one only.
    #[serde(default)]
    pub ndi: String,
    /// NDI Discovery Server(s) (`10.0.0.2`, comma separated); "" = mDNS.
    #[serde(default)]
    pub ndi_discovery: String,
    /// The machine name of the NDI sources (`NAME (source)`); "" = the computer's.
    #[serde(default)]
    pub ndi_machine: String,
    /// ST 2110 primary network (SMPTE 2022-7 red).
    #[serde(default)]
    pub media_red: String,
    /// ST 2110 secondary network (2022-7 blue).
    #[serde(default)]
    pub media_blue: String,
    /// The interface ptp4l runs on (its hardware clock disciplines the
    /// system clock through phc2sys).
    #[serde(default)]
    pub ptp: String,
    /// The MOS ports listen on this one only ("" = every interface).
    #[serde(default)]
    pub mos: String,
}

/// What the channel clocks follow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClockReference {
    /// The best reference there is, followed as it comes and goes: the
    /// DeckLink card when its reference input is locked, else PTP when
    /// locked, else the card's own clock (an SDI output), else the server's
    /// time.
    #[default]
    Auto,
    /// The server's time (NTP): frames fall on the same grid as with PTP,
    /// so servers on the same NTP time show theirs about together.
    Free,
    /// The system clock disciplined by PTP (ptp4l + phc2sys): frames fall on
    /// the SMPTE ST 2059 grid (multiples of the frame period since the PTP
    /// epoch), so every 2110 device of the plant agrees on them.
    Ptp,
    /// The genlock of a DeckLink card: the channel follows the frame
    /// boundaries of its first SDI destination.
    Decklink,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    /// The channels' default reference (each channel can choose another).
    #[serde(default)]
    pub reference: ClockReference,
    /// PTP domain (SMPTE ST 2059-2 default: 127).
    #[serde(default = "default_domain")]
    pub ptp_domain: u8,
    /// TAI − UTC in seconds (37 since 2017): the system clock is UTC, PTP
    /// and RTP timestamps count TAI.
    #[serde(default = "default_tai")]
    pub tai_offset: u32,
    /// Settings saved before « Auto » (1): their default « Free » was not
    /// chosen, it becomes « Auto ».
    #[serde(default)]
    pub version: u8,
}

impl SyncConfig {
    pub fn migrate(&mut self) {
        if self.version < 1 && self.reference == ClockReference::Free {
            self.reference = ClockReference::Auto;
        }
        self.version = 1;
    }
}

fn default_domain() -> u8 {
    127
}
fn default_tai() -> u32 {
    37
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            reference: ClockReference::Auto,
            ptp_domain: default_domain(),
            tai_offset: default_tai(),
            version: 1,
        }
    }
}

/// The IPv4 address of an interface by name.
pub fn interface_ip(name: &str) -> Option<Ipv4Addr> {
    if name.is_empty() {
        return None;
    }
    // An interface can have an IPv6 address listed before its IPv4 one.
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .filter(|i| i.name == name)
        .find_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            _ => None,
        })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiHardware {
    #[serde(default)]
    pub cards: Vec<SdiCard>,
    /// Fill (+ key) pairs the channels send to.
    #[serde(default)]
    pub outputs: Vec<SdiOutputDef>,
    #[serde(default)]
    pub inputs: Vec<SdiInputDef>,
    /// Detected cards the admin chose not to use (`DetectedCard::id`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<String>,
}

/// A DeckLink card the server uses: adopted from detection (its identity
/// filled), or declared by hand before detection existed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiCard {
    pub id: String,
    pub name: String,
    /// `miniMonitor`, `miniRecorder`, `sdi4k`, `duo2`, `quad2`, `8kPro`, `other`.
    #[serde(default = "other")]
    pub model: String,
    /// Index of the card's first device in Desktop Video (0 = first).
    #[serde(default)]
    pub device: u32,
    /// Its SDI connectors, in order (their role comes from the outputs and
    /// inputs using them).
    #[serde(default)]
    pub connectors: Vec<SdiConnector>,
    /// The model as Desktop Video names it ("DeckLink Quad 2"); "" = a card
    /// declared by hand, not seen by the driver yet.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_name: String,
    /// The identity of the detected card (`DetectedCard::id`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub persistent_id: String,
    /// The Desktop Video device of each connector (index = connector), as
    /// detected; refreshed when the server starts (the order of Desktop
    /// Video changes when cards are added). Empty: `device` + connector.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<u32>,
    /// How the card is used (a mode of its model, see `decklink::MODELS`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// Its outputs follow the reference input (genlock).
    #[serde(default)]
    pub genlock: bool,
    /// Genlock offset, in lines.
    #[serde(default)]
    pub genlock_offset: i32,
}

fn other() -> String {
    "other".into()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiConnector {
    /// What it is wired to ("Mixer key 3"…).
    #[serde(default)]
    pub name: String,
}

impl SdiCard {
    /// The Desktop Video device of a connector: cards with several
    /// connectors show one device per connector, the others one in all.
    pub fn device_of(&self, connector: u32) -> u32 {
        if let Some(d) = self.devices.get(connector as usize) {
            return *d;
        }
        match self.model.as_str() {
            "miniMonitor" | "miniRecorder" | "sdi4k" => self.device,
            _ => self.device + connector,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SdiMode {
    /// Fill and key on two connectors, for an external keyer.
    #[default]
    ExternalKey,
    /// The card's internal keyer, over its SDI input.
    InternalKey,
    FillOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiOutputDef {
    pub id: String,
    pub name: String,
    pub card: String,
    /// Connector of the fill (0 = first).
    #[serde(default)]
    pub fill: u32,
    /// Connector of the key (external keyer).
    #[serde(default)]
    pub key: Option<u32>,
    #[serde(default)]
    pub mode: SdiMode,
    #[serde(default)]
    pub genlock: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdiInputDef {
    pub id: String,
    pub name: String,
    pub card: String,
    #[serde(default)]
    pub connector: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct St2110Hardware {
    #[serde(default)]
    pub flows: Vec<St2110Flow>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowDirection {
    #[default]
    Output,
    /// Received (not implemented by the server yet).
    Input,
}

/// An ST 2110 flow: the multicast destinations of a sender (or the groups
/// a receiver joins).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct St2110Flow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub direction: FlowDirection,
    #[serde(default)]
    pub encoding: St2110Encoding,
    pub fill: SocketAddrV4,
    #[serde(default)]
    pub key: Option<SocketAddrV4>,
    /// Sending interface; none = the Media red one of the network roles.
    #[serde(default)]
    pub interface: Option<Ipv4Addr>,
    /// SMPTE 2022-7: the same packets again on the blue network, to these
    /// destinations (none = no redundancy).
    #[serde(default)]
    pub blue: Option<SocketAddrV4>,
    #[serde(default)]
    pub blue_key: Option<SocketAddrV4>,
    /// Blue interface; none = the Media blue one of the network roles.
    #[serde(default)]
    pub blue_interface: Option<Ipv4Addr>,
    #[serde(default = "default_ttl")]
    pub ttl: u32,
    #[serde(default = "default_pt")]
    pub payload_type: u8,
}

fn default_ttl() -> u32 {
    32
}
fn default_pt() -> u8 {
    96
}

/// `kind-1`, `kind-2`…: the first id not taken.
fn next_id(kind: &str, taken: &HashSet<String>) -> String {
    let mut n = 1;
    while taken.contains(&format!("{kind}-{n}")) {
        n += 1;
    }
    format!("{kind}-{n}")
}

impl HardwareConfig {
    /// Ids for everything (unique per list).
    pub fn ensure_ids(&mut self) {
        fn fill<'a>(kind: &str, ids: impl Iterator<Item = &'a mut String>) {
            let mut taken = HashSet::new();
            for id in ids {
                if id.is_empty() || !taken.insert(id.clone()) {
                    *id = next_id(kind, &taken);
                    taken.insert(id.clone());
                }
            }
        }
        fill("card", self.sdi.cards.iter_mut().map(|c| &mut c.id));
        fill("sdi-out", self.sdi.outputs.iter_mut().map(|c| &mut c.id));
        fill("sdi-in", self.sdi.inputs.iter_mut().map(|c| &mut c.id));
        fill("flow", self.st2110.flows.iter_mut().map(|c| &mut c.id));
    }

    pub fn card(&self, id: &str) -> Option<&SdiCard> {
        self.sdi.cards.iter().find(|c| c.id == id)
    }

    /// The settings of an SDI destination from its Hardware output.
    pub fn apply_sdi(&self, o: &mut cg_output::sdi::SdiConfig) {
        let Some(def) = self.sdi.outputs.iter().find(|d| d.id == o.hardware) else {
            return;
        };
        if let Some(card) = self.card(&def.card) {
            o.device = card.device_of(def.fill);
        }
        o.external_key = def.mode != SdiMode::InternalKey;
        o.fill_only = def.mode == SdiMode::FillOnly;
        let card = self.card(&def.card);
        o.genlock = def.genlock || card.is_some_and(|c| c.genlock);
        o.genlock_offset = card.map_or(0, |c| c.genlock_offset);
    }

    /// Adopted cards follow the card detected with their identity (the
    /// Desktop Video order changes when cards are added). Cards declared
    /// by hand take the identity of the detected card at their device
    /// index; their outputs and inputs keep the Desktop Video devices they
    /// had, now under the label of the connector that device is on. True
    /// when something changed.
    pub fn link_detected(&mut self, detected: &[crate::decklink::DetectedCard]) -> bool {
        let mut changed = false;
        let mut taken: HashSet<String> = self
            .sdi
            .cards
            .iter()
            .map(|c| c.persistent_id.clone())
            .filter(|p| !p.is_empty())
            .collect();
        // Connector renumberings of the cards declared by hand.
        let mut moves: Vec<(String, Vec<u32>)> = Vec::new();
        for c in &mut self.sdi.cards {
            let d = if c.persistent_id.is_empty() {
                let d = detected.iter().find(|d| {
                    !taken.contains(&d.id)
                        && d.devices.first() == Some(&c.device)
                        && (c.model == d.legacy_model || c.model == "other")
                });
                let Some(d) = d else { continue };
                taken.insert(d.id.clone());
                // Where each connector's device is on the detected card.
                let to: Vec<Option<u32>> = (0..c.connectors.len() as u32)
                    .map(|k| {
                        let dev = c.device_of(k);
                        d.devices.iter().position(|x| *x == dev).map(|p| p as u32)
                    })
                    .collect();
                let unique: HashSet<_> = to.iter().flatten().collect();
                if to
                    .iter()
                    .all(|t| t.is_some_and(|t| (t as usize) < c.connectors.len()))
                    && unique.len() == to.len()
                {
                    moves.push((c.id.clone(), to.into_iter().flatten().collect()));
                }
                c.persistent_id = d.id.clone();
                c.model_name = d.model_name.clone();
                c.model = d.legacy_model.clone();
                changed = true;
                d
            } else {
                match detected.iter().find(|d| d.id == c.persistent_id) {
                    Some(d) => d,
                    None => continue,
                }
            };
            let first = d.devices.first().copied().unwrap_or(c.device);
            let devices: Vec<u32> = (0..c.connectors.len())
                .map(|i| d.devices.get(i).copied().unwrap_or(first))
                .collect();
            if c.devices != devices || c.device != first {
                c.devices = devices;
                c.device = first;
                changed = true;
            }
        }
        for (card, to) in moves {
            let at = |k: u32| to.get(k as usize).copied().unwrap_or(k);
            for o in self.sdi.outputs.iter_mut().filter(|o| o.card == card) {
                o.fill = at(o.fill);
                o.key = o.key.map(at);
            }
            for i in self.sdi.inputs.iter_mut().filter(|i| i.card == card) {
                i.connector = at(i.connector);
            }
            if let Some(c) = self.sdi.cards.iter_mut().find(|c| c.id == card) {
                let old = c.connectors.clone();
                for (k, conn) in old.into_iter().enumerate() {
                    if let Some(slot) = c.connectors.get_mut(at(k as u32) as usize) {
                        *slot = conn;
                    }
                }
            }
        }
        changed
    }

    /// The settings of an ST 2110 destination from its Hardware flow.
    pub fn apply_st2110(&self, o: &mut cg_output::st2110::St2110Config) {
        let Some(f) = self.st2110.flows.iter().find(|f| f.id == o.hardware) else {
            return;
        };
        o.encoding = f.encoding;
        o.fill = f.fill;
        o.key = if f.encoding == St2110Encoding::FillKey {
            f.key
        } else {
            None
        };
        o.interface = f
            .interface
            .or_else(|| interface_ip(&self.network.media_red));
        let keyed = o.key.is_some();
        o.blue = f.blue.map(|fill| cg_output::st2110::Leg {
            fill,
            key: f.blue_key.filter(|_| keyed),
            interface: f
                .blue_interface
                .or_else(|| interface_ip(&self.network.media_blue)),
        });
        o.ttl = f.ttl;
        o.payload_type = f.payload_type;
        o.tai_offset = self.sync.tai_offset;
    }

    /// An SDI destination saved before the Hardware page: its card and
    /// output are declared from its own settings.
    pub fn adopt_sdi(&mut self, o: &mut cg_output::sdi::SdiConfig) {
        let mode = if o.fill_only {
            SdiMode::FillOnly
        } else if o.external_key {
            SdiMode::ExternalKey
        } else {
            SdiMode::InternalKey
        };
        let keyed = mode == SdiMode::ExternalKey;
        let card = self.new_card(o.device, if keyed { 2 } else { 1 });
        let taken = self.sdi.outputs.iter().map(|d| d.id.clone()).collect();
        let id = next_id("sdi-out", &taken);
        self.sdi.outputs.push(SdiOutputDef {
            id: id.clone(),
            name: o.name.clone(),
            card,
            fill: 0,
            key: keyed.then_some(1),
            mode,
            genlock: o.genlock,
        });
        o.hardware = id;
    }

    /// An SDI input saved before the Hardware page.
    pub fn adopt_sdi_input(&mut self, i: &mut crate::inputs::InputConfig) {
        let card = self.new_card(i.device, 1);
        let taken = self.sdi.inputs.iter().map(|d| d.id.clone()).collect();
        let id = next_id("sdi-in", &taken);
        self.sdi.inputs.push(SdiInputDef {
            id: id.clone(),
            name: i.name.clone(),
            card,
            connector: 0,
        });
        i.hardware = id;
    }

    fn new_card(&mut self, device: u32, connectors: usize) -> String {
        let taken = self.sdi.cards.iter().map(|c| c.id.clone()).collect();
        let id = next_id("card", &taken);
        self.sdi.cards.push(SdiCard {
            id: id.clone(),
            name: format!("DeckLink #{device}"),
            model: other(),
            device,
            connectors: vec![SdiConnector::default(); connectors],
            model_name: String::new(),
            persistent_id: String::new(),
            devices: Vec::new(),
            mode: String::new(),
            genlock: false,
            genlock_offset: 0,
        });
        id
    }

    /// An ST 2110 destination saved before the Hardware page: its flow is
    /// declared from its own settings.
    pub fn adopt_st2110(&mut self, o: &mut cg_output::st2110::St2110Config) {
        let taken = self.st2110.flows.iter().map(|f| f.id.clone()).collect();
        let id = next_id("flow", &taken);
        self.st2110.flows.push(St2110Flow {
            id: id.clone(),
            name: o.name.clone(),
            direction: FlowDirection::Output,
            encoding: o.encoding,
            fill: o.fill,
            key: o.key,
            interface: o.interface,
            blue: None,
            blue_key: None,
            blue_interface: None,
            ttl: o.ttl,
            payload_type: o.payload_type,
        });
        o.hardware = id;
    }

    pub fn validate(&self) -> Result<(), String> {
        let unique = |what: &str, ids: Vec<&str>| -> Result<(), String> {
            let mut seen = HashSet::new();
            for id in ids {
                if id.is_empty() || !seen.insert(id) {
                    return Err(format!("Hardware: {what} ids must be unique"));
                }
            }
            Ok(())
        };
        unique(
            "card",
            self.sdi.cards.iter().map(|c| c.id.as_str()).collect(),
        )?;
        unique(
            "SDI output",
            self.sdi.outputs.iter().map(|c| c.id.as_str()).collect(),
        )?;
        unique(
            "SDI input",
            self.sdi.inputs.iter().map(|c| c.id.as_str()).collect(),
        )?;
        unique(
            "ST 2110 flow",
            self.st2110.flows.iter().map(|c| c.id.as_str()).collect(),
        )?;
        for c in &self.sdi.cards {
            if c.connectors.is_empty() || c.connectors.len() > 16 {
                return Err(format!("card `{}`: 1 to 16 connectors", c.name));
            }
            if c.genlock_offset.unsigned_abs() > 1125 {
                return Err(format!("card `{}`: genlock offset out of range", c.name));
            }
        }
        // Each connector does one thing.
        let mut used = HashSet::new();
        let mut take = |name: &str, card: &str, connector: u32| -> Result<(), String> {
            let c = self
                .card(card)
                .ok_or_else(|| format!("SDI `{name}`: its card is not declared"))?;
            if connector as usize >= c.connectors.len() {
                return Err(format!(
                    "SDI `{name}`: card `{}` has no connector {}",
                    c.name,
                    connector + 1
                ));
            }
            if !used.insert((card.to_string(), connector)) {
                return Err(format!(
                    "card `{}`: connector {} is used twice",
                    c.name,
                    connector + 1
                ));
            }
            Ok(())
        };
        for o in &self.sdi.outputs {
            if o.name.trim().is_empty() {
                return Err("every SDI output needs a name".into());
            }
            take(&o.name, &o.card, o.fill)?;
            // A keyer (external or internal) takes the second connector of its pair.
            if let Some(k) = o.key.filter(|_| o.mode != SdiMode::FillOnly) {
                take(&o.name, &o.card, k)?;
            }
        }
        for i in &self.sdi.inputs {
            if i.name.trim().is_empty() {
                return Err("every SDI input needs a name".into());
            }
            take(&i.name, &i.card, i.connector)?;
        }
        let mut dests = HashSet::new();
        for f in &self.st2110.flows {
            if f.name.trim().is_empty() {
                return Err("every ST 2110 flow needs a name".into());
            }
            let key = f.key.filter(|_| f.encoding == St2110Encoding::FillKey);
            if f.fill.port() == 0 || key.is_some_and(|k| k.port() == 0) {
                return Err(format!("flow `{}`: a port is missing", f.name));
            }
            if key == Some(f.fill) {
                return Err(format!(
                    "flow `{}`: fill and key must use different destinations",
                    f.name
                ));
            }
            let blue_key = f.blue_key.filter(|_| key.is_some());
            if let Some(b) = f.blue {
                if key.is_some() && blue_key.is_none() {
                    return Err(format!(
                        "flow `{}`: the blue key destination is missing",
                        f.name
                    ));
                }
                if b.port() == 0 || blue_key.is_some_and(|k| k.port() == 0) {
                    return Err(format!("flow `{}`: a blue port is missing", f.name));
                }
            }
            if f.direction == FlowDirection::Output {
                let blue = f.blue.into_iter().chain(blue_key);
                for d in std::iter::once(f.fill).chain(key).chain(blue) {
                    if !dests.insert(d) {
                        return Err(format!("destination {d} is used by two flows"));
                    }
                }
            }
        }
        let n = &self.network;
        if !n.media_red.is_empty() && n.media_red == n.media_blue {
            return Err("the red and blue media networks must be different interfaces".into());
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ NDI --

/// Where the server keeps the NDI runtime settings (`NDI_CONFIG_DIR`).
pub fn ndi_config_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("ndi")
}

/// The NDI runtime settings for the network roles: NDI on its interface
/// only, and the Discovery Server when there is one. The runtime reads
/// them when it starts (once per server process).
pub fn ndi_settings(roles: &NetworkRoles) -> serde_json::Value {
    let mut ndi = serde_json::Map::new();
    if let Some(ip) = interface_ip(&roles.ndi) {
        ndi.insert(
            "adapters".into(),
            serde_json::json!({ "allowed": [ip.to_string()] }),
        );
    }
    let discovery = roles.ndi_discovery.trim();
    if !discovery.is_empty() {
        ndi.insert(
            "networks".into(),
            serde_json::json!({ "ips": "", "discovery": discovery }),
        );
    }
    // In place of the computer's name: receivers see « NAME (source) ».
    let machine = roles.ndi_machine.trim();
    if !machine.is_empty() {
        ndi.insert("machinename".into(), machine.into());
    }
    serde_json::json!({ "ndi": ndi })
}

/// The NDI settings the runtime started with (it reads them only once).
static NDI_STARTED_WITH: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The settings file changed since the NDI runtime read it.
pub fn ndi_restart_needed(data_dir: &std::path::Path) -> bool {
    if cg_output::ndi::loaded_path().is_none() {
        return false;
    }
    let file = ndi_config_dir(data_dir).join("ndi-config.v1.json");
    let now = std::fs::read_to_string(file).unwrap_or_default();
    let started = NDI_STARTED_WITH.lock().unwrap_or_else(|e| e.into_inner());
    started.as_ref().is_some_and(|s| *s != now)
}

/// Writes `ndi-config.v1.json` for the runtime (true when it changed).
pub fn write_ndi_settings(data_dir: &std::path::Path, roles: &NetworkRoles) -> bool {
    let dir = ndi_config_dir(data_dir);
    let file = dir.join("ndi-config.v1.json");
    let text = serde_json::to_string_pretty(&ndi_settings(roles)).unwrap_or_default();
    // Until the runtime loads, it will read this one.
    if cg_output::ndi::loaded_path().is_none() {
        *NDI_STARTED_WITH.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.clone());
    }
    if std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
        return false;
    }
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, &text)) {
        tracing::warn!("cannot write the NDI settings ({}): {e}", file.display());
        return false;
    }
    true
}

// ------------------------------------------------------------- detection --

/// `GET /api/hardware`: what this machine has.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    /// Blackmagic Desktop Video (the DeckLink driver) is installed.
    pub desktop_video: bool,
    /// Where it was found.
    pub desktop_video_path: Option<String>,
    /// This server can drive DeckLink cards (built with the SDK).
    pub decklink_support: bool,
    /// The DeckLink devices the driver lists (Desktop Video order).
    pub decklink_devices: Vec<String>,
    /// Why they cannot be listed.
    pub decklink_error: Option<String>,
    /// The DeckLink cards, each with the devices of its connectors.
    pub decklink_cards: Vec<crate::decklink::DetectedCard>,
    /// The bracket of the known models.
    pub decklink_models: &'static [crate::decklink::Model],
    /// The cards are made up (`STRATA_FAKE_DECKLINK`).
    pub decklink_fake: bool,
    /// Desktop Video version.
    pub desktop_video_version: Option<String>,
    /// Every network interface, with its link.
    pub nics: Vec<Nic>,
    /// This server receives ST 2110.
    pub st2110_receive: bool,
    pub interfaces: Vec<NetInterface>,
    pub os: &'static str,
    /// NTP / PTP status.
    pub sync: Option<crate::sync::SyncStatus>,
    /// The NDI network settings changed since the NDI runtime started.
    pub ndi_restart: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInterface {
    pub name: String,
    pub ip: String,
    pub loopback: bool,
}

/// A network interface and its link (Linux tells all; elsewhere only the
/// addresses are known).
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Nic {
    pub name: String,
    /// `10.10.0.21/24`.
    pub ipv4: Vec<String>,
    pub mac: Option<String>,
    pub mtu: Option<u32>,
    /// Link speed, Mb/s.
    pub speed: Option<u32>,
    /// The link is up (None: unknown).
    pub up: Option<bool>,
    /// `ethernet`, `wifi`, `virtual`.
    pub kind: Option<&'static str>,
    /// The adapter family, from its driver ("Mellanox ConnectX").
    pub model: Option<String>,
}

/// The adapter family of a Linux network driver.
#[cfg(target_os = "linux")]
fn driver_model(driver: &str) -> Option<&'static str> {
    Some(match driver {
        "mlx5_core" => "Mellanox ConnectX",
        "mlx4_en" | "mlx4_core" => "Mellanox ConnectX-3",
        "ice" => "Intel E810",
        "i40e" => "Intel X710",
        "ixgbe" => "Intel X520",
        "igb" => "Intel I350",
        "igc" => "Intel I225",
        "e1000e" | "e1000" => "Intel Gigabit",
        "bnxt_en" => "Broadcom NetXtreme-E",
        "tg3" => "Broadcom NetXtreme",
        "r8169" | "r8125" => "Realtek",
        "atlantic" => "Aquantia AQtion",
        "sfc" => "Solarflare",
        "virtio_net" => "virtio",
        _ => return None,
    })
}

/// The network interfaces, loopback aside.
fn nics() -> Vec<Nic> {
    let addrs = if_addrs::get_if_addrs().unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    #[cfg(target_os = "linux")]
    if let Ok(dir) = std::fs::read_dir("/sys/class/net") {
        names.extend(
            dir.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned()),
        );
    }
    for a in &addrs {
        if !names.contains(&a.name) {
            names.push(a.name.clone());
        }
    }
    let mut out: Vec<Nic> = names
        .into_iter()
        .filter(|n| !addrs.iter().any(|a| a.name == *n && a.is_loopback()) && n != "lo")
        .map(|name| {
            let ipv4 = addrs
                .iter()
                .filter(|a| a.name == name)
                .filter_map(|a| match &a.addr {
                    if_addrs::IfAddr::V4(v) => {
                        Some(format!("{}/{}", v.ip, u32::from(v.netmask).count_ones()))
                    }
                    _ => None,
                })
                .collect();
            let mut n = Nic {
                name,
                ipv4,
                ..Nic::default()
            };
            #[cfg(target_os = "linux")]
            {
                let dir = std::path::Path::new("/sys/class/net").join(&n.name);
                let read = |f: &str| {
                    std::fs::read_to_string(dir.join(f))
                        .ok()
                        .map(|s| s.trim().to_string())
                };
                n.mac = read("address").filter(|m| !m.is_empty() && m != "00:00:00:00:00:00");
                n.mtu = read("mtu").and_then(|v| v.parse().ok());
                n.up = read("operstate").map(|s| {
                    s == "up" || (s == "unknown" && read("carrier").as_deref() == Some("1"))
                });
                // -1 (or unreadable) when there is no link.
                n.speed = read("speed")
                    .and_then(|v| v.parse::<i64>().ok())
                    .filter(|v| *v > 0 && n.up == Some(true))
                    .map(|v| v as u32);
                let driver = std::fs::read_link(dir.join("device/driver"))
                    .ok()
                    .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()));
                n.kind = Some(
                    if dir.join("wireless").exists() || dir.join("phy80211").exists() {
                        "wifi"
                    } else if dir.join("device").exists() {
                        "ethernet"
                    } else {
                        "virtual"
                    },
                );
                n.model = driver.as_deref().and_then(driver_model).map(str::to_string);
            }
            #[cfg(not(target_os = "linux"))]
            {
                n.up = (!n.ipv4.is_empty()).then_some(true);
            }
            n
        })
        .collect();
    out.sort_by(|a, b| (a.ipv4.is_empty(), &a.name).cmp(&(b.ipv4.is_empty(), &b.name)));
    out
}

/// The places Desktop Video installs its driver library.
fn desktop_video_paths() -> Vec<std::path::PathBuf> {
    let p = |s: &str| std::path::PathBuf::from(s);
    match std::env::consts::OS {
        "macos" => vec![
            p("/Library/Frameworks/DeckLinkAPI.framework"),
            p("/Library/Application Support/Blackmagic Design/Desktop Video"),
            p("/Library/Application Support/Blackmagic Design"),
        ],
        "windows" => {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            vec![std::path::Path::new(&root).join(r"System32\DeckLinkAPI64.dll")]
        }
        _ => vec![
            p("/usr/lib/libDeckLinkAPI.so"),
            p("/usr/lib64/libDeckLinkAPI.so"),
            p("/usr/lib/x86_64-linux-gnu/libDeckLinkAPI.so"),
            p("/usr/lib/aarch64-linux-gnu/libDeckLinkAPI.so"),
        ],
    }
}

pub fn detect() -> HardwareInfo {
    let found = desktop_video_paths().into_iter().find(|p| p.exists());
    let devices = cg_output::decklink::devices();
    let mut interfaces: Vec<NetInterface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| i.ip().is_ipv4())
        .map(|i| NetInterface {
            ip: i.ip().to_string(),
            loopback: i.is_loopback(),
            name: i.name,
        })
        .collect();
    interfaces.sort_by(|a, b| (a.loopback, &a.name).cmp(&(b.loopback, &b.name)));
    let (cards, version, fake) = crate::decklink::detect();
    HardwareInfo {
        decklink_cards: cards,
        decklink_models: crate::decklink::MODELS,
        decklink_fake: fake,
        desktop_video_version: version,
        nics: nics(),
        desktop_video: found.is_some() || fake,
        desktop_video_path: found.map(|p| p.display().to_string()),
        decklink_support: cg_output::sdi::SUPPORTED,
        decklink_devices: devices.clone().unwrap_or_default(),
        decklink_error: devices.err(),
        st2110_receive: false,
        interfaces,
        os: std::env::consts::OS,
        sync: None,
        ndi_restart: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hand_declared_cards_take_the_identity_of_the_detected_one() {
        let cards = crate::decklink::group(&[]);
        assert!(cards.is_empty());
        let mut hw: HardwareConfig = serde_json::from_value(serde_json::json!({
            "sdi": {
                "cards": [{ "id": "card-1", "name": "Quad", "model": "quad2", "device": 0,
                            "connectors": [{}, {}, {}, {}, {}, {}, {}, {}] }],
                "outputs": [{ "id": "sdi-out-1", "name": "CG", "card": "card-1", "fill": 1, "key": 5,
                              "mode": "externalKey" }]
            }
        }))
        .unwrap();
        // Before detection: device + connector, as declared.
        assert_eq!(hw.sdi.cards[0].device_of(1), 1);
        let detected = vec![crate::decklink::DetectedCard {
            id: "7B10A04E".into(),
            model_name: "DeckLink Quad 2".into(),
            legacy_model: "quad2".into(),
            connectors: 8,
            devices: vec![0, 2, 4, 6, 1, 3, 5, 7],
            ports: vec![crate::decklink::Port::Both; 8],
            has_reference: true,
            reference_locked: Some(true),
            reference_kind: None,
            reference_format: None,
            interface: None,
            topological_id: None,
            persistent_id: None,
            profile: None,
            profiles: vec![],
            connector_profiles: vec![],
            internal_keyer: true,
            external_keyer: true,
        }];
        assert!(hw.link_detected(&detected));
        let c = &hw.sdi.cards[0];
        assert_eq!(c.persistent_id, "7B10A04E");
        assert_eq!(c.model_name, "DeckLink Quad 2");
        // SDI 2 is the third device Desktop Video lists.
        assert_eq!(c.device_of(1), 2);
        // The output keeps its devices (1 and 5), now on SDI 5 and SDI 7.
        let o = &hw.sdi.outputs[0];
        assert_eq!((o.fill, o.key), (4, Some(6)));
        assert!(!hw.link_detected(&detected));
        let mut o: cg_output::sdi::SdiConfig = serde_json::from_value(serde_json::json!({
            "name": "CG", "device": 0, "hardware": "sdi-out-1"
        }))
        .unwrap();
        hw.sdi.cards[0].genlock = true;
        hw.sdi.cards[0].genlock_offset = -2;
        hw.apply_sdi(&mut o);
        assert_eq!((o.device, o.genlock, o.genlock_offset), (1, true, -2));
    }

    fn flow() -> St2110Flow {
        St2110Flow {
            id: "flow-1".into(),
            name: "CG".into(),
            direction: FlowDirection::Output,
            encoding: St2110Encoding::FillKey,
            fill: "239.1.0.1:5004".parse().unwrap(),
            key: Some("239.1.0.2:5004".parse().unwrap()),
            interface: Some("10.1.0.5".parse().unwrap()),
            blue: Some("239.2.0.1:5004".parse().unwrap()),
            blue_key: Some("239.2.0.2:5004".parse().unwrap()),
            blue_interface: Some("10.2.0.5".parse().unwrap()),
            ttl: 32,
            payload_type: 96,
        }
    }

    #[test]
    fn a_flow_with_a_blue_leg_sends_twice() {
        let mut hw = HardwareConfig::default();
        hw.st2110.flows.push(flow());
        hw.sync.tai_offset = 37;
        let mut o: cg_output::st2110::St2110Config = serde_json::from_value(serde_json::json!({
            "name": "CG", "fill": "0.0.0.0:0", "hardware": "flow-1"
        }))
        .unwrap();
        hw.apply_st2110(&mut o);
        let blue = o.blue.unwrap();
        assert_eq!(blue.fill, "239.2.0.1:5004".parse().unwrap());
        assert_eq!(blue.key, Some("239.2.0.2:5004".parse().unwrap()));
        assert_eq!(blue.interface, Some("10.2.0.5".parse().unwrap()));
        assert_eq!(o.tai_offset, 37);
        assert!(hw.validate().is_ok());
    }

    #[test]
    fn blue_destinations_are_checked() {
        let mut hw = HardwareConfig::default();
        hw.st2110.flows.push(St2110Flow {
            blue_key: None,
            ..flow()
        });
        assert!(hw.validate().unwrap_err().contains("blue key"));
        let mut hw = HardwareConfig::default();
        hw.st2110.flows.push(St2110Flow {
            blue: Some("239.1.0.1:5004".parse().unwrap()),
            ..flow()
        });
        assert!(hw.validate().unwrap_err().contains("used by two"));
        let mut hw = HardwareConfig::default();
        hw.network.media_red = "eth1".into();
        hw.network.media_blue = "eth1".into();
        assert!(hw.validate().is_err());
    }

    #[test]
    fn ndi_settings_follow_the_roles() {
        let roles = NetworkRoles {
            ndi_discovery: "10.0.0.2".into(),
            // No such interface: no adapter restriction.
            ndi: "no-such-nic".into(),
            ..NetworkRoles::default()
        };
        let v = ndi_settings(&roles);
        assert_eq!(v["ndi"]["networks"]["discovery"], "10.0.0.2");
        assert!(v["ndi"].get("adapters").is_none());
        let lo = if_addrs::get_if_addrs()
            .unwrap()
            .into_iter()
            .find(|i| i.is_loopback() && i.ip().is_ipv4())
            .unwrap();
        let v = ndi_settings(&NetworkRoles {
            ndi: lo.name.clone(),
            ..NetworkRoles::default()
        });
        assert_eq!(v["ndi"]["adapters"]["allowed"][0], "127.0.0.1");
        assert!(v["ndi"].get("networks").is_none());
        assert!(v["ndi"].get("machinename").is_none(), "the computer's name");
        let v = ndi_settings(&NetworkRoles {
            ndi_machine: " REGIE-1 ".into(),
            ..NetworkRoles::default()
        });
        assert_eq!(v["ndi"]["machinename"], "REGIE-1");
    }
}
