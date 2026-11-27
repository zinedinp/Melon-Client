//! UI layer: widgets, screens, text. Emits `renderer::quad::DrawList` only;

pub mod assets;
pub mod background;
pub mod effects;
pub mod entity_preview;
pub mod face;
pub mod hud;
pub mod ime;
pub mod input_state;
pub mod overlay;
pub mod panorama;
pub mod screen;
pub mod screens;
pub mod sprite;
pub mod text;
pub mod widgets;

// The lower crates' modules, under the paths the moved code names them by.
use melon_assets::i18n;
use melon_audio as audio;
use melon_base::{platform, warn};
use melon_core::{cycle, instances, math, probe};
use melon_net::{self as network, dialog, join, singleplayer};
use melon_protocol::component;
use melon_render as renderer;
use melon_settings as settings;
use melon_world as world;
