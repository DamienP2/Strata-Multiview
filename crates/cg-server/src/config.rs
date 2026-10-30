//! Server settings editable from the web UI (`<data>/config.json`).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::Path;

use cg_core::FrameRate;
use cg_output::ndi::NdiConfig;
use cg_output::sdi::SdiConfig;
use cg_output::st2110::St2110Config;
use cg_output::stream::StreamConfig;
use cg_render::slate::{Slate, SlateStyle};
use serde::{Deserialize, Serialize};

use crate::hardware::{FlowDirection, HardwareConfig};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputsConfig {
    #[serde(default)]
    pub st2110: Vec<St2110Config>,
    #[serde(default)]
    pub sdi: Vec<SdiConfig>,
    #[serde(default)]
    pub ndi: Vec<NdiConfig>,
    /// Web links to the picture (`/output/<channel>/<picture>`).
    #[serde(default)]
    pub html5: Vec<Html5Config>,
    /// SRT, UDP, RTMP and HLS streams (encoded by ffmpeg).
    #[serde(default)]
    pub streams: Vec<StreamConfig>,
}

impl OutputsConfig {
    /// Gives every destination an id (`sdi-1`, `ndi-2`…): stable, so loading
    /// twice gives the same config.
    fn ensure_ids(&mut self, taken: &mut HashSet<String>) {
        fn fill(kind: &str, id: &mut String, taken: &mut HashSet<String>) {
            if !id.is_empty() && taken.insert(id.clone()) {
                return;
            }
            let mut n = 1;
            while taken.contains(&format!("{kind}-{n}")) {
                n += 1;
            }
            *id = format!("{kind}-{n}");
            taken.insert(id.clone());
        }
        self.st2110
            .iter_mut()
            .for_each(|o| fill("st2110", &mut o.id, taken));
        self.sdi
            .iter_mut()
            .for_each(|o| fill("sdi", &mut o.id, taken));
        self.ndi
            .iter_mut()
            .for_each(|o| fill("ndi", &mut o.id, taken));
        self.html5
            .iter_mut()
            .for_each(|o| fill("html5", &mut o.id, taken));
        self.streams
            .iter_mut()
            .for_each(|o| fill("stream", &mut o.id, taken));
    }

    pub fn is_empty(&self) -> bool {
        self.ids().next().is_none()
    }

    /// Ids of the destinations, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.st2110
            .iter()
            .map(|o| o.id.as_str())
            .chain(self.sdi.iter().map(|o| o.id.as_str()))
            .chain(self.ndi.iter().map(|o| o.id.as_str()))
            .chain(self.html5.iter().map(|o| o.id.as_str()))
            .chain(self.streams.iter().map(|o| o.id.as_str()))
    }
}

/// A web page playing the picture (browser source, web player…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Html5Config {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// CSS background of the page ("" = transparent).
    #[serde(default)]
    pub background: String,
}

/// A place the channels belong to (a studio, a gallery, a truck…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub id: String,
    pub name: String,
}

/// Another shape of the same program (e.g. 9:16 for social media): same
/// playout, its own size and outputs. The project format closest in aspect
/// ratio is used, so one take drives every rendition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendition {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub outputs: OutputsConfig,
    /// Still picture (`/assets/…`) its outputs show while no project is loaded.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slate: String,
    /// The design shown while no project is loaded (see [`slate_style`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slate_style: Option<SlateStyle>,
}

/// One independent output channel (a studio, a control room…): its own
/// playout clock, video format, baseband outputs and HTML5 output page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Interlaced SDI (1080i50): `frame_rate` counts fields, one image is
    /// rendered a field, and the SDI outputs weave two in a row into a frame.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interlaced: bool,
    /// Show project currently loaded on the channel.
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub outputs: OutputsConfig,
    /// Extra renditions (9:16, 1:1…) driven by the same playout.
    #[serde(default)]
    pub renditions: Vec<Rendition>,
    /// Location id ("" = none).
    #[serde(default)]
    pub location: String,
    /// Still picture (`/assets/…`) the main picture shows while no project is loaded.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub slate: String,
    /// The design the main picture shows while no project is loaded (see [`slate_style`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slate_style: Option<SlateStyle>,
    /// Frames rendered ahead of when they are sent (each on its own thread):
    /// absorbs a slow frame now and then, at the cost of that much delay.
    #[serde(default = "default_render_ahead")]
    pub render_ahead: u32,
    /// What the channel clock follows (none: the Hardware page default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<crate::hardware::ClockReference>,
    /// The live input under the graphics (the source wired after the
    /// program in Output streams): the pictures fed through it are the
    /// graphics keyed over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Pictures ("main", rendition ids) wired straight to the program:
    /// graphics only, no source under them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics_only: Vec<String>,
    /// The source format the user chose to convert live ("1080p25"): the
    /// source node stays quiet while the source keeps that format.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_convert: String,
    /// The live inputs placed as sources in the Video streams canvas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Destinations wired to no picture (their wire was removed): kept,
    /// not started.
    #[serde(default, skip_serializing_if = "OutputsConfig::is_empty")]
    pub unwired: OutputsConfig,
}

fn default_render_ahead() -> u32 {
    2
}

/// The design a picture shows while no project is loaded. Not set (older
/// settings): its still picture when it has one, else the first design.
pub fn slate_style(style: Option<SlateStyle>, image: &str) -> SlateStyle {
    style.unwrap_or(if image.trim().is_empty() {
        SlateStyle::Aurora
    } else {
        SlateStyle::Custom
    })
}

impl ChannelConfig {
    /// (rendition id, width, height, outputs): the main picture ("main")
    /// then the extra renditions.
    pub fn pictures(&self) -> Vec<(&str, u32, u32, &OutputsConfig)> {
        std::iter::once(("main", self.width, self.height, &self.outputs))
            .chain(
                self.renditions
                    .iter()
                    .map(|r| (r.id.as_str(), r.width, r.height, &r.outputs)),
            )
            .collect()
    }

    /// What a picture of the channel shows while no project is loaded
    /// (`None`: no such picture).
    pub fn slate_of(&self, picture: &str) -> Option<Slate> {
        let (width, height, image, style) = if picture == "main" {
            (self.width, self.height, &self.slate, self.slate_style)
        } else {
            let r = self.renditions.iter().find(|r| r.id == picture)?;
            (r.width, r.height, &r.slate, r.slate_style)
        };
        Some(Slate {
            style: slate_style(style, image),
            image: image.trim().to_string(),
            channel: self.name.clone(),
            width,
            height,
            rate: (self.frame_rate.num, self.frame_rate.den),
        })
    }

    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(50, 1),
            interlaced: false,
            background: None,
            graphics_only: Vec::new(),
            source_convert: String::new(),
            sources: Vec::new(),
            project_id: None,
            outputs: OutputsConfig::default(),
            renditions: Vec::new(),
            location: String::new(),
            slate: String::new(),
            slate_style: None,
            render_ahead: default_render_ahead(),
            clock: None,
            unwired: OutputsConfig::default(),
        }
    }

    /// Gives the destinations without one an id.
    pub fn ensure_ids(&mut self) {
        let mut taken = HashSet::new();
        self.outputs.ensure_ids(&mut taken);
        for r in &mut self.renditions {
            r.outputs.ensure_ids(&mut taken);
        }
        self.unwired.ensure_ids(&mut taken);
    }

    /// Every destination list: the pictures', then the unwired ones.
    fn all_outputs_mut(&mut self) -> impl Iterator<Item = &mut OutputsConfig> {
        std::iter::once(&mut self.outputs)
            .chain(self.renditions.iter_mut().map(|r| &mut r.outputs))
            .chain(std::iter::once(&mut self.unwired))
    }
}

/// MOS 2.8 device settings (newsroom systems: Saga, iNews, ENPS, Octopus…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MosConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Our MOS ID, as declared in the newsroom system (e.g. "strata.mos").
    #[serde(default = "default_mos_id")]
    pub mos_id: String,
    /// The newsroom system's ID (informational; any NCS is accepted).
    #[serde(default)]
    pub ncs_id: String,
    /// Profile 0/1 port (objects).
    #[serde(default = "default_lower_port")]
    pub lower_port: u16,
    /// Profile 2+ port (running orders).
    #[serde(default = "default_upper_port")]
    pub upper_port: u16,
    /// Project receiving the running orders ("" = the project of the items).
    #[serde(default)]
    pub project_id: String,
}

fn default_mos_id() -> String {
    "strata.mos".into()
}
fn default_lower_port() -> u16 {
    10540
}
fn default_upper_port() -> u16 {
    10541
}

impl Default for MosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mos_id: default_mos_id(),
            ncs_id: String::new(),
            lower_port: default_lower_port(),
            upper_port: default_upper_port(),
            project_id: String::new(),
        }
    }
}

/// OSC (UDP) and TCP text control of the playout (`crate::apiv1`): no
/// login on these, so they are off until enabled and can be limited to
/// some machines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlConfig {
    #[serde(default)]
    pub osc_enabled: bool,
    #[serde(default = "default_osc_port")]
    pub osc_port: u16,
    #[serde(default)]
    pub tcp_enabled: bool,
    #[serde(default = "default_tcp_port")]
    pub tcp_port: u16,
    /// Addresses (or networks `10.0.0.0/24`) allowed to send commands;
    /// empty = any.
    #[serde(default)]
    pub allow: Vec<String>,
}

fn default_osc_port() -> u16 {
    9000
}
fn default_tcp_port() -> u16 {
    9001
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            osc_enabled: false,
            osc_port: default_osc_port(),
            tcp_enabled: false,
            tcp_port: default_tcp_port(),
            allow: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Friendly name shown in the UI and the tray app.
    #[serde(default = "default_name")]
    pub name: String,
    /// Web UI / API listening address. `None` = installer / CLI default.
    #[serde(default)]
    pub bind: Option<SocketAddr>,
    /// Let HTML5 output pages (OBS / vMix / CasparCG browser sources) play
    /// without logging in.
    #[serde(default = "yes")]
    pub public_outputs: bool,
    /// Default IANA time zone of clocks ("" = the server's zone).
    #[serde(default)]
    pub timezone: String,
    /// How the machine keeps its time (NTP), set from Settings › Time.
    #[serde(default)]
    pub time: crate::timectl::TimeConfig,
    /// Path of the NDI runtime library ("" = search the usual places).
    #[serde(default)]
    pub ndi_library: String,
    /// Places the channels are sorted by.
    #[serde(default)]
    pub locations: Vec<Location>,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
    #[serde(default)]
    pub mos: MosConfig,
    /// Live video inputs (streams, NDI, SDI) shown by the Video widget.
    #[serde(default)]
    pub inputs: Vec<crate::inputs::InputConfig>,
    /// DeckLink cards and ST 2110 flows (Hardware page).
    #[serde(default)]
    pub hardware: HardwareConfig,
    /// Control surfaces without HTTP (Bitfocus Buttons over OSC or TCP).
    #[serde(default)]
    pub control: ControlConfig,
    /// Pre-0.2 single-channel outputs, moved into the first channel on load.
    #[serde(default, skip_serializing)]
    outputs: Option<OutputsConfig>,
}

fn default_name() -> String {
    "Strata".into()
}

fn yes() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            bind: None,
            public_outputs: true,
            timezone: String::new(),
            time: Default::default(),
            ndi_library: String::new(),
            locations: Vec::new(),
            channels: Vec::new(),
            mos: MosConfig::default(),
            inputs: Vec::new(),
            hardware: HardwareConfig::default(),
            control: ControlConfig::default(),
            outputs: None,
        }
    }
}

const FILE: &str = "config.json";

impl ServerConfig {
    /// Loads `config.json` (migrating older layouts). The returned config
    /// always has at least one channel.
    pub fn load(data_dir: &Path) -> Self {
        let mut c = match std::fs::read(data_dir.join(FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::error!("invalid {FILE} ({e}); using defaults");
                Self::default()
            }),
            Err(_) => {
                let mut c = Self::default();
                if let Ok(bytes) = std::fs::read(data_dir.join("outputs.json")) {
                    match serde_json::from_slice::<OutputsConfig>(&bytes) {
                        Ok(o) => c.outputs = Some(o),
                        Err(e) => tracing::error!("invalid outputs.json: {e}"),
                    }
                }
                c
            }
        };
        // The product was called Pulse CG: a server never renamed takes the new name.
        if c.name == "Pulse CG" {
            c.name = default_name();
        }
        let legacy = c.outputs.take();
        if c.channels.is_empty() {
            let mut ch = ChannelConfig::new("main", "Channel 1");
            ch.outputs = legacy.unwrap_or_default();
            c.channels.push(ch);
            let _ = c.save(data_dir);
        } else if let Some(o) = legacy {
            c.channels[0].outputs = o;
            let _ = c.save(data_dir);
        }
        let before = c.clone();
        c.normalize();
        if c != before {
            let _ = c.save(data_dir);
        }
        c
    }

    /// Ids for every destination, and a location for the channels of a
    /// config made before locations existed.
    pub fn normalize(&mut self) {
        self.hardware.sync.migrate();
        self.time.normalize();
        if self.locations.is_empty() {
            // A config from before the outputs canvas: its channels already
            // had their web page, which is now a destination like the others.
            for ch in &mut self.channels {
                if ch.outputs.html5.is_empty() {
                    ch.outputs.html5.push(Html5Config {
                        id: String::new(),
                        name: "Web page".into(),
                        background: String::new(),
                    });
                }
            }
        }
        for ch in &mut self.channels {
            ch.ensure_ids();
        }
        // Inputs: `input-1`, `input-2`…, unique.
        let mut taken = HashSet::new();
        for i in &mut self.inputs {
            if i.id.is_empty() || !taken.insert(i.id.clone()) {
                let mut n = 1;
                while taken.contains(&format!("input-{n}")) {
                    n += 1;
                }
                i.id = format!("input-{n}");
                taken.insert(i.id.clone());
            }
        }
        self.link_hardware();
        if self.locations.is_empty() {
            self.locations.push(Location {
                id: "main".into(),
                name: "Principal".into(),
            });
        }
        // The default site of older versions had an English name.
        for l in &mut self.locations {
            if l.id == "main" && l.name == "Main" {
                l.name = "Principal".into();
            }
        }
        let known: HashSet<String> = self.locations.iter().map(|l| l.id.clone()).collect();
        let first = self.locations[0].id.clone();
        for ch in &mut self.channels {
            if !known.contains(&ch.location) {
                ch.location = first.clone();
            }
        }
    }

    /// SDI and ST 2110 destinations and inputs take their settings from
    /// the Hardware page. Those saved before it declare their card, output
    /// or flow there from their own settings, so nothing is lost.
    fn link_hardware(&mut self) {
        use crate::inputs::InputKind;
        let hw = &mut self.hardware;
        hw.ensure_ids();
        for ch in &mut self.channels {
            for outputs in ch.all_outputs_mut() {
                for o in &mut outputs.sdi {
                    if o.hardware.is_empty() {
                        hw.adopt_sdi(o);
                    }
                    hw.apply_sdi(o);
                }
                for o in &mut outputs.st2110 {
                    if o.hardware.is_empty() {
                        hw.adopt_st2110(o);
                    }
                    hw.apply_st2110(o);
                }
            }
        }
        for i in &mut self.inputs {
            match i.kind {
                InputKind::Sdi => {
                    if i.hardware.is_empty() {
                        hw.adopt_sdi_input(i);
                    }
                    let def = hw.sdi.inputs.iter().find(|d| d.id == i.hardware);
                    if let Some((def, card)) = def.and_then(|d| Some((d, hw.card(&d.card)?))) {
                        i.device = card.device_of(def.connector);
                    }
                }
                InputKind::St2110 => {}
                _ => i.hardware.clear(),
            }
        }
    }

    /// Destinations and inputs name things the Hardware page declares, each
    /// used once.
    fn validate_hardware(&self) -> Result<(), String> {
        use crate::inputs::InputKind;
        let hw = &self.hardware;
        hw.validate()?;
        let mut used = HashSet::new();
        for ch in &self.channels {
            let wired: Vec<_> = ch.pictures().into_iter().map(|p| p.3).collect();
            for (outputs, running) in wired
                .into_iter()
                .map(|o| (o, true))
                .chain(std::iter::once((&ch.unwired, false)))
            {
                for o in &outputs.sdi {
                    if !hw.sdi.outputs.iter().any(|d| d.id == o.hardware) {
                        return Err(format!(
                            "output `{}`: choose one of the SDI outputs defined in Hardware",
                            o.name
                        ));
                    }
                    if running && o.enabled && !used.insert(o.hardware.as_str()) {
                        return Err(format!(
                            "an SDI output of Hardware is used by two destinations (`{}`)",
                            o.name
                        ));
                    }
                }
                for o in &outputs.st2110 {
                    let ok = hw
                        .st2110
                        .flows
                        .iter()
                        .any(|f| f.id == o.hardware && f.direction == FlowDirection::Output);
                    if !ok {
                        return Err(format!(
                            "output `{}`: choose one of the ST 2110 output flows defined in Hardware",
                            o.name
                        ));
                    }
                    if running && o.enabled && !used.insert(o.hardware.as_str()) {
                        return Err(format!(
                            "an ST 2110 flow of Hardware is used by two destinations (`{}`)",
                            o.name
                        ));
                    }
                }
            }
        }
        for i in &self.inputs {
            let ok = match i.kind {
                InputKind::Sdi => hw.sdi.inputs.iter().any(|d| d.id == i.hardware),
                InputKind::St2110 => hw
                    .st2110
                    .flows
                    .iter()
                    .any(|f| f.id == i.hardware && f.direction == FlowDirection::Input),
                _ => true,
            };
            if !ok {
                return Err(format!(
                    "input `{}`: choose one of the inputs defined in Hardware",
                    i.name
                ));
            }
        }
        Ok(())
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let tmp = data_dir.join("config.json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, data_dir.join(FILE))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the server name cannot be empty".into());
        }
        if self.bind.is_some_and(|b| b.port() == 0) {
            return Err("port 0 is not allowed".into());
        }
        self.time.validate()?;
        if !self.timezone.trim().is_empty()
            && jiff::tz::TimeZone::get(self.timezone.trim()).is_err()
        {
            return Err(format!("unknown time zone `{}`", self.timezone));
        }
        if self.channels.is_empty() {
            return Err("at least one channel is required".into());
        }
        if self.mos.enabled {
            if self.mos.mos_id.trim().is_empty() {
                return Err("MOS: the MOS ID cannot be empty".into());
            }
            if self.mos.lower_port == 0
                || self.mos.upper_port == 0
                || self.mos.lower_port == self.mos.upper_port
            {
                return Err("MOS: two different ports are needed".into());
            }
        }
        let mut loc_ids = HashSet::new();
        for l in &self.locations {
            if l.id.trim().is_empty() || !loc_ids.insert(l.id.as_str()) {
                return Err("location ids must be unique".into());
            }
            if l.name.trim().is_empty() {
                return Err("every location needs a name".into());
            }
        }
        self.validate_hardware()?;
        let mut ndi_names = HashSet::new();
        let mut ids = HashSet::new();
        for ch in &self.channels {
            let valid_id = !ch.id.is_empty()
                && ch
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid_id || !ids.insert(ch.id.as_str()) {
                return Err(format!(
                    "channel ids must be unique and use letters, digits, - or _ (`{}`)",
                    ch.id
                ));
            }
            if ch.name.trim().is_empty() {
                return Err("every channel needs a name".into());
            }
            if !(16..=7680).contains(&ch.width) || !(16..=4320).contains(&ch.height) {
                return Err(format!("channel `{}`: invalid resolution", ch.name));
            }
            if ch.frame_rate.num == 0 || ch.frame_rate.den == 0 || ch.frame_rate.as_f64() > 120.0 {
                return Err(format!("channel `{}`: invalid frame rate", ch.name));
            }
            let mut rendition_ids = HashSet::from(["main"]);
            for r in &ch.renditions {
                let ok = !r.id.is_empty()
                    && r.id
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
                if !ok || !rendition_ids.insert(r.id.as_str()) {
                    return Err(format!(
                        "channel `{}`: rendition ids must be unique (letters, digits, - or _)",
                        ch.name
                    ));
                }
                if !(16..=7680).contains(&r.width) || !(16..=7680).contains(&r.height) {
                    return Err(format!("rendition `{}`: invalid resolution", r.name));
                }
            }
            let mut out_ids: HashSet<&str> = ch.unwired.ids().collect();
            for (_, _, _, outputs) in ch.pictures() {
                for id in outputs.ids() {
                    if !out_ids.insert(id) {
                        return Err(format!("channel `{}`: output ids must be unique", ch.name));
                    }
                }
                for o in &outputs.ndi {
                    if o.name.trim().is_empty() || !ndi_names.insert(o.name.trim().to_lowercase()) {
                        return Err(format!(
                            "NDI source names must be unique and non-empty (`{}`)",
                            o.name
                        ));
                    }
                }
            }
            let mut names = HashSet::new();
            for (_, _, _, outputs) in ch.pictures() {
                for o in &outputs.st2110 {
                    if o.name.trim().is_empty() || !names.insert(o.name.as_str()) {
                        return Err(format!(
                            "channel `{}`: output names must be unique and non-empty",
                            ch.name
                        ));
                    }
                    if o.fill.port() == 0 || o.key.is_some_and(|k| k.port() == 0) {
                        return Err(format!(
                            "output `{}`: a destination port is missing",
                            o.name
                        ));
                    }
                    if o.encoding == cg_output::st2110::St2110Encoding::FillKey
                        && Some(o.fill) == o.key
                    {
                        return Err(format!(
                            "output `{}`: fill and key must use different destinations",
                            o.name
                        ));
                    }
                }
                for o in &outputs.sdi {
                    if o.name.trim().is_empty() || !names.insert(o.name.as_str()) {
                        return Err(format!(
                            "channel `{}`: output names must be unique and non-empty",
                            ch.name
                        ));
                    }
                }
            }
        }
        // The same multicast destination on two channels is always a mistake.
        let mut dests = HashSet::new();
        for o in self
            .channels
            .iter()
            .flat_map(|c| c.pictures())
            .flat_map(|(_, _, _, outputs)| &outputs.st2110)
        {
            for d in o
                .streams()
                .into_iter()
                .flat_map(|s| [Some(s.red), s.blue])
                .flatten()
            {
                if !dests.insert(d) {
                    return Err(format!("destination {d} is used by two outputs"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cg-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn migrates_outputs_json_into_first_channel() {
        let dir = tmp("outputs");
        std::fs::write(
            dir.join("outputs.json"),
            r#"{"st2110":[{"name":"CG1","fill":"239.1.1.1:5004"}]}"#,
        )
        .unwrap();
        let c = ServerConfig::load(&dir);
        assert_eq!(c.channels.len(), 1);
        assert_eq!(c.channels[0].outputs.st2110[0].name, "CG1");
        assert_eq!(ServerConfig::load(&dir), c);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn migrates_v01_config() {
        let dir = tmp("v01");
        std::fs::write(dir.join(FILE), r#"{"name":"Régie","outputs":{"st2110":[{"name":"A","fill":"239.1.1.1:5004"}],"sdi":[]}}"#).unwrap();
        let c = ServerConfig::load(&dir);
        assert_eq!(c.name, "Régie");
        assert_eq!(c.channels[0].outputs.st2110[0].name, "A");
        let raw = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(
            !raw.contains("\"outputs\":{\"st2110\":[{\"name\":\"A\"") || raw.contains("channels")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_shared_destinations() {
        let mut c = ServerConfig::default();
        let o: St2110Config =
            serde_json::from_str(r#"{"name":"A","fill":"239.1.1.1:5004"}"#).unwrap();
        for id in ["a", "b"] {
            let mut ch = ChannelConfig::new(id, id);
            ch.outputs.st2110.push(o.clone());
            c.channels.push(ch);
        }
        c.normalize();
        assert!(c.validate().unwrap_err().contains("used by two"));
    }

    #[test]
    fn slates_default_to_the_first_design_or_the_picture_set() {
        let ch: ChannelConfig = serde_json::from_str(
            r#"{"id":"a","name":"Studio","width":1920,"height":1080,"frameRate":{"num":25,"den":1},
            "renditions":[{"id":"v","name":"9:16","width":1080,"height":1920,"slate":"/assets/v.png"},
            {"id":"s","name":"1:1","width":1080,"height":1080,"slate":"/assets/s.png","slateStyle":"lines"}]}"#,
        )
        .unwrap();
        let main = ch.slate_of("main").unwrap();
        assert_eq!(main.style, SlateStyle::Aurora);
        assert_eq!(
            (main.channel.as_str(), main.width, main.rate),
            ("Studio", 1920, (25, 1))
        );
        let v = ch.slate_of("v").unwrap();
        assert_eq!(
            (v.style, v.image.as_str(), v.height),
            (SlateStyle::Custom, "/assets/v.png", 1920)
        );
        assert_eq!(ch.slate_of("s").unwrap().style, SlateStyle::Lines);
        assert!(ch.slate_of("nope").is_none());
        // Saved as read: an unset design stays unset.
        let json = serde_json::to_value(&ch).unwrap();
        assert!(json.get("slateStyle").is_none());
        assert_eq!(json["renditions"][1]["slateStyle"], "lines");
    }
}
