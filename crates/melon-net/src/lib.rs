//! Minimal client for Minecraft protocol 777 (MC 1.26.3), hand-rolled: the
//! login, the play session and what it hears, the server list ping and LAN
//! worlds; the [`dialog`]s servers describe; the integrated server
//! ([`singleplayer`]); and [`join`]ing either.

pub mod dialog;
pub mod join;
pub mod lan;
pub mod login;
pub mod play;
pub mod singleplayer;
pub mod status;

pub use login::{ClientInfo, Identity, JoinInfo, connect_and_join, join_server, join_stream};
pub use melon_protocol::connection::Stream;
pub use play::{Session, Teleport, WorldEvent};

pub const PROTOCOL_VERSION: i32 = 777;
