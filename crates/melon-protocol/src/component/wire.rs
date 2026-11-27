//! Text components and styles as the play protocol sends them: NBT since
//! 1.20.3, read through Pumpkin and decoded from its JSON form.

use anyhow::Result;
use pumpkin_protocol::ser::NetworkReadSliceExt;
use pumpkin_util::version::JavaMinecraftVersion;

use super::{Component, Style, decode};
use crate::packet::Reader;

/// The protocol version whose NBT reader the components go through.
const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

impl Reader<'_> {
    /// A text component (NBT since 1.20.3), turned into JSON by Pumpkin's reader.
    pub fn component(&mut self) -> Result<Component> {
        let tag = self
            .remaining()
            .get_nbt(&VERSION)
            .map_err(|e| anyhow::anyhow!("text component: {e}"))?;
        Ok(tag.map_or_else(Component::empty, |tag| {
            decode::component(&pumpkin_protocol::ser::nbt_tag_to_json_ext(&tag, true))
        }))
    }

    /// A text component as plain text (keys of translated parts stay keys).
    pub fn component_text(&mut self) -> Result<String> {
        Ok(self.component()?.plain_text())
    }

    /// A `Style` on its own (`Style.Serializer.TRUSTED_STREAM_CODEC`).
    pub fn style(&mut self) -> Result<Style> {
        let tag = self
            .remaining()
            .get_nbt(&VERSION)
            .map_err(|e| anyhow::anyhow!("style: {e}"))?;
        Ok(tag.map_or_else(Style::default, |tag| {
            decode::style_of(&pumpkin_protocol::ser::nbt_tag_to_json_ext(&tag, true))
        }))
    }
}
