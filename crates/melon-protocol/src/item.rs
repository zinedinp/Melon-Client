//! Item stacks on the wire.

use anyhow::{Result, bail};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer;

use crate::packet::Reader;

impl Reader<'_> {
    /// `ItemStack.OPTIONAL_STREAM_CODEC`, through Pumpkin's decoder.
    pub fn item_stack(&mut self) -> Result<ItemStack> {
        ItemStackSerializer::read(self.remaining())
            .map(ItemStackSerializer::to_stack)
            .map_err(|e| anyhow::anyhow!("item stack: {e}"))
    }

    /// `Item.STREAM_CODEC`: the item's registry number.
    pub fn item_by_id(&mut self) -> Result<&'static Item> {
        let id = self.var_int()?;
        match u16::try_from(id).ok().and_then(Item::from_id) {
            Some(item) => Ok(item),
            None => bail!("unknown item {id}"),
        }
    }

    /// `ItemStackTemplate.STREAM_CODEC`.
    pub fn item_template(&mut self) -> Result<ItemStack> {
        let item = self.var_int()?;
        let count = self.var_int()?;
        let mut head = Vec::new();
        crate::varint::write_var_int(&mut head, count)?;
        crate::varint::write_var_int(&mut head, item)?;
        let rest = *self.remaining();
        let mut joined = head.clone();
        joined.extend_from_slice(rest);
        let mut slice = joined.as_slice();
        let stack = ItemStackSerializer::read(&mut slice)
            .map(ItemStackSerializer::to_stack)
            .map_err(|e| anyhow::anyhow!("item stack template: {e}"))?;
        let used = joined.len() - slice.len() - head.len();
        self.skip(used)?;
        Ok(stack)
    }
}
