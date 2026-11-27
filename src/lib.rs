//! The client: the game loop ([`game`]) and the app around it ([`app`]),
//! over the crates in `crates/`.
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

pub mod app;
pub mod game;
