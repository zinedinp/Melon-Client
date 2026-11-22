//! `StonecutterScreen`.

use pumpkin_data::item_stack::ItemStack;

use super::scroll::Scroller;
use super::{Frame, Loader, MenuDraw, MenuScreen};
use crate::gui::overlay::container_input::MouseButtonKind;
use crate::gui::overlay::{OverlayAction, OverlayContext};
use crate::gui::sprite::GuiSprite;
use crate::world::inventory::{OpenContainer, stonecutting};

const COLUMNS: usize = 4;
const ROWS: usize = 3;
const PER_PAGE: usize = COLUMNS * ROWS;
/// Where the recipe grid starts in the panel, and one button's size.
const GRID: (f32, f32) = (52.0, 14.0);
const BUTTON: (f32, f32) = (16.0, 18.0);
/// The scroll bar.
const BAR_X: f32 = 119.0;
const BAR_PRESS_Y: f32 = 9.0;
const BAR_TOP: f32 = 15.0;
const TRACK: f32 = 54.0;
const THUMB: (f32, f32) = (12.0, 15.0);

#[derive(Clone, Copy)]
pub struct Sprites {
    recipe: GuiSprite,
    highlighted: GuiSprite,
    selected: GuiSprite,
    scroller: GuiSprite,
    scroller_disabled: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            recipe: load.sprite("stonecutter/recipe")?,
            highlighted: load.sprite("stonecutter/recipe_highlighted")?,
            selected: load.sprite("stonecutter/recipe_selected")?,
            scroller: load.sprite("stonecutter/scroller")?,
            scroller_disabled: load.sprite("stonecutter/scroller_disabled")?,
        })
    }
}

#[derive(Default)]
pub struct StonecutterScreen {
    scroll: Scroller,
    /// What the input in the slot cuts into, for the item that made it.
    options: Vec<ItemStack>,
    input: Option<ItemStack>,
}

impl StonecutterScreen {
    /// `containerChanged`.
    fn refresh(&mut self, menu: &OpenContainer, sets: &crate::world::recipes::RecipeSets) {
        let input = &menu.slots[0];
        if self.input.as_ref().is_some_and(|known| {
            known.item.id == input.item.id && known.is_empty() == input.is_empty()
        }) {
            return;
        }
        self.options = stonecutting::options_from(sets, input);
        self.input = Some(input.clone());
        if self.options.is_empty() {
            self.scroll.reset();
        }
    }

    fn offscreen_rows(&self) -> usize {
        Scroller::offscreen_rows(self.options.len(), COLUMNS, ROWS)
    }

    /// `isScrollBarActive`.
    fn scrollable(&self) -> bool {
        self.options.len() > PER_PAGE
    }

    fn start(&self) -> usize {
        self.scroll.start_index(self.offscreen_rows(), COLUMNS)
    }

    /// The shown options with the panel position of each button.
    fn visible(&self) -> impl Iterator<Item = (usize, f32, f32)> + '_ {
        let start = self.start();
        (start..(start + PER_PAGE).min(self.options.len())).map(move |index| {
            let slot = index - start;
            (
                index,
                GRID.0 + (slot % COLUMNS) as f32 * BUTTON.0,
                GRID.1 + (slot / COLUMNS) as f32 * BUTTON.1,
            )
        })
    }
}

impl MenuScreen for StonecutterScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        self.refresh(draw.menu, draw.ui.recipe_sets);
        let sprites = draw.sprites.stonecutter;
        let bar = if self.scrollable() {
            &sprites.scroller
        } else {
            &sprites.scroller_disabled
        };
        draw.sprite(
            bar,
            BAR_X,
            BAR_TOP + self.scroll.thumb(41.0),
            THUMB.0,
            THUMB.1,
        );
        let selected = draw.menu.data(0);
        for (index, x, y) in self.visible() {
            let hovered = draw.frame.hovers(x, y + 1.0, BUTTON.0, BUTTON.1);
            let sprite = if usize::try_from(selected) == Ok(index) {
                &sprites.selected
            } else if hovered {
                &sprites.highlighted
            } else {
                &sprites.recipe
            };
            draw.sprite(sprite, x, y + 1.0, BUTTON.0, BUTTON.1);
        }
        for (index, x, y) in self.visible() {
            draw.item(&self.options[index], x, y + 2.0);
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        for (index, x, y) in self.visible() {
            if draw.frame.hovers(x, y + 2.0, BUTTON.0, BUTTON.1) {
                draw.item_tooltip(&self.options[index]);
            }
        }
    }

    fn press(
        &mut self,
        frame: Frame,
        _button: MouseButtonKind,
        ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        let menu = ctx.container.as_deref_mut()?;
        self.refresh(menu, ctx.recipe_sets);
        if self.options.is_empty() {
            return None;
        }
        let hit = self
            .visible()
            .find(|&(_, x, y)| frame.hovers(x, y, BUTTON.0, BUTTON.1))
            .map(|(index, ..)| index);
        if let Some(index) = hit {
            menu.set_data(0, index as i16);
            return Some(
                OverlayAction::MenuButton {
                    container: menu.id,
                    button: index as i32,
                }
                .with_sound("ui.stonecutter.select_recipe"),
            );
        }
        if frame.hovers(BAR_X, BAR_PRESS_Y, THUMB.0, TRACK) {
            self.scroll.grab();
        }
        None
    }

    fn drag(&mut self, frame: Frame, _ctx: &mut OverlayContext) {
        if self.scroll.is_dragging() && self.scrollable() {
            self.scroll
                .drag_to(frame.cursor.1, frame.top + GRID.1, TRACK, THUMB.1);
        }
    }

    fn release(&mut self) {
        self.scroll.release();
    }

    fn wheel(&mut self, _frame: Frame, lines: f32, _ctx: &mut OverlayContext) -> bool {
        if self.scrollable() {
            self.scroll.wheel(lines, self.offscreen_rows());
        }
        true
    }

    fn shown_stacks(&self) -> &[ItemStack] {
        &self.options
    }
}
