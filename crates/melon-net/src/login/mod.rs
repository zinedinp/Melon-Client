//! Getting into a server.

mod client_info;
mod identity;
mod join;
pub mod resolver;
pub mod world_registries;

pub use client_info::ClientInfo;
pub use client_info::write_client_information;
pub use identity::Identity;
pub use join::{JoinInfo, connect_and_join, join_server, join_stream};
pub use join::{
    SharedWriter, configure_loop, play, read_login, read_pack_pop, read_pack_push, read_spawn_info,
};
