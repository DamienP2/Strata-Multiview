//! Core of the Strata Multiview engine: data model, animation evaluation and playout logic.
//! No rendering or I/O here so it can be reused by every output.

pub mod alarm;
pub mod anim;
pub mod easing;
pub mod model;
pub mod palette;
pub mod playout;
pub mod rich;
pub mod sample;
pub mod signal;
pub mod wid;
pub mod widgets;

pub use anim::{resolve_template, Data, ResolvedLayer};
pub use easing::Easing;
pub use model::*;
pub use playout::{
    ChannelState, Command, LayerClock, Leaving, Phase, PlayoutError, PlayoutItem, WidgetControl,
};
