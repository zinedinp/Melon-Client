//! Drawing the creative inventory.

use super::{
    COLUMNS, CreativeOverlay, CreativeTextures, IMAGE_HEIGHT, IMAGE_WIDTH, Slot, TAB_HEIGHT,
    TAB_WIDTH,
};
use crate::hud::item_slot::{self, ItemPainter};
use melon_assets::i18n::Lang;
use melon_core::math::argb;
use crate::overlay::container_input::ContainerView;
use crate::overlay::inventory::{SlotDrag, draw_carried, draw_slots};
use crate::overlay::{Overlay, OverlayDraw};
use melon_render::quad::DrawList;
use crate::text::font::Glyphs;
use crate::text::{StyledText, colors};
use melon_world::creative_tabs::{CreativeTab, Row, TabType, tabs};
use pumpkin_data::item_stack::ItemStack;

impl CreativeOverlay {
    /// Draws the screen over the world.
    pub(super) fn draw_screen(&self, ui: &mut OverlayDraw) {
        let draw_list = &mut *ui.draw_list;
        let (textures, slot_textures) = (ui.textures.creative, ui.textures.inventory);
        let (painter, inventory, lang) = (ui.painter, ui.inventory, ui.lang);
        let (size, player) = (ui.size, ui.entity_preview);
        let (width, height) = size;
        let (left, top) = (self.left, self.top);
        crate::background::push_transparent_background(draw_list, painter.white, width, height);
        for (i, &tab) in self.tabs.iter().enumerate() {
            if i != self.selected {
                self.draw_tab(draw_list, textures, painter, &tabs()[tab], false);
            }
        }
        let background = match self.tab().background.as_str() {
            "tab_item_search" => textures.search,
            "tab_inventory" => textures.inventory,
            _ => textures.items,
        };
        draw_list.push_rect(
            background,
            [left, top, IMAGE_WIDTH, IMAGE_HEIGHT],
            [0.0, 0.0, IMAGE_WIDTH / 256.0, IMAGE_HEIGHT / 256.0],
            [1.0; 4],
        );
        if self.tab().scrollbar {
            let sprite = if self.can_scroll() {
                textures.scroller
            } else {
                textures.scroller_disabled
            };
            let y = top + 18.0 + ((112.0 - 17.0) * self.scroll).floor();
            sprite.draw(draw_list, left + 175.0, y, 12.0, 15.0);
        }
        self.draw_tab(draw_list, textures, painter, self.tab(), true);
        if let (Some(texture), Some(preview)) = (player, self.entity_preview()) {
            crate::overlay::inventory::push_entity_preview(draw_list, texture, &preview);
        }
        if self.tab_type() == TabType::Search {
            self.draw_search(draw_list, painter.glyphs());
        }
        if self.tab().show_title {
            painter.font.push_text_flat(
                draw_list,
                painter.font_textures,
                lang.get_dynamic(&self.tab().title),
                left + 8.0,
                top + 6.0,
                argb(0xFF40_4040),
            );
        }

        let hovered = self.hovered_slot(self.cursor);
        let positioned = self.slots().map(|(i, slot, (x, y))| {
            let (stack, icon) = match slot {
                Slot::Picker(p) => (self.picker(p).0, None),
                Slot::Player(menu) => (&inventory.slots()[menu], slot_textures.empty_icon(menu)),
                Slot::Destroy => (&self.empty, None),
            };
            (i, left + x, top + y, stack, icon)
        });
        let hovered_pos = hovered.and_then(|h| {
            self.slots()
                .find(|(i, _, _)| *i == h)
                .map(|(_, _, (x, y))| (left + x, top + y))
        });
        let view = self.view(inventory);
        let max_in = |slot| view.max_in(slot);
        draw_slots(
            draw_list,
            slot_textures,
            painter,
            positioned,
            hovered_pos,
            self.mouse.quick_craft().map(|quick_craft| SlotDrag {
                quick_craft,
                carried: inventory.carried(),
                max_in: &max_in,
            }),
        );
        draw_carried(
            draw_list,
            painter,
            inventory,
            self.mouse.quick_craft(),
            self.cursor,
            |slot| (view.item(slot).clone(), view.max_in(slot)),
        );

        if !inventory.carried().is_empty() {
            return;
        }
        if let Some(index) = hovered {
            match self.slot_kind(index) {
                Some(Slot::Destroy) => {
                    painter.tooltip(draw_list, lang.get("inventory.binSlot"), self.cursor, size);
                }
                Some(slot) => {
                    let (stack, locked) = match slot {
                        Slot::Picker(p) => self.picker(p),
                        Slot::Player(menu) => (&inventory.slots()[menu], false),
                        Slot::Destroy => (&self.empty, false),
                    };
                    if locked {
                        let column = index % COLUMNS + 1;
                        let text = lang.format("inventory.hotbarInfo", &["C", &column.to_string()]);
                        painter.tooltip(draw_list, &text, self.cursor, size);
                    } else if !stack.is_empty() {
                        let lines =
                            self.tooltip_lines(stack, matches!(slot, Slot::Picker(_)), lang);
                        painter.tooltip_lines(draw_list, &lines, self.cursor, size);
                    }
                }
                None => {}
            }
            return;
        }
        let (xm, ym) = self.cursor;
        for &tab in &self.tabs {
            let tab = &tabs()[tab];
            let (x, y) = Self::tab_position(tab);
            let (x, y) = (left + x + 3.0, top + y + 3.0);
            if xm >= x - 1.0 && xm < x + 22.0 && ym >= y - 1.0 && ym < y + 28.0 {
                painter.tooltip(draw_list, lang.get_dynamic(&tab.title), self.cursor, size);
                break;
            }
        }
    }

    /// `getTooltipFromContainerItem`.
    pub(super) fn tooltip_lines(
        &self,
        stack: &ItemStack,
        picker: bool,
        lang: &Lang,
    ) -> Vec<StyledText> {
        let mut lines = item_slot::tooltip_lines(lang, stack);
        if self.tab_type() == TabType::Category && picker {
            return lines;
        }
        let mut at = 1;
        for &tab in &self.tabs {
            let tab = &tabs()[tab];
            if tab.kind != TabType::Search && tab.contains(stack) {
                lines.insert(
                    at,
                    StyledText::colored(lang.get_dynamic(&tab.title), colors::BLUE),
                );
                at += 1;
            }
        }
        lines
    }

    /// `extractTabButton`.
    pub(super) fn draw_tab(
        &self,
        draw_list: &mut DrawList,
        textures: &CreativeTextures,
        painter: ItemPainter,
        tab: &CreativeTab,
        selected: bool,
    ) {
        let column = usize::from(tab.column).min(6);
        let top_row = tab.row == Row::Top;
        let (x, _) = Self::tab_position(tab);
        let x = self.left + x;
        let y = if top_row {
            self.top - 28.0
        } else {
            self.top + IMAGE_HEIGHT - 4.0
        };
        let sprites = if top_row {
            &textures.top
        } else {
            &textures.bottom
        };
        let sprite = sprites[usize::from(selected)][column];
        sprite.draw(draw_list, x, y, TAB_WIDTH, TAB_HEIGHT);
        let icon_y = y + 16.0 - 8.0 + if top_row { 1.0 } else { -1.0 };
        painter.stack(draw_list, &tab.icon, x + 13.0 - 8.0, icon_y);
    }

    /// The borderless search box at (82, 6), white text and a blinking cursor.
    pub(super) fn draw_search(&self, draw_list: &mut DrawList, glyphs: Glyphs<'_>) {
        let (bx, by, width, _) = super::SEARCH_BOX;
        let (x, y) = (self.left + bx, self.top + by);
        self.search
            .draw(draw_list, glyphs, (x, y), width, [1.0; 4], true);
    }
}
