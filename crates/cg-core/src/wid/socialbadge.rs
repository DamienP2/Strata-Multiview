//! Social badge: a platform's badge (a rounded square in its colour with a
//! simple glyph, not the brand's logo) or a person's avatar (their picture,
//! or their initials on a colour from their name), for the message card
//! templates (`{{platform}}`, `{{avatar}}`, `{{name}}` bound to the social
//! source's `onAir`). Drawn as the social wall draws them
//! (`cg-render/src/wid/socialbadge.rs`, `web/src/widgets/socialbadge.tsx`).

use serde::{Deserialize, Serialize};

use super::ActCtx;
use crate::model::Layer;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Show {
    #[default]
    Platform,
    Avatar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub show: Show,
    /// youtube, twitch, facebook, instagram, x, tiktok, whatsapp, sms (`{{key}}` allowed).
    pub platform: String,
    /// Avatar: the picture (web address or asset; empty = the initials).
    pub avatar: String,
    /// Avatar: the name its initials and colour come from (`author`: the
    /// layer has its own `name`).
    pub author: String,
    /// Font of the initials (and of an unknown platform's letter).
    pub font: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show: Show::Platform,
            platform: "youtube".into(),
            avatar: String::new(),
            author: "Jean-Marc Ollivier".into(),
            font: "Inter".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {}

/// No operator action.
pub fn act(
    _layer: &Layer,
    _st: &mut State,
    op: &str,
    _args: &serde_json::Value,
    _ctx: &ActCtx,
) -> Result<(), String> {
    Err(format!("unknown action `{op}`"))
}
