//! Web page: a web address or some HTML shown in the graphic. Only a
//! browser can draw it: the HTML5 outputs and the previews show it
//! (`web/src/widgets/web.tsx`); the engine's own outputs (SDI, NDI, ST 2110)
//! leave its box empty.

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// A web address (`{{key}}` allowed).
    #[default]
    Url,
    /// HTML code (an iframe's `srcdoc`).
    Html,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub source: Source,
    pub url: String,
    pub html: String,
    /// Loaded again every N seconds (0 = never), on the same wall-clock
    /// beats (server time) in every browser; the new page loads hidden and
    /// replaces the shown one once loaded (no blank frame).
    pub refresh: f64,
    /// Page zoom, %.
    pub zoom: f32,
    /// The page's own background (on white); off: the page is made
    /// transparent (HTML code, or through the server: its root and body
    /// backgrounds are cleared).
    pub opaque: bool,
    /// Fetched by the server, for the many sites that refuse to be shown
    /// in a frame (cg-server/src/webproxy.rs).
    pub proxy: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: Source::Url,
            url: "https://example.com".into(),
            html: String::new(),
            refresh: 0.0,
            zoom: 100.0,
            opaque: false,
            proxy: true,
        }
    }
}

/// When the operator last asked for a reload (Unix ms, 0 = never).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct State {
    pub reloaded_at: i64,
}

/// Operator action: `reload`.
pub fn act(
    _layer: &Layer,
    st: &mut State,
    op: &str,
    _args: &serde_json::Value,
    ctx: &ActCtx,
) -> Result<(), String> {
    match op {
        "reload" => {
            st.reloaded_at = ctx.now_ms;
            Ok(())
        }
        _ => Err(format!("unknown action `{op}`")),
    }
}
