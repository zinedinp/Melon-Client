//! Joining a world.

use std::path::PathBuf;

use anyhow::Result;

use crate::network::{self, ClientInfo, Identity, Session};
use crate::singleplayer::WorldSettings;
use crate::singleplayer::server::{self, IntegratedServer};

/// A world joined and ready to play.
pub struct Joined {
    pub session: Session,
    pub server: Option<IntegratedServer>,
}

impl Joined {
    /// Whether this is a singleplayer world on the integrated server.
    pub fn is_integrated(&self) -> bool {
        self.server.is_some()
    }
}

/// Where a join goes.
pub enum Target {
    /// A server by address, `host[:port]`.
    Server(String),
    /// A singleplayer world, run on the integrated server.
    World {
        save_path: PathBuf,
        game_dir: PathBuf,
        settings: WorldSettings,
    },
}

impl Target {
    /// `connect.connecting` for a server, `connect.joining` for a world.
    pub fn status_key(&self) -> &'static str {
        match self {
            Target::Server(_) => "connect.connecting",
            Target::World { .. } => "connect.joining",
        }
    }
}

/// Connects and logs in, telling the server about the options in `client`.
pub fn join(
    target: Target,
    identity: &Identity,
    client: &ClientInfo,
    simulation_distance: u8,
) -> Result<Joined> {
    match target {
        Target::Server(address) => {
            let session = network::join_server(&address, identity, client)?;
            if crate::probe::enabled() {
                let info = &session.info;
                crate::probe::section(
                    "Server",
                    format!(
                        "- address `{address}`\n- player `{}` entity {}\n- dimension `{}`, game mode {:?}\n- online mode {}, hardcore {}, flat {}",
                        identity.username(),
                        info.entity_id,
                        info.dimension_name,
                        info.game_mode,
                        info.online_mode,
                        info.hardcore,
                        info.is_flat
                    ),
                );
            }
            Ok(Joined {
                session,
                server: None,
            })
        }
        Target::World {
            save_path,
            game_dir,
            settings,
        } => {
            let (server, pipe) = server::start(server::Options {
                save_path,
                game_dir,
                settings,
                view_distance: crate::settings::options::RENDER_DISTANCE_RANGE.1,
                simulation_distance,
            })?;
            let session = network::join_stream(pipe, identity, client, Box::new(|| {}))?;
            Ok(Joined {
                session,
                server: Some(server),
            })
        }
    }
}
