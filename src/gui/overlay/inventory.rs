//! Vanilla `InventoryScreen` (E).

use crate::world::item_model::{KeyedStack, StackKey};
use pumpkin_data::item_stack::ItemStack;

use crate::assets::Assets;
use crate::gui::entity_preview::EntityPreview;
use crate::gui::hud::item_slot::{self, ItemPainter};
use crate::gui::overlay::container::draw_panel;
use crate::gui::overlay::container_input::{
    self, ContainerMouse, ContainerView, MouseButtonKind, QuickCraft, SlotClick,
};
use crate::gui::overlay::menu_layout::MenuLayout;
use crate::gui::overlay::recipe_book::{self, Mode, RecipeBookPanel};
use crate::gui::overlay::{
    ChatExit, Overlay, OverlayAction, OverlayContext, OverlayDraw, OverlayKind,
};
use crate::gui::screen::TextEvent;
use crate::gui::sprite::GuiSprite;
use crate::gui::text::{StyledText, colors};
use crate::i18n::Lang;
use crate::math::argb;
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::settings::input::InputBind;
use crate::world::inventory::{self, Inventory};

const IMAGE_WIDTH: f32 = 176.0;
const IMAGE_HEIGHT: f32 = 166.0;

pub struct InventoryTextures {
    pub background: TextureId,
    /// `slot_highlight_back`/`_front`: 24x24 around the hovered slot.
    pub highlight_back: GuiSprite,
    pub highlight_front: GuiSprite,
    /// Empty armor slots' outlines, head to feet, then the offhand's shield.
    pub empty_armor: [GuiSprite; 4],
    pub empty_offhand: GuiSprite,
}

impl InventoryTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let background = assets.texture(renderer, "minecraft:gui/container/inventory.png")?;
        let mut load = |id: &str| GuiSprite::load(renderer, assets, id);
        Ok(Self {
            background,
            highlight_back: load("minecraft:gui/sprites/container/slot_highlight_back.png")?,
            highlight_front: load("minecraft:gui/sprites/container/slot_highlight_front.png")?,
            empty_armor: [
                load("minecraft:gui/sprites/container/slot/helmet.png")?,
                load("minecraft:gui/sprites/container/slot/chestplate.png")?,
                load("minecraft:gui/sprites/container/slot/leggings.png")?,
                load("minecraft:gui/sprites/container/slot/boots.png")?,
            ],
            empty_offhand: load("minecraft:gui/sprites/container/slot/shield.png")?,
        })
    }

    /// `Slot.getNoItemIcon` for the player's own menu slots.
    pub fn empty_icon(&self, menu_slot: usize) -> Option<GuiSprite> {
        match menu_slot {
            5..=8 => Some(self.empty_armor[menu_slot - 5]),
            inventory::OFFHAND_SLOT => Some(self.empty_offhand),
            _ => None,
        }
    }
}

/// `InventoryMenu`'s slots in menu order.
pub fn survival_layout() -> MenuLayout {
    MenuLayout::new(IMAGE_WIDTH, IMAGE_HEIGHT)
        .title_at(97.0, 6.0)
        .slot(154.0, 28.0)
        .grid(98.0, 18.0, 2, 2)
        .grid(8.0, 8.0, 1, 4)
        .player_inventory(8.0, 84.0)
        .slot(77.0, 62.0)
}

/// Where the panel is.
pub struct InventoryLayout {
    menu: MenuLayout,
    title: String,
}

impl InventoryLayout {
    /// Top-left of slot `slot` on screen.
    fn slot_origin(&self, slot: usize) -> (f32, f32) {
        self.menu.slot_origin(slot).unwrap_or_default()
    }

    fn hovered_slot(&self, cursor: (f32, f32)) -> Option<usize> {
        self.menu.slot_at(cursor)
    }

    fn outside(&self, mx: f32, my: f32) -> bool {
        self.menu.outside((mx, my))
    }
}

/// The survival inventory as the mouse logic sees it.
pub struct SurvivalView<'a> {
    layout: &'a InventoryLayout,
    inventory: &'a Inventory,
}

/// `Slot.container` for the player's menu.
fn container_of(slot: usize) -> u8 {
    match slot {
        inventory::RESULT_SLOT => 0,
        1..=4 => 1,
        _ => 2,
    }
}

impl ContainerView for SurvivalView<'_> {
    fn slot_at(&self, x: f32, y: f32) -> Option<usize> {
        self.layout.hovered_slot((x, y))
    }

    fn clicked_outside(&self, x: f32, y: f32) -> bool {
        self.layout.outside(x, y)
    }

    fn carried(&self) -> &ItemStack {
        self.inventory.carried()
    }

    fn item(&self, slot: usize) -> &ItemStack {
        &self.inventory.slots()[slot]
    }

    fn slots(&self) -> Vec<usize> {
        (0..inventory::SLOT_COUNT).collect()
    }

    fn can_drag_to(&self, slot: usize, carried: &ItemStack) -> bool {
        inventory::can_item_quick_replace(&self.inventory.slots()[slot], carried, true)
            && inventory::may_place(slot, carried)
    }

    /// `InventoryMenu.canTakeItemForPickAll`: not from the result.
    fn can_pickup_all(&self, slot: usize) -> bool {
        slot != inventory::RESULT_SLOT
    }

    fn same_container(&self, a: usize, b: usize) -> bool {
        container_of(a) == container_of(b)
    }
}

pub struct InventoryOverlay {
    layout: InventoryLayout,
    mouse: ContainerMouse,
    cursor: (f32, f32),
    /// The recipe book of the 2x2 grid.
    book: RecipeBookPanel,
    size: (f32, f32),
}

impl InventoryOverlay {
    pub fn new(lang: &Lang, width: f32, height: f32) -> Self {
        let mut screen = Self {
            layout: InventoryLayout {
                menu: survival_layout(),
                title: lang.get("container.crafting").to_owned(),
            },
            mouse: ContainerMouse::default(),
            cursor: (0.0, 0.0),
            book: RecipeBookPanel::new(Mode::Crafting { grid: 2 }, true),
            size: (width, height),
        };
        screen.resize(width, height);
        screen
    }

    pub fn hovered_slot(&self) -> Option<usize> {
        self.layout.hovered_slot(self.cursor)
    }

    /// The clicks to send: a throw from an empty slot does nothing.
    fn clicks(clicks: Vec<SlotClick>, inventory: &Inventory) -> OverlayAction {
        let clicks: Vec<_> = clicks
            .into_iter()
            .filter(|click| {
                click.kind != inventory::ClickType::Throw
                    || click.slot.is_none_or(|s| !inventory.slots()[s].is_empty())
            })
            .collect();
        if clicks.is_empty() {
            OverlayAction::None
        } else {
            OverlayAction::Clicks {
                container: 0,
                clicks,
            }
        }
    }

    /// Draws the screen over the world, the player in its box.
    fn draw_screen(
        &self,
        draw_list: &mut DrawList,
        textures: &InventoryTextures,
        painter: ItemPainter,
        inventory: &Inventory,
        size: (f32, f32),
        player: Option<TextureId>,
    ) {
        let (width, height) = size;
        let layout = &self.layout;
        crate::gui::background::push_transparent_background(
            draw_list,
            painter.white,
            width,
            height,
        );
        draw_panel(draw_list, textures.background, &layout.menu);
        let (title_x, title_y) = layout.menu.title_origin(0.0);
        painter.font.push_text_flat(
            draw_list,
            painter.font_textures,
            &layout.title,
            title_x,
            title_y,
            argb(0xFF40_4040),
        );
        if let (Some(texture), Some(preview)) = (player, self.entity_preview()) {
            push_entity_preview(draw_list, texture, &preview);
        }

        let hovered = self.hovered_slot();
        let drag = self.mouse.quick_craft();
        let slots = (0..inventory::SLOT_COUNT).map(|slot| {
            let (x, y) = layout.slot_origin(slot);
            (
                slot,
                x,
                y,
                &inventory.slots()[slot],
                textures.empty_icon(slot),
            )
        });
        let max_in = |slot| inventory::slot_max(slot, inventory.carried());
        draw_slots(
            draw_list,
            textures,
            painter,
            slots,
            hovered.map(|s| layout.slot_origin(s)),
            drag.map(|quick_craft| SlotDrag {
                quick_craft,
                carried: inventory.carried(),
                max_in: &max_in,
            }),
        );
    }

    /// The carried stack and the hovered slot's tooltip, over the recipe
    /// book (`AbstractRecipeBookScreen.extractContents`' last stratum).
    fn draw_hand_and_tooltip(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        inventory: &Inventory,
        lang: &Lang,
        size: (f32, f32),
    ) {
        let hovered = self.hovered_slot();
        let drag = self.mouse.quick_craft();
        draw_carried(draw_list, painter, inventory, drag, self.cursor, |slot| {
            (
                ItemStack::clone(&inventory.slots()[slot]),
                inventory::slot_max(slot, inventory.carried()),
            )
        });
        if inventory.carried().is_empty()
            && let Some(slot) = hovered
            && !inventory.slots()[slot].is_empty()
        {
            let lines = item_slot::tooltip_lines(lang, &inventory.slots()[slot]);
            painter.tooltip_lines(draw_list, &lines, self.cursor, size);
        }
    }
}

impl Overlay for InventoryOverlay {
    fn kind(&self) -> OverlayKind {
        OverlayKind::Inventory
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.size = (width, height);
        let left = self.book.menu_left(width, IMAGE_WIDTH);
        self.layout.menu.place_at(left, height);
    }

    fn escape(&mut self) -> OverlayAction {
        if self.book.escape() {
            OverlayAction::None
        } else {
            OverlayAction::Close(ChatExit::Intentional)
        }
    }

    fn refresh_lang(&mut self, lang: &Lang) {
        self.layout.title = lang.get("container.crafting").to_owned();
    }

    fn mouse_moved(&mut self, x: f32, y: f32, dragging: bool, ctx: &mut OverlayContext) {
        self.cursor = (x, y);
        self.book.mouse_moved(x, y);
        if dragging {
            let view = SurvivalView {
                layout: &self.layout,
                inventory: ctx.inventory,
            };
            self.mouse.drag(&view, (x, y));
        }
    }

    fn mouse_button(
        &mut self,
        button: MouseButtonKind,
        pressed: bool,
        ctx: &mut OverlayContext,
    ) -> OverlayAction {
        if pressed
            && let Some(actions) =
                self.book
                    .press(button, ctx.recipes, &self.layout.menu, 0, ctx.shift)
        {
            self.resize(self.size.0, self.size.1);
            return OverlayAction::Sequence(actions);
        }
        let view = SurvivalView {
            layout: &self.layout,
            inventory: ctx.inventory,
        };
        let creative = ctx.game_mode.has_infinite_materials();
        let clicks = if pressed {
            self.mouse
                .press(&view, button, self.cursor, ctx.shift, creative)
        } else {
            self.mouse.release(&view, button, self.cursor, ctx.shift)
        };
        Self::clicks(clicks, ctx.inventory)
    }

    fn key(
        &mut self,
        bind: Option<&InputBind>,
        text: &[TextEvent],
        ctx: &mut OverlayContext,
    ) -> Vec<OverlayAction> {
        if self.book.text(text) {
            return Vec::new();
        }
        let Some(bind) = bind else {
            return Vec::new();
        };
        if container_input::is_close_bind(bind, ctx.config) {
            return vec![OverlayAction::Close(ChatExit::Interrupted)];
        }
        let carried_empty = ctx.inventory.carried().is_empty();
        self.hovered_slot()
            .and_then(|slot| {
                container_input::slot_bind(bind, ctx.config, slot, carried_empty, ctx.control)
            })
            .map(|click| Self::clicks(vec![click], ctx.inventory))
            .into_iter()
            .collect()
    }

    fn draw(&mut self, ui: &mut OverlayDraw) {
        self.draw_screen(
            ui.draw_list,
            ui.textures.inventory,
            ui.painter,
            ui.inventory,
            ui.size,
            ui.entity_preview,
        );
        if let Some(sprites) = ui.textures.container.recipe_book.as_ref() {
            self.book.draw(ui, sprites, ui.recipes, &self.layout.menu);
            if let Some((container, display)) = ui.ghost
                && *container == 0
            {
                self.book.draw_ghost(ui, &self.layout.menu, display);
            }
        }
        self.draw_hand_and_tooltip(ui.draw_list, ui.painter, ui.inventory, ui.lang, ui.size);
        if let Some(sprites) = ui.textures.container.recipe_book.as_ref() {
            self.book.draw_overlay(ui, sprites);
        }
    }

    fn tick(&mut self, ctx: &mut OverlayContext) -> Vec<OverlayAction> {
        let stock = recipe_book::stock_of(ctx.inventory, None, self.book.mode());
        let actions = self.book.tick(ctx.recipes, stock, ctx.size, ctx.lang);
        self.resize(ctx.size.0, ctx.size.1);
        actions
    }

    /// `InventoryScreen.extractBackground`.
    fn entity_preview(&self) -> Option<EntityPreview> {
        let (left, top) = (self.layout.menu.left(), self.layout.menu.top());
        Some(EntityPreview {
            x0: left + 26.0,
            y0: top + 8.0,
            x1: left + 75.0,
            y1: top + 78.0,
            size: 30.0,
            offset_y: 0.0625,
            mouse: self.cursor,
            entity: None,
        })
    }

    fn effects_panel(&self) -> Option<crate::gui::hud::effects::EffectsPanel> {
        Some(crate::gui::hud::effects::EffectsPanel {
            left: self.layout.menu.left(),
            top: self.layout.menu.top(),
            width: IMAGE_WIDTH,
            mouse: self.cursor,
        })
    }

    fn visit_stacks(
        &self,
        inventory: &Inventory,
        visit: &mut dyn FnMut(&dyn StackKey) -> bool,
    ) -> bool {
        inventory
            .slots()
            .iter()
            .chain([inventory.carried()])
            .chain(self.book.shown_stacks())
            .all(|stack| stack.is_empty() || visit(stack))
    }
}

/// The texture an [`EntityPreview`] was drawn into, over its box.
pub fn push_entity_preview(draw_list: &mut DrawList, texture: TextureId, preview: &EntityPreview) {
    draw_list.push_rect(
        texture,
        preview.x0,
        preview.y0,
        preview.x1 - preview.x0,
        preview.y1 - preview.y0,
        [0.0, 0.0, 1.0, 1.0],
        [1.0; 4],
    );
}

/// A drag over slots in progress, for previewing what it leaves in each.
pub struct SlotDrag<'a> {
    pub quick_craft: &'a QuickCraft,
    pub carried: &'a ItemStack,
    /// How many of `carried` a slot (by this screen's index) takes.
    pub max_in: &'a dyn Fn(usize) -> u8,
}

/// `AbstractContainerScreen.extractSlots`.
pub fn draw_slots<'a>(
    draw_list: &mut DrawList,
    textures: &InventoryTextures,
    painter: ItemPainter,
    slots: impl Iterator<Item = (usize, f32, f32, &'a KeyedStack, Option<GuiSprite>)>,
    hovered: Option<(f32, f32)>,
    drag: Option<SlotDrag>,
) {
    if let Some((x, y)) = hovered {
        textures
            .highlight_back
            .draw(draw_list, x - 4.0, y - 4.0, 24.0, 24.0);
    }
    for (slot, x, y, stack, empty_icon) in slots {
        let preview = drag.as_ref().and_then(|drag| {
            container_input::drag_preview(
                drag.quick_craft,
                slot,
                stack,
                drag.carried,
                (drag.max_in)(slot),
            )
        });
        match preview {
            Some((preview, capped)) => {
                draw_list.push_fill(painter.white, x, y, 16.0, 16.0, argb(0x80FF_FFFF));
                let count = capped
                    .then(|| StyledText::colored(&preview.item_count.to_string(), colors::YELLOW));
                painter.stack_with_count(draw_list, &preview, x, y, count);
            }
            None if stack.is_empty() => {
                if let Some(icon) = empty_icon {
                    icon.draw(draw_list, x, y, 16.0, 16.0);
                }
            }
            None => painter.stack(draw_list, stack, x, y),
        }
    }
    if let Some((x, y)) = hovered {
        textures
            .highlight_front
            .draw(draw_list, x - 4.0, y - 4.0, 24.0, 24.0);
    }
}

/// `extractCarriedItem`.
pub fn draw_carried(
    draw_list: &mut DrawList,
    painter: ItemPainter,
    inventory: &Inventory,
    drag: Option<&QuickCraft>,
    (mx, my): (f32, f32),
    slot_item: impl Fn(usize) -> (ItemStack, u8),
) {
    let carried = inventory.carried();
    if carried.is_empty() {
        return;
    }
    match drag.filter(|d| d.slots.len() > 1) {
        Some(drag) => {
            let remaining = container_input::drag_remainder(drag, carried, slot_item);
            if remaining > 0 {
                let shown = carried.copy_with_count(remaining as u8);
                painter.stack(draw_list, &shown, mx - 8.0, my - 8.0);
            }
        }
        None => painter.stack(draw_list, carried, mx - 8.0, my - 8.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_survival_inventory_has_every_menu_slot_inside_its_panel() {
        let layout = survival_layout();
        assert_eq!(layout.len(), inventory::SLOT_COUNT);
        assert!(layout.slots_outside_panel().is_empty());
        assert_eq!(
            layout.slot_position(inventory::RESULT_SLOT),
            Some((154.0, 28.0))
        );
        assert_eq!(layout.slot_position(4), Some((116.0, 36.0)));
        assert_eq!(layout.slot_position(8), Some((8.0, 62.0)));
        assert_eq!(layout.slot_position(9), Some((8.0, 84.0)));
        assert_eq!(layout.slot_position(44), Some((152.0, 142.0)));
        assert_eq!(
            layout.slot_position(inventory::OFFHAND_SLOT),
            Some((77.0, 62.0))
        );
    }
}
