//! `ClientInformation`: the settings the server is told about.

use anyhow::Result;
use melon_protocol::{packet, varint};
use melon_world::entity::player::{Arm, ShownParts};
use std::io::Write;

/// The options the server is told about (vanilla `ClientInformation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    /// `Options.languageCode`, e.g. `en_us`.
    pub language: String,
    /// `Options.renderDistance`, in chunks: how far the server sends them.
    pub view_distance: u8,
    /// `ChatVisiblity` id: 0 everything, 1 commands only, 2 hidden.
    pub chat_visibility: u8,
    /// `Options.chatColors`.
    pub chat_colors: bool,
    /// `Options.modelParts`: what of the skin others see.
    pub model_parts: ShownParts,
    pub main_hand: Arm,
    /// `ParticleStatus` id: 0 all, 1 decreased, 2 minimal.
    pub particle_status: u8,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            language: "en_us".to_owned(),
            view_distance: 12,
            chat_visibility: 0,
            chat_colors: true,
            model_parts: ShownParts::default(),
            main_hand: Arm::Right,
            particle_status: 0,
        }
    }
}

impl ClientInfo {
    /// What the server is told about these options.
    pub fn from_config(config: &melon_settings::options::Config) -> Self {
        Self {
            language: config.lang.code().to_owned(),
            view_distance: config.render_distance,
            chat_visibility: config.chat.visibility as u8,
            chat_colors: config.chat.colors,
            model_parts: config.model_parts,
            main_hand: config.main_hand,
            particle_status: config.particles.id(),
        }
    }
}

/// `ClientInformation`'s fields, the same in configuration and play.
pub fn write_client_information(buf: &mut Vec<u8>, info: &ClientInfo) -> Result<()> {
    packet::write_string(buf, &info.language)?;
    buf.write_all(&[info.view_distance])?;
    varint::write_var_int(buf, i32::from(info.chat_visibility))?;
    packet::write_bool(buf, info.chat_colors)?;
    buf.write_all(&[info.model_parts.mask()])?;
    varint::write_var_int(buf, info.main_hand.network_id())?;
    packet::write_bool(buf, false)?;
    packet::write_bool(buf, true)?;
    Ok(varint::write_var_int(buf, i32::from(info.particle_status))?)
}
