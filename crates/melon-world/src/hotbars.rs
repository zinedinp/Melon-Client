//! Vanilla `HotbarManager`: nine saved creative hotbars in `hotbar.nbt`

use std::path::{Path, PathBuf};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

use crate::inventory::empty;

pub const HOTBARS: usize = 9;
const SLOTS: usize = 9;
/// `SharedConstants.getCurrentVersion().dataVersion()` for 26.3's world data.
pub const DATA_VERSION: i32 = 4671;

pub struct Hotbars {
    path: PathBuf,
    pub hotbars: [[ItemStack; SLOTS]; HOTBARS],
}

impl Hotbars {
    /// `hotbar.nbt` in the game folder, like `options.txt`; empty when it is missing.
    pub fn load(game_dir: &Path) -> Self {
        let path = game_dir.join("hotbar.nbt");
        let mut hotbars = Self {
            hotbars: std::array::from_fn(|_| std::array::from_fn(|_| empty())),
            path,
        };
        if let Ok(bytes) = std::fs::read(&hotbars.path) {
            let mut cursor = std::io::Cursor::new(bytes.as_slice());
            let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
            match pumpkin_nbt::Nbt::read(&mut reader) {
                Ok(nbt) => hotbars.read(&nbt.root_tag),
                Err(e) => crate::warn!("hotbar.nbt unreadable: {e}"),
            }
        }
        hotbars
    }

    fn read(&mut self, root: &NbtCompound) {
        for (i, hotbar) in self.hotbars.iter_mut().enumerate() {
            let Some(list) = root.get_list(&i.to_string()) else {
                continue;
            };
            for (slot, tag) in hotbar.iter_mut().zip(list) {
                if let NbtTag::Compound(compound) = tag {
                    *slot = ItemStack::read_item_stack(compound).unwrap_or_else(empty);
                }
            }
        }
    }

    pub fn is_empty(&self, index: usize) -> bool {
        self.hotbars[index].iter().all(ItemStack::is_empty)
    }

    /// `Hotbar.storeFrom` and `HotbarManager.save`.
    pub fn store(&mut self, index: usize, items: [ItemStack; SLOTS]) {
        self.hotbars[index] = items;
        let mut root = NbtCompound::new();
        root.put_int("DataVersion", DATA_VERSION);
        for (i, hotbar) in self.hotbars.iter().enumerate() {
            let list = hotbar
                .iter()
                .map(|stack| NbtTag::Compound(stack.to_nbt().unwrap_or_default()))
                .collect();
            root.put_list(&i.to_string(), list);
        }
        let bytes = pumpkin_nbt::Nbt::new(String::new(), root).write();
        if let Err(e) = std::fs::write(&self.path, bytes) {
            crate::warn!("hotbar.nbt not written: {e:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    #[test]
    fn saved_hotbars_read_back() {
        let dir = std::env::temp_dir().join(format!("melon-hotbars-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hotbars = Hotbars::load(&dir);
        assert!(hotbars.is_empty(3));
        let mut items: [ItemStack; SLOTS] = std::array::from_fn(|_| empty());
        items[2] = ItemStack::new(16, &Item::OAK_LOG);
        hotbars.store(3, items);
        let again = Hotbars::load(&dir);
        assert_eq!(again.hotbars[3][2].item_count, 16);
        assert_eq!(again.hotbars[3][2].item.id, Item::OAK_LOG.id);
        assert!(again.is_empty(0));
        let _ = std::fs::remove_dir_all(dir);
    }
}
