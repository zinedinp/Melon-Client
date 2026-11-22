//! Vanilla `AbstractContainerScreen` for server-opened menus: generic 9×N

use pumpkin_data::item_stack::ItemStack;

use crate::assets::Assets;
use crate::gui::entity_preview::EntityPreview;
use crate::gui::hud::item_slot::{self};
use crate::gui::overlay::container_input::{
    self, ContainerMouse, ContainerView, MouseButtonKind, SlotClick,
};
use crate::gui::overlay::inventory::{SlotDrag, draw_carried, draw_slots, push_entity_preview};
use crate::gui::overlay::menu_layout::{MenuLayout, SLOT_PITCH, Strip};
use crate::gui::overlay::menus::{self, Frame, MenuDraw, MenuScreen, MenuSprites};
use crate::gui::overlay::recipe_book::{self, Mode, RecipeBookPanel, RecipeBookSprites};
use crate::gui::overlay::{
    ChatExit, Overlay, OverlayAction, OverlayContext, OverlayDraw, OverlayKind,
};
use crate::gui::screen::TextEvent;
use crate::gui::text::{self, StyledText};
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::settings::input::InputBind;
use crate::world::inventory::{self, ContainerKind, OpenContainer};

const IMAGE_WIDTH: f32 = 176.0;

pub struct ContainerTextures {
    pub generic_54: TextureId,
    pub hopper: TextureId,
    pub dispenser: TextureId,
    pub crafting: TextureId,
    pub furnace: TextureId,
    pub blast_furnace: TextureId,
    pub smoker: TextureId,
    pub anvil: TextureId,
    pub grindstone: TextureId,
    pub smithing: TextureId,
    pub cartography: TextureId,
    pub loom: TextureId,
    pub stonecutter: TextureId,
    pub brewing: TextureId,
    pub beacon: TextureId,
    pub enchanting: TextureId,
    pub villager: TextureId,
    pub horse: TextureId,
    pub nautilus: TextureId,
    /// What the menus draw over their panels: progress, buttons, scrollers.
    pub menus: MenuSprites,
    /// The recipe book beside the crafting table and the furnaces.
    pub recipe_book: Option<RecipeBookSprites>,
}

impl ContainerTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        Ok(Self {
            generic_54: assets.texture(renderer, "minecraft:gui/container/generic_54.png")?,
            hopper: assets.texture(renderer, "minecraft:gui/container/hopper.png")?,
            dispenser: assets.texture(renderer, "minecraft:gui/container/dispenser.png")?,
            crafting: assets.texture(renderer, "minecraft:gui/container/crafting_table.png")?,
            furnace: assets.texture(renderer, "minecraft:gui/container/furnace.png")?,
            blast_furnace: assets.texture(renderer, "minecraft:gui/container/blast_furnace.png")?,
            smoker: assets.texture(renderer, "minecraft:gui/container/smoker.png")?,
            anvil: assets.texture(renderer, "minecraft:gui/container/anvil.png")?,
            grindstone: assets.texture(renderer, "minecraft:gui/container/grindstone.png")?,
            smithing: assets.texture(renderer, "minecraft:gui/container/smithing.png")?,
            cartography: assets
                .texture(renderer, "minecraft:gui/container/cartography_table.png")?,
            loom: assets.texture(renderer, "minecraft:gui/container/loom.png")?,
            stonecutter: assets.texture(renderer, "minecraft:gui/container/stonecutter.png")?,
            brewing: assets.texture(renderer, "minecraft:gui/container/brewing_stand.png")?,
            beacon: assets.texture(renderer, "minecraft:gui/container/beacon.png")?,
            enchanting: assets.texture(renderer, "minecraft:gui/container/enchanting_table.png")?,
            villager: assets.texture(renderer, "minecraft:gui/container/villager.png")?,
            horse: assets.texture(renderer, "minecraft:gui/container/horse.png")?,
            nautilus: assets.texture(renderer, "minecraft:gui/container/nautilus.png")?,
            menus: MenuSprites::load(renderer, assets)?,
            recipe_book: RecipeBookSprites::load(renderer, assets)
                .map_err(|e| crate::warn!("Recipe book: {e:#}"))
                .ok(),
        })
    }
}

pub struct ContainerOverlay {
    kind: ContainerKind,
    layout: MenuLayout,
    /// What the menu's screen has beyond its slots, if anything.
    screen: Option<Box<dyn MenuScreen>>,
    mouse: ContainerMouse,
    cursor: (f32, f32),
    /// The recipe book of the crafting table and the furnaces.
    book: Option<RecipeBookPanel>,
}

impl ContainerOverlay {
    pub fn new(width: f32, height: f32, kind: ContainerKind) -> Self {
        let mut overlay = Self {
            kind,
            layout: menu_layout(kind),
            screen: menus::screen_for(kind),
            mouse: ContainerMouse::default(),
            cursor: (0.0, 0.0),
            book: Mode::for_kind(kind).map(|mode| RecipeBookPanel::new(mode, false)),
        };
        overlay.place(width, height);
        overlay
    }

    /// Centres the panel, or shifts it right of an open recipe book.
    fn place(&mut self, width: f32, height: f32) {
        match &self.book {
            Some(book) => {
                let left = book.menu_left(width, self.layout.size().0);
                self.layout.place_at(left, height);
            }
            None => self.layout.place(width, height),
        }
    }

    /// Where the panel and the mouse are, for the menu's screen.
    fn frame(&self) -> Frame {
        Frame {
            left: self.layout.left(),
            top: self.layout.top(),
            cursor: self.cursor,
        }
    }

    fn clicks(container: i32, clicks: Vec<SlotClick>, menu: &OpenContainer) -> OverlayAction {
        let clicks: Vec<_> = clicks
            .into_iter()
            .filter(|click| {
                click.kind != inventory::ClickType::Throw
                    || click
                        .slot
                        .is_none_or(|s| menu.slots.get(s).is_some_and(|st| !st.is_empty()))
            })
            .collect();
        if clicks.is_empty() {
            OverlayAction::None
        } else {
            OverlayAction::Clicks { container, clicks }
        }
    }
}

/// Each menu's panel and slots, in menu slot order: the container's own
pub fn menu_layout(kind: ContainerKind) -> MenuLayout {
    match kind {
        ContainerKind::Generic { rows } => {
            let rows_height = f32::from(rows) * SLOT_PITCH + 17.0;
            MenuLayout::new(IMAGE_WIDTH, 114.0 + f32::from(rows) * SLOT_PITCH)
                .strips(vec![
                    Strip {
                        y: 0.0,
                        v: 0.0,
                        height: rows_height,
                    },
                    Strip {
                        y: rows_height,
                        v: 126.0,
                        height: 96.0,
                    },
                ])
                .grid(8.0, 18.0, 9, usize::from(rows))
                .player_inventory(8.0, rows_height + 14.0)
                .inventory_label()
        }
        ContainerKind::Hopper => MenuLayout::new(IMAGE_WIDTH, 133.0)
            .grid(44.0, 20.0, 5, 1)
            .player_inventory(8.0, 51.0)
            .inventory_label(),
        ContainerKind::Dispenser => MenuLayout::new(IMAGE_WIDTH, 166.0)
            .centered_title()
            .grid(62.0, 17.0, 3, 3)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Crafting => MenuLayout::new(IMAGE_WIDTH, 166.0)
            .title_at(29.0, 6.0)
            .slot(124.0, 35.0)
            .grid(30.0, 17.0, 3, 3)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Furnace | ContainerKind::BlastFurnace | ContainerKind::Smoker => {
            standard(166.0)
                .centered_title()
                .slot(56.0, 17.0)
                .slot(56.0, 53.0)
                .slot(116.0, 35.0)
                .player_inventory(8.0, 84.0)
                .inventory_label()
        }
        ContainerKind::Anvil => standard(166.0)
            .title_at(60.0, 6.0)
            .slot(27.0, 47.0)
            .slot(76.0, 47.0)
            .slot(134.0, 47.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Grindstone => standard(166.0)
            .slot(49.0, 19.0)
            .slot(49.0, 40.0)
            .slot(129.0, 34.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Smithing => standard(166.0)
            .title_at(44.0, 15.0)
            .slot(8.0, 48.0)
            .slot(26.0, 48.0)
            .slot(44.0, 48.0)
            .slot(98.0, 48.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Cartography => standard(166.0)
            .title_at(8.0, 4.0)
            .slot(15.0, 15.0)
            .slot(15.0, 52.0)
            .slot(145.0, 39.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Loom => standard(166.0)
            .title_at(8.0, 4.0)
            .slot(13.0, 26.0)
            .slot(33.0, 26.0)
            .slot(23.0, 45.0)
            .slot(143.0, 57.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Stonecutter => standard(166.0)
            .title_at(8.0, 5.0)
            .slot(20.0, 33.0)
            .slot(143.0, 33.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Brewing => standard(166.0)
            .centered_title()
            .slot(56.0, 51.0)
            .slot(79.0, 58.0)
            .slot(102.0, 51.0)
            .slot(79.0, 17.0)
            .slot(17.0, 17.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Beacon => MenuLayout::new(230.0, 219.0)
            .without_title()
            .slot(136.0, 110.0)
            .player_inventory(36.0, 137.0),
        ContainerKind::Enchantment => standard(166.0)
            .slot(15.0, 47.0)
            .slot(35.0, 47.0)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Merchant => MenuLayout::new(276.0, 166.0)
            .on_sheet(512.0, 256.0)
            .without_title()
            .slot(136.0, 37.0)
            .slot(162.0, 37.0)
            .slot(220.0, 37.0)
            .player_inventory(108.0, 84.0)
            .inventory_label_at(107.0),
        ContainerKind::Mount { columns, .. } => standard(166.0)
            .title_at(8.0, 6.0)
            .slot(8.0, 18.0)
            .slot(8.0, 36.0)
            .grid(80.0, 18.0, usize::from(columns), 3)
            .player_inventory(8.0, 84.0)
            .inventory_label(),
        ContainerKind::Unsupported => MenuLayout::new(IMAGE_WIDTH, 80.0).strips(Vec::new()),
    }
}

/// The usual 176 wide panel `height` tall, cut whole from its sheet.
fn standard(height: f32) -> MenuLayout {
    MenuLayout::new(IMAGE_WIDTH, height)
}

/// The sheet a menu's panel is drawn from.
fn sheet(textures: &ContainerTextures, kind: ContainerKind) -> Option<TextureId> {
    match kind {
        ContainerKind::Generic { .. } => Some(textures.generic_54),
        ContainerKind::Hopper => Some(textures.hopper),
        ContainerKind::Dispenser => Some(textures.dispenser),
        ContainerKind::Crafting => Some(textures.crafting),
        ContainerKind::Furnace => Some(textures.furnace),
        ContainerKind::BlastFurnace => Some(textures.blast_furnace),
        ContainerKind::Smoker => Some(textures.smoker),
        ContainerKind::Anvil => Some(textures.anvil),
        ContainerKind::Grindstone => Some(textures.grindstone),
        ContainerKind::Smithing => Some(textures.smithing),
        ContainerKind::Cartography => Some(textures.cartography),
        ContainerKind::Loom => Some(textures.loom),
        ContainerKind::Stonecutter => Some(textures.stonecutter),
        ContainerKind::Brewing => Some(textures.brewing),
        ContainerKind::Beacon => Some(textures.beacon),
        ContainerKind::Enchantment => Some(textures.enchanting),
        ContainerKind::Merchant => Some(textures.villager),
        ContainerKind::Mount { look, .. } if look.nautilus => Some(textures.nautilus),
        ContainerKind::Mount { .. } => Some(textures.horse),
        ContainerKind::Unsupported => None,
    }
}

/// ContainerView that reads the cursor stack from the player's inventory.
struct LiveView<'a> {
    layout: &'a MenuLayout,
    menu: &'a OpenContainer,
    carried: &'a ItemStack,
}

impl ContainerView for LiveView<'_> {
    fn slot_at(&self, x: f32, y: f32) -> Option<usize> {
        self.layout
            .slot_at((x, y))
            .filter(|&slot| slot < self.menu.slots.len())
    }

    fn clicked_outside(&self, x: f32, y: f32) -> bool {
        self.layout.outside((x, y))
    }

    fn carried(&self) -> &ItemStack {
        self.carried
    }

    fn item(&self, slot: usize) -> &ItemStack {
        &self.menu.slots[slot]
    }

    fn slots(&self) -> Vec<usize> {
        (0..self.menu.slots.len().min(self.layout.len())).collect()
    }

    fn can_drag_to(&self, slot: usize, carried: &ItemStack) -> bool {
        !self.menu.kind.is_result(slot)
            && inventory::can_item_quick_replace(&self.menu.slots[slot], carried, true)
    }

    fn same_container(&self, a: usize, b: usize) -> bool {
        let split = self.menu.kind.container_slots();
        (a < split) == (b < split)
    }
}

impl ContainerOverlay {
    /// Follows the menu the server has open, should its type change.
    fn sync_kind(&mut self, kind: ContainerKind, size: (f32, f32)) {
        if kind != self.kind {
            self.kind = kind;
            self.layout = menu_layout(kind);
            self.screen = menus::screen_for(kind);
            self.book = Mode::for_kind(kind).map(|mode| RecipeBookPanel::new(mode, false));
        }
        self.place(size.0, size.1);
    }
}

impl Overlay for ContainerOverlay {
    fn kind(&self) -> OverlayKind {
        OverlayKind::Container
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.place(width, height);
    }

    /// Escape leaves the recipe book's search field or popup first.
    fn escape(&mut self) -> OverlayAction {
        if self.book.as_mut().is_some_and(RecipeBookPanel::escape) {
            OverlayAction::None
        } else {
            OverlayAction::Close(ChatExit::Intentional)
        }
    }

    fn mouse_moved(&mut self, x: f32, y: f32, dragging: bool, ctx: &mut OverlayContext) {
        self.cursor = (x, y);
        if let Some(book) = &mut self.book {
            book.mouse_moved(x, y);
        }
        if dragging && let Some(menu) = ctx.container.as_deref() {
            let view = LiveView {
                layout: &self.layout,
                menu,
                carried: ctx.inventory.carried(),
            };
            self.mouse.drag(&view, (x, y));
        }
        if dragging {
            let frame = self.frame();
            if let Some(screen) = self.screen.as_mut() {
                screen.drag(frame, ctx);
            }
        }
    }

    fn tick(&mut self, ctx: &mut OverlayContext) -> Vec<OverlayAction> {
        let mut actions = self
            .screen
            .as_mut()
            .map_or_else(Vec::new, |screen| screen.tick(ctx));
        if let Some(book) = &mut self.book {
            let stock = recipe_book::stock_of(ctx.inventory, ctx.container.as_deref(), book.mode());
            actions.extend(book.tick(ctx.recipes, stock, ctx.size, ctx.lang));
            self.place(ctx.size.0, ctx.size.1);
        }
        actions
    }

    fn scroll(&mut self, lines: f32, ctx: &mut OverlayContext) {
        let frame = self.frame();
        if let Some(screen) = self.screen.as_mut() {
            screen.wheel(frame, lines, ctx);
        }
    }

    fn mouse_button(
        &mut self,
        button: MouseButtonKind,
        pressed: bool,
        ctx: &mut OverlayContext,
    ) -> OverlayAction {
        if pressed
            && let Some(book) = &mut self.book
            && let Some(menu) = ctx.container.as_deref()
            && let Some(actions) = book.press(button, ctx.recipes, &self.layout, menu.id, ctx.shift)
        {
            self.place(ctx.size.0, ctx.size.1);
            return OverlayAction::Sequence(actions);
        }
        let frame = self.frame();
        if let Some(screen) = self.screen.as_mut() {
            if !pressed {
                screen.release();
            } else if let Some(action) = screen.press(frame, button, ctx) {
                return action;
            }
        }
        let Some(menu) = ctx.container.as_deref() else {
            return OverlayAction::None;
        };
        let view = LiveView {
            layout: &self.layout,
            menu,
            carried: ctx.inventory.carried(),
        };
        let creative = ctx.game_mode.has_infinite_materials();
        let clicks = if pressed {
            self.mouse
                .press(&view, button, self.cursor, ctx.shift, creative)
        } else {
            self.mouse.release(&view, button, self.cursor, ctx.shift)
        };
        Self::clicks(menu.id, clicks, menu)
    }

    fn key(
        &mut self,
        bind: Option<&InputBind>,
        text: &[TextEvent],
        ctx: &mut OverlayContext,
    ) -> Vec<OverlayAction> {
        if let Some(book) = &mut self.book
            && book.text(text)
        {
            return Vec::new();
        }
        if let Some(screen) = self.screen.as_mut()
            && let Some(actions) = screen.key(bind, text, ctx)
        {
            return actions;
        }
        let Some(bind) = bind else {
            return Vec::new();
        };
        if container_input::is_close_bind(bind, ctx.config) {
            return vec![OverlayAction::Close(ChatExit::Interrupted)];
        }
        let Some(menu) = ctx.container.as_deref() else {
            return Vec::new();
        };
        let carried_empty = ctx.inventory.carried().is_empty();
        let view = LiveView {
            layout: &self.layout,
            menu,
            carried: ctx.inventory.carried(),
        };
        view.slot_at(self.cursor.0, self.cursor.1)
            .and_then(|slot| {
                container_input::slot_bind(bind, ctx.config, slot, carried_empty, ctx.control)
            })
            .map(|click| Self::clicks(menu.id, vec![click], menu))
            .into_iter()
            .collect()
    }

    fn draw(&mut self, ui: &mut OverlayDraw) {
        let Some(menu) = ui.container else {
            return;
        };
        self.sync_kind(menu.kind, ui.size);
        draw_container(self, ui, menu);
    }

    /// `AbstractMountInventoryScreen.extractBackground`: the animal in the
    fn entity_preview(&self) -> Option<EntityPreview> {
        let ContainerKind::Mount { look, .. } = self.kind else {
            return None;
        };
        let (left, top) = (self.layout.left(), self.layout.top());
        Some(EntityPreview {
            x0: left + 26.0,
            y0: top + 18.0,
            x1: left + 78.0,
            y1: top + 70.0,
            size: 17.0,
            offset_y: 0.25,
            mouse: self.cursor,
            entity: Some(look.entity),
        })
    }

    fn visit_stacks(
        &self,
        inventory: &crate::world::inventory::Inventory,
        visit: &mut dyn FnMut(&ItemStack) -> bool,
    ) -> bool {
        let shown = self.screen.as_ref().map_or(&[][..], |s| s.shown_stacks());
        let book = self.book.as_ref().map_or(&[][..], |b| b.shown_stacks());
        (inventory.carried().is_empty() || visit(inventory.carried()))
            && shown
                .iter()
                .chain(book)
                .all(|stack| stack.is_empty() || visit(stack))
    }
}

fn draw_container<'a>(
    overlay: &mut ContainerOverlay,
    ui: &mut OverlayDraw<'a>,
    menu: &'a OpenContainer,
) {
    let (width, height) = ui.size;
    crate::gui::background::push_transparent_background(
        ui.draw_list,
        ui.painter.white,
        width,
        height,
    );
    if let Some(texture) = sheet(ui.textures.container, menu.kind) {
        draw_panel(ui.draw_list, texture, &overlay.layout);
    }
    let recipe_sprites = ui.textures.container.recipe_book.as_ref();
    if let (Some(book), Some(sprites)) = (&mut overlay.book, recipe_sprites) {
        book.draw(ui, sprites, ui.recipes, &overlay.layout);
    }
    let sprites: &'a MenuSprites = &ui.textures.container.menus;
    let frame = overlay.frame();
    let mut screen = overlay.screen.take();
    if let Some(screen) = screen.as_mut() {
        screen.background(&mut MenuDraw {
            ui,
            sprites,
            menu,
            frame,
        });
    }
    if let (Some(texture), Some(preview)) = (ui.entity_preview, overlay.entity_preview()) {
        push_entity_preview(ui.draw_list, texture, &preview);
    }
    draw_titles(&overlay.layout, ui, menu);
    draw_slot_grid(overlay, screen.as_deref(), sprites, ui, menu);
    if let Some(screen) = screen.as_mut() {
        screen.foreground(&mut MenuDraw {
            ui,
            sprites,
            menu,
            frame,
        });
    }
    overlay.screen = screen;
    if let Some(book) = &overlay.book
        && recipe_sprites.is_some()
        && let Some((container, display)) = ui.ghost
        && *container == menu.id
    {
        book.draw_ghost(ui, &overlay.layout, display);
    }
    draw_hand_and_tooltip(overlay, ui, menu);
    if let (Some(book), Some(sprites)) = (&overlay.book, recipe_sprites) {
        book.draw_overlay(ui, sprites);
    }
}

/// The panel's strips of its sheet (256x256 but for the villager's).
pub fn draw_panel(draw_list: &mut DrawList, texture: TextureId, layout: &MenuLayout) {
    let (width, _) = layout.size();
    let (sheet_w, sheet_h) = layout.sheet_size();
    for strip in layout.strip_list() {
        let (x, y) = (layout.left(), layout.top() + strip.y);
        draw_list.push_rect(
            texture,
            x,
            y,
            width,
            strip.height,
            [
                0.0,
                strip.v / sheet_h,
                width / sheet_w,
                (strip.v + strip.height) / sheet_h,
            ],
            [1.0; 4],
        );
    }
}

/// Vanilla `AbstractContainerScreen` label colour (`4210752`).
const LABEL_COLOR: [f32; 3] = [0x40 as f32 / 255.0; 3];

/// The container's title and "Inventory", flat like vanilla's labels. The
fn draw_titles(layout: &MenuLayout, ui: &mut OverlayDraw, menu: &OpenContainer) {
    if layout.shows_title() {
        let (x, y) = layout.title_origin(text::width(ui.painter.font, &menu.title));
        draw_label(ui, &menu.title, x, y);
    }
    if let Some((x, y)) = layout.inventory_label_origin() {
        draw_label(
            ui,
            &StyledText::plain(ui.lang.get("container.inventory")),
            x,
            y,
        );
    }
}

fn draw_label(ui: &mut OverlayDraw, text: &StyledText, x: f32, y: f32) {
    let mut paint = ui.painter.paint(LABEL_COLOR);
    paint.shadow = false;
    text::draw(ui.draw_list, &paint, text, x, y);
}

/// The slots with what the mouse drags over them.
fn draw_slot_grid(
    overlay: &ContainerOverlay,
    screen: Option<&dyn MenuScreen>,
    sprites: &MenuSprites,
    ui: &mut OverlayDraw,
    menu: &OpenContainer,
) {
    let layout = &overlay.layout;
    let hovered = layout
        .slot_at(overlay.cursor)
        .filter(|&slot| slot < menu.slots.len());
    let drag = overlay.mouse.quick_craft();
    let slots = layout.slot_origins().filter_map(|(slot, (x, y))| {
        let icon = screen.and_then(|screen| screen.empty_icon(slot, menu, sprites));
        Some((slot, x, y, menu.slots.get(slot)?, icon))
    });
    let max_in_slot = ui.inventory.carried().get_max_stack_size();
    let max_in = move |_slot| max_in_slot;
    draw_slots(
        ui.draw_list,
        ui.textures.inventory,
        ui.painter,
        slots,
        hovered.and_then(|s| layout.slot_origin(s)),
        drag.map(|quick_craft| SlotDrag {
            quick_craft,
            carried: ui.inventory.carried(),
            max_in: &max_in,
        }),
    );
}

/// The carried stack and the hovered slot's tooltip, over the screen's
/// foreground and the recipe book (`AbstractRecipeBookScreen.extractContents`).
fn draw_hand_and_tooltip(overlay: &ContainerOverlay, ui: &mut OverlayDraw, menu: &OpenContainer) {
    let hovered = overlay
        .layout
        .slot_at(overlay.cursor)
        .filter(|&slot| slot < menu.slots.len());
    let drag = overlay.mouse.quick_craft();
    draw_carried(
        ui.draw_list,
        ui.painter,
        ui.inventory,
        drag,
        overlay.cursor,
        |slot| {
            (
                menu.slots
                    .get(slot)
                    .cloned()
                    .unwrap_or_else(inventory::empty),
                ui.inventory.carried().get_max_stack_size(),
            )
        },
    );
    if ui.inventory.carried().is_empty()
        && let Some(slot) = hovered
        && !menu.slots[slot].is_empty()
    {
        let lines = item_slot::tooltip_lines(ui.lang, &menu.slots[slot]);
        ui.painter
            .tooltip_lines(ui.draw_list, &lines, overlay.cursor, ui.size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::MountLook;

    const LOOK: MountLook = MountLook {
        entity: 1,
        saddle: true,
        armor: true,
        nautilus: false,
    };

    const KINDS: [ContainerKind; 25] = [
        ContainerKind::Generic { rows: 1 },
        ContainerKind::Generic { rows: 2 },
        ContainerKind::Generic { rows: 3 },
        ContainerKind::Generic { rows: 4 },
        ContainerKind::Generic { rows: 5 },
        ContainerKind::Generic { rows: 6 },
        ContainerKind::Hopper,
        ContainerKind::Dispenser,
        ContainerKind::Crafting,
        ContainerKind::Furnace,
        ContainerKind::BlastFurnace,
        ContainerKind::Smoker,
        ContainerKind::Anvil,
        ContainerKind::Grindstone,
        ContainerKind::Smithing,
        ContainerKind::Cartography,
        ContainerKind::Loom,
        ContainerKind::Stonecutter,
        ContainerKind::Brewing,
        ContainerKind::Beacon,
        ContainerKind::Enchantment,
        ContainerKind::Merchant,
        ContainerKind::Mount {
            columns: 0,
            look: LOOK,
        },
        ContainerKind::Mount {
            columns: 5,
            look: LOOK,
        },
        ContainerKind::Unsupported,
    ];

    #[test]
    fn every_menu_has_a_slot_for_each_of_its_items() {
        for kind in KINDS {
            assert_eq!(menu_layout(kind).len(), kind.slot_count(), "{kind:?}");
        }
    }

    #[test]
    fn no_slot_reaches_past_its_panel() {
        for kind in KINDS {
            assert_eq!(
                menu_layout(kind).slots_outside_panel(),
                Vec::<usize>::new(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_player_inventory_sits_where_vanilla_adds_it() {
        let main_y = |kind: ContainerKind| {
            let layout = menu_layout(kind);
            let first = kind.container_slots();
            let (_, main) = layout.slot_position(first).unwrap();
            let (_, hotbar) = layout.slot_position(first + 27).unwrap();
            assert_eq!(hotbar, main + 58.0, "{kind:?}");
            main
        };
        assert_eq!(main_y(ContainerKind::Generic { rows: 3 }), 85.0);
        assert_eq!(main_y(ContainerKind::Generic { rows: 6 }), 139.0);
        assert_eq!(main_y(ContainerKind::Hopper), 51.0);
        assert_eq!(main_y(ContainerKind::Dispenser), 84.0);
        assert_eq!(main_y(ContainerKind::Crafting), 84.0);
        for kind in [
            ContainerKind::Furnace,
            ContainerKind::Anvil,
            ContainerKind::Grindstone,
            ContainerKind::Smithing,
            ContainerKind::Cartography,
            ContainerKind::Loom,
            ContainerKind::Stonecutter,
            ContainerKind::Brewing,
            ContainerKind::Enchantment,
        ] {
            assert_eq!(main_y(kind), 84.0, "{kind:?}");
        }
        assert_eq!(main_y(ContainerKind::Beacon), 137.0);
        assert_eq!(main_y(ContainerKind::Merchant), 84.0);
    }
}
