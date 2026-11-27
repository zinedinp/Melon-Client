//! Minimal client for Minecraft protocol 777 (MC 1.26.3), hand-rolled.

//!

pub mod lan;
pub mod login;
pub mod play;
pub(crate) use melon_protocol as protocol;
pub mod status;

pub use login::{ClientInfo, Identity, JoinInfo, connect_and_join, join_server, join_stream};
pub use play::{Session, Teleport, WorldEvent};
pub use protocol::connection::Stream;

pub const PROTOCOL_VERSION: i32 = 777;
