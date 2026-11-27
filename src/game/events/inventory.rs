//! The inventory, server-opened menus and the recipe book.
//!
//! Vanilla's inventory menu and an open menu share the player's slots; here
//! the open menu holds a copy, kept in step both ways.

use pumpkin_data::item_stack::ItemStack;

use melon_protocol::component::Component;
use crate::game::InWorld;
use melon_gui::overlay::OverlayKind;
use melon_gui::text::StyledText;
use melon_assets::i18n::Lang;
use melon_net::play::game::recipes::{FLAG_HIGHLIGHT, RecipeEvent};
use melon_world::entity::kind::EntityKind;
use melon_world::inventory::merchant::Offers;
use melon_world::inventory::{ContainerKind, MountLook, OpenContainer, empty};

/// The open menu, if `id` is its id.
fn menu_with(open: &mut Option<OpenContainer>, id: i32) -> Option<&mut OpenContainer> {
    open.as_mut().filter(|menu| menu.id == id)
}

impl InWorld {
    /// `handleContainerContent`.
    pub(super) fn set_container_content(
        &mut self,
        container: i32,
        state_id: i32,
        slots: Vec<ItemStack>,
        carried: ItemStack,
    ) {
        if container == 0 {
            // `carried` is the inventory menu's, out of sight while another
            // menu (or the creative one, which keeps its own) is open.
            let keep_cursor = self.menus.open_container.is_some()
                || self.game_mode.current.has_infinite_materials();
            let cursor = keep_cursor.then(|| self.inventory.carried().clone());
            self.inventory.set_all(state_id, slots, carried);
            if let Some(cursor) = cursor {
                self.inventory.set_carried(ItemStack::clone(&cursor));
            }
            self.sync_open_menu();
        } else if let Some(menu) = menu_with(&mut self.menus.open_container, container) {
            menu.set_all(state_id, slots);
            menu.copy_player_into(&mut self.inventory);
            self.inventory.set_carried(carried);
        }
    }

    /// `handleContainerSetSlot`.
    pub(super) fn set_container_slot(
        &mut self,
        container: i32,
        state_id: i32,
        slot: i16,
        item: ItemStack,
    ) {
        match container {
            0 => {
                self.inventory.set_slot(state_id, slot, item);
                self.sync_open_menu();
            }
            // How servers set the cursor before `ClientboundSetCursorItemPacket`;
            // vanilla drops it now, kept for servers that still send it.
            -1 if slot == -1 => self.inventory.set_carried(item),
            id => {
                if let Some(menu) = menu_with(&mut self.menus.open_container, id) {
                    menu.set_slot(state_id, slot, item);
                    menu.copy_player_into(&mut self.inventory);
                }
            }
        }
    }

    /// `handleContainerSetData`.
    pub(super) fn set_container_data(&mut self, container: i32, id: i16, value: i16) {
        if let Some(menu) = menu_with(&mut self.menus.open_container, container)
            && let Ok(id) = usize::try_from(id)
        {
            menu.set_data(id, value);
        }
    }

    /// `handleMerchantOffers`.
    pub(super) fn set_merchant_offers(&mut self, container: i32, offers: Offers) {
        if let Some(menu) = menu_with(&mut self.menus.open_container, container) {
            menu.trades = Some(offers);
        }
    }

    /// `handleSetCursorItem`: not while the creative inventory, which holds
    /// its own cursor, is open.
    pub(super) fn set_cursor_item(&mut self, item: ItemStack) {
        if self.overlay_kind() != Some(OverlayKind::Creative) {
            self.inventory.set_carried(item);
        }
    }

    /// `handleSetPlayerInventory`, by `Inventory` index.
    pub(super) fn set_player_inventory_slot(&mut self, slot: i32, item: ItemStack) {
        self.inventory.set_inventory_slot(slot, item);
        self.sync_open_menu();
    }

    /// `handleSetHeldSlot`: hotbar slots only.
    pub(super) fn set_held_slot(&mut self, slot: i32) {
        if let Ok(slot @ 0..9) = u8::try_from(slot) {
            self.inventory.select(slot);
        }
    }

    /// `handleOpenScreen`.
    pub(super) fn open_screen(
        &mut self,
        container: i32,
        window_type: i32,
        title: &Component,
        lang: &Lang,
    ) {
        let kind = ContainerKind::from_window_type(window_type);
        let title = StyledText::from_component(title, lang);
        self.open_menu(OpenContainer::new(container, kind, title));
    }

    /// `handleMountScreenOpen`: only for an `AbstractHorse` or an
    /// `AbstractNautilus` the client knows.
    pub(super) fn open_mount_screen(
        &mut self,
        container: i32,
        columns: i32,
        entity_id: i32,
        lang: &Lang,
    ) {
        let Some(mount) = self.entities.get(entity_id) else {
            return;
        };
        let Some((saddle, armor)) = mount.kind.mount_slots() else {
            return;
        };
        let title = match &mount.custom_name {
            Some(name) => StyledText::from_component(name, lang),
            None => StyledText::plain(
                lang.get_dynamic(&format!("entity.minecraft.{}", mount.ty.resource_name)),
            ),
        };
        let kind = ContainerKind::Mount {
            columns: columns.clamp(0, 9) as u8,
            look: MountLook {
                entity: entity_id,
                saddle,
                armor,
                nautilus: matches!(
                    mount.kind,
                    EntityKind::Nautilus | EntityKind::ZombieNautilus
                ),
            },
        };
        self.open_menu(OpenContainer::new(container, kind, title));
    }

    /// `handleContainerClose`: `clientSideCloseContainer`, whatever its id.
    pub(super) fn close_container(&mut self) {
        if self.menus.open_container.take().is_some() {
            self.inventory.set_carried(empty());
        }
        self.menus.pending_container_overlay = false;
        self.menus.pending_container_close = true;
    }

    /// Replaces the open menu, which drops its cursor.
    fn open_menu(&mut self, mut menu: OpenContainer) {
        if self.menus.open_container.is_some() {
            self.inventory.set_carried(empty());
        }
        menu.copy_player_from(&self.inventory);
        self.menus.open_container = Some(menu);
        self.menus.pending_container_close = false;
        self.menus.pending_container_overlay = true;
    }

    /// The open menu's player slots after the inventory changed.
    fn sync_open_menu(&mut self) {
        if let Some(menu) = &mut self.menus.open_container {
            menu.copy_player_from(&self.inventory);
        }
    }

    /// `ClientPacketListener`'s handlers for the recipe packets.
    pub(super) fn apply_recipes(&mut self, event: RecipeEvent) {
        match event {
            RecipeEvent::Update(sets) => self.recipe_sets = sets,
            RecipeEvent::BookAdd { entries, replace } => {
                if replace {
                    self.recipes.clear();
                }
                for (entry, flags) in entries {
                    if flags & FLAG_HIGHLIGHT != 0 {
                        self.recipes.add_highlight(entry.id);
                    }
                    self.recipes.add(entry);
                }
                self.recipes.rebuild();
            }
            RecipeEvent::BookRemove(ids) => {
                for id in ids {
                    self.recipes.remove(id);
                }
                self.recipes.rebuild();
            }
            RecipeEvent::BookSettings(settings) => self.recipes.set_settings(settings),
            RecipeEvent::Ghost { container, display } => {
                self.menus.ghost_recipe = Some((container, display));
            }
        }
    }
}
