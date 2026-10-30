//! Map: a stylised offline vector map (no network, ever). The geometry
//! (Natural Earth countries, coasts, borders and lakes; IGN regions and
//! departements of France; labels; a gazetteer; Paris detail) is
//! `map_world.json`, shared with the web renderer. Projections, key views
//! with animated (fly) transitions, « Aller à… » a place or coordinates,
//! markers, great-circle routes drawn from their launch time, coloured
//! zones (regions, departements or countries) from data, labels by
//! priority in the project's language, the attribution of the data drawn.
//! Drawn by `cg-render/src/wid/map.rs` and `web/src/widgets/map.tsx` (keep in sync).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize};

pub use super::table::AutoOn;
use super::ActCtx;
use crate::anim::{interpolate_text, Data};
use crate::model::{Layer, LayerKind};

// ------------------------------------------------------------- geometry --

/// `[lon, lat]` degrees.
pub type Ll = [f64; 2];

/// A zone of the map (a region, a departement or a country): its id
/// (region id, departement code, ISO 3166 alpha-3), names, label point,
/// bounding box [west, south, east, north] and outline.
#[derive(Debug)]
pub struct Zone {
    pub id: String,
    /// French name (and the English one of a country).
    pub name: String,
    pub en: String,
    /// Country: ISO alpha-2; region: INSEE code.
    pub code: String,
    pub lon: f64,
    pub lat: f64,
    pub bbox: [f64; 4],
    pub rings: Vec<Vec<Ll>>,
}

/// A place label: English name, lat, lon, importance 1–10, min zoom, kind
/// (city, poi, country, continent), French name.
#[derive(Debug, Deserialize)]
pub struct Place(
    pub String,
    pub f64,
    pub f64,
    pub f64,
    pub f64,
    pub String,
    pub String,
);

impl Place {
    /// Its name in a language ("fr…" = French).
    pub fn name(&self, lang: &str) -> &str {
        if lang.starts_with("fr") {
            &self.6
        } else {
            &self.0
        }
    }
}

/// A place « Aller à… » finds: name (French), other name (English, may
/// be empty), lat, lon, kind (c city, k capital).
#[derive(Debug, Deserialize)]
pub struct Gaz(pub String, pub String, pub f64, pub f64, pub String);

#[derive(Debug, Deserialize)]
pub struct Paris {
    pub urban: Vec<Ll>,
    pub parks: Vec<Vec<Ll>>,
    pub seine: Vec<Ll>,
    pub canal: Vec<Ll>,
    pub peri: Vec<Ll>,
    pub motorways: Vec<Vec<Ll>>,
    pub streets: Vec<Vec<Ll>>,
    pub stands: Vec<Ll>,
    pub bowl: Vec<Ll>,
    pub pitch: Vec<Ll>,
    pub lines: Vec<Ll>,
    pub halfway: Vec<Ll>,
    pub circle: Vec<Ll>,
}

#[derive(Debug)]
pub struct World {
    pub land: Vec<Vec<Ll>>,
    pub lakes: Vec<Vec<Ll>>,
    pub borders: Vec<Vec<Ll>>,
    pub countries: Vec<Zone>,
    pub regions: Vec<Zone>,
    pub deps: Vec<Zone>,
    pub places: Vec<Place>,
    pub gaz: Vec<Gaz>,
    pub paris: Paris,
}

/// Quantized, delta-encoded arcs ([x0, y0, dx1, dy1, …]) and their transform.
#[derive(Deserialize)]
struct RawArcs {
    tf: [f64; 4],
    arcs: Vec<Vec<i64>>,
}

impl RawArcs {
    fn decode(&self) -> Vec<Vec<Ll>> {
        let [sx, sy, tx, ty] = self.tf;
        self.arcs
            .iter()
            .map(|a| {
                let (mut x, mut y) = (0i64, 0i64);
                a.chunks(2)
                    .map(|p| {
                        x += p[0];
                        y += p[1];
                        [x as f64 * sx + tx, y as f64 * sy + ty]
                    })
                    .collect()
            })
            .collect()
    }
}

/// A ring (or line) of arc indexes: `~i` = arc i reversed; shared ends once.
fn ring(refs: &[i64], arcs: &[Vec<Ll>]) -> Vec<Ll> {
    let mut out: Vec<Ll> = Vec::new();
    for &i in refs {
        let (k, rev) = if i >= 0 {
            (i as usize, false)
        } else {
            ((!i) as usize, true)
        };
        let Some(a) = arcs.get(k) else { continue };
        let skip = usize::from(!out.is_empty());
        if rev {
            out.extend(a.iter().rev().skip(skip));
        } else {
            out.extend(a.iter().skip(skip));
        }
    }
    out
}

#[derive(Deserialize)]
struct RawZone {
    id: String,
    name: String,
    #[serde(default)]
    en: String,
    #[serde(default)]
    iso2: String,
    #[serde(default)]
    code: String,
    lon: f64,
    lat: f64,
    #[serde(rename = "box")]
    bbox: [f64; 4],
    rings: Vec<Vec<i64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorld {
    world: RawArcs,
    fr: RawArcs,
    lake_arcs: RawArcs,
    land: Vec<Vec<i64>>,
    borders: Vec<Vec<i64>>,
    lakes: Vec<Vec<i64>>,
    countries: Vec<RawZone>,
    regions: Vec<RawZone>,
    deps: Vec<RawZone>,
    places: Vec<Place>,
    gaz: Vec<Gaz>,
    paris: Paris,
}

/// The built-in map data (parsed and decoded once).
pub fn world() -> &'static World {
    static W: OnceLock<World> = OnceLock::new();
    W.get_or_init(|| {
        let raw: RawWorld =
            serde_json::from_str(include_str!("map_world.json")).expect("map_world.json");
        let (wa, fa, la) = (raw.world.decode(), raw.fr.decode(), raw.lake_arcs.decode());
        let rings = |list: &[Vec<i64>], arcs: &[Vec<Ll>]| -> Vec<Vec<Ll>> {
            list.iter().map(|r| ring(r, arcs)).collect()
        };
        let zones = |list: Vec<RawZone>, arcs: &[Vec<Ll>]| -> Vec<Zone> {
            list.into_iter()
                .map(|z| Zone {
                    rings: rings(&z.rings, arcs),
                    id: z.id,
                    name: z.name,
                    en: z.en,
                    code: if z.iso2.is_empty() { z.code } else { z.iso2 },
                    lon: z.lon,
                    lat: z.lat,
                    bbox: z.bbox,
                })
                .collect()
        };
        World {
            land: rings(&raw.land, &wa),
            lakes: rings(&raw.lakes, &la),
            borders: rings(&raw.borders, &wa),
            countries: zones(raw.countries, &wa),
            regions: zones(raw.regions, &fa),
            deps: zones(raw.deps, &fa),
            places: raw.places,
            gaz: raw.gaz,
            paris: raw.paris,
        }
    })
}

// ---------------------------------------------------------- projections --

pub const MAXLAT: f64 = 85.0511;
const D2R: f64 = std::f64::consts::PI / 180.0;
const EE: [f64; 4] = [1.340264, -0.081106, 0.000893, 0.003796];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    /// Web Mercator: city and stadium views; repeats across 180°.
    #[default]
    Mercator,
    /// Equal Earth: world maps with true areas.
    #[serde(alias = "equalEarth")]
    Equalearth,
    /// Plate carrée; repeats across 180°.
    Equirect,
}

impl Projection {
    /// The world repeats left and right (a route crosses 180° the short way).
    pub fn wraps(self) -> bool {
        self != Projection::Equalearth
    }
}

fn eex() -> f64 {
    (2.0 * 3f64.sqrt() * std::f64::consts::PI) / (3.0 * EE[0])
}

/// (lon, lat) → (X, Y) in [0, 1] (the whole world is one unit wide).
pub fn project(p: Projection, lon: f64, lat: f64) -> (f64, f64) {
    match p {
        Projection::Equalearth => {
            let l = lon * D2R;
            let th = ((3f64.sqrt() / 2.0) * (lat.clamp(-90.0, 90.0) * D2R).sin()).asin();
            let t2 = th * th;
            let t6 = t2 * t2 * t2;
            let x = (2.0 * 3f64.sqrt() * l * th.cos())
                / (3.0 * (9.0 * EE[3] * t6 * t2 + 7.0 * EE[2] * t6 + 3.0 * EE[1] * t2 + EE[0]));
            let y = th * (EE[0] + EE[1] * t2 + t6 * (EE[2] + EE[3] * t2));
            let e = eex();
            (0.5 + x / (2.0 * e), 0.5 - y / (2.0 * e))
        }
        Projection::Equirect => ((lon + 180.0) / 360.0, 0.5 - lat / 360.0),
        Projection::Mercator => {
            let f = lat.clamp(-MAXLAT, MAXLAT) * D2R;
            (
                (lon + 180.0) / 360.0,
                0.5 - (std::f64::consts::FRAC_PI_4 + f / 2.0).tan().ln()
                    / (2.0 * std::f64::consts::PI),
            )
        }
    }
}

/// (X, Y) → (lon, lat).
pub fn unproject(p: Projection, x: f64, y: f64) -> (f64, f64) {
    match p {
        Projection::Equalearth => {
            let e = eex();
            let (x, y) = ((x - 0.5) * 2.0 * e, (0.5 - y) * 2.0 * e);
            let mut th = y / EE[0];
            for _ in 0..10 {
                let t2 = th * th;
                let t6 = t2 * t2 * t2;
                th -= (th * (EE[0] + EE[1] * t2 + t6 * (EE[2] + EE[3] * t2)) - y)
                    / (EE[0] + 3.0 * EE[1] * t2 + 7.0 * EE[2] * t6 + 9.0 * EE[3] * t6 * t2);
            }
            let th = th.clamp(-std::f64::consts::FRAC_PI_3, std::f64::consts::FRAC_PI_3);
            let t2 = th * th;
            let t6 = t2 * t2 * t2;
            let lat = ((2.0 * th.sin()) / 3f64.sqrt()).clamp(-1.0, 1.0).asin() / D2R;
            let lon =
                (3.0 * x * (9.0 * EE[3] * t6 * t2 + 7.0 * EE[2] * t6 + 3.0 * EE[1] * t2 + EE[0]))
                    / (2.0 * 3f64.sqrt() * th.cos())
                    / D2R;
            (lon.clamp(-180.0, 180.0), lat)
        }
        Projection::Equirect => (x * 360.0 - 180.0, ((0.5 - y) * 360.0).clamp(-90.0, 90.0)),
        Projection::Mercator => (
            x * 360.0 - 180.0,
            ((0.5 - y) * 2.0 * std::f64::consts::PI).sinh().atan() / D2R,
        ),
    }
}

/// Longitude in [-180, 180).
pub fn norm_lon(l: f64) -> f64 {
    (((l + 180.0) % 360.0) + 360.0) % 360.0 - 180.0
}

pub fn valid_ll(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0
}

// -------------------------------------------------------------- settings --

/// A number typed as text or as a number (`{{key}}` allowed).
fn num_or_str<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        serde_json::Value::Null => String::new(),
        v => v.to_string(),
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BaseStyle {
    /// Clean vector.
    #[default]
    Vector,
    /// Old « Satellite » (only a gradient): drawn as Vector.
    Satellite,
    /// Monochrome.
    Mono,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Colors {
    pub water: String,
    pub land: String,
    pub border: String,
    /// Graticule (empty = the borders' colour).
    pub grid: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            water: "#0b1a3a".into(),
            land: "#26365a".into(),
            border: "#9aa6d6".into(),
            grid: String::new(),
        }
    }
}

/// A view of the map: centre, zoom (Web Mercator levels), rotation and 3D tilt (degrees).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View {
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
    pub rot: f64,
    pub tilt: f64,
}

impl Default for View {
    fn default() -> Self {
        Self {
            lat: 46.45,
            lon: 2.6,
            zoom: 6.25,
            rot: 0.0,
            tilt: 0.0,
        }
    }
}

impl View {
    /// Zoom 1.5–18, tilt 0–60°.
    pub fn clamped(self) -> View {
        View {
            lat: if self.lat.is_finite() {
                self.lat.clamp(-85.0, 85.0)
            } else {
                0.0
            },
            lon: if self.lon.is_finite() {
                norm_lon(self.lon)
            } else {
                0.0
            },
            zoom: if self.zoom.is_finite() {
                self.zoom.clamp(1.5, 18.0)
            } else {
                2.0
            },
            rot: if self.rot.is_finite() { self.rot } else { 0.0 },
            tilt: if self.tilt.is_finite() {
                self.tilt.clamp(0.0, 60.0)
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KeyView {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
    pub rot: f64,
    pub tilt: f64,
}

impl Default for KeyView {
    fn default() -> Self {
        Self {
            name: "View".into(),
            lat: 46.45,
            lon: 2.6,
            zoom: 6.25,
            rot: 0.0,
            tilt: 0.0,
        }
    }
}

impl KeyView {
    pub fn view(&self) -> View {
        View {
            lat: self.lat,
            lon: self.lon,
            zoom: self.zoom,
            rot: self.rot,
            tilt: self.tilt,
        }
        .clamped()
    }
}

fn default_views() -> Vec<KeyView> {
    let v = |name: &str, lat, lon, zoom, rot, tilt| KeyView {
        name: name.into(),
        lat,
        lon,
        zoom,
        rot,
        tilt,
    };
    vec![
        v("World", 25.0, 10.0, 2.65, 0.0, 0.0),
        v("France", 46.45, 2.6, 6.25, 0.0, 0.0),
        v("Paris", 48.868, 2.335, 12.5, 0.0, 0.0),
        v("Stadium", 48.9245, 2.3602, 17.1, -14.0, 40.0),
    ]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ease {
    Linear,
    #[default]
    Inout,
    Out,
    In,
    Sine,
}

pub fn ease_inout(k: f64) -> f64 {
    if k < 0.5 {
        4.0 * k * k * k
    } else {
        1.0 - (-2.0 * k + 2.0).powi(3) / 2.0
    }
}

impl Ease {
    pub fn apply(self, k: f64) -> f64 {
        let k = k.clamp(0.0, 1.0);
        match self {
            Ease::Linear => k,
            Ease::Inout => ease_inout(k),
            Ease::Out => 1.0 - (1.0 - k).powi(3),
            Ease::In => k * k * k,
            Ease::Sine => (1.0 - (std::f64::consts::PI * k).cos()) / 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Transition {
    /// Seconds (0 = cut).
    pub dur: f64,
    pub ease: Ease,
    /// Zoom out, travel, zoom in.
    pub fly: bool,
    /// Height of the fly, 0–1.
    pub fly_amt: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            dur: 2.6,
            ease: Ease::Inout,
            fly: true,
            fly_amt: 0.8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layers {
    pub markers: bool,
    pub routes: bool,
    pub zones: bool,
    pub labels: bool,
}

impl Default for Layers {
    fn default() -> Self {
        Self {
            markers: true,
            routes: true,
            zones: true,
            labels: true,
        }
    }
}

impl Layers {
    pub fn get(&self, name: &str) -> Option<bool> {
        Some(match name {
            "markers" => self.markers,
            "routes" => self.routes,
            "zones" => self.zones,
            "labels" => self.labels,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerIcon {
    #[default]
    Pin,
    Dot,
    Star,
    Stadium,
    Plane,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Marker {
    /// What the Operator's `marker` action names (else the name).
    pub id: String,
    pub on: bool,
    pub name: String,
    /// Degrees, as typed (a number, or `{{key}}` from data).
    #[serde(deserialize_with = "num_or_str")]
    pub lat: String,
    #[serde(deserialize_with = "num_or_str")]
    pub lon: String,
    pub icon: MarkerIcon,
    pub color: String,
}

impl Default for Marker {
    fn default() -> Self {
        Self {
            id: String::new(),
            on: true,
            name: "Marker".into(),
            lat: "48.8566".into(),
            lon: "2.3522".into(),
            icon: MarkerIcon::Pin,
            color: "#ff3b5c".into(),
        }
    }
}

impl Marker {
    pub fn key(&self) -> &str {
        if self.id.trim().is_empty() {
            &self.name
        } else {
            &self.id
        }
    }
    /// (lat, lon) when valid.
    pub fn pos(&self, data: &Data) -> Option<(f64, f64)> {
        let lat = interpolate_text(&self.lat, data);
        let lon = interpolate_text(&self.lon, data);
        let (lat, lon) = (
            lat.trim().parse::<f64>().ok()?,
            lon.trim().parse::<f64>().ok()?,
        );
        valid_ll(lat, lon).then_some((lat, lon))
    }
}

fn default_markers() -> Vec<Marker> {
    let m = |name: &str, lat: &str, lon: &str, icon, color: &str| Marker {
        id: String::new(),
        on: true,
        name: name.into(),
        lat: lat.into(),
        lon: lon.into(),
        icon,
        color: color.into(),
    };
    vec![
        m(
            "Stade de France",
            "48.9245",
            "2.3602",
            MarkerIcon::Stadium,
            "#22c55e",
        ),
        m("Team hotel", "48.8738", "2.295", MarkerIcon::Pin, "#ff3b5c"),
        m("Lyon", "45.764", "4.8357", MarkerIcon::Dot, "#a594ff"),
        m("Marseille", "43.2965", "5.3698", MarkerIcon::Dot, "#a594ff"),
        m(
            "Tokyo 2026 HQ",
            "35.6762",
            "139.6503",
            MarkerIcon::Star,
            "#facc15",
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Route {
    pub id: String,
    pub on: bool,
    pub name: String,
    /// Waypoints: city names or `lat,lon`, separated by `>`.
    pub path: String,
    pub color: String,
    /// Seconds of the animated drawing.
    pub dur: f64,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            id: String::new(),
            on: true,
            name: "Route".into(),
            path: "Paris > New York".into(),
            color: "#38bdf8".into(),
            dur: 4.0,
        }
    }
}

impl Route {
    pub fn key(&self) -> &str {
        if self.id.trim().is_empty() {
            &self.name
        } else {
            &self.id
        }
    }
}

fn default_routes() -> Vec<Route> {
    vec![
        Route {
            id: String::new(),
            on: true,
            name: "Team coach".into(),
            path: "Lyon > Paris".into(),
            color: "#22c55e".into(),
            dur: 3.0,
        },
        Route {
            id: String::new(),
            on: true,
            name: "World tour".into(),
            path: "Paris > Los Angeles > Tokyo".into(),
            color: "#ff3b5c".into(),
            dur: 5.0,
        },
    ]
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    /// Great circle (shortest path on the globe).
    #[default]
    Gc,
    Straight,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneMode {
    /// The value is a category (party, winner…) coloured from the list.
    #[default]
    Category,
    /// The value is a number coloured on a scale.
    Scale,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub color: String,
}

impl Default for Category {
    fn default() -> Self {
        Self {
            name: "Category".into(),
            color: "#a594ff".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Stop {
    pub v: f64,
    pub color: String,
}

impl Default for Stop {
    fn default() -> Self {
        Self {
            v: 0.0,
            color: "#ffffff".into(),
        }
    }
}

/// Corner of the box: tl, tr, bl, br.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Corner {
    Tl,
    Tr,
    #[default]
    Bl,
    Br,
}

impl Corner {
    pub fn top(self) -> bool {
        matches!(self, Corner::Tl | Corner::Tr)
    }
    pub fn left(self) -> bool {
        matches!(self, Corner::Tl | Corner::Bl)
    }
}

fn row(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Which zones: French regions, departements, or countries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneLevel {
    #[default]
    Regions,
    Deps,
    Countries,
}

impl ZoneLevel {
    pub fn list(self) -> &'static [Zone] {
        let w = world();
        match self {
            ZoneLevel::Regions => &w.regions,
            ZoneLevel::Deps => &w.deps,
            ZoneLevel::Countries => &w.countries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Zones {
    pub level: ZoneLevel,
    pub mode: ZoneMode,
    /// Region | value | details…, first row = header. Cells may hold `{{key}}`.
    pub rows: Vec<Vec<String>>,
    pub categories: Vec<Category>,
    pub scale: Vec<Stop>,
    /// After a scale value (°C, %…).
    pub unit: String,
    pub title: String,
    pub opacity: f64,
    pub legend: bool,
    pub legend_pos: Corner,
}

impl Default for Zones {
    fn default() -> Self {
        let c = |n: &str, col: &str| Category {
            name: n.into(),
            color: col.into(),
        };
        let s = |v: f64, col: &str| Stop {
            v,
            color: col.into(),
        };
        Self {
            level: ZoneLevel::Regions,
            mode: ZoneMode::Category,
            rows: vec![
                row(&["Region", "Winner", "%"]),
                row(&["Hauts-de-France", "Red Union", "38 %"]),
                row(&["Normandie", "Blue Alliance", "33 %"]),
                row(&["Île-de-France", "Centre Now", "34 %"]),
                row(&["Grand Est", "Blue Alliance", "36 %"]),
                row(&["Bretagne", "Green Wave", "31 %"]),
                row(&["Pays de la Loire", "Centre Now", "35 %"]),
                row(&["Centre-Val de Loire", "Blue Alliance", "32 %"]),
                row(&["Bourgogne-Franche-Comté", "Red Union", "30 %"]),
                row(&["Nouvelle-Aquitaine", "Green Wave", "34 %"]),
                row(&["Auvergne-Rhône-Alpes", "Centre Now", "33 %"]),
                row(&["Occitanie", "Red Union", "37 %"]),
                row(&["Provence-Alpes-Côte d'Azur", "Blue Alliance", "41 %"]),
                row(&["Corse", "Blue Alliance", "39 %"]),
            ],
            categories: vec![
                c("Blue Alliance", "#2563eb"),
                c("Red Union", "#e11d48"),
                c("Green Wave", "#16a34a"),
                c("Centre Now", "#f59e0b"),
            ],
            scale: vec![
                s(-5.0, "#3b82f6"),
                s(5.0, "#38bdf8"),
                s(12.0, "#a3e635"),
                s(20.0, "#facc15"),
                s(28.0, "#f97316"),
                s(36.0, "#dc2626"),
            ],
            unit: "°C".into(),
            title: "Regional vote · leader".into(),
            opacity: 0.85,
            legend: true,
            legend_pos: Corner::Bl,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Labels {
    pub cities: bool,
    pub countries: bool,
    /// Places less important than this are not shown (1–10).
    pub min_imp: f64,
    pub size: f64,
    /// Old setting, ignored: the less important of two overlapping labels is always hidden.
    pub collide: bool,
    pub pad: f64,
    /// Hidden labels drawn in red (Designer only).
    pub show_hidden: bool,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            cities: true,
            countries: true,
            min_imp: 3.0,
            size: 24.0,
            collide: true,
            pad: 8.0,
            show_hidden: false,
        }
    }
}

/// Old setting (a free choice of provider), ignored: the attribution names
/// the data actually drawn ([`attribution`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    #[default]
    Own,
    Osm,
    Sat,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttStyle {
    #[default]
    Dark,
    Light,
    Minimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Attribution {
    pub provider: Provider,
    pub pos: Corner,
    pub style: AttStyle,
    pub show: bool,
}

impl Default for Attribution {
    fn default() -> Self {
        Self {
            provider: Provider::Own,
            pos: Corner::Br,
            style: AttStyle::Dark,
            show: true,
        }
    }
}

/// The attribution of the data drawn, in the project's language, and
/// whether its licence requires it on screen (IGN zones: Licence Ouverte).
pub fn attribution(c: &Config, st: &State, zoom: f64, lang: &str) -> (String, bool) {
    let ign = st.layer(c, "zones") && zoom <= 10.0 && c.zones.level != ZoneLevel::Countries;
    let mut src = vec!["Natural Earth"];
    if ign {
        src.push("IGN");
    }
    let head = if lang.starts_with("fr") {
        "Données\u{a0}: "
    } else {
        "Data: "
    };
    (format!("{head}{}", src.join(", ")), ign)
}

/// A place name as shown: the Designer's renaming, else in the project's language.
pub fn place_name<'a>(c: &'a Config, en: &'a str, fr: &'a str, lang: &str) -> &'a str {
    let n = norm(en);
    let nf = norm(fr);
    if let Some(r) = c
        .names
        .iter()
        .find(|r| !r.from.trim().is_empty() && (norm(&r.from) == n || norm(&r.from) == nf))
    {
        return &r.to;
    }
    if lang.starts_with("fr") {
        fr
    } else {
        en
    }
}

/// A place renamed in the Designer (original name → shown).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rename {
    pub from: String,
    pub to: String,
}

/// Settings of the widget, flattened into the layer's JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub style: BaseStyle,
    pub colors: Colors,
    pub show_borders: bool,
    pub show_roads: bool,
    pub graticule: bool,
    /// Corner radius of the map, px.
    pub radius: f64,
    pub projection: Projection,
    /// The view before any operator action.
    pub view: View,
    pub views: Vec<KeyView>,
    pub transition: Transition,
    pub layers: Layers,
    pub markers: Vec<Marker>,
    pub mk_size: f64,
    pub routes: Vec<Route>,
    pub curve: Curve,
    pub route_w: f64,
    pub plane_head: bool,
    pub zones: Zones,
    pub labels: Labels,
    pub attribution: Attribution,
    /// Designer only: dragging on the stage pans the map.
    pub drag_pans: bool,
    /// Places renamed (labels, zones).
    pub names: Vec<Rename>,
    /// The key views are chained from the take: every `auto_secs` seconds,
    /// on Continue, or on a trigger (`auto_on`).
    pub auto_views: bool,
    pub auto_secs: f64,
    pub auto_on: AutoOn,
    pub auto_trigger: String,
    /// Zone put forward from the take (its id, "" = none).
    pub hl_zone: String,
    /// The Operator's layer pads / marker pads are exposed (the Designer's choice).
    pub op_layers: bool,
    pub op_markers: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            style: BaseStyle::Vector,
            colors: Colors::default(),
            show_borders: true,
            show_roads: true,
            graticule: false,
            radius: 16.0,
            projection: Projection::Mercator,
            view: View::default(),
            views: default_views(),
            transition: Transition::default(),
            layers: Layers::default(),
            markers: default_markers(),
            mk_size: 36.0,
            routes: default_routes(),
            curve: Curve::Gc,
            route_w: 6.0,
            plane_head: true,
            zones: Zones::default(),
            labels: Labels::default(),
            attribution: Attribution::default(),
            drag_pans: false,
            names: Vec::new(),
            auto_views: false,
            auto_secs: 6.0,
            auto_on: AutoOn::Timed,
            auto_trigger: String::new(),
            hl_zone: String::new(),
            op_layers: false,
            op_markers: false,
        }
    }
}

// ---------------------------------------------------------------- state --

/// What the operator changes on air.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    /// Key view shown (None = the template's view).
    pub cur: Option<usize>,
    /// Transition: the view it started from and when (Unix ms).
    pub from: Option<View>,
    pub at: i64,
    /// Layers / markers / routes shown or hidden by the operator.
    pub layers: BTreeMap<String, bool>,
    pub markers: BTreeMap<String, bool>,
    pub routes_on: BTreeMap<String, bool>,
    /// When each route was launched (Unix ms): drawn from then.
    pub routes: BTreeMap<String, i64>,
    /// Highlighted zone (its id).
    pub highlight: String,
    /// « Aller à… »: the place shown instead of a key view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goto: Option<Goto>,
    /// The operator acted (else the Designer's chained views and highlight apply from the take).
    pub ctl: bool,
    /// Views chained: the next one every `interval` s counted from `auto_at`
    /// (`on` timed), or on Continue / on the trigger `trigger` (`interval`
    /// then 0). The view shown is `cur` plus the intervals elapsed (wrapping).
    pub auto: bool,
    pub interval: f64,
    pub auto_at: i64,
    pub on: AutoOn,
    pub trigger: String,
    /// Paused at (Unix ms), 0 = running.
    pub paused_at: i64,
}

/// A place the operator went to, and its view.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Goto {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
}

impl State {
    pub fn layer(&self, c: &Config, name: &str) -> bool {
        self.layers
            .get(name)
            .copied()
            .unwrap_or_else(|| c.layers.get(name).unwrap_or(false))
    }
    pub fn marker_on(&self, m: &Marker) -> bool {
        self.markers.get(m.key()).copied().unwrap_or(m.on)
    }
    pub fn route_on(&self, r: &Route) -> bool {
        self.routes_on.get(r.key()).copied().unwrap_or(r.on)
    }
}

/// The key view shown as the start: the one equal to the template's view, if any.
fn start_index(c: &Config) -> Option<usize> {
    let v = c.view.clamped();
    c.views.iter().position(|k| {
        let k = k.view();
        (k.lat - v.lat).abs() < 1e-3
            && (k.lon - v.lon).abs() < 1e-3
            && (k.zoom - v.zoom).abs() < 1e-2
    })
}

/// The view the state goes to (or shows): the place gone to, the key view, else the template's.
fn target(c: &Config, st: &State) -> View {
    match (&st.goto, st.cur.and_then(|i| c.views.get(i))) {
        (Some(g), _) => View {
            lat: g.lat,
            lon: g.lon,
            zoom: g.zoom,
            rot: 0.0,
            tilt: 0.0,
        }
        .clamped(),
        (None, Some(k)) => k.view(),
        (None, None) => c.view.clamped(),
    }
}

// ---------------------------------------------------------- chained views --

/// The state on air: the operator's once he acted, else the Designer's
/// chained views (counted from the take) and zone put forward.
pub fn live(c: &Config, st: &State, now_ms: i64, age_secs: f64) -> State {
    if st.ctl {
        return st.clone();
    }
    let mut s = st.clone();
    if s.highlight.is_empty() {
        if let Some(z) = zone_of(c.zones.level, c.hl_zone.trim()) {
            s.highlight = z.id.clone();
        }
    }
    if c.auto_views && !s.auto && !c.views.is_empty() {
        s.on = c.auto_on;
        s.trigger = c.auto_trigger.trim().to_string();
        if c.auto_on == AutoOn::Timed {
            if c.auto_secs > 0.0 {
                s.auto = true;
                s.interval = c.auto_secs.clamp(0.5, 3600.0);
                s.auto_at = (now_ms - (age_secs.max(0.0) * 1000.0) as i64).max(1);
            }
        } else {
            s.auto = true;
            s.interval = 0.0;
        }
    }
    s
}

/// The key view the chain counts from: the one shown, else the template's view when it is one.
fn base_index(c: &Config, st: &State) -> Option<usize> {
    st.cur.or_else(|| start_index(c))
}

/// The key view after `k` timed advances from `base` (none = the template's view: the first comes first).
fn index_after(base: Option<usize>, k: usize, n: usize) -> Option<usize> {
    if k == 0 {
        return base;
    }
    Some(match base {
        Some(b) => (b + k) % n,
        None => (k - 1) % n,
    })
}

/// The key view shown at `now` (None = the template's view or a place gone
/// to) and how many timed advances brought it.
pub fn shown_view(c: &Config, st: &State, now_ms: i64) -> (Option<usize>, usize) {
    let n = c.views.len();
    if n == 0 || !st.auto || st.interval <= 0.0 || st.auto_at <= 0 {
        return (st.cur, 0);
    }
    let t = if st.paused_at > 0 {
        st.paused_at
    } else {
        now_ms
    };
    let step = ((st.interval * 1000.0) as i64).max(1);
    let k = ((t - st.auto_at).max(0) / step) as usize;
    if k == 0 {
        return (st.cur, 0);
    }
    (index_after(base_index(c, st), k, n), k)
}

/// Makes the view shown at `now` the view set (flying from the one before it).
fn settle(c: &Config, st: &mut State, now_ms: i64) {
    let (idx, k) = shown_view(c, st, now_ms);
    if k == 0 {
        return;
    }
    let n = c.views.len();
    let step = ((st.interval * 1000.0) as i64).max(1);
    let from = if k == 1 {
        target(c, st)
    } else {
        index_after(base_index(c, st), k - 1, n)
            .and_then(|i| c.views.get(i))
            .map(|v| v.view())
            .unwrap_or_else(|| c.view.clamped())
    };
    st.cur = idx;
    st.goto = None;
    st.at = st.auto_at + k as i64 * step;
    st.from = (c.transition.dur > 0.0).then_some(from);
    // The count goes on from the last advance (its phase kept).
    st.auto_at = st.at;
}

/// The state as drawn at `now`: the take's chain and highlight applied, the
/// timed views gone to so far. What the renderers draw.
pub fn shown(c: &Config, st: &State, now_ms: i64, age_secs: f64) -> State {
    let mut s = live(c, st, now_ms, age_secs);
    settle(c, &mut s, now_ms);
    s
}

/// Seconds left before the next timed view (None = not running).
pub fn next_in(st: &State, now_ms: i64) -> Option<f64> {
    if !st.auto || st.interval <= 0.0 || st.auto_at <= 0 {
        return None;
    }
    let t = if st.paused_at > 0 {
        st.paused_at
    } else {
        now_ms
    };
    let step = st.interval * 1000.0;
    let e = (t - st.auto_at).max(0) as f64 % step;
    Some((step - e) / 1000.0)
}

/// The views wait for Continue (F2): chained on it, not on the last view.
pub fn waits_continue(c: &Config, st: &State, now_ms: i64) -> bool {
    let n = c.views.len();
    st.auto
        && st.on == AutoOn::Continue
        && n > 0
        && shown_view(c, st, now_ms).0.is_none_or(|i| i + 1 < n)
}

/// Continue (F2): the next view when the views wait for it.
pub fn on_continue(layer: &Layer, st: &mut State, ctx: &ActCtx) -> bool {
    let LayerKind::Map { x: c } = &layer.kind else {
        return false;
    };
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    if !waits_continue(c, &live_st, ctx.now_ms) {
        return false;
    }
    act(layer, st, "next", &serde_json::Value::Null, ctx).is_ok()
}

/// The trigger `name`: the next view when the views wait for it (the last
/// view goes back to the first).
pub fn on_trigger(layer: &Layer, st: &mut State, name: &str, ctx: &ActCtx) -> bool {
    let LayerKind::Map { x: c } = &layer.kind else {
        return false;
    };
    let live_st = live(c, st, ctx.now_ms, ctx.age_secs);
    if !(!c.views.is_empty()
        && live_st.auto
        && live_st.on == AutoOn::Trigger
        && super::same_trigger(&live_st.trigger, name))
    {
        return false;
    }
    act(layer, st, "next", &serde_json::Value::Null, ctx).is_ok()
}

/// A timed chain counts again from now when the operator moves the view.
fn restart(st: &mut State, now: i64) {
    if st.auto && st.interval > 0.0 {
        st.auto_at = now;
        if st.paused_at > 0 {
            st.paused_at = now;
        }
    }
}

/// The view at `now` (ms) and the transition's progress (None = not moving).
/// `box_w`: width of the layer (the fly's height depends on it).
pub fn view_at(c: &Config, st: &State, now: i64, box_w: f64) -> (View, Option<f64>) {
    let to = target(c, st);
    let Some(from) = st.from else {
        return (to, None);
    };
    let dur = c.transition.dur * 1000.0;
    if dur.is_nan() || dur <= 0.0 {
        return (to, None);
    }
    let k = (now - st.at) as f64 / dur;
    if k >= 1.0 {
        return (to, None);
    }
    let k = k.max(0.0);
    (interpolate(c, from.clamped(), to, k, box_w), Some(k))
}

/// The view at `k` (0–1) of the way from `a` to `b` (eased, with the fly).
pub fn interpolate(c: &Config, a: View, b: View, k: f64, box_w: f64) -> View {
    let e = c.transition.ease.apply(k);
    let p = c.projection;
    let (x0, y0) = project(p, a.lon, a.lat);
    let (x1, y1) = project(p, b.lon, b.lat);
    let mut dx = x1 - x0;
    if p.wraps() {
        dx -= dx.round();
    }
    let dy = y1 - y0;
    let mut x = x0 + dx * e;
    if p.wraps() {
        x -= x.floor();
    }
    let (lon, lat) = unproject(p, x, y0 + dy * e);
    let mut zoom = a.zoom + (b.zoom - a.zoom) * e;
    if c.transition.fly {
        let dist = dx.hypot(dy);
        let zfit = ((0.55 * box_w.max(1.0)) / (dist * 256.0).max(1e-9)).log2();
        let dip = ((a.zoom + b.zoom) / 2.0 - zfit).max(0.0) * c.transition.fly_amt;
        zoom -= dip * (std::f64::consts::PI * e).sin();
    }
    let mut dr = b.rot - a.rot;
    dr -= 360.0 * (dr / 360.0).round();
    View {
        lat,
        lon: norm_lon(lon),
        zoom: zoom.clamp(1.5, 18.0),
        rot: a.rot + dr * e,
        tilt: a.tilt + (b.tilt - a.tilt) * e,
    }
}

// --------------------------------------------------------------- routes --

/// A place by name (cities, POIs and countries of the built-in labels).
pub fn place(name: &str) -> Option<(f64, f64)> {
    let n = norm(name);
    if n.is_empty() {
        return None;
    }
    let w = world();
    w.places
        .iter()
        .find(|p| p.5 != "continent" && (norm(&p.0) == n || norm(&p.6) == n))
        .map(|p| (p.1, p.2))
        .or_else(|| {
            w.gaz
                .iter()
                .find(|g| norm(&g.0) == n || (!g.1.is_empty() && norm(&g.1) == n))
                .map(|g| (g.2, g.3))
        })
}

/// What « Aller à… » found: its name and where to look (a point and the
/// zoom to see it, or a zone's box to fit).
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Zoom to use (a city, coordinates), or None: fit `bbox`.
    pub zoom: Option<f64>,
    pub bbox: [f64; 4],
}

/// Finds a place typed by the operator: « lat, lon », a country, a region,
/// a departement (name or code), a city (French communes of 8 000
/// inhabitants or more, prefectures, the world's large cities), a label.
pub fn find_place(q: &str, lang: &str) -> Option<Found> {
    let q = q.trim();
    let parts: Vec<&str> = q
        .split([',', ';', ' '])
        .filter(|s| !s.trim().is_empty())
        .collect();
    if parts.len() == 2 {
        if let (Ok(lat), Ok(lon)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
            if valid_ll(lat, lon) {
                return Some(Found {
                    name: format!("{lat:.2}, {lon:.2}"),
                    lat,
                    lon,
                    zoom: None,
                    bbox: [lon, lat, lon, lat],
                });
            }
            return None;
        }
    }
    let n = norm(q);
    if n.is_empty() {
        return None;
    }
    let up = q.to_uppercase();
    let w = world();
    let zone = |z: &Zone| Found {
        name: if lang.starts_with("fr") || z.en.is_empty() {
            z.name.clone()
        } else {
            z.en.clone()
        },
        lat: z.lat,
        lon: z.lon,
        zoom: None,
        bbox: z.bbox,
    };
    // Codes first (FR, FRA, 69, 2A), then names.
    for list in [&w.countries, &w.deps] {
        if let Some(z) = list
            .iter()
            .find(|z| z.id == up || (!z.code.is_empty() && z.code == up))
        {
            return Some(zone(z));
        }
    }
    for list in [&w.countries, &w.regions, &w.deps] {
        if let Some(z) = list
            .iter()
            .find(|z| norm(&z.name) == n || (!z.en.is_empty() && norm(&z.en) == n))
        {
            return Some(zone(z));
        }
    }
    if let Some(g) = w
        .gaz
        .iter()
        .find(|g| norm(&g.0) == n || (!g.1.is_empty() && norm(&g.1) == n))
    {
        return Some(Found {
            name: if lang.starts_with("fr") || g.1.is_empty() {
                g.0.clone()
            } else {
                g.1.clone()
            },
            lat: g.2,
            lon: g.3,
            zoom: Some(9.0),
            bbox: [g.3, g.2, g.3, g.2],
        });
    }
    w.places
        .iter()
        .find(|p| p.5 != "continent" && (norm(&p.0) == n || norm(&p.6) == n))
        .map(|p| Found {
            name: p.name(lang).to_string(),
            lat: p.1,
            lon: p.2,
            zoom: Some(if p.5 == "poi" { 15.0 } else { 9.0 }),
            bbox: [p.2, p.1, p.2, p.1],
        })
}

/// The zoom showing a box [w, s, e, n] in 80 % of a w × h layer.
pub fn fit_zoom(p: Projection, b: [f64; 4], w: f64, h: f64) -> f64 {
    let (x0, y0) = project(p, b[0], b[3]);
    let (x1, y1) = project(p, b[2], b[1]);
    let (dx, dy) = ((x1 - x0).abs().max(1e-6), (y1 - y0).abs().max(1e-6));
    ((0.8 * w / (dx * 256.0)).min(0.8 * h / (dy * 256.0)))
        .log2()
        .clamp(1.5, 12.0)
}

/// Waypoints (lat, lon) of a route path, and the ones not understood.
pub fn waypoints(path: &str) -> (Vec<(f64, f64)>, Vec<String>) {
    let (mut out, mut bad) = (Vec::new(), Vec::new());
    for w in path
        .split(['>', '→'])
        .map(str::trim)
        .filter(|w| !w.is_empty())
    {
        let parts: Vec<&str> = w
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        let ll = if parts.len() == 2 {
            match (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                (Ok(a), Ok(b)) if valid_ll(a, b) => Some((a, b)),
                _ => None,
            }
        } else {
            None
        };
        match ll.or_else(|| place(w)) {
            Some(p) => out.push(p),
            None => bad.push(w.to_string()),
        }
    }
    (out, bad)
}

/// The route as (lat, lon) points; longitudes are UNWRAPPED (continuous),
/// so a leg across 180° stays short (it continues past ±180).
pub fn route_line(path: &str, curve: Curve) -> (Vec<(f64, f64)>, Vec<String>) {
    let (wp, bad) = waypoints(path);
    let mut line: Vec<(f64, f64)> = Vec::new();
    let v = |la: f64, lo: f64| {
        [
            (la * D2R).cos() * (lo * D2R).cos(),
            (la * D2R).cos() * (lo * D2R).sin(),
            (la * D2R).sin(),
        ]
    };
    for i in 0..wp.len().saturating_sub(1) {
        let ((la1, lo1), (la2, lo2)) = (wp[i], wp[i + 1]);
        let (a, b) = (v(la1, lo1), v(la2, lo2));
        let om = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
            .clamp(-1.0, 1.0)
            .acos();
        let n = ((om / D2R / 1.5).ceil() as usize).max(8);
        for j in (if i > 0 { 1 } else { 0 })..=n {
            let f = j as f64 / n as f64;
            let (la, mut lo);
            if curve == Curve::Straight || om < 1e-6 {
                let mut dl = lo2 - lo1;
                dl -= 360.0 * (dl / 360.0).round();
                la = la1 + (la2 - la1) * f;
                lo = lo1 + dl * f;
            } else {
                let (sa, sb) = (((1.0 - f) * om).sin() / om.sin(), (f * om).sin() / om.sin());
                let q = [
                    sa * a[0] + sb * b[0],
                    sa * a[1] + sb * b[1],
                    sa * a[2] + sb * b[2],
                ];
                la = q[2].atan2(q[0].hypot(q[1])) / D2R;
                lo = q[1].atan2(q[0]) / D2R;
            }
            if let Some(&(_, pl)) = line.last() {
                lo = pl + ((((lo - pl + 180.0) % 360.0) + 360.0) % 360.0 - 180.0);
            }
            line.push((la, lo));
        }
    }
    (line, bad)
}

/// Drawn share of a route (0–1) at `now`: launched routes draw over their duration.
pub fn route_progress(st: &State, r: &Route, now: i64) -> f64 {
    match st.routes.get(r.key()) {
        Some(&at) => ease_inout(((now - at) as f64 / (r.dur.max(0.1) * 1000.0)).clamp(0.0, 1.0)),
        None => 1.0,
    }
}

// ---------------------------------------------------------------- zones --

/// Lower case a–z only, accents folded (Île-de-France = iledefrance).
pub fn norm(s: &str) -> String {
    let mut out = String::new();
    for ch in s.to_lowercase().chars() {
        let m = match ch {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => "a",
            'ç' => "c",
            'é' | 'è' | 'ê' | 'ë' => "e",
            'î' | 'ï' | 'í' | 'ì' => "i",
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => "o",
            'ù' | 'û' | 'ü' | 'ú' => "u",
            'œ' => "oe",
            'æ' => "ae",
            'ÿ' => "y",
            'ñ' => "n",
            c if c.is_ascii_lowercase() => {
                out.push(c);
                continue;
            }
            _ => continue,
        };
        out.push_str(m);
    }
    out
}

/// Region by id or name.
/// A zone of a level by its name (French or English), id or code (69, 2A, FR, FRA).
pub fn zone_of(level: ZoneLevel, name: &str) -> Option<&'static Zone> {
    let n = norm(name);
    let up = name.trim().to_uppercase();
    if up.is_empty() {
        return None;
    }
    level.list().iter().find(|z| {
        z.id.eq_ignore_ascii_case(&up)
            || (!z.code.is_empty() && z.code == up)
            || (!n.is_empty() && (norm(&z.name) == n || (!z.en.is_empty() && norm(&z.en) == n)))
    })
}

/// A region by id or name (the regions of France).
pub fn region_of(name: &str) -> Option<&'static Zone> {
    zone_of(ZoneLevel::Regions, name)
}

/// `#rrggbb` mix of two colours.
pub fn hex_mix(a: &str, b: &str, k: f64) -> String {
    let p = |s: &str| {
        let s = s.trim().trim_start_matches('#');
        let v = u32::from_str_radix(s.get(0..6).unwrap_or("000000"), 16).unwrap_or(0);
        [(v >> 16) & 255, (v >> 8) & 255, v & 255]
    };
    let (pa, pb) = (p(a), p(b));
    let c: Vec<String> = (0..3)
        .map(|i| {
            let v = pa[i] as f64 + (pb[i] as f64 - pa[i] as f64) * k;
            format!("{:02x}", v.round().clamp(0.0, 255.0) as u32)
        })
        .collect();
    format!("#{}", c.join(""))
}

/// Colour of a value on the scale.
pub fn scale_color(stops: &[Stop], v: f64) -> String {
    let Some(first) = stops.first() else {
        return "#94a3b8".into();
    };
    if v <= first.v {
        return first.color.clone();
    }
    for w in stops.windows(2) {
        if v <= w[1].v {
            let span = w[1].v - w[0].v;
            let k = if span > 0.0 { (v - w[0].v) / span } else { 1.0 };
            return hex_mix(&w[0].color, &w[1].color, k);
        }
    }
    stops.last().map(|s| s.color.clone()).unwrap_or_default()
}

/// Fill of a zone: colour and text (value and details).
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneFill {
    pub color: String,
    pub text: String,
}

/// Zones coloured from the data rows, and the region names not found.
pub fn zone_fills(z: &Zones, data: &Data) -> (BTreeMap<String, ZoneFill>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut unmatched = Vec::new();
    for r in z.rows.iter().skip(1) {
        let cell = |i: usize| interpolate_text(r.get(i).map(String::as_str).unwrap_or(""), data);
        let (name, value) = (cell(0), cell(1));
        if name.trim().is_empty() {
            continue;
        }
        let Some(reg) = zone_of(z.level, &name) else {
            unmatched.push(name);
            continue;
        };
        let details: Vec<String> = (2..r.len())
            .map(cell)
            .filter(|s| !s.trim().is_empty())
            .collect();
        let (color, text) = match z.mode {
            ZoneMode::Category => {
                let n = norm(&value);
                let color = z
                    .categories
                    .iter()
                    .find(|c| norm(&c.name) == n)
                    .map(|c| c.color.clone())
                    .unwrap_or_else(|| "#94a3b8".into());
                let mut t = value.trim().to_string();
                for d in &details {
                    t = format!("{t} {d}");
                }
                (color, t)
            }
            ZoneMode::Scale => match value.trim().replace(',', ".").parse::<f64>() {
                Ok(v) => {
                    let mut t = format!("{} {}", value.trim(), z.unit).trim().to_string();
                    for d in &details {
                        t = format!("{t} {d}");
                    }
                    (scale_color(&z.scale, v), t)
                }
                // No value: the renderers write « No data » in the project's language.
                Err(_) => ("#94a3b8".into(), String::new()),
            },
        };
        out.insert(reg.id.clone(), ZoneFill { color, text });
    }
    (out, unmatched)
}

// -------------------------------------------------------------- actions --

fn arg_str(args: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        match args.get(k) {
            Some(serde_json::Value::String(s)) => return Some(s.clone()),
            Some(serde_json::Value::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    match args {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn arg_on(args: &serde_json::Value) -> Option<bool> {
    args.get("on").and_then(|v| v.as_bool())
}

/// Index of a named item (by key, name, or 0-based index).
fn find<T>(items: &[T], key: &str, keys: impl Fn(&T) -> (String, String)) -> Option<usize> {
    let k = key.trim();
    items
        .iter()
        .position(|it| {
            let (a, b) = keys(it);
            a.eq_ignore_ascii_case(k) || b.eq_ignore_ascii_case(k)
        })
        .or_else(|| k.parse::<usize>().ok().filter(|&i| i < items.len()))
}

/// Applies an operator action (`op`, with its `args`) to the state.
///
/// - `next` / `prev`: next / previous key view (animated).
/// - `go {view}`: a key view by name or index.
/// - `reset`: back to the template's view, no highlight, every layer, marker and route as designed.
/// - `layer {name, on?}`: markers / routes / zones / labels (toggles without `on`).
/// - `marker {id, on?}`: one marker (id or name).
/// - `highlight {zone}`: a region by id or name (empty = none).
/// - `route {id}`: (re)draws a route from now.
/// - `auto {seconds}`: the views go on by themselves (chained, timed);
///   `advance {on: timed | continue | trigger, seconds?, trigger?}` · `pause` ·
///   `resume` · `stop` (not chained, back to the first view).
pub fn act(
    layer: &Layer,
    st: &mut State,
    op: &str,
    args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    let LayerKind::Map { x: c } = &layer.kind else {
        return Err("not a map".into());
    };
    let now = ctx.now_ms;
    let box_w = layer
        .props
        .get(&crate::Prop::Width)
        .and_then(|v| v.as_number())
        .unwrap_or(1600.0);
    let box_h = layer
        .props
        .get(&crate::Prop::Height)
        .and_then(|v| v.as_number())
        .unwrap_or(840.0);
    // From now on the operator's state: what was on air so far, kept.
    *st = live(c, st, now, ctx.age_secs);
    st.ctl = true;
    settle(c, st, now);
    let go = |st: &mut State, i: usize| -> Result<(), String> {
        let k = c.views.get(i).ok_or("no such view")?;
        if !valid_ll(k.lat, k.lon) {
            return Err(format!("View “{}” has invalid coordinates", k.name));
        }
        let (cur, _) = view_at(c, st, now, box_w);
        st.cur = Some(i);
        st.goto = None;
        if c.transition.dur > 0.0 {
            st.from = Some(cur);
            st.at = now;
        } else {
            st.from = None;
        }
        restart(st, now);
        Ok(())
    };
    match op {
        "next" | "prev" => {
            let n = c.views.len();
            if n == 0 {
                return Err("no key views".into());
            }
            let cur = st.cur.or_else(|| start_index(c));
            let i = match (op, cur) {
                ("next", Some(i)) => (i + 1) % n,
                ("next", None) => 0,
                (_, Some(i)) => (i + n - 1) % n,
                (_, None) => n - 1,
            };
            go(st, i)
        }
        "go" | "view" => {
            let name = arg_str(args, &["view", "name", "index"]).ok_or("which view?")?;
            let i = find(&c.views, &name, |v| (v.name.clone(), String::new()))
                .ok_or_else(|| format!("no view named “{name}”"))?;
            go(st, i)
        }
        "reset" => {
            // Back to the template's view (flying there), no highlight, layers as
            // designed; the chain, kept, counts again from now.
            let (cur, _) = view_at(c, st, now, box_w);
            *st = State {
                ctl: true,
                auto: st.auto,
                on: st.on,
                interval: st.interval,
                trigger: std::mem::take(&mut st.trigger),
                auto_at: if st.auto && st.interval > 0.0 { now } else { 0 },
                ..State::default()
            };
            if c.transition.dur > 0.0 {
                st.from = Some(cur);
                st.at = now;
            }
            Ok(())
        }
        "auto" => {
            let secs = args
                .get("seconds")
                .and_then(|v| v.as_f64())
                .unwrap_or(if st.interval > 0.0 { st.interval } else { 6.0 });
            if secs <= 0.0 {
                st.auto = false;
                st.paused_at = 0;
                return Ok(());
            }
            if !(0.5..=3600.0).contains(&secs) {
                return Err("interval: 0.5 to 3600 s".into());
            }
            if st.auto && st.auto_at > 0 {
                // New interval: keep the view, count from now.
                st.auto_at = if st.paused_at > 0 { st.paused_at } else { now };
            } else {
                st.auto_at = now;
                st.paused_at = 0;
            }
            st.auto = true;
            st.on = AutoOn::Timed;
            st.interval = secs;
            Ok(())
        }
        "advance" => {
            let on = match args.get("on").and_then(|v| v.as_str()).unwrap_or("timed") {
                "continue" => AutoOn::Continue,
                "trigger" => AutoOn::Trigger,
                "timed" => AutoOn::Timed,
                other => return Err(format!("unknown advance mode “{other}”")),
            };
            if let Some(name) = args.get("trigger").and_then(|v| v.as_str()) {
                st.trigger = name.trim().to_string();
            }
            if on == AutoOn::Timed {
                let secs =
                    args.get("seconds")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(if st.interval > 0.0 {
                            st.interval
                        } else {
                            c.auto_secs
                        });
                return act(
                    layer,
                    st,
                    "auto",
                    &serde_json::json!({ "seconds": secs }),
                    ctx,
                );
            }
            st.auto = true;
            st.on = on;
            st.interval = 0.0;
            st.auto_at = 0;
            st.paused_at = 0;
            Ok(())
        }
        "stop" => {
            st.auto = false;
            st.paused_at = 0;
            if c.views.is_empty() {
                return Ok(());
            }
            go(st, 0)
        }
        "pause" => {
            if st.auto && st.paused_at == 0 {
                st.paused_at = now;
            }
            Ok(())
        }
        "resume" => {
            if st.auto && st.paused_at > 0 {
                st.auto_at += now - st.paused_at;
                st.paused_at = 0;
            }
            Ok(())
        }
        "goto" => {
            let q = arg_str(args, &["place", "q", "name"]).ok_or("where?")?;
            let lang = args.get("lang").and_then(|v| v.as_str()).unwrap_or("fr");
            let f = find_place(&q, lang).ok_or_else(|| format!("“{}” not found", q.trim()))?;
            let (cur, _) = view_at(c, st, now, box_w);
            let zoom = match f.zoom {
                Some(z) => z,
                None if f.bbox[0] == f.bbox[2] => cur.zoom.max(6.0),
                None => fit_zoom(c.projection, f.bbox, box_w, box_h),
            };
            let (lat, lon) = if f.zoom.is_none() && f.bbox[0] != f.bbox[2] {
                // The box's middle (on the projection).
                let (x0, y0) = project(c.projection, f.bbox[0], f.bbox[3]);
                let (x1, y1) = project(c.projection, f.bbox[2], f.bbox[1]);
                let (lon, lat) = unproject(c.projection, (x0 + x1) / 2.0, (y0 + y1) / 2.0);
                (lat, lon)
            } else {
                (f.lat, f.lon)
            };
            st.goto = Some(Goto {
                name: f.name,
                lat,
                lon,
                zoom,
            });
            st.cur = None;
            if c.transition.dur > 0.0 {
                st.from = Some(cur);
                st.at = now;
            } else {
                st.from = None;
            }
            Ok(())
        }
        "layer" => {
            let name = arg_str(args, &["name", "layer"])
                .ok_or("which layer?")?
                .to_lowercase();
            if c.layers.get(&name).is_none() {
                return Err(format!(
                    "no layer “{name}” (markers, routes, zones, labels)"
                ));
            }
            let on = arg_on(args).unwrap_or(!st.layer(c, &name));
            st.layers.insert(name, on);
            Ok(())
        }
        "marker" => {
            let id = arg_str(args, &["id", "name", "marker"]).ok_or("which marker?")?;
            let i = find(&c.markers, &id, |m| (m.key().to_string(), m.name.clone()))
                .ok_or_else(|| format!("no marker “{id}”"))?;
            let m = &c.markers[i];
            let on = arg_on(args).unwrap_or(!st.marker_on(m));
            st.markers.insert(m.key().to_string(), on);
            if on {
                st.layers.insert("markers".into(), true);
            }
            Ok(())
        }
        "highlight" => {
            let z = arg_str(args, &["zone", "id", "name"]).unwrap_or_default();
            if z.trim().is_empty() {
                st.highlight.clear();
                return Ok(());
            }
            let r = zone_of(c.zones.level, &z).ok_or_else(|| format!("no zone “{z}”"))?;
            st.highlight = r.id.clone();
            st.layers.insert("zones".into(), true);
            Ok(())
        }
        "route" => {
            let id = arg_str(args, &["id", "name", "route"]).ok_or("which route?")?;
            let i = find(&c.routes, &id, |r| (r.key().to_string(), r.name.clone()))
                .ok_or_else(|| format!("no route “{id}”"))?;
            let key = c.routes[i].key().to_string();
            st.routes.insert(key.clone(), now);
            st.routes_on.insert(key, true);
            st.layers.insert("routes".into(), true);
            Ok(())
        }
        _ => Err(format!("unknown action `{op}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn layer(c: Config) -> Layer {
        let mut l: Layer = serde_json::from_str(
            r#"{"id":"m","name":"Map","type":"map","props":{"width":1600,"height":840}}"#,
        )
        .unwrap();
        l.kind = LayerKind::Map { x: c };
        l
    }

    fn ctx(data: &Data, now: i64) -> ActCtx<'_> {
        ActCtx {
            now_ms: now,
            age_secs: 0.0,
            data,
            fps: 50.0,
        }
    }

    #[test]
    fn world_loads_and_old_json_loads() {
        assert!(world().land.len() > 10);
        assert_eq!(world().regions.len(), 18);
        assert_eq!(world().deps.len(), 101);
        assert!(world().countries.len() > 150);
        // Rings close and stay on the globe.
        for z in &world().deps {
            for r in &z.rings {
                assert!(r.len() > 3, "{}", z.name);
                assert!(r.iter().all(|p| valid_ll(p[1], p[0])));
            }
        }
        let l: Layer =
            serde_json::from_str(r#"{"id":"a","name":"M","type":"map","props":{}}"#).unwrap();
        let LayerKind::Map { x } = l.kind else {
            panic!()
        };
        assert_eq!(x, Config::default());
        // Coordinates typed as numbers load as text.
        let m: Marker = serde_json::from_str(r#"{"lat":12.5,"lon":"{{lon}}"}"#).unwrap();
        assert_eq!(m.lat, "12.5");
    }

    #[test]
    fn projections_round_trip() {
        for p in [
            Projection::Mercator,
            Projection::Equalearth,
            Projection::Equirect,
        ] {
            let (x, y) = project(p, 139.65, 35.67);
            let (lon, lat) = unproject(p, x, y);
            assert!(
                (lon - 139.65).abs() < 1e-6 && (lat - 35.67).abs() < 1e-6,
                "{p:?}"
            );
        }
    }

    #[test]
    fn invalid_markers_are_ignored() {
        let d = Data::new();
        let mut m = Marker {
            lat: "123.4".into(),
            ..Default::default()
        };
        assert!(m.pos(&d).is_none());
        m.lat = "".into();
        assert!(m.pos(&d).is_none());
        m.lat = "abc".into();
        assert!(m.pos(&d).is_none());
        m.lat = "{{la}}".into();
        let d: Data = [("la".to_string(), "45.5".to_string())].into();
        assert_eq!(m.pos(&d), Some((45.5, 2.3522)));
    }

    #[test]
    fn antimeridian_takes_the_short_way() {
        let (line, bad) = route_line("Los Angeles > Tokyo", Curve::Gc);
        assert!(bad.is_empty());
        // Unwrapped: longitudes keep decreasing past -180 (over the Pacific).
        let last = line.last().unwrap().1;
        assert!(last < -180.0, "{last}");
        for w in line.windows(2) {
            assert!((w[1].1 - w[0].1).abs() < 10.0);
        }
        let (_, bad) = route_line("Paris > Atlantis > 10,20", Curve::Gc);
        assert_eq!(bad, vec!["Atlantis".to_string()]);
    }

    #[test]
    fn zones_from_data() {
        let mut z = Zones::default();
        z.rows.push(row(&["Narnia", "Red Union"]));
        z.rows.push(row(&["corse", "{{w}}"]));
        let d: Data = [("w".to_string(), "Green Wave".to_string())].into();
        let (f, un) = zone_fills(&z, &d);
        assert_eq!(f["idf"].color, "#f59e0b");
        assert_eq!(f["cor"].color, "#16a34a");
        assert_eq!(un, vec!["Narnia".to_string()]);
        assert_eq!(
            scale_color(&z.scale, 8.5),
            hex_mix("#38bdf8", "#a3e635", 0.5)
        );
    }

    #[test]
    fn actions() {
        let l = layer(Config::default());
        let d = Data::new();
        let mut st = State::default();
        act(
            &l,
            &mut st,
            "next",
            &serde_json::Value::Null,
            &ctx(&d, 1000),
        )
        .unwrap();
        // Starts at France (the template's view = key view 1): next is Paris.
        assert_eq!(st.cur, Some(2));
        assert!(st.from.is_some());
        let c = Config::default();
        let (v, k) = view_at(&c, &st, 1000 + 1300, 1600.0);
        assert!(k.is_some() && v.zoom < 12.5);
        let (v, k) = view_at(&c, &st, 1000 + 2600, 1600.0);
        assert!(k.is_none() && (v.zoom - 12.5).abs() < 1e-9);
        act(
            &l,
            &mut st,
            "go",
            &serde_json::json!({"view":"stadium"}),
            &ctx(&d, 5000),
        )
        .unwrap();
        assert_eq!(st.cur, Some(3));
        assert!(act(
            &l,
            &mut st,
            "go",
            &serde_json::json!({"view":"Mars"}),
            &ctx(&d, 5000)
        )
        .is_err());
        act(
            &l,
            &mut st,
            "layer",
            &serde_json::json!({"name":"zones","on":false}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert!(!st.layer(&c, "zones"));
        act(
            &l,
            &mut st,
            "highlight",
            &serde_json::json!({"zone":"Île-de-France"}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert_eq!(st.highlight, "idf");
        assert!(st.layer(&c, "zones"));
        act(
            &l,
            &mut st,
            "marker",
            &serde_json::json!({"id":"Lyon"}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert_eq!(st.markers.get("Lyon"), Some(&false));
        act(
            &l,
            &mut st,
            "route",
            &serde_json::json!({"id":"World tour"}),
            &ctx(&d, 10_000),
        )
        .unwrap();
        assert!(route_progress(&st, &c.routes[1], 12_500) < 1.0);
        assert_eq!(route_progress(&st, &c.routes[1], 15_000), 1.0);
        act(
            &l,
            &mut st,
            "reset",
            &serde_json::Value::Null,
            &ctx(&d, 20_000),
        )
        .unwrap();
        // Back to the template's view, flying from where it was.
        assert_eq!(st.cur, None);
        assert!(st.highlight.is_empty() && st.layers.is_empty());
        assert!(st.from.is_some() && st.at == 20_000);
    }

    #[test]
    fn goto_a_place_and_zones_of_every_level() {
        let l = layer(Config::default());
        let d = Data::new();
        let mut st = State::default();
        act(
            &l,
            &mut st,
            "goto",
            &serde_json::json!({"place":"lyon"}),
            &ctx(&d, 1000),
        )
        .unwrap();
        let g = st.goto.clone().unwrap();
        assert_eq!(g.name, "Lyon");
        assert!((g.lat - 45.76).abs() < 0.1 && (g.lon - 4.83).abs() < 0.1);
        let c = Config::default();
        let (v, _) = view_at(&c, &st, 1000 + 10_000, 1600.0);
        assert!((v.lat - g.lat).abs() < 1e-9);
        // Coordinates, a departement code, a country in English.
        act(
            &l,
            &mut st,
            "goto",
            &serde_json::json!({"place":"43.3, 5.4"}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert_eq!(st.goto.as_ref().unwrap().lat, 43.3);
        act(
            &l,
            &mut st,
            "goto",
            &serde_json::json!({"place":"2A"}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert_eq!(st.goto.as_ref().unwrap().name, "Corse-du-Sud");
        act(
            &l,
            &mut st,
            "goto",
            &serde_json::json!({"place":"Spain", "lang":"en"}),
            &ctx(&d, 0),
        )
        .unwrap();
        assert_eq!(st.goto.as_ref().unwrap().name, "Spain");
        assert!(act(
            &l,
            &mut st,
            "goto",
            &serde_json::json!({"place":"Atlantis"}),
            &ctx(&d, 0)
        )
        .is_err());
        // A key view leaves the place.
        act(&l, &mut st, "next", &serde_json::Value::Null, &ctx(&d, 0)).unwrap();
        assert!(st.goto.is_none());
        // Zones: departements by code or name, countries by ISO code or name.
        assert_eq!(zone_of(ZoneLevel::Deps, "69").unwrap().name, "Rhône");
        assert_eq!(zone_of(ZoneLevel::Deps, "finistere").unwrap().id, "29");
        assert_eq!(
            zone_of(ZoneLevel::Countries, "Allemagne").unwrap().id,
            "DEU"
        );
        assert_eq!(zone_of(ZoneLevel::Countries, "DE").unwrap().id, "DEU");
        assert_eq!(zone_of(ZoneLevel::Regions, "Bretagne").unwrap().id, "bre");
        // Names in the project's language, renamed in the Designer.
        let mut c = Config::default();
        assert_eq!(place_name(&c, "London", "Londres", "fr"), "Londres");
        assert_eq!(place_name(&c, "London", "Londres", "en"), "London");
        c.names.push(Rename {
            from: "Londres".into(),
            to: "LONDON".into(),
        });
        assert_eq!(place_name(&c, "London", "Londres", "fr"), "LONDON");
        // The attribution names the data drawn.
        let (txt, req) = attribution(&c, &State::default(), 6.0, "fr");
        assert_eq!(txt, "Données\u{a0}: Natural Earth, IGN");
        assert!(req);
    }
}

#[cfg(test)]
mod chained_views {
    use super::*;

    #[test]
    fn views_chained_from_the_take_on_continue_and_on_a_trigger() {
        // Four key views (World, France, Paris, Stadium); the template's view is France (1).
        let mut c = Config {
            auto_views: true,
            auto_secs: 5.0,
            hl_zone: "Île-de-France".into(),
            ..Default::default()
        };
        let d = Data::new();
        let cx = ActCtx {
            now_ms: 100_000,
            age_secs: 12.0,
            data: &d,
            fps: 50.0,
        };
        // Timed from the take (on air 12 s ago, 5 s per view): two advances from France → Stadium.
        let st = live(&c, &State::default(), 100_000, 12.0);
        assert!(st.auto && st.on == AutoOn::Timed && st.interval == 5.0 && st.auto_at == 88_000);
        assert_eq!(st.highlight, "idf");
        assert_eq!(shown_view(&c, &st, 92_000), (None, 0));
        assert_eq!(shown_view(&c, &st, 93_500), (Some(2), 1));
        assert_eq!(shown_view(&c, &st, 100_000), (Some(3), 2));
        assert_eq!(shown_view(&c, &st, 110_000), (Some(1), 4));
        assert_eq!(next_in(&st, 93_500), Some(4.5));
        // Drawn: the view set, flying from the one before, the count going on in phase.
        let sh = shown(&c, &State::default(), 100_000, 12.0);
        assert_eq!((sh.cur, sh.at, sh.auto_at), (Some(3), 98_000, 98_000));
        assert_eq!(sh.from.map(|v| v.zoom), Some(c.views[2].view().zoom));
        assert_eq!(shown_view(&c, &sh, 103_500), (Some(0), 1));
        // Chained on Continue: F2 turns the views, until the last one.
        c.auto_on = AutoOn::Continue;
        let l = super::tests::layer(c.clone());
        let mut st = State::default();
        assert!(waits_continue(&c, &live(&c, &st, 100_000, 12.0), 100_000));
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(st.cur, Some(2));
        assert!(st.ctl && st.auto && st.on == AutoOn::Continue);
        assert!(on_continue(&l, &mut st, &cx));
        assert_eq!(st.cur, Some(3));
        assert!(!on_continue(&l, &mut st, &cx));
        // A trigger: only its name, and it wraps.
        act(
            &l,
            &mut st,
            "advance",
            &serde_json::json!({"on": "trigger", "trigger": "villes"}),
            &cx,
        )
        .unwrap();
        assert!(!on_trigger(&l, &mut st, "other", &cx));
        assert!(on_trigger(&l, &mut st, " VILLES ", &cx));
        assert_eq!(st.cur, Some(0));
        // Stop: not chained, back to the first view; timed again from Advance.
        act(&l, &mut st, "next", &serde_json::Value::Null, &cx).unwrap();
        act(&l, &mut st, "stop", &serde_json::Value::Null, &cx).unwrap();
        assert!(!st.auto && st.cur == Some(0));
        act(
            &l,
            &mut st,
            "advance",
            &serde_json::json!({"on": "timed", "seconds": 4}),
            &cx,
        )
        .unwrap();
        assert!(st.auto && st.on == AutoOn::Timed && st.interval == 4.0 && st.auto_at == 100_000);
        assert_eq!(shown_view(&c, &st, 104_500), (Some(1), 1));
        // Paused: the view stands; resumed: the count goes on.
        let cx2 = ActCtx {
            now_ms: 104_500,
            age_secs: 16.5,
            data: &d,
            fps: 50.0,
        };
        act(&l, &mut st, "pause", &serde_json::Value::Null, &cx2).unwrap();
        assert_eq!(shown_view(&c, &st, 130_000), (Some(1), 0));
        act(&l, &mut st, "resume", &serde_json::Value::Null, &cx2).unwrap();
        assert_eq!(shown_view(&c, &st, 104_600), (Some(1), 0));
        // Reset keeps the chain, counting again from now, back to the template's view.
        act(&l, &mut st, "reset", &serde_json::Value::Null, &cx2).unwrap();
        assert!(st.auto && st.cur.is_none() && st.auto_at == 104_500 && st.highlight.is_empty());
        assert_eq!(shown_view(&c, &st, 108_600), (Some(2), 1));
    }
}
