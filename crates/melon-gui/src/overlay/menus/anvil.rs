//! `AnvilScreen`.

use pumpkin_data::item_stack::ItemStack;

use super::{Frame, Loader, MenuDraw, MenuScreen};
use crate::hud::item_slot;
use crate::overlay::container_input::MouseButtonKind;
use crate::overlay::{OverlayAction, OverlayContext};
use crate::screen::TextEvent;
use crate::sprite::GuiSprite;
use crate::widgets::text_input::{Edit, TextInput};
use melon_core::math::argb;
use melon_settings::input::InputBind;
use melon_world::inventory::{empty, stacks_equal};

/// `EditBox.setMaxLength(50)`.
const MAX_NAME: usize = 50;
/// Where the field's text starts and how wide it is.
const FIELD: (f32, f32, f32) = (62.0, 24.0, 103.0);
/// `ExperienceItemCost` limit: from here an anvil job is too expensive.
const TOO_EXPENSIVE: i16 = 40;

#[derive(Clone, Copy)]
pub struct Sprites {
    field: GuiSprite,
    field_disabled: GuiSprite,
    error: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            field: load.sprite("anvil/text_field")?,
            field_disabled: load.sprite("anvil/text_field_disabled")?,
            error: load.sprite("anvil/error")?,
        })
    }
}

pub struct AnvilScreen {
    name: TextInput,
    /// The first slot as the field last saw it (`slotChanged(0)`).
    item: ItemStack,
    /// What the server was last told the name is (`AnvilMenu.itemName`).
    sent: Option<String>,
    /// The left button went down in the field and is still held.
    selecting: bool,
}

impl Default for AnvilScreen {
    fn default() -> Self {
        Self {
            name: TextInput::new(MAX_NAME),
            item: empty(),
            sent: None,
            selecting: false,
        }
    }
}

impl AnvilScreen {
    /// The field follows the first slot: its item's name, or nothing.
    fn follow_slot(
        &mut self,
        stack: &ItemStack,
        lang: &melon_assets::i18n::Lang,
    ) -> Option<String> {
        if stacks_equal(&self.item, stack) {
            return None;
        }
        self.item = stack.clone();
        let name = if stack.is_empty() {
            String::new()
        } else {
            item_slot::hover_name(lang, stack)
        };
        self.name.set_value(&name);
        self.rename(lang)
    }

    /// `onNameChanged`: the name to send, if the server doesn't have it.
    fn rename(&mut self, lang: &melon_assets::i18n::Lang) -> Option<String> {
        if self.item.is_empty() {
            return None;
        }
        let mut name = self.name.value();
        if self.item.get_custom_name().is_none() && name == item_slot::hover_name(lang, &self.item)
        {
            name.clear();
        }
        if self.sent.as_deref() == Some(name.as_str()) {
            return None;
        }
        self.sent = Some(name.clone());
        Some(name)
    }
}

impl MenuScreen for AnvilScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let sprites = draw.sprites.anvil;
        let sprite = if draw.menu.slots[0].is_empty() {
            sprites.field_disabled
        } else {
            sprites.field
        };
        draw.sprite(&sprite, 59.0, 20.0, 110.0, 16.0);
        let slots = &draw.menu.slots;
        if (!slots[0].is_empty() || !slots[1].is_empty()) && slots[2].is_empty() {
            draw.sprite(&sprites.error, 99.0, 45.0, 28.0, 21.0);
        }
        if !draw.menu.slots[0].is_empty() {
            let (left, top) = (draw.frame.left, draw.frame.top);
            let painter = draw.ui.painter;
            self.name.draw(
                draw.ui.draw_list,
                painter.glyphs(),
                (left + FIELD.0, top + FIELD.1),
                FIELD.2 + 8.0,
                argb(0xFFFF_FFFF),
                true,
            );
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        let cost = draw.menu.data(0);
        if cost <= 0 {
            return;
        }
        let free = draw.ui.infinite_materials;
        let (line, color) = if cost >= TOO_EXPENSIVE && !free {
            (
                draw.ui.lang.get("container.repair.expensive").to_owned(),
                0xFFFF_6060,
            )
        } else if draw.menu.slots[2].is_empty() {
            return;
        } else {
            let line = draw
                .ui
                .lang
                .format("container.repair.cost", &[&cost.to_string()]);
            let can_pay = free || draw.ui.experience_level >= i32::from(cost);
            (line, if can_pay { 0xFF80_FF20 } else { 0xFFFF_6060 })
        };
        let x = 176.0 - 8.0 - draw.text_width(&line) - 2.0;
        draw.fill(x - 2.0, 67.0, 176.0 - 8.0 - (x - 2.0), 12.0, 0x4F00_0000);
        draw.text_shadowed(&line, x, 69.0, color);
    }

    fn tick(&mut self, ctx: &mut OverlayContext) -> Vec<OverlayAction> {
        let Some(menu) = ctx.container.as_deref() else {
            return Vec::new();
        };
        let item = menu.slots[0].clone();
        self.follow_slot(&item, ctx.lang)
            .map(OverlayAction::RenameItem)
            .into_iter()
            .collect()
    }

    /// `name.mouseClicked`: the cursor where the click landed.
    fn press(
        &mut self,
        frame: Frame,
        button: MouseButtonKind,
        ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        let menu = ctx.container.as_deref()?;
        let (x, y) = frame.local();
        let inside =
            (FIELD.0..FIELD.0 + FIELD.2).contains(&x) && (FIELD.1..FIELD.1 + 12.0).contains(&y);
        if menu.slots[0].is_empty() || !inside {
            return None;
        }
        self.name.click(x - FIELD.0, ctx.shift);
        self.selecting = button == MouseButtonKind::Left;
        Some(OverlayAction::None)
    }

    fn drag(&mut self, frame: Frame, _ctx: &mut OverlayContext) {
        if self.selecting {
            self.name.drag(frame.local().0 - FIELD.0);
        }
    }

    fn release(&mut self) {
        self.selecting = false;
    }

    fn key(
        &mut self,
        _bind: Option<&InputBind>,
        text: &[TextEvent],
        ctx: &mut OverlayContext,
    ) -> Option<Vec<OverlayAction>> {
        let menu = ctx.container.as_deref()?;
        if menu.slots[0].is_empty() {
            return None;
        }
        let mut actions = Vec::new();
        for event in text {
            if self.name.handle(event) == Edit::Changed
                && let Some(name) = self.rename(ctx.lang)
            {
                actions.push(OverlayAction::RenameItem(name));
            }
        }
        Some(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use melon_assets::i18n::Lang;
    use pumpkin_data::item::Item;

    #[test]
    fn a_new_item_names_the_field_and_tells_the_server_no_custom_name() {
        let lang = Lang::empty();
        let mut screen = AnvilScreen::default();
        let stack = ItemStack::new(1, &Item::DIAMOND_SWORD);
        assert_eq!(screen.follow_slot(&stack, &lang), Some(String::new()));
        assert_eq!(screen.follow_slot(&stack, &lang), None);
        screen.name.set_value("Excalibur");
        assert_eq!(screen.rename(&lang), Some("Excalibur".to_owned()));
        assert_eq!(screen.rename(&lang), None);
    }

    #[test]
    fn an_empty_first_slot_clears_the_field_and_sends_nothing() {
        let lang = Lang::empty();
        let mut screen = AnvilScreen::default();
        screen.follow_slot(&ItemStack::new(1, &Item::DIAMOND_SWORD), &lang);
        assert_eq!(screen.follow_slot(&empty(), &lang), None);
        assert_eq!(screen.name.value(), "");
    }
}
