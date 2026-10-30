//! Relations between values (the Logic canvas): a small node graph per
//! template, evaluated every frame after animation and data bindings.
//!
//! Sources (a layer property, the measured size of a text, a data field, a
//! constant) flow through operations (maths, clamp, conditions) into
//! targets that set, add to or multiply a layer property. Example: a plate
//! whose width follows the width of a name, plus a margin, with a minimum.
//!
//! The web renderer mirrors this file (`web/src/engine/logic.ts`).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::anim::Data;
use crate::model::{Color, Prop, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Logic {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

impl Logic {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Position in the editor canvas.
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMetric {
    /// Width of the longest line, in the layer's pixels.
    #[default]
    Width,
    Height,
    Lines,
    Chars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Abs,
    Round,
    Floor,
    Ceil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cmp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
    Empty,
    NotEmpty,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetMode {
    #[default]
    Set,
    Add,
    Multiply,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    /// A property of a layer (after animation, bindings and earlier links).
    LayerProp {
        layer: String,
        prop: Prop,
    },
    /// Measured size of a text layer's current content.
    TextSize {
        layer: String,
        #[serde(default)]
        metric: TextMetric,
    },
    /// A data field (number, or text for conditions).
    Field {
        key: String,
    },
    Constant {
        value: f64,
    },
    /// Inputs `a` and `b`.
    Math {
        op: MathOp,
    },
    /// Input `in`.
    Clamp {
        min: f64,
        max: f64,
    },
    /// `then` if `a` cmp `b`, else `else`.
    Condition {
        cmp: Cmp,
    },
    /// Writes input `in` into a layer property.
    Target {
        layer: String,
        prop: Prop,
        #[serde(default)]
        mode: TargetMode,
    },
    /// A text (compared with a data field).
    Text {
        #[serde(default)]
        value: String,
    },
    /// 1 when the event has happened on air (watched by the server; 0 elsewhere).
    Event {
        on: crate::model::TriggerEvent,
    },
    /// Inputs `a` and `b`: 1 when both (and) or either (or) are true.
    Bool {
        op: BoolOp,
    },
    /// On air: the graphic goes out when input `when` is true (its IN/OUT
    /// point set to « Logique »).
    Out {},
    /// On air: the pause `marker` plays on when input `when` is true.
    Resume {
        marker: String,
    },
    /// On air: fires the trigger `name` (as the API or Companion) when
    /// input `when` becomes true.
    Fire {
        name: String,
    },
    /// On air: the data set `dataset` shows its next row when input `when`
    /// becomes true.
    NextRow {
        dataset: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoolOp {
    And,
    Or,
}

/// What the graph asks of a graphic on air.
#[derive(Debug, Clone, PartialEq)]
pub enum AirAction {
    Out,
    Resume(String),
    /// Fires a trigger (`node`: the node asking, for its edge).
    Fire {
        node: String,
        name: String,
    },
    NextRow {
        node: String,
        dataset: String,
    },
}

/// Whether an event node's event has happened: (node id, event).
pub type Events<'a> = dyn FnMut(&str, &crate::model::TriggerEvent) -> bool + 'a;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Input port of `to`: a, b, in, then, else.
    pub port: String,
}

/// A value flowing through the graph.
#[derive(Debug, Clone, PartialEq)]
pub enum LVal {
    Num(f64),
    Text(String),
}

impl LVal {
    pub fn num(&self) -> f64 {
        match self {
            LVal::Num(n) => *n,
            LVal::Text(t) => t.trim().replace(',', ".").parse().unwrap_or(0.0),
        }
    }
    fn is_empty(&self) -> bool {
        match self {
            LVal::Num(n) => *n == 0.0,
            LVal::Text(t) => t.trim().is_empty(),
        }
    }
}

/// Resolved property values by layer id; updated in place by the targets.
pub type Values = HashMap<String, BTreeMap<Prop, Value>>;

/// Text measurement: (layer id, its current values) -> metric value.
pub type Measure<'a> = dyn FnMut(&str, &BTreeMap<Prop, Value>, TextMetric) -> f64 + 'a;

struct Graph<'a> {
    nodes: HashMap<&'a str, &'a Node>,
    inputs: HashMap<(&'a str, &'a str), &'a str>,
}

impl<'a> Graph<'a> {
    fn new(logic: &'a Logic) -> Self {
        Self {
            nodes: logic.nodes.iter().map(|n| (n.id.as_str(), n)).collect(),
            inputs: logic
                .edges
                .iter()
                .map(|e| ((e.to.as_str(), e.port.as_str()), e.from.as_str()))
                .collect(),
        }
    }

    /// Layers a node reads, directly or upstream (for ordering targets).
    fn reads(
        &self,
        id: &str,
        seen: &mut HashSet<String>,
        out: &mut HashSet<(String, Option<Prop>)>,
    ) {
        if !seen.insert(id.to_string()) {
            return;
        }
        let Some(n) = self.nodes.get(id) else { return };
        match &n.kind {
            NodeKind::LayerProp { layer, prop } => {
                out.insert((layer.clone(), Some(*prop)));
            }
            // A text size depends on every property of its layer (font size…).
            NodeKind::TextSize { layer, .. } => {
                out.insert((layer.clone(), None));
            }
            _ => {}
        }
        for port in ["a", "b", "in", "then", "else", "when"] {
            if let Some(from) = self.inputs.get(&(id, port)) {
                self.reads(from, seen, out);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn eval(
        &self,
        id: &str,
        values: &Values,
        data: &Data,
        measure: &mut Measure,
        events: &mut Events,
        depth: usize,
    ) -> LVal {
        let Some(n) = self.nodes.get(id) else {
            return LVal::Num(0.0);
        };
        if depth > 64 {
            return LVal::Num(0.0);
        }
        let mut input = |port: &str| -> Option<LVal> {
            self.inputs
                .get(&(id, port))
                .map(|from| self.eval(from, values, data, measure, events, depth + 1))
        };
        match &n.kind {
            NodeKind::LayerProp { layer, prop } => LVal::Num(
                values
                    .get(layer)
                    .and_then(|v| v.get(prop))
                    .and_then(Value::as_number)
                    .unwrap_or(0.0),
            ),
            NodeKind::TextSize { layer, metric } => match values.get(layer) {
                Some(v) => LVal::Num(measure(layer, v, *metric)),
                None => LVal::Num(0.0),
            },
            NodeKind::Field { key } => LVal::Text(data.get(key).cloned().unwrap_or_default()),
            NodeKind::Constant { value } => LVal::Num(*value),
            NodeKind::Math { op } => {
                let a = input("a").map(|v| v.num()).unwrap_or(0.0);
                let b = input("b").map(|v| v.num()).unwrap_or(0.0);
                LVal::Num(match op {
                    MathOp::Add => a + b,
                    MathOp::Sub => a - b,
                    MathOp::Mul => a * b,
                    MathOp::Div => {
                        if b == 0.0 {
                            0.0
                        } else {
                            a / b
                        }
                    }
                    MathOp::Min => a.min(b),
                    MathOp::Max => a.max(b),
                    MathOp::Abs => a.abs(),
                    MathOp::Round => a.round(),
                    MathOp::Floor => a.floor(),
                    MathOp::Ceil => a.ceil(),
                })
            }
            NodeKind::Clamp { min, max } => {
                let v = input("in").map(|v| v.num()).unwrap_or(0.0);
                LVal::Num(v.max(*min).min(max.max(*min)))
            }
            NodeKind::Condition { cmp } => {
                let a = input("a").unwrap_or(LVal::Num(0.0));
                let b = input("b").unwrap_or(LVal::Num(0.0));
                let numeric = |v: &LVal| {
                    matches!(v, LVal::Num(_))
                        || matches!(v, LVal::Text(t) if t.trim().replace(',', ".").parse::<f64>().is_ok())
                };
                let ok = match cmp {
                    Cmp::Empty => a.is_empty(),
                    Cmp::NotEmpty => !a.is_empty(),
                    Cmp::Eq | Cmp::Ne if !(numeric(&a) && numeric(&b)) => {
                        let s = |v: &LVal| match v {
                            LVal::Text(t) => t.trim().to_string(),
                            LVal::Num(n) => n.to_string(),
                        };
                        (s(&a) == s(&b)) == (*cmp == Cmp::Eq)
                    }
                    Cmp::Eq => a.num() == b.num(),
                    Cmp::Ne => a.num() != b.num(),
                    Cmp::Gt => a.num() > b.num(),
                    Cmp::Ge => a.num() >= b.num(),
                    Cmp::Lt => a.num() < b.num(),
                    Cmp::Le => a.num() <= b.num(),
                };
                input(if ok { "then" } else { "else" }).unwrap_or(LVal::Num(if ok {
                    1.0
                } else {
                    0.0
                }))
            }
            NodeKind::Target { .. } => input("in").unwrap_or(LVal::Num(0.0)),
            NodeKind::Text { value } => LVal::Text(value.clone()),
            NodeKind::Event { on } => LVal::Num(if events(id, on) { 1.0 } else { 0.0 }),
            NodeKind::Bool { op } => {
                let a = input("a").is_some_and(|v| truthy(&v));
                let b = input("b").is_some_and(|v| truthy(&v));
                LVal::Num(match op {
                    BoolOp::And => (a && b) as u8 as f64,
                    BoolOp::Or => (a || b) as u8 as f64,
                })
            }
            NodeKind::Out {}
            | NodeKind::Resume { .. }
            | NodeKind::Fire { .. }
            | NodeKind::NextRow { .. } => input("when").unwrap_or(LVal::Num(0.0)),
        }
    }
}

/// A value read as true: a number not 0, a text not empty, « false » or « 0 ».
fn truthy(v: &LVal) -> bool {
    match v {
        LVal::Num(n) => *n != 0.0,
        LVal::Text(t) => {
            let t = t.trim();
            !t.is_empty() && t != "0" && !t.eq_ignore_ascii_case("false")
        }
    }
}

/// What the graph's « On air » nodes ask now: out, or a pause playing on.
/// `events` says whether an event node's event has happened.
pub fn air_actions(logic: &Logic, data: &Data, events: &mut Events) -> Vec<AirAction> {
    let g = Graph::new(logic);
    let values = Values::new();
    let mut measure = |_: &str, _: &BTreeMap<Prop, Value>, _: TextMetric| 0.0;
    let mut out = Vec::new();
    for n in &logic.nodes {
        let action = match &n.kind {
            NodeKind::Out {} => AirAction::Out,
            NodeKind::Resume { marker } => AirAction::Resume(marker.clone()),
            NodeKind::Fire { name } => AirAction::Fire {
                node: n.id.clone(),
                name: name.clone(),
            },
            NodeKind::NextRow { dataset } => AirAction::NextRow {
                node: n.id.clone(),
                dataset: dataset.clone(),
            },
            _ => continue,
        };
        if g.inputs.contains_key(&(n.id.as_str(), "when"))
            && truthy(&g.eval(&n.id, &values, data, &mut measure, events, 0))
        {
            out.push(action);
        }
    }
    out
}

/// The graph has « On air » nodes (the server evaluates it on air).
pub fn drives_air(logic: &Logic) -> bool {
    logic.nodes.iter().any(|n| {
        matches!(
            n.kind,
            NodeKind::Out {}
                | NodeKind::Resume { .. }
                | NodeKind::Fire { .. }
                | NodeKind::NextRow { .. }
        )
    })
}

/// Applies the graph's targets to `values`, in dependency order (a target
/// reading what another one writes comes after it; cycles are skipped).
pub fn apply(logic: &Logic, values: &mut Values, data: &Data, measure: &mut Measure) {
    if logic.nodes.is_empty() {
        return;
    }
    let g = Graph::new(logic);
    let targets: Vec<&Node> = logic
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Target { .. }))
        .collect();
    let writes = |n: &Node| match &n.kind {
        NodeKind::Target { layer, prop, .. } => (layer.clone(), *prop),
        _ => unreachable!(),
    };
    let reads: Vec<HashSet<(String, Option<Prop>)>> = targets
        .iter()
        .map(|t| {
            let mut out = HashSet::new();
            g.reads(&t.id, &mut HashSet::new(), &mut out);
            out
        })
        .collect();
    // Kahn's algorithm over "target i must run before target j".
    let n = targets.len();
    let mut indeg = vec![0usize; n];
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let (layer, prop) = writes(targets[i]);
        for j in 0..n {
            if i != j
                && (reads[j].contains(&(layer.clone(), Some(prop)))
                    || reads[j].contains(&(layer.clone(), None)))
            {
                after[i].push(j);
                indeg[j] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(i) = ready.first().copied() {
        ready.remove(0);
        order.push(i);
        for &j in &after[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                ready.push(j);
            }
        }
    }
    for i in order {
        let t = targets[i];
        let NodeKind::Target { layer, prop, mode } = &t.kind else {
            continue;
        };
        if !g.inputs.contains_key(&(t.id.as_str(), "in")) {
            continue;
        }
        let v = g.eval(&t.id, values, data, measure, &mut |_, _| false, 0);
        let Some(layer_values) = values.get_mut(layer) else {
            continue;
        };
        if prop.is_color() {
            if let LVal::Text(s) = &v {
                if let Some(c) = Color::from_hex(s) {
                    layer_values.insert(*prop, Value::Color(c));
                }
            }
            continue;
        }
        let current = layer_values
            .get(prop)
            .and_then(Value::as_number)
            .unwrap_or(0.0);
        let x = v.num();
        let out = match mode {
            TargetMode::Set => x,
            TargetMode::Add => current + x,
            TargetMode::Multiply => current * x,
        };
        if out.is_finite() {
            layer_values.insert(*prop, Value::Number(out));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: id.into(),
            kind,
            x: 0.0,
            y: 0.0,
        }
    }
    fn edge(from: &str, to: &str, port: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            port: port.into(),
        }
    }

    #[test]
    fn plate_follows_text_width_with_condition() {
        // plate.width = max(text width + 2 × 30, 200); hidden when the name is empty.
        let logic = Logic {
            nodes: vec![
                node(
                    "tw",
                    NodeKind::TextSize {
                        layer: "name".into(),
                        metric: TextMetric::Width,
                    },
                ),
                node("pad", NodeKind::Constant { value: 60.0 }),
                node("sum", NodeKind::Math { op: MathOp::Add }),
                node("min", NodeKind::Constant { value: 200.0 }),
                node("max", NodeKind::Math { op: MathOp::Max }),
                node(
                    "w",
                    NodeKind::Target {
                        layer: "plate".into(),
                        prop: Prop::Width,
                        mode: TargetMode::Set,
                    },
                ),
                // x of a second element follows the plate's right edge.
                node(
                    "pw",
                    NodeKind::LayerProp {
                        layer: "plate".into(),
                        prop: Prop::Width,
                    },
                ),
                node(
                    "x",
                    NodeKind::Target {
                        layer: "logo".into(),
                        prop: Prop::X,
                        mode: TargetMode::Set,
                    },
                ),
                node("f", NodeKind::Field { key: "name".into() }),
                node("one", NodeKind::Constant { value: 1.0 }),
                node("zero", NodeKind::Constant { value: 0.0 }),
                node("if", NodeKind::Condition { cmp: Cmp::Empty }),
                node(
                    "op",
                    NodeKind::Target {
                        layer: "plate".into(),
                        prop: Prop::Opacity,
                        mode: TargetMode::Multiply,
                    },
                ),
            ],
            edges: vec![
                // Declared before its source on purpose: order comes from the graph.
                edge("pw", "x", "in"),
                edge("tw", "sum", "a"),
                edge("pad", "sum", "b"),
                edge("sum", "max", "a"),
                edge("min", "max", "b"),
                edge("max", "w", "in"),
                edge("f", "if", "a"),
                edge("zero", "if", "then"),
                edge("one", "if", "else"),
                edge("if", "op", "in"),
            ],
        };
        let mut nodes = logic.nodes.clone();
        nodes.rotate_left(6); // targets in any order
        let logic = Logic { nodes, ..logic };
        let run = |name: &str| {
            let mut values: Values = HashMap::new();
            for l in ["name", "plate", "logo"] {
                values.insert(
                    l.into(),
                    [
                        (Prop::Width, Value::Number(100.0)),
                        (Prop::Opacity, Value::Number(1.0)),
                    ]
                    .into(),
                );
            }
            let mut data = Data::new();
            data.insert("name".into(), name.into());
            let len = name.chars().count() as f64;
            apply(&logic, &mut values, &data, &mut |_, _, _| len * 20.0);
            values
        };
        let v = run("Ada Lovelace"); // 12 chars -> 240 px
        assert_eq!(v["plate"][&Prop::Width], Value::Number(300.0));
        assert_eq!(v["logo"][&Prop::X], Value::Number(300.0));
        assert_eq!(v["plate"][&Prop::Opacity], Value::Number(1.0));
        let v = run("Al"); // 40 + 60 < 200
        assert_eq!(v["plate"][&Prop::Width], Value::Number(200.0));
        let v = run("");
        assert_eq!(v["plate"][&Prop::Opacity], Value::Number(0.0));
    }

    #[test]
    fn json_shape() {
        let n: Node = serde_json::from_str(
            r#"{"id":"a","type":"textSize","layer":"t","metric":"height","x":10,"y":20}"#,
        )
        .unwrap();
        assert_eq!(
            n.kind,
            NodeKind::TextSize {
                layer: "t".into(),
                metric: TextMetric::Height
            }
        );
        let t: Node =
            serde_json::from_str(r#"{"id":"b","type":"target","layer":"p","prop":"width"}"#)
                .unwrap();
        assert!(matches!(
            t.kind,
            NodeKind::Target {
                mode: TargetMode::Set,
                ..
            }
        ));
    }
}
