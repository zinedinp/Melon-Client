//! The world as the client knows it: chunks, blocks and their models, entities,
//! inventories, recipes, the player, and the meshes it all becomes, in the
//! formats the renderer draws ([`mesh`]); and its [`particle`]s.
//!
//! It draws nothing itself: the renderer reads what is here.

pub mod atmosphere;
pub mod biome;
pub mod biome_zoom;
pub mod block;
pub mod block_entity;
pub mod block_models;
pub mod block_motion;
pub mod block_placement;
pub mod block_tint;
pub mod border;
pub mod chunk;
pub mod clock;
pub mod clouds;
pub mod colormap;
pub mod creative_tabs;
pub mod culling;
pub mod dimension;
pub mod entity;
pub mod fluid;
pub mod fog;
pub mod game_mode;
pub mod geometry;
pub mod glint;
pub mod head_profile;
pub mod hotbars;
pub mod inventory;
pub mod item_atlas;
pub mod item_model;
pub mod java_random;
pub mod json_model;
pub mod level;
pub mod level_settings;
pub mod lightmap;
pub mod map_color;
pub mod mesh;
pub mod model_part;
pub mod nibble_decode;
pub mod particle;
pub mod physics;
pub mod pick;
pub mod player;
pub mod projection;
pub mod recipes;
pub mod scoreboard;
pub mod section;
pub mod section_decode;
pub mod shadow;
pub mod sky;
pub mod smooth_light;
pub mod terrain;
pub mod texture_animation;
pub mod transparency;
pub mod weather;
#[cfg(target_arch = "x86_64")]
pub mod x86;

// The lower crates' modules, under the paths the moved code names them by.
#[cfg(test)]
use melon_assets::i18n;
use melon_assets::{assets, text};
use melon_base::{platform, warn};
use melon_core::{cycle, math, probe, util};
use melon_protocol::component;
