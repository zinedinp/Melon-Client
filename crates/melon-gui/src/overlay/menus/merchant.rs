//! `MerchantScreen`.

use pumpkin_data::item_stack::ItemStack;

use super::{Frame, Loader, MenuDraw, MenuScreen};
use crate::overlay::container_input::MouseButtonKind;
use crate::overlay::{OverlayAction, OverlayContext};
use crate::sprite::GuiSprite;
use crate::world::inventory::OpenContainer;
use crate::world::inventory::merchant::{self, Offer};

const ROWS: usize = 7;
/// A trade button: where the first is, its size and the pitch.
const BUTTON_X: f32 = 5.0;
const BUTTON_Y: f32 = 18.0;
const BUTTON: (f32, f32) = (88.0, 20.0);
/// The scroll bar's column, top and height, and the thumb's height.
const BAR_X: f32 = 94.0;
const BAR_Y: f32 = 18.0;
const BAR_HEIGHT: i32 = 139;
const THUMB: f32 = 27.0;

#[derive(Clone, Copy)]
pub struct Sprites {
    button: GuiSprite,
    button_highlighted: GuiSprite,
    scroller: GuiSprite,
    scroller_disabled: GuiSprite,
    out_of_stock: GuiSprite,
    xp_background: GuiSprite,
    xp_current: GuiSprite,
    xp_result: GuiSprite,
    arrow: GuiSprite,
    arrow_out_of_stock: GuiSprite,
    discount: GuiSprite,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        Ok(Self {
            button: load.widget("button")?,
            button_highlighted: load.widget("button_highlighted")?,
            scroller: load.sprite("villager/scroller")?,
            scroller_disabled: load.sprite("villager/scroller_disabled")?,
            out_of_stock: load.sprite("villager/out_of_stock")?,
            xp_background: load.sprite("villager/experience_bar_background")?,
            xp_current: load.sprite("villager/experience_bar_current")?,
            xp_result: load.sprite("villager/experience_bar_result")?,
            arrow: load.sprite("villager/trade_arrow")?,
            arrow_out_of_stock: load.sprite("villager/trade_arrow_out_of_stock")?,
            discount: load.sprite("villager/discount_strikethrough")?,
        })
    }
}

#[derive(Default)]
pub struct MerchantScreen {
    /// `shopItem`: the picked trade.
    selected: usize,
    /// `scrollOff`: the first trade shown.
    scroll_off: usize,
    dragging: bool,
}

fn offers(menu: &OpenContainer) -> &[Offer] {
    menu.trades.as_ref().map_or(&[], |t| &t.offers)
}

/// `canScroll`.
fn can_scroll(count: usize) -> bool {
    count > ROWS
}

/// `scrollOff` that puts the thumb at `y` (the mouse), `mouseDragged`.
fn scroll_for(y: f32, top: f32, count: usize) -> usize {
    let max = count.saturating_sub(ROWS) as f32;
    let scrolling = (y - top - 13.5) / (BAR_HEIGHT as f32 - THUMB) * max + 0.5;
    scrolling.clamp(0.0, max) as usize
}

/// The thumb's offset below the track's top for the scroll position.
fn thumb_offset(count: usize, scroll_off: usize) -> Option<i32> {
    let steps = (count + 1).saturating_sub(ROWS) as i32;
    if steps <= 1 {
        return None;
    }
    let left_over = BAR_HEIGHT - (THUMB as i32 + (steps - 1) * BAR_HEIGHT / steps);
    let step_height = 1 + left_over / steps + BAR_HEIGHT / steps;
    Some(if scroll_off as i32 == steps - 1 {
        113
    } else {
        113.min(scroll_off as i32 * step_height)
    })
}

impl MerchantScreen {
    fn selected_offer<'a>(&self, menu: &'a OpenContainer) -> Option<&'a Offer> {
        offers(menu).get(self.selected)
    }

    /// The experience the picked trade would give, while its payment is in the slots.
    fn future_xp(&self, menu: &OpenContainer) -> i32 {
        let Some(offer) = self.selected_offer(menu) else {
            return 0;
        };
        let pays = |slot: &ItemStack, cost: &ItemStack| {
            cost.is_empty() && slot.is_empty()
                || !slot.is_empty()
                    && slot.item.id == cost.item.id
                    && slot.item_count >= cost.item_count
        };
        if pays(&menu.slots[0], &offer.cost_a()) && pays(&menu.slots[1], &offer.cost_b) {
            offer.xp
        } else {
            0
        }
    }

    /// The row of trade `row` (0-6) on screen: where its button is.
    fn button_y(row: usize) -> f32 {
        BUTTON_Y + row as f32 * BUTTON.1
    }

    fn row_at(frame: Frame, count: usize) -> Option<usize> {
        (0..ROWS.min(count))
            .find(|&row| frame.hovers(BUTTON_X, Self::button_y(row), BUTTON.0, BUTTON.1))
    }
}

impl MenuScreen for MerchantScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        let menu = draw.menu;
        if self.selected_offer(menu).is_some_and(|o| o.out_of_stock) {
            let sprite = draw.sprites.merchant.out_of_stock;
            draw.sprite(&sprite, 83.0 + 99.0, 35.0, 28.0, 21.0);
        }
        let list = offers(menu);
        if self.selected >= list.len() {
            self.selected = 0;
        }
        self.scroll_off = self.scroll_off.min(list.len().saturating_sub(ROWS));
        if list.is_empty() {
            return;
        }
        let sprites = draw.sprites.merchant;
        match thumb_offset(list.len(), self.scroll_off) {
            Some(offset) => {
                draw.sprite(&sprites.scroller, BAR_X, BAR_Y + offset as f32, 6.0, THUMB)
            }
            None => draw.sprite(&sprites.scroller_disabled, BAR_X, BAR_Y, 6.0, THUMB),
        }
        for row in 0..ROWS.min(list.len()) {
            let y = Self::button_y(row);
            let hovered = draw.frame.hovers(BUTTON_X, y, BUTTON.0, BUTTON.1);
            let sprite = if hovered {
                &sprites.button_highlighted
            } else {
                &sprites.button
            };
            draw.sprite(sprite, BUTTON_X, y, BUTTON.0, BUTTON.1);
        }
        let shown = if can_scroll(list.len()) {
            &list[self.scroll_off..(self.scroll_off + ROWS).min(list.len())]
        } else {
            list
        };
        let mut offer_y = 17.0;
        for offer in shown {
            let y = offer_y + 2.0;
            let (cost_a, base_a) = (offer.cost_a(), &offer.base_cost_a);
            if base_a.item_count == cost_a.item_count {
                draw.item(&cost_a, 10.0, y);
            } else {
                draw.icon(&cost_a, 10.0, y);
                draw.count(base_a.item_count, 10.0, y, base_a.item_count == 1);
                draw.count(cost_a.item_count, 24.0, y, cost_a.item_count == 1);
                draw.sprite(&sprites.discount, 17.0, y + 12.0, 9.0, 2.0);
            }
            if !offer.cost_b.is_empty() {
                draw.item(&offer.cost_b, 40.0, y);
            }
            let arrow = if offer.out_of_stock {
                &sprites.arrow_out_of_stock
            } else {
                &sprites.arrow
            };
            draw.sprite(arrow, 60.0, y + 3.0, 10.0, 9.0);
            draw.item(&offer.result, 73.0, y);
            offer_y += 20.0;
        }
        if let Some(trades) = menu.trades.as_ref().filter(|t| t.show_progress) {
            self.progress_bar(draw, trades.level, trades.xp);
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        let menu = draw.menu;
        let lang = draw.ui.lang;
        let trades = menu.trades.as_ref();
        match trades {
            Some(t) if (1..=5).contains(&t.level) && t.show_progress => {
                let key = format!("merchant.level.{}", t.level);
                let title = lang.format(
                    "merchant.title",
                    &[&menu.title.to_plain(), lang.get_dynamic(&key)],
                );
                let width = draw.text_width(&title);
                draw.text(
                    &title,
                    (49.0 + 276.0 / 2.0 - width / 2.0).floor(),
                    6.0,
                    0xFF40_4040,
                );
            }
            _ => {
                let width = draw.styled_width(&menu.title);
                draw.styled(
                    &menu.title,
                    (49.0 + 276.0 / 2.0 - width / 2.0).floor(),
                    6.0,
                    0xFF40_4040,
                );
            }
        }
        let label = lang.get("merchant.trades").to_owned();
        let width = draw.text_width(&label);
        draw.text(&label, (5.0 - width / 2.0 + 48.0).floor(), 6.0, 0xFF40_4040);

        let list = offers(menu);
        if list.is_empty() {
            return;
        }
        if self.selected_offer(menu).is_some_and(|o| o.out_of_stock)
            && draw.frame.hovers(186.0, 35.0, 22.0, 21.0)
            && trades.is_some_and(|t| t.can_restock)
        {
            let text = lang.get("merchant.deprecated").to_owned();
            draw.tooltip(&[text]);
            return;
        }
        let Some(row) = Self::row_at(draw.frame, list.len()) else {
            return;
        };
        let Some(offer) = list.get(row + self.scroll_off) else {
            return;
        };
        let (mx, _) = draw.frame.local();
        let x = mx - BUTTON_X;
        let stack = if x < 20.0 {
            Some(offer.cost_a())
        } else if x < 50.0 && x > 30.0 {
            (!offer.cost_b.is_empty()).then(|| offer.cost_b.clone())
        } else if x > 65.0 {
            Some(offer.result.clone())
        } else {
            None
        };
        if let Some(stack) = stack {
            draw.item_tooltip(&stack);
        }
    }

    fn press(
        &mut self,
        frame: Frame,
        _button: MouseButtonKind,
        ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        let menu = ctx.container.as_deref()?;
        let count = offers(menu).len();
        if can_scroll(count) {
            let (mx, my) = frame.local();
            if mx > BAR_X && mx < BAR_X + 6.0 && my > BAR_Y && my <= BAR_Y + BAR_HEIGHT as f32 + 1.0
            {
                self.dragging = true;
            }
        }
        let row = Self::row_at(frame, count)?;
        self.selected = row + self.scroll_off;
        Some(OverlayAction::SelectTrade(self.selected as i32).with_sound(crate::overlay::CLICK))
    }

    fn drag(&mut self, frame: Frame, ctx: &mut OverlayContext) {
        if self.dragging
            && let Some(menu) = ctx.container.as_deref()
        {
            self.scroll_off = scroll_for(frame.cursor.1, frame.top + BAR_Y, offers(menu).len());
        }
    }

    fn release(&mut self) {
        self.dragging = false;
    }

    fn wheel(&mut self, _frame: Frame, lines: f32, ctx: &mut OverlayContext) -> bool {
        if let Some(menu) = ctx.container.as_deref() {
            let count = offers(menu).len();
            if can_scroll(count) {
                let max = (count - ROWS) as f32;
                self.scroll_off = (self.scroll_off as f32 - lines).clamp(0.0, max) as usize;
            }
        }
        true
    }

    fn shown_stacks(&self) -> &[ItemStack] {
        &[]
    }
}

impl MerchantScreen {
    /// `extractProgressBar`.
    fn progress_bar(&self, draw: &mut MenuDraw, level: i32, xp: i32) {
        if level >= 5 {
            return;
        }
        let sprites = draw.sprites.merchant;
        draw.sprite(&sprites.xp_background, 136.0, 16.0, 102.0, 5.0);
        let min = merchant::min_xp(level);
        if xp < min || !merchant::can_level_up(level) {
            return;
        }
        let multiplier = 102.0 / (merchant::max_xp(level) - min) as f32;
        let width = ((multiplier * (xp - min) as f32).floor() as i32).min(102);
        draw.sprite_part(
            &sprites.xp_current,
            [102.0, 5.0],
            [0.0, 0.0],
            (136.0, 16.0),
            (width as f32, 5.0),
        );
        let future = self.future_xp(draw.menu);
        if future > 0 {
            let future_width = ((future as f32 * multiplier).floor() as i32).min(102 - width);
            draw.sprite_part(
                &sprites.xp_result,
                [102.0, 5.0],
                [width as f32, 0.0],
                (136.0 + width as f32, 16.0),
                (future_width as f32, 5.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumb_sits_at_the_top_and_at_the_bottom_of_its_track() {
        assert_eq!(thumb_offset(7, 0), None);
        assert_eq!(thumb_offset(8, 0), Some(0));
        assert_eq!(thumb_offset(8, 1), Some(113));
        assert_eq!(thumb_offset(12, 5), Some(113));
        assert_eq!(thumb_offset(12, 0), Some(0));
    }

    #[test]
    fn dragging_the_thumb_picks_the_nearest_step() {
        let top = 50.0;
        assert_eq!(scroll_for(top, top, 12), 0);
        assert_eq!(scroll_for(top + 500.0, top, 12), 5);
        let middle = top + 13.5 + (BAR_HEIGHT as f32 - THUMB) / 2.0;
        assert_eq!(scroll_for(middle, top, 12), 3);
    }
}
