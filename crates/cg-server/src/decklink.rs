//! DeckLink cards as the machine has them (Hardware › SDI): the devices
//! Desktop Video lists, grouped into physical cards, with the layout of
//! their bracket (the per-model table below) and the Desktop Video device
//! behind each SDI connector.
//!
//! `STRATA_FAKE_DECKLINK=quad2,8kpro` (also `duo2`, `minimonitor`) makes up
//! cards instead of asking the driver, to work on the page without them.

use serde::Serialize;

use cg_output::decklink::DeviceInfo;

/// The connectors of a bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Jack {
    /// DIN 1.0/2.3 ("micro-BNC"), gold, ≈7 mm nut.
    Din,
    /// Full-size BNC, chrome, ≈12.7 mm nut.
    Bnc,
}

/// What an SDI connector can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Port {
    In,
    Out,
    Both,
}

/// What a card can key (DeckLink SDK, `IDeckLinkKeyer`): a keyer works on
/// a pair of connectors in full duplex: the external keyer sends the fill
/// on the first and the key on the second; the internal keyer keys over
/// the video coming into the first and sends the result on the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Keying {
    pub internal: bool,
    pub external: bool,
    /// The connector pairs (numbers as printed, the first one first).
    pub pairs: &'static [[u32; 2]],
    /// One Desktop Video profile for the whole card (8K Pro); else each
    /// pair has its own (Duo 2, Quad 2: half or full duplex per pair).
    pub card_profile: bool,
}

const NO_KEYING: Keying = Keying {
    internal: false,
    external: false,
    pairs: &[],
    card_profile: false,
};

/// A DeckLink model, seen from the back of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// As `IDeckLink::GetModelName` reports it (compared ignoring case).
    pub name: &'static str,
    /// The model id of the cards declared by hand before detection.
    pub legacy: &'static str,
    pub jack: Jack,
    /// SDI connectors (REF not counted).
    pub connectors: u32,
    /// The connector numbers from the top of the vertical bracket to the
    /// bottom (the REF, when there is one, comes last, next to the
    /// motherboard); empty: not known, in numeric order.
    pub order: &'static [u32],
    /// Has a reference input; None: as the driver says.
    pub reference: Option<bool>,
    /// What each connector can do; empty: inputs and outputs.
    pub ports: &'static [Port],
    pub keying: Keying,
}

/// The models whose bracket is known. To add one or fix an order, edit
/// its entry (connector numbers as printed on the bracket, top to bottom).
pub const MODELS: &[Model] = &[
    // Nine gold DIN connectors, from a photo of the user's machine.
    Model {
        name: "DeckLink Quad 2",
        legacy: "quad2",
        jack: Jack::Din,
        connectors: 8,
        order: &[8, 4, 7, 3, 6, 2, 5, 1],
        reference: Some(true),
        ports: &[],
        // Keying on each pair, HD at most.
        keying: Keying {
            internal: true,
            external: true,
            pairs: &[[1, 2], [3, 4], [5, 6], [7, 8]],
            card_profile: false,
        },
    },
    // Five chrome BNC (4 × 12G-SDI + REF), from the same photo. Most likely
    // the 8K Pro (forums report its 1-3-2-4 order) but not confirmed: fix
    // the order here when the card says otherwise.
    Model {
        name: "DeckLink 8K Pro",
        legacy: "8kPro",
        jack: Jack::Bnc,
        connectors: 4,
        order: &[4, 2, 3, 1],
        reference: Some(true),
        ports: &[],
        // Four sub-devices in half duplex (each connector an input or an
        // output), or two in full duplex (a pair each: keying).
        keying: Keying {
            internal: true,
            external: true,
            pairs: &[[1, 2], [3, 4]],
            card_profile: true,
        },
    },
    // The following brackets are not documented in the SDK: connectors in
    // numeric order, the REF only when the driver reports one.
    Model {
        name: "DeckLink Duo 2",
        legacy: "duo2",
        jack: Jack::Din,
        connectors: 4,
        order: &[],
        reference: None,
        ports: &[],
        keying: Keying {
            internal: true,
            external: true,
            pairs: &[[1, 2], [3, 4]],
            card_profile: false,
        },
    },
    Model {
        name: "DeckLink SDI 4K",
        legacy: "sdi4k",
        jack: Jack::Bnc,
        connectors: 2,
        order: &[],
        reference: None,
        ports: &[Port::In, Port::Out],
        // Over its input, out of its output.
        keying: Keying {
            internal: true,
            external: false,
            pairs: &[[1, 2]],
            card_profile: false,
        },
    },
    Model {
        name: "DeckLink Mini Monitor",
        legacy: "miniMonitor",
        jack: Jack::Bnc,
        connectors: 1,
        order: &[],
        reference: None,
        ports: &[Port::Out],
        keying: NO_KEYING,
    },
    Model {
        name: "DeckLink Mini Monitor 4K",
        legacy: "miniMonitor",
        jack: Jack::Bnc,
        connectors: 1,
        order: &[],
        reference: None,
        ports: &[Port::Out],
        keying: NO_KEYING,
    },
    Model {
        name: "DeckLink Mini Recorder",
        legacy: "miniRecorder",
        jack: Jack::Bnc,
        connectors: 1,
        order: &[],
        reference: None,
        ports: &[Port::In],
        keying: NO_KEYING,
    },
    Model {
        name: "DeckLink Mini Recorder 4K",
        legacy: "miniRecorder",
        jack: Jack::Bnc,
        connectors: 1,
        order: &[],
        reference: None,
        ports: &[Port::In],
        keying: NO_KEYING,
    },
];

/// The table entry of a model name.
pub fn model(name: &str) -> Option<&'static Model> {
    let name = name.trim();
    MODELS.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// A physical card: the devices Desktop Video lists for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCard {
    /// What identifies the card across reboots (hex of its device group
    /// id, else of its persistent id; `dev-N` when the driver gives none).
    pub id: String,
    pub model_name: String,
    /// The model id of hand-declared cards (`other` when not in the table).
    pub legacy_model: String,
    /// SDI connectors (REF not counted).
    pub connectors: u32,
    /// The Desktop Video device of each connector (index = connector − 1).
    pub devices: Vec<u32>,
    /// What each connector can do.
    pub ports: Vec<Port>,
    pub has_reference: bool,
    pub reference_locked: Option<bool>,
    /// `blackBurst` or `triLevel`, when a signal is there.
    pub reference_kind: Option<&'static str>,
    /// Its format ("625i50", "1080i50").
    pub reference_format: Option<String>,
    /// `pcie`, `usb`, `thunderbolt`.
    pub interface: Option<&'static str>,
    /// Where the card sits (hex).
    pub topological_id: Option<String>,
    /// Of its first sub-device (hex).
    pub persistent_id: Option<String>,
    /// The active profile and those it can switch to (fourcc).
    pub profile: Option<String>,
    pub profiles: Vec<String>,
    /// The active profile of the device of each connector.
    #[serde(skip)]
    pub connector_profiles: Vec<Option<String>>,
    /// The driver says the card has an internal / external keyer.
    pub internal_keyer: bool,
    pub external_keyer: bool,
}

fn fourcc(v: u32) -> String {
    String::from_utf8_lossy(&v.to_be_bytes()).into_owned()
}

fn from_fourcc(s: &str) -> Option<u32> {
    let b: [u8; 4] = s.as_bytes().try_into().ok()?;
    Some(u32::from_be_bytes(b))
}

fn hex(v: i64) -> String {
    format!("{:X}", v as u64)
}

/// The reference signal of a `BMDDisplayMode`: black burst (SD) or
/// tri-level (HD), and its format.
fn reference_of(mode: u32) -> (&'static str, String) {
    let name = match &mode.to_be_bytes() {
        b"ntsc" | b"nt23" | b"ntsp" => "525i59.94",
        b"pal " | b"palp" => "625i50",
        b"Hi50" => "1080i50",
        b"Hi59" => "1080i59.94",
        b"Hi60" => "1080i60",
        b"Hp23" => "1080p23.98",
        b"Hp24" => "1080p24",
        b"Hp25" => "1080p25",
        b"Hp29" => "1080p29.97",
        b"Hp30" => "1080p30",
        b"Hp50" => "1080p50",
        b"Hp59" => "1080p59.94",
        b"Hp60" => "1080p60",
        b"hp50" => "720p50",
        b"hp59" => "720p59.94",
        b"hp60" => "720p60",
        _ => "",
    };
    let sd = name.starts_with("525") || name.starts_with("625");
    let name = if name.is_empty() {
        fourcc(mode).trim().to_string()
    } else {
        name.to_string()
    };
    (if sd { "blackBurst" } else { "triLevel" }, name)
}

/// The connector a device starts at (1 = SDI 1). Desktop Video names the
/// sub-devices of a card after it ("DeckLink Quad (5)") and lists them in
/// its own order (1, 5, 2, 6… on a Quad 2): the number in the name wins,
/// else the sub-device index + 1.
fn connector_of(d: &DeviceInfo, connectors: u32) -> Option<u32> {
    let named = d
        .display_name
        .trim_end()
        .strip_suffix(')')
        .and_then(|s| s.rsplit_once('('))
        .and_then(|(_, n)| n.trim().parse::<u32>().ok());
    let sub = d
        .sub_device_index
        .and_then(|i| u32::try_from(i).ok())
        .map(|i| i + 1);
    named
        .filter(|n| (1..=connectors).contains(n))
        .or(sub.filter(|n| (1..=connectors).contains(n)))
}

/// Devices grouped into cards: by device group id (shared by the
/// sub-devices of a card), else by persistent id, else one card each.
pub fn group(devices: &[DeviceInfo]) -> Vec<DetectedCard> {
    let key = |d: &DeviceInfo| {
        d.group_id
            .or(d.persistent_id)
            .map(hex)
            .unwrap_or_else(|| format!("dev-{}", d.index))
    };
    let mut keys: Vec<String> = Vec::new();
    for d in devices {
        let k = key(d);
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys.into_iter()
        .map(|k| {
            let mut devs: Vec<&DeviceInfo> = devices.iter().filter(|d| key(d) == k).collect();
            devs.sort_by_key(|d| (d.sub_device_index.unwrap_or(i64::MAX), d.index));
            card(k, &devs)
        })
        .collect()
}

fn card(id: String, devs: &[&DeviceInfo]) -> DetectedCard {
    let first = devs[0];
    let m = model(&first.model_name);
    let count = devs.len() as u32;
    let subs = first.sub_devices.and_then(|n| u32::try_from(n).ok());
    let connectors = m
        .map(|m| m.connectors)
        .unwrap_or_else(|| subs.unwrap_or(count).max(count).max(1));
    // The device of each connector; a single device drives them all.
    let mut slots: Vec<Option<u32>> = vec![None; connectors as usize];
    if count == 1 {
        slots.fill(Some(first.index));
    } else {
        let mut left = Vec::new();
        for d in devs {
            match connector_of(d, connectors) {
                Some(n) if slots[n as usize - 1].is_none() => slots[n as usize - 1] = Some(d.index),
                _ => left.push(d.index),
            }
        }
        for s in slots.iter_mut().filter(|s| s.is_none()) {
            if left.is_empty() {
                break;
            }
            *s = Some(left.remove(0));
        }
    }
    // Connectors no device drives in the active profile (a full-duplex
    // pair): the card's first device.
    let devices: Vec<u32> = slots
        .into_iter()
        .map(|s| s.unwrap_or(first.index))
        .collect();
    let connector_profiles = devices
        .iter()
        .map(|i| {
            devs.iter()
                .find(|d| d.index == *i)
                .and_then(|d| d.profile)
                .map(fourcc)
        })
        .collect();
    let ports = (0..connectors as usize)
        .map(|i| match m.and_then(|m| m.ports.get(i)) {
            Some(p) => *p,
            None if !first.caps.input && first.caps.output => Port::Out,
            None if first.caps.input && !first.caps.output => Port::In,
            None => Port::Both,
        })
        .collect();
    let has_reference = m
        .and_then(|m| m.reference)
        .unwrap_or_else(|| devs.iter().any(|d| d.has_reference_input));
    let reference_locked = devs.iter().find_map(|d| d.reference_locked);
    let reference = devs
        .iter()
        .find_map(|d| d.reference_mode)
        .filter(|_| reference_locked == Some(true))
        .map(reference_of);
    DetectedCard {
        id,
        model_name: if first.model_name.is_empty() {
            first.display_name.clone()
        } else {
            first.model_name.clone()
        },
        legacy_model: m.map_or("other", |m| m.legacy).to_string(),
        connectors,
        devices,
        ports,
        has_reference,
        reference_locked: reference_locked.filter(|_| has_reference),
        reference_kind: reference.as_ref().map(|r| r.0),
        reference_format: reference.map(|r| r.1),
        interface: first.interface.map(|i| match &i.to_be_bytes() {
            b"usb " => "usb",
            b"thun" => "thunderbolt",
            _ => "pcie",
        }),
        topological_id: first.topological_id.map(hex),
        persistent_id: first.persistent_id.map(hex),
        profile: first.profile.map(fourcc),
        profiles: first.profiles.iter().map(|p| fourcc(*p)).collect(),
        connector_profiles,
        internal_keyer: devs.iter().any(|d| d.caps.internal_keyer),
        external_keyer: devs.iter().any(|d| d.caps.external_keyer),
    }
}

/// The models asked for in `STRATA_FAKE_DECKLINK`, if set.
fn fake_models() -> Option<Vec<String>> {
    let v = std::env::var("STRATA_FAKE_DECKLINK").ok()?;
    let v: Vec<String> = v
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    (!v.is_empty()).then_some(v)
}

/// A made-up card: model, display name, Desktop Video order of its
/// connectors, group id, reference (locked, signal), profiles.
type Fake = (
    &'static str,
    &'static str,
    &'static [u32],
    i64,
    Option<(bool, &'static [u8; 4])>,
    &'static [&'static str],
);

/// Made-up devices, as a driver would list them.
fn fake_devices(models: &[String]) -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    let caps = cg_output::decklink::Caps {
        output: true,
        input: true,
        internal_keyer: true,
        external_keyer: true,
        format_detection: true,
    };
    for (slot, m) in models.iter().enumerate() {
        // Desktop Video order of the sub-devices (connector numbers).
        let (model, short, order, group, reference, profiles): Fake = match m.as_str() {
            "quad2" => (
                "DeckLink Quad 2",
                "DeckLink Quad",
                &[1, 5, 2, 6, 3, 7, 4, 8],
                0x7B10_A04E,
                Some((true, b"pal ")),
                &["1dfd", "2dhd"],
            ),
            "8kpro" => (
                "DeckLink 8K Pro",
                "DeckLink 8K Pro",
                &[1, 3, 2, 4],
                0x6A3F_291C,
                Some((false, b"\0\0\0\0")),
                &["1dfd", "1dhd", "2dfd", "4dhd"],
            ),
            "duo2" => (
                "DeckLink Duo 2",
                "DeckLink Duo",
                &[1, 2, 3, 4],
                0x3C21_0B7A,
                None,
                &["1dfd", "2dhd"],
            ),
            "minimonitor" => (
                "DeckLink Mini Monitor 4K",
                "DeckLink Mini Monitor 4K",
                &[1],
                0x51E0_77D2,
                None,
                &[],
            ),
            _ => continue,
        };
        let n = order.len();
        for c in order {
            let mode = reference
                .map(|r| u32::from_be_bytes(*r.1))
                .filter(|m| *m != 0);
            out.push(DeviceInfo {
                index: out.len() as u32,
                display_name: if n > 1 {
                    format!("{short} ({c})")
                } else {
                    short.to_string()
                },
                model_name: model.into(),
                persistent_id: Some(group * 16 + i64::from(*c)),
                group_id: Some(group),
                topological_id: Some(((slot as i64 + 2) << 16) + i64::from(*c) - 1),
                sub_device_index: Some(i64::from(*c) - 1),
                sub_devices: Some(n as i64),
                has_reference_input: reference.is_some(),
                reference_locked: reference.map(|r| r.0),
                reference_mode: mode,
                interface: from_fourcc("pci "),
                profile: profiles.last().and_then(|p| from_fourcc(p)),
                profiles: profiles.iter().filter_map(|p| from_fourcc(p)).collect(),
                caps: if model.contains("Monitor") {
                    cg_output::decklink::Caps {
                        input: false,
                        format_detection: false,
                        ..caps
                    }
                } else {
                    caps
                },
            });
        }
    }
    out
}

/// What the driver lists (or the fake cards), and the Desktop Video
/// version. A missing runtime is an empty list, not an error.
pub fn detect() -> (Vec<DetectedCard>, Option<String>, bool) {
    if let Some(models) = fake_models() {
        return (group(&fake_devices(&models)), Some("14.2".into()), true);
    }
    let devices = cg_output::decklink::device_infos().unwrap_or_default();
    let version = if devices.is_empty() {
        None
    } else {
        cg_output::decklink::runtime_version()
    };
    (group(&devices), version, false)
}

/// The connector pairs of a card (0-based): from the model table, else
/// 1+2, 3+4… when the driver says the card keys.
pub fn pairs_of(d: &DetectedCard) -> Vec<(u32, u32)> {
    match model(&d.model_name) {
        Some(m) => m
            .keying
            .pairs
            .iter()
            .map(|p| (p[0] - 1, p[1] - 1))
            .collect(),
        None if d.internal_keyer || d.external_keyer => {
            (0..d.connectors / 2).map(|i| (2 * i, 2 * i + 1)).collect()
        }
        None => Vec::new(),
    }
}

/// The Desktop Video profiles the roles of an adopted card need: (device,
/// profile) to activate. A keyer (internal or external) needs its pair in
/// full duplex: one sub-device for both connectors (`1dfd` on a Duo 2 or a
/// Quad 2, whose pairs each have a profile; `2dfd` for the whole 8K Pro);
/// else half duplex, each connector its own device (`2dhd` / `4dhd`).
pub fn wanted_profiles(
    hw: &crate::hardware::HardwareConfig,
    card: &crate::hardware::SdiCard,
    d: &DetectedCard,
) -> Vec<(u32, &'static str)> {
    use crate::hardware::SdiMode;
    let keyed: Vec<u32> = hw
        .sdi
        .outputs
        .iter()
        .filter(|o| o.card == card.id && o.mode != SdiMode::FillOnly)
        .map(|o| o.fill)
        .collect();
    let pairs = pairs_of(d);
    let has = |p: &str| d.profiles.iter().any(|x| x == p);
    let card_profile = model(&d.model_name).is_some_and(|m| m.keying.card_profile);
    let mut out = Vec::new();
    if card_profile {
        let full = pairs.iter().any(|(a, _)| keyed.contains(a));
        let wanted = if full {
            ["2dfd", "1dfd"].into_iter().find(|p| has(p))
        } else {
            ["4dhd", "2dhd"].into_iter().find(|p| has(p))
        };
        if let (Some(p), Some(dev)) = (wanted, d.devices.first()) {
            if d.profile.as_deref() != Some(p) {
                out.push((*dev, p));
            }
        }
        return out;
    }
    for (a, _) in pairs {
        let full = keyed.contains(&a);
        let wanted = if full {
            ["1dfd", "2dfd"].into_iter().find(|p| has(p))
        } else {
            ["2dhd", "4dhd"].into_iter().find(|p| has(p))
        };
        let (Some(p), Some(dev)) = (wanted, d.devices.get(a as usize)) else {
            continue;
        };
        let now = d.connector_profiles.get(a as usize).cloned().flatten();
        if now.as_deref() != Some(p) {
            out.push((*dev, p));
        }
    }
    out
}

/// Switches the adopted cards to the profiles their connectors' roles need
/// (a card in use by another application refuses: it is said, the engine
/// goes on). True when a profile changed (the devices are listed again then).
pub fn apply_profiles(hw: &crate::hardware::HardwareConfig, cards: &[DetectedCard]) -> bool {
    let mut changed = false;
    for c in &hw.sdi.cards {
        let Some(d) = cards.iter().find(|d| d.id == c.persistent_id) else {
            continue;
        };
        for (device, wanted) in wanted_profiles(hw, c, d) {
            let Some(profile) = from_fourcc(wanted) else {
                continue;
            };
            match cg_output::decklink::set_profile(device, profile) {
                Ok(()) => {
                    tracing::info!("{}: profile {wanted} activated", c.name);
                    changed = true;
                }
                Err(e) => tracing::warn!("{}: cannot activate profile {wanted}: {e}", c.name),
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad2_sub_devices_map_to_their_connectors() {
        let cards = group(&fake_devices(&["quad2".into(), "8kpro".into()]));
        assert_eq!(cards.len(), 2);
        let q = &cards[0];
        assert_eq!(q.model_name, "DeckLink Quad 2");
        assert_eq!(q.legacy_model, "quad2");
        assert_eq!(q.connectors, 8);
        // Listed 1, 5, 2, 6, 3, 7, 4, 8: SDI 5 is the second device.
        assert_eq!(q.devices, vec![0, 2, 4, 6, 1, 3, 5, 7]);
        assert!(q.has_reference);
        assert_eq!(q.reference_kind, Some("blackBurst"));
        assert_eq!(q.reference_format.as_deref(), Some("625i50"));
        let p = &cards[1];
        assert_eq!(p.devices, vec![8, 10, 9, 11]);
        assert_eq!(p.reference_locked, Some(false));
        assert_eq!(p.profile.as_deref(), Some("4dhd"));
        assert_ne!(p.id, q.id);
    }

    /// A keyer puts its pair in full duplex: on a Duo 2 that pair alone
    /// (its own profile), on an 8K Pro the whole card.
    #[test]
    fn keyers_ask_for_full_duplex() {
        let cards = group(&fake_devices(&["duo2".into(), "8kpro".into()]));
        let (duo, pro) = (&cards[0], &cards[1]);
        assert_eq!(pairs_of(duo), vec![(0, 1), (2, 3)]);
        let hw = |card: &str, mode: &str| -> crate::hardware::HardwareConfig {
            serde_json::from_value(serde_json::json!({
                "sdi": {
                    "cards": [{ "id": "c", "name": "C", "model": card, "device": 0,
                                "connectors": [{}, {}, {}, {}] }],
                    "outputs": [{ "id": "o", "name": "CG", "card": "c", "fill": 0, "key": 1,
                                  "mode": mode }]
                }
            }))
            .unwrap()
        };
        let duo_hw = hw("duo2", "externalKey");
        let card = &duo_hw.sdi.cards[0];
        // Fake cards start half duplex: SDI 1 + 2 goes full, SDI 3 + 4 stays.
        assert_eq!(
            wanted_profiles(&duo_hw, card, duo),
            vec![(duo.devices[0], "1dfd")]
        );
        let plain = hw("duo2", "fillOnly");
        assert!(wanted_profiles(&plain, &plain.sdi.cards[0], duo).is_empty());
        let pro_hw = hw("8kPro", "internalKey");
        assert_eq!(
            wanted_profiles(&pro_hw, &pro_hw.sdi.cards[0], pro),
            vec![(pro.devices[0], "2dfd")]
        );
        let plain = hw("8kPro", "fillOnly");
        assert!(wanted_profiles(&plain, &plain.sdi.cards[0], pro).is_empty());
    }

    #[test]
    fn unknown_cards_fall_back_to_what_the_driver_says() {
        let d = DeviceInfo {
            index: 3,
            display_name: "DeckLink Something".into(),
            model_name: "DeckLink Something".into(),
            caps: cg_output::decklink::Caps {
                output: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let c = &group(&[d])[0];
        assert_eq!(c.id, "dev-3");
        assert_eq!(c.legacy_model, "other");
        assert_eq!((c.connectors, c.devices.clone()), (1, vec![3]));
        assert_eq!(c.ports, vec![Port::Out]);
        assert!(!c.has_reference);
    }

    #[test]
    fn the_layout_table_is_consistent() {
        for m in MODELS {
            if !m.order.is_empty() {
                let mut o = m.order.to_vec();
                o.sort();
                assert_eq!(o, (1..=m.connectors).collect::<Vec<_>>(), "{}", m.name);
            }
            assert!(m.ports.is_empty() || m.ports.len() == m.connectors as usize);
        }
        assert_eq!(model("decklink quad 2").map(|m| m.legacy), Some("quad2"));
    }
}
