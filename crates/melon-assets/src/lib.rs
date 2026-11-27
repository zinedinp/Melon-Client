//! What the client reads from disk before it draws or plays anything.
//!
//! - [`assets`]: the vanilla `assets/` tree, resource packs (local, server
//!   and mod ones) and their core shaders.
//! - [`i18n`]: the languages, from the packs' `lang/` files.
//! - [`text`]: components resolved against a language, as styled runs.

pub mod assets;
pub mod i18n;
pub mod text;

// The lower crates' modules, under the paths the moved code names them by.
use melon_base::{home, warn};
use melon_core::{instances, probe};
use melon_protocol::component;
