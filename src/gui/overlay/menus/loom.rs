//! `LoomScreen`.

use pumpkin_data::banner_pattern::BannerPattern;
use pumpkin_data::dye_color::DyeColor;
use pumpkin_data::item_stack::ItemStack;

use super::scroll::Scroller;
use super::{Frame, Loader, MenuDraw, MenuScreen, MenuSprites};
use crate::gui::overlay::container_input::MouseButtonKind;
use crate::gui::overlay::{OverlayAction, OverlayContext};
use crate::gui::sprite::GuiSprite;
use crate::math::argb;
use crate::renderer::TextureId;
use crate::world::inventory::{OpenContainer, loom};

const COLUMNS: usize = 4;
const ROWS: usize = 4;
const PER_PAGE: usize = COLUMNS * ROWS;
/// Where the first pattern button is, and its size.
const GRID: (f32, f32) = (60.0, 13.0);
const BUTTON: f32 = 14.0;
/// The scroll bar.
const BAR_X: f32 = 119.0;
const BAR_Y: f32 = 13.0;
const BAR_PRESS_Y: f32 = 9.0;
const TRACK: f32 = 56.0;
const THUMB: (f32, f32) = (12.0, 15.0);
/// The banner's picture.
const PREVIEW: (f32, f32, f32, f32) = (141.0, 8.0, 20.0, 40.0);
const SHEET: f32 = 64.0;

#[derive(Clone)]
pub struct Sprites {
    pattern: GuiSprite,
    highlighted: GuiSprite,
    selected: GuiSprite,
    scroller: GuiSprite,
    scroller_disabled: GuiSprite,
    error: GuiSprite,
    empty_banner: GuiSprite,
    empty_dye: GuiSprite,
    empty_pattern: GuiSprite,
    /// The texture of each banner pattern, by `BannerPattern::id`.
    textures: Vec<Option<TextureId>>,
}

impl Sprites {
    pub fn load(load: &mut Loader) -> anyhow::Result<Self> {
        let textures = BannerPattern::all()
            .iter()
            .map(|pattern| {
                load.entity(&format!("banner/{}", pattern.to_name()))
                    .ok()
                    .map(|sprite| sprite.texture)
            })
            .collect();
        Ok(Self {
            pattern: load.sprite("loom/pattern")?,
            highlighted: load.sprite("loom/pattern_highlighted")?,
            selected: load.sprite("loom/pattern_selected")?,
            scroller: load.sprite("loom/scroller")?,
            scroller_disabled: load.sprite("loom/scroller_disabled")?,
            error: load.sprite("loom/error")?,
            empty_banner: load.sprite("slot/banner")?,
            empty_dye: load.sprite("slot/dye")?,
            empty_pattern: load.sprite("slot/banner_pattern")?,
            textures,
        })
    }

    fn texture(&self, pattern: BannerPattern) -> Option<TextureId> {
        self.textures.get(pattern.id() as usize).copied().flatten()
    }
}

#[derive(Default)]
pub struct LoomScreen {
    scroll: Scroller,
    /// The first row shown.
    start_row: usize,
    /// What the loom offers for the banner, dye and pattern in its slots.
    patterns: Vec<BannerPattern>,
    /// Whether the buttons show (`displayPatterns`).
    display: bool,
    /// The banner already has all the layers it takes.
    full: bool,
    /// The slots the lists were made for.
    inputs: Option<[ItemStack; 3]>,
}

impl LoomScreen {
    /// `containerChanged`: lists the patterns when the inputs changed.
    fn refresh(&mut self, menu: &OpenContainer) {
        let inputs = [&menu.slots[0], &menu.slots[1], &menu.slots[2]];
        let same = self.inputs.as_ref().is_some_and(|known| {
            known
                .iter()
                .zip(inputs)
                .all(|(a, b)| crate::world::inventory::stacks_equal(a, b))
        });
        if same {
            return;
        }
        self.inputs = Some(inputs.map(|s| ItemStack::clone(s)));
        let (banner, dye, pattern) = (inputs[0], inputs[1], inputs[2]);
        self.full = loom::layers(banner).len() >= loom::MAX_LAYERS;
        self.patterns = if banner.is_empty() || dye.is_empty() {
            Vec::new()
        } else {
            loom::selectable_patterns(pattern)
        };
        self.display =
            !banner.is_empty() && !dye.is_empty() && !self.full && !self.patterns.is_empty();
        if self.start_row >= self.total_rows() {
            self.start_row = 0;
            self.scroll.reset();
        }
    }

    fn total_rows(&self) -> usize {
        self.patterns.len().div_ceil(COLUMNS)
    }

    fn offscreen_rows(&self) -> usize {
        self.total_rows().saturating_sub(ROWS)
    }

    /// `isScrollBarActive`.
    fn scrollable(&self) -> bool {
        self.display && self.patterns.len() > PER_PAGE
    }

    fn set_row_from_bar(&mut self) {
        self.start_row = (self.scroll.offs() * self.offscreen_rows() as f32 + 0.5) as usize;
    }

    /// The shown buttons: pattern index and panel position.
    fn buttons(&self) -> impl Iterator<Item = (usize, f32, f32)> + '_ {
        (0..ROWS).flat_map(move |row| {
            (0..COLUMNS).filter_map(move |column| {
                let index = (row + self.start_row) * COLUMNS + column;
                (self.display && index < self.patterns.len()).then_some({
                    (
                        index,
                        GRID.0 + column as f32 * BUTTON,
                        GRID.1 + row as f32 * BUTTON,
                    )
                })
            })
        })
    }
}

/// The part of a 64x64 pattern texture the flag shows, as UVs.
fn flag_uv(left: f32, top: f32, width: f32, height: f32) -> [f32; 4] {
    [
        left / SHEET,
        top / SHEET,
        (left + width) / SHEET,
        (top + height) / SHEET,
    ]
}

/// A dye's colour as an opaque tint.
fn tint(color: DyeColor) -> [f32; 4] {
    argb(0xFF00_0000 | color.texture_diffuse_color())
}

impl MenuScreen for LoomScreen {
    fn background(&mut self, draw: &mut MenuDraw) {
        self.refresh(draw.menu);
        let sprites: &MenuSprites = draw.sprites;
        let loom_sprites = &sprites.loom;
        let thumb = if self.scrollable() {
            &loom_sprites.scroller
        } else {
            &loom_sprites.scroller_disabled
        };
        draw.sprite(
            thumb,
            BAR_X,
            BAR_Y + self.scroll.thumb(41.0),
            THUMB.0,
            THUMB.1,
        );

        let result = &draw.menu.slots[3];
        if !result.is_empty() && !self.full {
            self.draw_banner(draw, result);
        } else if self.full {
            draw.sprite(&loom_sprites.error, 143.0 - 5.0, 57.0 - 5.0, 26.0, 26.0);
        }

        let selected = draw.menu.data(0);
        for (index, x, y) in self.buttons().collect::<Vec<_>>() {
            let hovered = draw.frame.hovers(x, y, BUTTON, BUTTON);
            let sprite = if usize::try_from(selected) == Ok(index) {
                &loom_sprites.selected
            } else if hovered {
                &loom_sprites.highlighted
            } else {
                &loom_sprites.pattern
            };
            draw.sprite(sprite, x, y, BUTTON, BUTTON);
            self.draw_button_icon(draw, self.patterns[index], x, y);
        }
    }

    fn foreground(&mut self, draw: &mut MenuDraw) {
        let dye = loom::dye_color(&draw.menu.slots[1]);
        for (index, x, y) in self.buttons().collect::<Vec<_>>() {
            if draw.frame.hovers(x, y, BUTTON, BUTTON) {
                let key = format!("{}.{}", self.patterns[index].translation_key(), dye.name());
                let text = draw.ui.lang.get_dynamic(&key).to_owned();
                draw.tooltip(&[text]);
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
        self.refresh(menu);
        if !self.display {
            return None;
        }
        let hit = self
            .buttons()
            .find(|&(_, x, y)| frame.hovers(x, y, BUTTON, BUTTON))
            .map(|(index, ..)| index);
        if let Some(index) = hit {
            menu.set_data(0, index as i16);
            return Some(
                OverlayAction::MenuButton {
                    container: menu.id,
                    button: index as i32,
                }
                .with_sound("ui.loom.select_pattern"),
            );
        }
        if frame.hovers(BAR_X, BAR_PRESS_Y, THUMB.0, TRACK) {
            self.scroll.grab();
        }
        None
    }

    fn drag(&mut self, frame: Frame, _ctx: &mut OverlayContext) {
        if self.scroll.is_dragging() && self.display && self.offscreen_rows() > 0 {
            self.scroll
                .drag_to(frame.cursor.1, frame.top + BAR_Y, TRACK, THUMB.1);
            self.set_row_from_bar();
        }
    }

    fn release(&mut self) {
        self.scroll.release();
    }

    fn wheel(&mut self, _frame: Frame, lines: f32, _ctx: &mut OverlayContext) -> bool {
        if self.display && self.offscreen_rows() > 0 {
            self.scroll.wheel(lines, self.offscreen_rows());
            self.set_row_from_bar();
        }
        true
    }

    fn empty_icon(
        &self,
        slot: usize,
        _menu: &OpenContainer,
        sprites: &MenuSprites,
    ) -> Option<GuiSprite> {
        match slot {
            0 => Some(sprites.loom.empty_banner),
            1 => Some(sprites.loom.empty_dye),
            2 => Some(sprites.loom.empty_pattern),
            _ => None,
        }
    }
}

impl LoomScreen {
    /// The pattern on its button.
    fn draw_button_icon(&self, draw: &mut MenuDraw, pattern: BannerPattern, x: f32, y: f32) {
        let (left, top) = (draw.frame.left + x + 4.0, draw.frame.top + y + 2.0);
        let white = draw.ui.painter.white;
        draw.ui
            .draw_list
            .push_fill(white, left, top, 5.0, 10.0, tint(DyeColor::Gray));
        if let Some(texture) = draw.sprites.loom.texture(pattern) {
            draw.ui.draw_list.push_rect(texture, [left, top, 5.0, 10.0], flag_uv(0.0, 1.0, 21.0, 40.0), [1.0; 4]);
        }
    }

    /// The banner in the result slot, flat.
    fn draw_banner(&self, draw: &mut MenuDraw, banner: &ItemStack) {
        let Some(base) = loom::banner_color(banner) else {
            return;
        };
        let (x, y, w, h) = PREVIEW;
        let (left, top) = (draw.frame.left + x, draw.frame.top + y);
        let uv = flag_uv(1.0, 1.0, 20.0, 40.0);
        let sprites: &MenuSprites = draw.sprites;
        let layers = std::iter::once((BannerPattern::Base, base)).chain(loom::layers(banner));
        for (pattern, color) in layers {
            if let Some(texture) = sprites.loom.texture(pattern) {
                draw.ui
                    .draw_list
                    .push_rect(texture, [left, top, w, h], uv, tint(color));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::inventory::{ContainerKind, empty};
    use pumpkin_data::item::Item;

    fn menu() -> OpenContainer {
        OpenContainer::new(1, ContainerKind::Loom, String::new())
    }

    #[test]
    fn the_patterns_show_with_a_banner_and_a_dye() {
        let mut screen = LoomScreen::default();
        let mut menu = menu();
        screen.refresh(&menu);
        assert!(!screen.display);
        menu.slots[0] = ItemStack::new(1, &Item::WHITE_BANNER).into();
        menu.slots[1] = ItemStack::new(1, &Item::RED_DYE).into();
        screen.refresh(&menu);
        assert!(screen.display);
        assert!(!screen.patterns.is_empty());
        menu.slots[1] = empty().into();
        screen.refresh(&menu);
        assert!(!screen.display);
        assert!(screen.patterns.is_empty());
    }

    #[test]
    fn the_buttons_follow_the_row_the_list_is_scrolled_to() {
        let mut screen = LoomScreen {
            display: true,
            patterns: vec![BannerPattern::Border; 20],
            ..LoomScreen::default()
        };
        assert_eq!(screen.buttons().count(), PER_PAGE);
        assert_eq!(screen.buttons().next().unwrap().0, 0);
        screen.start_row = 1;
        assert_eq!(screen.buttons().next().unwrap().0, 4);
        screen.start_row = 1;
        assert_eq!(screen.buttons().count(), PER_PAGE);
        assert_eq!(screen.offscreen_rows(), 1);
    }
}
