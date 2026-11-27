//! What a server-opened menu's screen draws and does beyond its slots.

mod anvil;
mod beacon;
mod brewing;
mod cartography;
mod enchantment;
mod furnace;
mod grindstone;
mod loom;
mod merchant;
mod mount;
pub mod scroll;
mod smithing;
mod stonecutter;

use pumpkin_data::item_stack::ItemStack;

use super::container_input::MouseButtonKind;
use super::{OverlayAction, OverlayContext, OverlayDraw};
use melon_core::math::argb;
use melon_render::{Renderer, TextureId};
use crate::screen::TextEvent;
use melon_settings::input::InputBind;
use crate::sprite::GuiSprite;
use crate::text::font::Font;
use melon_world::inventory::{ContainerKind, OpenContainer};
use melon_assets::assets::Assets;

/// Where the panel is on screen and where the mouse is, for drawing and clicks.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    pub left: f32,
    pub top: f32,
    pub cursor: (f32, f32),
}

impl Frame {
    /// The mouse in panel coordinates.
    pub fn local(&self) -> (f32, f32) {
        (self.cursor.0 - self.left, self.cursor.1 - self.top)
    }

    /// Whether the mouse is in the `w` x `h` rectangle at `(x, y)` of the panel.
    pub fn hovers(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let (mx, my) = self.local();
        mx >= x && my >= y && mx < x + w && my < y + h
    }
}

/// What a screen draws with.
pub struct MenuDraw<'m, 'a> {
    pub ui: &'m mut OverlayDraw<'a>,
    pub sprites: &'a MenuSprites,
    pub menu: &'a OpenContainer,
    pub frame: Frame,
}

impl MenuDraw<'_, '_> {
    /// `sprite` at `(x, y)` of the panel, `w` x `h`.
    pub fn sprite(&mut self, sprite: &GuiSprite, x: f32, y: f32, w: f32, h: f32) {
        sprite.draw(
            self.ui.draw_list,
            self.frame.left + x,
            self.frame.top + y,
            w,
            h,
        );
    }

    /// The part of `sprite` (`sprite_size` big) from `tex` on, `w` x `h`
    pub fn sprite_part(
        &mut self,
        sprite: &GuiSprite,
        sprite_size: [f32; 2],
        tex: [f32; 2],
        at: (f32, f32),
        size: (f32, f32),
    ) {
        sprite.draw_region(
            self.ui.draw_list,
            sprite_size,
            tex,
            [
                self.frame.left + at.0,
                self.frame.top + at.1,
                size.0,
                size.1,
            ],
        );
    }

    /// Styled label, default `color` (`0xAARRGGBB`) for runs without their own.
    pub fn styled(&mut self, text: &crate::text::StyledText, x: f32, y: f32, color: u32) {
        let rgb = [
            ((color >> 16) & 0xFF) as f32 / 255.0,
            ((color >> 8) & 0xFF) as f32 / 255.0,
            (color & 0xFF) as f32 / 255.0,
        ];
        let mut paint = self.ui.painter.paint(rgb);
        paint.shadow = false;
        crate::text::draw(
            self.ui.draw_list,
            &paint,
            text,
            self.frame.left + x,
            self.frame.top + y,
        );
    }

    /// [`crate::text::width`] of styled text.
    pub fn styled_width(&self, text: &crate::text::StyledText) -> f32 {
        crate::text::width(self.ui.painter.font, text)
    }

    /// A flat grey label like the container screens' (`text(..., false)`).
    pub fn text(&mut self, text: &str, x: f32, y: f32, color: u32) {
        self.ui.painter.font.push_text_flat(
            self.ui.draw_list,
            self.ui.painter.font_textures,
            text,
            self.frame.left + x,
            self.frame.top + y,
            argb(color),
        );
    }

    /// [`Self::text`] with the drop shadow (`text(..., true)`).
    pub fn text_shadowed(&mut self, text: &str, x: f32, y: f32, color: u32) {
        self.ui.painter.font.push_text(
            self.ui.draw_list,
            self.ui.painter.font_textures,
            text,
            self.frame.left + x,
            self.frame.top + y,
            argb(color),
        );
    }

    /// The width `text` takes.
    pub fn text_width(&self, text: &str) -> f32 {
        self.ui.painter.font.text_width(text)
    }

    /// A rectangle of the panel filled with `color` (`0xAARRGGBB`).
    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        self.ui.draw_list.push_fill(
            self.ui.painter.white,
            self.frame.left + x,
            self.frame.top + y,
            w,
            h,
            argb(color),
        );
    }

    /// A tooltip of styled `lines` at the mouse.
    pub fn tooltip_styled(&mut self, lines: &[crate::text::StyledText]) {
        self.ui
            .painter
            .tooltip_lines(self.ui.draw_list, lines, self.frame.cursor, self.ui.size);
    }

    /// A tooltip of plain `lines` at the mouse.
    pub fn tooltip(&mut self, lines: &[String]) {
        let lines: Vec<_> = lines
            .iter()
            .map(|line| crate::text::StyledText::plain(line))
            .collect();
        self.ui
            .painter
            .tooltip_lines(self.ui.draw_list, &lines, self.frame.cursor, self.ui.size);
    }

    /// `stack`'s icon alone at `(x, y)` of the panel (`fakeItem`).
    pub fn icon(&mut self, stack: &ItemStack, x: f32, y: f32) {
        self.ui.painter.stack_with_count(
            self.ui.draw_list,
            stack,
            self.frame.left + x,
            self.frame.top + y,
            Some(crate::text::StyledText::plain("")),
        );
    }

    /// An item count over the slot at `(x, y)` of the panel.
    pub fn count(&mut self, count: u8, x: f32, y: f32, always: bool) {
        if count == 1 && !always {
            return;
        }
        let text = crate::text::StyledText::plain(&count.to_string());
        let painter = self.ui.painter;
        let width = crate::text::width(painter.font, &text);
        crate::text::draw(
            self.ui.draw_list,
            &painter.paint(crate::text::colors::WHITE),
            &text,
            self.frame.left + x + 19.0 - 2.0 - width,
            self.frame.top + y + 6.0 + 3.0,
        );
    }

    /// `stack` at `(x, y)` of the panel.
    pub fn item(&mut self, stack: &ItemStack, x: f32, y: f32) {
        self.ui.painter.stack(
            self.ui.draw_list,
            stack,
            self.frame.left + x,
            self.frame.top + y,
        );
    }

    /// A tooltip of `stack` at the mouse.
    pub fn item_tooltip(&mut self, stack: &ItemStack) {
        let lines = crate::hud::item_slot::tooltip_lines(self.ui.lang, stack);
        self.ui
            .painter
            .tooltip_lines(self.ui.draw_list, &lines, self.frame.cursor, self.ui.size);
    }
}

/// The rest of one menu's screen.
pub trait MenuScreen {
    /// After the panel, under the slots: bars, buttons, lists.
    fn background(&mut self, _draw: &mut MenuDraw) {}

    /// After the slots and the labels, with the tooltips.
    fn foreground(&mut self, _draw: &mut MenuDraw) {}

    /// A press at `frame.cursor`; `Some` when the screen took it.
    fn press(
        &mut self,
        _frame: Frame,
        _button: MouseButtonKind,
        _ctx: &mut OverlayContext,
    ) -> Option<OverlayAction> {
        None
    }

    /// A client tick: what the screen has to send for changes it noticed.
    fn tick(&mut self, _ctx: &mut OverlayContext) -> Vec<OverlayAction> {
        Vec::new()
    }

    /// The mouse moved with a button held.
    fn drag(&mut self, _frame: Frame, _ctx: &mut OverlayContext) {}

    /// The button went up.
    fn release(&mut self) {}

    /// The wheel, `lines` up; `true` when the screen used it.
    fn wheel(&mut self, _frame: Frame, _lines: f32, _ctx: &mut OverlayContext) -> bool {
        false
    }

    /// A key before the slots see it (a focused text field takes the letters).
    fn key(
        &mut self,
        _bind: Option<&InputBind>,
        _text: &[TextEvent],
        _ctx: &mut OverlayContext,
    ) -> Option<Vec<OverlayAction>> {
        None
    }

    /// `Slot.getNoItemIcon`: what an empty slot shows.
    fn empty_icon(
        &self,
        _slot: usize,
        _menu: &OpenContainer,
        _sprites: &MenuSprites,
    ) -> Option<GuiSprite> {
        None
    }

    /// Stacks the screen shows besides the slots', for baking their icons.
    fn shown_stacks(&self) -> &[ItemStack] {
        &[]
    }
}

/// The screen of a menu, if it has more than slots.
pub fn screen_for(kind: ContainerKind) -> Option<Box<dyn MenuScreen>> {
    match kind {
        ContainerKind::Furnace | ContainerKind::BlastFurnace | ContainerKind::Smoker => {
            Some(Box::new(furnace::FurnaceScreen::new(kind)))
        }
        ContainerKind::Stonecutter => Some(Box::new(stonecutter::StonecutterScreen::default())),
        ContainerKind::Anvil => Some(Box::new(anvil::AnvilScreen::default())),
        ContainerKind::Grindstone => Some(Box::new(grindstone::GrindstoneScreen)),
        ContainerKind::Smithing => Some(Box::new(smithing::SmithingScreen)),
        ContainerKind::Cartography => Some(Box::new(cartography::CartographyScreen)),
        ContainerKind::Brewing => Some(Box::new(brewing::BrewingScreen)),
        ContainerKind::Beacon => Some(Box::new(beacon::BeaconScreen::default())),
        ContainerKind::Merchant => Some(Box::new(merchant::MerchantScreen::default())),
        ContainerKind::Enchantment => Some(Box::new(enchantment::EnchantmentScreen)),
        ContainerKind::Loom => Some(Box::new(loom::LoomScreen::default())),
        ContainerKind::Mount { columns, look } => {
            Some(Box::new(mount::MountScreen { columns, look }))
        }
        _ => None,
    }
}

/// The Standard Galactic Alphabet.
pub struct Galactic {
    pub font: Font,
    pub pages: Vec<TextureId>,
}

impl Galactic {
    fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut font = Font::load(assets, "minecraft:alt")?;
        font.white = renderer.white_texture();
        let pages = font
            .pages
            .iter()
            .map(|page| renderer.load_texture(page))
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self { font, pages })
    }
}

/// The sprites the menu screens draw, loaded with the container sheets.
pub struct MenuSprites {
    furnace: furnace::Sprites,
    stonecutter: stonecutter::Sprites,
    anvil: anvil::Sprites,
    grindstone: grindstone::Sprites,
    mount: mount::Sprites,
    smithing: smithing::Sprites,
    cartography: cartography::Sprites,
    brewing: brewing::Sprites,
    beacon: beacon::Sprites,
    merchant: merchant::Sprites,
    enchantment: enchantment::Sprites,
    loom: loom::Sprites,
    galactic: Galactic,
}

impl MenuSprites {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut load = Loader { renderer, assets };
        Ok(Self {
            furnace: furnace::Sprites::load(&mut load)?,
            stonecutter: stonecutter::Sprites::load(&mut load)?,
            anvil: anvil::Sprites::load(&mut load)?,
            grindstone: grindstone::Sprites::load(&mut load)?,
            mount: mount::Sprites::load(&mut load)?,
            smithing: smithing::Sprites::load(&mut load)?,
            cartography: cartography::Sprites::load(&mut load)?,
            brewing: brewing::Sprites::load(&mut load)?,
            beacon: beacon::Sprites::load(&mut load)?,
            merchant: merchant::Sprites::load(&mut load)?,
            enchantment: enchantment::Sprites::load(&mut load)?,
            loom: loom::Sprites::load(&mut load)?,
            galactic: Galactic::load(load.renderer, load.assets)?,
        })
    }
}

/// Loads `gui/sprites/container/<name>.png`.
struct Loader<'a> {
    renderer: &'a mut Renderer,
    assets: &'a Assets,
}

impl Loader<'_> {
    /// `entity/<name>`, a texture of the entity sheets.
    fn entity(&mut self, name: &str) -> anyhow::Result<GuiSprite> {
        GuiSprite::load(
            self.renderer,
            self.assets,
            &format!("minecraft:entity/{name}.png"),
        )
    }

    /// `gui/sprites/widget/<name>`, the button sprites a screen reuses.
    fn widget(&mut self, name: &str) -> anyhow::Result<GuiSprite> {
        GuiSprite::load(
            self.renderer,
            self.assets,
            &format!("minecraft:gui/sprites/widget/{name}.png"),
        )
    }

    /// `mob_effect/<name>`, a status effect's icon.
    fn effect(&mut self, name: &str) -> anyhow::Result<GuiSprite> {
        GuiSprite::load(
            self.renderer,
            self.assets,
            &format!("minecraft:mob_effect/{name}.png"),
        )
    }

    fn sprite(&mut self, name: &str) -> anyhow::Result<GuiSprite> {
        GuiSprite::load(
            self.renderer,
            self.assets,
            &format!("minecraft:gui/sprites/container/{name}.png"),
        )
    }
}
