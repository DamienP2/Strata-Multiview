//! Demo project used when the server starts without data.

use std::collections::BTreeMap;

use crate::easing::Easing;
use crate::model::*;

fn kf(frame: f64, v: f64, easing: Easing) -> Keyframe {
    Keyframe {
        frame,
        value: Value::Number(v),
        easing,
    }
}

fn hex(s: &str) -> Value {
    Value::Color(Color::from_hex(s).expect("valid hex"))
}

fn layer(
    id: &str,
    name: &str,
    kind: LayerKind,
    parent: Option<&str>,
    props: &[(Prop, Value)],
) -> Layer {
    Layer {
        id: id.into(),
        name: name.into(),
        parent: parent.map(Into::into),
        kind,
        visible: true,
        locked: false,
        manual_start: false,
        start_on: None,
        start_trigger: String::new(),
        scale_lock: false,
        lock_ratio: false,
        drop_shadow: false,
        timeline: None,
        props: props.iter().copied().collect(),
        tracks: BTreeMap::new(),
        bindings: BTreeMap::new(),
        setting_bindings: BTreeMap::new(),
        overrides: BTreeMap::new(),
        live_changes: BTreeMap::new(),
        span: None,
        motion: None,
    }
}

fn text(t: &str, weight: u16) -> LayerKind {
    LayerKind::Text {
        x: Default::default(),
        wrap: false,
        transform: Default::default(),
        decoration: Default::default(),
        line_height: None,
        scroll: Default::default(),
        speed: 120.0,
        separator: "   •   ".into(),
        looping: false,
        overflow: Default::default(),
        squeeze: 0.8,
        min_size: 0.6,
        warn_below: 0.8,
        text: t.into(),
        font_family: "Inter".into(),
        font_weight: weight,
        align: TextAlign::Left,
        vertical_align: VerticalAlign::Middle,
        uppercase: false,
    }
}

fn marker(frame: f64, name: &str, kind: MarkerKind) -> Marker {
    Marker {
        id: new_id(),
        frame,
        name: name.into(),
        kind,
        resume: Default::default(),
    }
}

pub fn lower_third() -> Template {
    use Prop::*;
    let n = Value::Number;
    let mut group = layer(
        "lt-group",
        "Lower third",
        LayerKind::Group {
            clip: true,
            lottie: None,
        },
        None,
        &[
            (X, n(160.0)),
            (Y, n(820.0)),
            (Width, n(1100.0)),
            (Height, n(150.0)),
        ],
    );
    group.tracks.insert(
        CropRight,
        vec![
            kf(0.0, 1.0, Easing::EaseOut),
            kf(18.0, 0.0, Easing::Linear),
            kf(100.0, 0.0, Easing::EaseIn),
            kf(115.0, 1.0, Easing::Linear),
        ],
    );

    let mut accent = layer(
        "lt-accent",
        "Accent bar",
        LayerKind::rect(),
        Some("lt-group"),
        &[
            (X, n(0.0)),
            (Y, n(0.0)),
            (Width, n(14.0)),
            (Height, n(150.0)),
            (Fill, hex("#ff3b5c")),
        ],
    );
    accent.tracks.insert(
        Height,
        vec![kf(0.0, 0.0, Easing::Back), kf(14.0, 150.0, Easing::Linear)],
    );

    let plate = layer(
        "lt-plate",
        "Plate",
        LayerKind::rect(),
        Some("lt-group"),
        &[
            (X, n(14.0)),
            (Y, n(0.0)),
            (Width, n(1086.0)),
            (Height, n(95.0)),
            (Fill, hex("#0f1222e6")),
        ],
    );
    let mut sub = layer(
        "lt-sub",
        "Subtitle plate",
        LayerKind::rect(),
        Some("lt-group"),
        &[
            (X, n(14.0)),
            (Y, n(95.0)),
            (Width, n(700.0)),
            (Height, n(55.0)),
            (Fill, hex("#ff3b5c")),
        ],
    );
    sub.tracks.insert(
        Width,
        vec![
            kf(8.0, 0.0, Easing::EaseOut),
            kf(26.0, 700.0, Easing::Linear),
        ],
    );

    let mut name = layer(
        "lt-name",
        "Name",
        text("{{name}}", 700),
        Some("lt-group"),
        &[
            (X, n(48.0)),
            (Y, n(0.0)),
            (Width, n(1040.0)),
            (Height, n(95.0)),
            (FontSize, n(56.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    name.tracks.insert(
        Opacity,
        vec![kf(10.0, 0.0, Easing::Linear), kf(22.0, 1.0, Easing::Linear)],
    );
    name.tracks.insert(
        X,
        vec![
            kf(10.0, 18.0, Easing::EaseOut),
            kf(24.0, 48.0, Easing::Linear),
        ],
    );

    let mut title = layer(
        "lt-title",
        "Title",
        text("{{title}}", 500),
        Some("lt-group"),
        &[
            (X, n(48.0)),
            (Y, n(95.0)),
            (Width, n(660.0)),
            (Height, n(55.0)),
            (FontSize, n(30.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    if let LayerKind::Text { uppercase, .. } = &mut title.kind {
        *uppercase = true;
    }
    title.tracks.insert(
        Opacity,
        vec![kf(18.0, 0.0, Easing::Linear), kf(30.0, 1.0, Easing::Linear)],
    );

    Template {
        revision: 0,
        marker_model: 0,
        background: None,
        zone: String::new(),
        id: "lower-third".into(),
        name: "Lower third".into(),
        duration: 125.0,
        markers: vec![
            marker(40.0, "On air", MarkerKind::Pause),
            marker(100.0, "Out", MarkerKind::Out),
        ],
        fields: vec![
            Field {
                key: "name".into(),
                label: "Name".into(),
                kind: FieldKind::Text,
                default: "Jane Doe".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
            Field {
                key: "title".into(),
                label: "Title".into(),
                kind: FieldKind::Text,
                default: "Special correspondent".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
        ],
        layers: vec![group, accent, plate, sub, name, title],
    }
}

pub fn score_bug() -> Template {
    use Prop::*;
    let n = Value::Number;
    let mut bg = layer(
        "sb-bg",
        "Background",
        LayerKind::rect(),
        None,
        &[
            (X, n(80.0)),
            (Y, n(60.0)),
            (Width, n(420.0)),
            (Height, n(56.0)),
            (Fill, hex("#0f1222f0")),
            (CornerRadius, n(10.0)),
        ],
    );
    bg.tracks.insert(
        Opacity,
        vec![
            kf(0.0, 0.0, Easing::Linear),
            kf(10.0, 1.0, Easing::Linear),
            kf(50.0, 1.0, Easing::Linear),
            kf(60.0, 0.0, Easing::Linear),
        ],
    );
    bg.tracks.insert(
        Y,
        vec![kf(0.0, 30.0, Easing::Back), kf(14.0, 60.0, Easing::Linear)],
    );
    let mk_text = |id: &str, name: &str, t: &str, x: f64, w: f64, align: TextAlign| {
        let mut l = layer(
            id,
            name,
            LayerKind::Text {
                x: Default::default(),
                wrap: false,
                transform: Default::default(),
                decoration: Default::default(),
                line_height: None,
                scroll: Default::default(),
                speed: 120.0,
                separator: "   •   ".into(),
                looping: false,
                overflow: Default::default(),
                squeeze: 0.8,
                min_size: 0.6,
                warn_below: 0.8,
                text: t.into(),
                font_family: "Inter".into(),
                font_weight: 700,
                align,
                vertical_align: VerticalAlign::Middle,
                uppercase: true,
            },
            None,
            &[
                (X, n(x)),
                (Y, n(60.0)),
                (Width, n(w)),
                (Height, n(56.0)),
                (FontSize, n(30.0)),
                (Fill, hex("#ffffff")),
            ],
        );
        l.tracks.insert(
            Opacity,
            vec![
                kf(6.0, 0.0, Easing::Linear),
                kf(16.0, 1.0, Easing::Linear),
                kf(50.0, 1.0, Easing::Linear),
                kf(58.0, 0.0, Easing::Linear),
            ],
        );
        l
    };
    let mut score = layer(
        "sb-score-bg",
        "Score plate",
        LayerKind::rect(),
        None,
        &[
            (X, n(230.0)),
            (Y, n(60.0)),
            (Width, n(120.0)),
            (Height, n(56.0)),
            (Fill, hex("#ff3b5c")),
        ],
    );
    score.tracks.insert(
        ScaleX,
        vec![
            kf(4.0, 0.0, Easing::Back),
            kf(16.0, 1.0, Easing::Linear),
            kf(50.0, 1.0, Easing::EaseIn),
            kf(58.0, 0.0, Easing::Linear),
        ],
    );
    score.props.insert(AnchorX, n(0.5));
    Template {
        revision: 0,
        marker_model: 0,
        background: None,
        zone: String::new(),
        id: "score-bug".into(),
        name: "Score bug".into(),
        duration: 60.0,
        markers: vec![
            marker(20.0, "On air", MarkerKind::Pause),
            marker(50.0, "Out", MarkerKind::Out),
        ],
        fields: vec![
            Field {
                key: "home".into(),
                label: "Home".into(),
                kind: FieldKind::Text,
                default: "PSG".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
            Field {
                key: "away".into(),
                label: "Away".into(),
                kind: FieldKind::Text,
                default: "OM".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
            Field {
                key: "homeScore".into(),
                label: "Home score".into(),
                kind: FieldKind::Number,
                default: "0".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
            Field {
                key: "awayScore".into(),
                label: "Away score".into(),
                kind: FieldKind::Number,
                default: "0".into(),
                min: None,
                max: None,
                options: Vec::new(),
            },
        ],
        layers: vec![
            bg,
            score,
            mk_text("sb-home", "Home", "{{home}}", 100.0, 120.0, TextAlign::Left),
            mk_text(
                "sb-score",
                "Score",
                "{{homeScore}} - {{awayScore}}",
                230.0,
                120.0,
                TextAlign::Center,
            ),
            mk_text(
                "sb-away",
                "Away",
                "{{away}}",
                360.0,
                120.0,
                TextAlign::Right,
            ),
        ],
    }
}

fn font(weight: u16, align: TextAlign) -> FontStyle {
    FontStyle {
        font_family: "Inter".into(),
        font_weight: weight,
        align,
        vertical_align: VerticalAlign::Middle,
        uppercase: false,
    }
}

fn fade(l: &mut Layer, in_at: f64, out_at: f64) {
    l.tracks.insert(
        Prop::Opacity,
        vec![
            kf(in_at, 0.0, Easing::Linear),
            kf(in_at + 10.0, 1.0, Easing::Linear),
            kf(out_at, 1.0, Easing::Linear),
            kf(out_at + 10.0, 0.0, Easing::Linear),
        ],
    );
}

/// Corner clock (time of day): the Clock widget, its names in the project's language.
pub fn clock() -> Template {
    use Prop::*;
    let n = Value::Number;
    let mut bg = layer(
        "ck-bg",
        "Background",
        LayerKind::rect(),
        None,
        &[
            (X, n(1640.0)),
            (Y, n(60.0)),
            (Width, n(200.0)),
            (Height, n(64.0)),
            (Fill, hex("#0f1222e6")),
            (CornerRadius, n(10.0)),
        ],
    );
    fade(&mut bg, 0.0, 30.0);
    let mut time = layer(
        "ck-time",
        "Clock",
        LayerKind::Clock {
            x: Box::new(crate::wid::timer::Config {
                timer_mode: Some(crate::wid::timer::TimerMode::Clock),
                lang: String::new(),
                ..Default::default()
            }),
            mode: ClockMode::Time,
            format: "HH:mm:ss".into(),
            timezone: String::new(),
            target: String::new(),
            font: font(700, TextAlign::Center),
        },
        None,
        &[
            (X, n(1640.0)),
            (Y, n(60.0)),
            (Width, n(200.0)),
            (Height, n(64.0)),
            (FontSize, n(36.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    fade(&mut time, 4.0, 30.0);
    Template {
        revision: 0,
        marker_model: 0,
        background: None,
        zone: String::new(),
        id: "clock".into(),
        name: "Clock".into(),
        duration: 45.0,
        markers: vec![
            marker(15.0, "On air", MarkerKind::Pause),
            marker(30.0, "Out", MarkerKind::Out),
        ],
        fields: vec![],
        layers: vec![bg, time],
    }
}

/// News ticker: one headline per line of the `items` field.
pub fn ticker() -> Template {
    use Prop::*;
    let n = Value::Number;
    let mut bar = layer(
        "tk-bar",
        "Bar",
        LayerKind::rect(),
        None,
        &[
            (X, n(0.0)),
            (Y, n(990.0)),
            (Width, n(1920.0)),
            (Height, n(60.0)),
            (Fill, hex("#0f1222f0")),
        ],
    );
    bar.tracks.insert(
        Y,
        vec![
            kf(0.0, 1080.0, Easing::EaseOut),
            kf(15.0, 990.0, Easing::Linear),
            kf(60.0, 990.0, Easing::EaseIn),
            kf(75.0, 1080.0, Easing::Linear),
        ],
    );
    let mut label = layer(
        "tk-label",
        "Label",
        LayerKind::Text {
            x: Default::default(),
            wrap: false,
            transform: Default::default(),
            decoration: Default::default(),
            line_height: None,
            scroll: Default::default(),
            speed: 120.0,
            separator: "   •   ".into(),
            looping: false,
            overflow: Default::default(),
            squeeze: 0.8,
            min_size: 0.6,
            warn_below: 0.8,
            text: "{{label}}".into(),
            font_family: "Inter".into(),
            font_weight: 800,
            align: TextAlign::Center,
            vertical_align: VerticalAlign::Middle,
            uppercase: true,
        },
        None,
        &[
            (X, n(0.0)),
            (Y, n(990.0)),
            (Width, n(220.0)),
            (Height, n(60.0)),
            (FontSize, n(26.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    label.tracks.insert(Y, bar.tracks[&Y].clone());
    let mut label_bg = layer(
        "tk-label-bg",
        "Label plate",
        LayerKind::rect(),
        None,
        &[
            (X, n(0.0)),
            (Y, n(990.0)),
            (Width, n(220.0)),
            (Height, n(60.0)),
            (Fill, hex("#ff3b5c")),
        ],
    );
    label_bg.tracks.insert(Y, bar.tracks[&Y].clone());
    let mut crawl = layer(
        "tk-crawl",
        "Crawl",
        LayerKind::Ticker {
            x: Default::default(),
            text: "{{items}}".into(),
            separator: "   •   ".into(),
            speed: 180.0,
            font: font(500, TextAlign::Left),
        },
        None,
        &[
            (X, n(240.0)),
            (Y, n(990.0)),
            (Width, n(1680.0)),
            (Height, n(60.0)),
            (FontSize, n(30.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    crawl.tracks.insert(Y, bar.tracks[&Y].clone());
    Template {
        revision: 0,
        marker_model: 0,
        background: None,
        zone: String::new(),
        id: "ticker".into(),
        name: "News ticker".into(),
        duration: 80.0,
        markers: vec![marker(20.0, "On air", MarkerKind::Pause), marker(60.0, "Out", MarkerKind::Out)],
        fields: vec![
            Field { key: "label".into(), label: "Label".into(), kind: FieldKind::Text, default: "Breaking".into(), min: None, max: None, options: Vec::new() },
            Field {
                key: "items".into(),
                label: "Headlines (one per line)".into(),
                kind: FieldKind::Multiline,
                default: "Strata renders HTML5, SDI and ST 2110 from one engine\nTickers, clocks and rolls are now built in\nOne project per show, one channel per studio".into(), min: None, max: None, options: Vec::new() },
        ],
        layers: vec![bar, label_bg, label, crawl],
    }
}

fn simple(id: &str, name: &str, duration: f64, fields: Vec<Field>, layers: Vec<Layer>) -> Template {
    Template {
        revision: 0,
        marker_model: 0,
        background: None,
        zone: String::new(),
        id: id.into(),
        name: name.into(),
        duration,
        markers: vec![
            marker(15.0, "On air", MarkerKind::Pause),
            marker(duration - 15.0, "Out", MarkerKind::Out),
        ],
        fields,
        layers,
    }
}

fn field(key: &str, label: &str, kind: FieldKind, default: &str) -> Field {
    Field {
        key: key.into(),
        label: label.into(),
        kind,
        default: default.into(),
        min: None,
        max: None,
        options: Vec::new(),
    }
}

/// A plate over the whole picture (tests of zones and spans).
pub fn full_frame() -> Template {
    use Prop::*;
    let n = Value::Number;
    let plate = layer(
        "ff-plate",
        "Plate",
        LayerKind::rect(),
        None,
        &[
            (X, n(0.0)),
            (Y, n(0.0)),
            (Width, n(1920.0)),
            (Height, n(1080.0)),
            (Fill, hex("#202840")),
        ],
    );
    simple("full-frame", "Full frame", 120.0, Vec::new(), vec![plate])
}

/// "Live in" countdown to a time of day: the Time widget, its target from the `target` field.
pub fn countdown() -> Template {
    use Prop::*;
    let n = Value::Number;
    let mut bg = layer(
        "cd-bg",
        "Plate",
        LayerKind::rect(),
        None,
        &[
            (X, n(710.0)),
            (Y, n(420.0)),
            (Width, n(500.0)),
            (Height, n(240.0)),
            (Fill, hex("#0f1222f0")),
            (CornerRadius, n(20.0)),
        ],
    );
    fade(&mut bg, 0.0, 30.0);
    let mut label = layer(
        "cd-label",
        "Label",
        text("{{label}}", 600),
        None,
        &[
            (X, n(710.0)),
            (Y, n(440.0)),
            (Width, n(500.0)),
            (Height, n(60.0)),
            (FontSize, n(30.0)),
            (Fill, hex("#ff3b5c")),
        ],
    );
    fade(&mut label, 4.0, 30.0);
    use crate::wid::timer::{CdTarget, Config, StartMode, TimerMode};
    let mut clock = layer(
        "cd-time",
        "Countdown",
        LayerKind::Clock {
            x: Box::new(Config {
                timer_mode: Some(TimerMode::Countdown),
                cd_target: CdTarget::Time,
                t_time: "20:00".into(),
                start_mode: StartMode::Auto,
                ..Default::default()
            }),
            mode: ClockMode::Countdown,
            format: "HH:mm:ss".into(),
            timezone: String::new(),
            target: String::new(),
            font: font(800, TextAlign::Center),
        },
        None,
        &[
            (X, n(710.0)),
            (Y, n(500.0)),
            (Width, n(500.0)),
            (Height, n(140.0)),
            (FontSize, n(96.0)),
            (Fill, hex("#ffffff")),
        ],
    );
    fade(&mut clock, 4.0, 30.0);
    clock
        .setting_bindings
        .insert("tTime".into(), "target".into());
    simple(
        "countdown",
        "Countdown",
        45.0,
        vec![
            field("label", "Label", FieldKind::Text, "LIVE IN"),
            field(
                "target",
                "Target (HH:mm or date-time)",
                FieldKind::Text,
                "20:00",
            ),
        ],
        vec![bg, label, clock],
    )
}

fn zone(id: &str, name: &str, layer: u32, color: &str) -> Zone {
    Zone {
        id: id.into(),
        name: name.into(),
        layer,
        color: color.into(),
        transition: ZoneTransition::Sequential,
        areas: Default::default(),
    }
}

pub fn demo_project() -> Project {
    let mut p = demo_project_layers();
    p.zones = vec![
        zone("lower-third", "Lower third", 10, "#8fa3ff"),
        zone("top-left", "Top left", 20, "#ff3b5c"),
        zone("top-right", "Top right", 30, "#22c55e"),
        zone("ticker", "Ticker", 40, "#f5a524"),
        zone("full-screen", "Full screen", 50, "#38bdf8"),
        zone("bottom-right", "Bottom right", 70, "#e879f9"),
    ];
    // Two graphics share a corner: taking one takes the other out first.
    let zone_of = |template: &str| match template {
        "lower-third" => "lower-third",
        "score-bug" | "analog-clock" => "top-left",
        "clock" => "top-right",
        "ticker" => "ticker",
        "qr-code" => "bottom-right",
        _ => "full-screen",
    };
    for t in &mut p.templates {
        t.zone = zone_of(&t.id).into();
    }
    p.ensure_zones();
    p
}

fn demo_project_layers() -> Project {
    Project {
        guides: Default::default(),
        language: "fr".into(),
        colors: Vec::new(),
        id: "demo".into(),
        name: "Demo show".into(),
        description: String::new(),
        width: 1920,
        height: 1080,
        frame_rate: FrameRate::new(50, 1),
        zones: Vec::new(),
        formats: Vec::new(),
        drafts: Vec::new(),
        templates: vec![
            lower_third(),
            score_bug(),
            clock(),
            ticker(),
            countdown(),
            full_frame(),
        ],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_pick_by_aspect_and_apply_overrides() {
        use crate::model::*;
        let mut p = super::demo_project();
        p.formats.push(Format {
            id: "vertical".into(),
            name: "9:16".into(),
            width: 1080,
            height: 1920,
        });
        assert_eq!(p.format_for(1920, 1080), ("main", 1920, 1080));
        assert_eq!(p.format_for(720, 1280), ("vertical", 1080, 1920));
        assert_eq!(p.format_for(1080, 1080).0, "main");
        let t = &mut p.templates[0];
        let id = t.layers[0].id.clone();
        t.layers[0].overrides.insert(
            "vertical".into(),
            LayerOverride {
                props: [(Prop::X, Value::Number(42.0))].into(),
                ..Default::default()
            },
        );
        let v = t.in_format("vertical");
        let l = v.layers.iter().find(|l| l.id == id).unwrap();
        assert_eq!(l.props.get(&Prop::X), Some(&Value::Number(42.0)));
        assert!(!l.tracks.contains_key(&Prop::X));
        assert!(matches!(t.in_format("main"), std::borrow::Cow::Borrowed(_)));
        // Overrides survive JSON, and are omitted when empty.
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
        assert!(!serde_json::to_string(&super::demo_project())
            .unwrap()
            .contains("overrides"));
    }

    #[test]
    fn demo_roundtrips_through_json() {
        let p = super::demo_project();
        let json = serde_json::to_string(&p).unwrap();
        let back: crate::Project = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }
}
