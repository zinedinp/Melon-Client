//! Minimal client for Minecraft protocol 777 (MC 1.26.3), hand-rolled: the
//! login, the play session and what it hears, the server list ping and LAN
//! worlds; the [`dialog`]s servers describe; the integrated server
//! ([`singleplayer`]); and [`join`]ing either.

pub mod dialog;
pub mod join;
pub mod lan;
pub mod login;
pub mod play;
pub use melon_protocol as protocol;
pub mod singleplayer;
pub mod status;

// The lower crates' modules, under the paths the moved code names them by.
#[cfg(test)]
use melon_assets::i18n;
use melon_assets::{assets, text};
use melon_audio as audio;
use melon_base::{auth, home, info, json_file, platform, warn};
use melon_core::{cycle, instances, probe, util};
use melon_protocol::component;
use melon_settings as settings;
use melon_world::{self as world, particle};

pub use login::{ClientInfo, Identity, JoinInfo, connect_and_join, join_server, join_stream};
pub use play::{Session, Teleport, WorldEvent};
pub use protocol::connection::Stream;

pub const PROTOCOL_VERSION: i32 = 777;
