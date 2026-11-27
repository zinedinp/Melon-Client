//! The client: the game loop ([`game`]) and the app around it ([`app`]),
//! over the crates in `crates/`. Their modules are re-exported here under the
//! paths the client has always named them by.
//!
//! Each crate uses only ones below it in this list:
//!
//! ```text
//! melon (game, app)
//! melon-gui         widgets, screens, HUD, fonts
//! melon-render      the Vulkan renderer
//! melon-net         login, play session, dialogs, the integrated server
//! melon-settings    options.txt, key binds, server list
//! melon-world       chunks, models, entities, particles, their meshes
//! melon-audio       sound events, music, positional playback
//! melon-assets      assets tree, resource packs, languages, styled text
//! melon-protocol    the wire, text components
//! melon-core        math, utilities, instances, the tester's report
//! melon-base        what the launcher shares: accounts, folders, 2D Vulkan
//! ```

pub use melon_assets::{assets, i18n, text};
pub use melon_audio as audio;
pub use melon_base::{auth, home, info, json_file, platform, prepare, warn};
pub use melon_core::{cycle, instances, math, probe, util};
pub use melon_gui as gui;
pub use melon_net::{self as network, dialog, join, singleplayer};
pub use melon_protocol::component;
pub use melon_render as renderer;
pub use melon_settings as settings;
pub use melon_world::{self as world, particle};

pub mod app;
pub mod game;
