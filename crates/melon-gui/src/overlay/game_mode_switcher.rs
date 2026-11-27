//! Vanilla `GameModeSwitcherScreen` (F3 + F4).

use crate::world::item_model::StackKey;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

use crate::assets::Assets;
use crate::gui::hud::item_slot::ItemPainter;
use crate::gui::overlay::{
    ChatExit, Overlay, OverlayAction, OverlayContext, OverlayDraw, OverlayKind,
};
use crate::gui::sprite::GuiSprite;
use crate::gui::text::{self, StyledText, TextStyle, colors};
use crate::i18n::Lang;
use crate::renderer::quad::DrawList;
use crate::renderer::{Renderer, TextureId};
use crate::world::game_mode::{GameMode, LocalGameMode};
use crate::world::inventory::Inventory;

/// `GameModeIcon.values().length * 31 - 5`.
const ALL_SLOTS_WIDTH: f32 = 119.0;
const SLOT_AREA: f32 = 26.0;
const SLOT_STRIDE: f32 = 31.0;

/// `GameModeIcon`, in its cycle order.
const ICONS: [GameMode; 4] = [
    GameMode::Creative,
    GameMode::Survival,
    GameMode::Adventure,
    GameMode::Spectator,
];

pub struct SwitcherTextures {
    background: TextureId,
    slot: GuiSprite,
    selection: GuiSprite,
}

impl SwitcherTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        Ok(Self {
            background: assets
                .texture(renderer, "minecraft:gui/container/gamemode_switcher.png")?,
            slot: GuiSprite::load(
                renderer,
                assets,
                "minecraft:gui/sprites/gamemode_switcher/slot.png",
            )?,
            selection: GuiSprite::load(
                renderer,
                assets,
                "minecraft:gui/sprites/gamemode_switcher/selection.png",
            )?,
        })
    }
}

/// The icon item of a mode (`GameModeIcon.renderStack`).
pub fn icon(mode: GameMode) -> ItemStack {
    let item = match mode {
        GameMode::Creative => &Item::GRASS_BLOCK,
        GameMode::Survival => &Item::IRON_SWORD,
        GameMode::Adventure => Item::from_registry_key("buried_treasure_map").unwrap_or(&Item::MAP),
        GameMode::Spectator => &Item::ENDER_EYE,
    };
    ItemStack::new(1, item)
}

pub struct GameModeSwitcher {
    hovered: GameMode,
    /// The mouse where it was first seen; hovering selects once it moves.
    first_mouse: Option<(f32, f32)>,
    mouse: (f32, f32),
}

impl GameModeSwitcher {
    /// `getDefaultSelected`.
    pub fn new(mode: LocalGameMode) -> Self {
        let hovered = mode
            .previous
            .unwrap_or(if mode.current == GameMode::Creative {
                GameMode::Survival
            } else {
                GameMode::Creative
            });
        Self {
            hovered,
            first_mouse: None,
            mouse: (0.0, 0.0),
        }
    }

    /// F4: `GameModeIcon.getNext`.
    fn cycle(&mut self) {
        let i = ICONS.iter().position(|&m| m == self.hovered).unwrap_or(0);
        self.hovered = ICONS[(i + 1) % ICONS.len()];
        self.first_mouse = None;
    }

    fn slot_origin(i: usize, size: (f32, f32)) -> (f32, f32) {
        let (w, h) = size;
        (
            (w / 2.0).floor() - (ALL_SLOTS_WIDTH / 2.0).floor() + i as f32 * SLOT_STRIDE,
            (h / 2.0).floor() - 31.0,
        )
    }

    fn on_mouse_move(&mut self, x: f32, y: f32, size: (f32, f32)) {
        self.mouse = (x, y);
        let first = *self.first_mouse.get_or_insert((x, y));
        if first == (x, y) {
            return;
        }
        for (i, &mode) in ICONS.iter().enumerate() {
            let (sx, sy) = Self::slot_origin(i, size);
            if x >= sx && x < sx + SLOT_AREA && y >= sy && y < sy + SLOT_AREA {
                self.hovered = mode;
            }
        }
    }

    fn draw_screen(
        &self,
        draw_list: &mut DrawList,
        textures: &SwitcherTextures,
        painter: ItemPainter,
        lang: &Lang,
        size: (f32, f32),
    ) {
        let (w, h) = size;
        let (cx, cy) = ((w / 2.0).floor(), (h / 2.0).floor());
        draw_list.push_rect(
            textures.background,
            [cx - 62.0, cy - 31.0 - 27.0, 125.0, 75.0],
            [0.0, 0.0, 125.0 / 128.0, 75.0 / 128.0],
            [1.0; 4],
        );
        let paint = painter.paint(colors::WHITE);
        let centered = |draw_list: &mut DrawList, text: &StyledText, y: f32| {
            let width = text::width(painter.font, text);
            text::draw(draw_list, &paint, text, (cx - width / 2.0).floor(), y);
        };
        let name = StyledText::plain(lang.get(self.hovered.translation_key()));
        centered(draw_list, &name, cy - 31.0 - 20.0);
        let pattern = lang
            .raw_dynamic("debug.gamemodes.select_next")
            .unwrap_or("%s Next");
        let mut hint = StyledText::default();
        let (before, after) = pattern.split_once("%s").unwrap_or((pattern, ""));
        hint.push(before, &TextStyle::default());
        hint.push("F4", &TextStyle::colored(colors::AQUA));
        hint.push(after, &TextStyle::default());
        centered(draw_list, &hint, cy + 5.0);
        for (i, &mode) in ICONS.iter().enumerate() {
            let (x, y) = Self::slot_origin(i, size);
            textures.slot.draw(draw_list, x, y, SLOT_AREA, SLOT_AREA);
            if mode == self.hovered {
                textures
                    .selection
                    .draw(draw_list, x, y, SLOT_AREA, SLOT_AREA);
            }
            painter.stack(draw_list, &icon(mode), x + 5.0, y + 5.0);
        }
    }
}

impl Overlay for GameModeSwitcher {
    fn kind(&self) -> OverlayKind {
        OverlayKind::Switcher
    }

    fn mouse_moved(&mut self, x: f32, y: f32, _dragging: bool, ctx: &mut OverlayContext) {
        self.on_mouse_move(x, y, ctx.size);
    }

    /// Escape closes it without switching.
    fn escape(&mut self) -> OverlayAction {
        OverlayAction::Close(ChatExit::Interrupted)
    }

    fn cycle_game_mode(&mut self) {
        self.cycle();
    }

    /// `GameModeSwitcherScreen`: letting go of F3 switches to the hovered mode.
    fn released_debug_key(&self) -> Option<GameMode> {
        Some(self.hovered)
    }

    fn draw(&mut self, ui: &mut OverlayDraw) {
        self.draw_screen(
            ui.draw_list,
            ui.textures.switcher,
            ui.painter,
            ui.lang,
            ui.size,
        );
    }

    fn visit_stacks(
        &self,
        _inventory: &Inventory,
        visit: &mut dyn FnMut(&dyn StackKey) -> bool,
    ) -> bool {
        ICONS.iter().all(|&mode| visit(&icon(mode)))
    }
}
