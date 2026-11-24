//! What the Melon client and the Melon launcher share. Nothing here knows the game.

pub mod auth;
pub mod gfx;
pub mod home;
pub mod i18n;
pub mod json_file;
pub mod platform;
pub mod prepare;
pub mod sysinfo;
pub mod util;

/// Held by each test that changes environment variables: tests run in parallel.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
