//! Vanilla `SpectatorGui` and `SpectatorMenu`: a spectator's hotbar.

use std::time::{Duration, Instant};

use crate::hud::item_slot::ItemPainter;
use melon_assets::i18n::Lang;
use melon_render::quad::DrawList;
use melon_render::{Renderer, TextureId};
use crate::sprite::GuiSprite;
use crate::text::{self, StyledText, colors};
use melon_assets::assets::Assets;

const FADE_DELAY: Duration = Duration::from_millis(5000);
const FADE_TIME: f32 = 2000.0;

pub struct SpectatorTextures {
    close: GuiSprite,
    scroll_left: GuiSprite,
    scroll_right: GuiSprite,
    teleport_to_player: GuiSprite,
    teleport_to_team: GuiSprite,
}

impl SpectatorTextures {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let mut load = |name: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/spectator/{name}.png"),
            )
        };
        Ok(Self {
            close: load("close")?,
            scroll_left: load("scroll_left")?,
            scroll_right: load("scroll_right")?,
            teleport_to_player: load("teleport_to_player")?,
            teleport_to_team: load("teleport_to_team")?,
        })
    }
}

/// `SpectatorMenuItem`s.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Empty,
    Close,
    ScrollLeft,
    ScrollRight { enabled: bool },
    TeleportToPlayer { enabled: bool },
    TeleportToTeam,
    Player { id: uuid::Uuid, name: String },
}

impl Item {
    fn enabled(&self) -> bool {
        match self {
            Item::Empty | Item::TeleportToTeam => false,
            Item::ScrollRight { enabled } | Item::TeleportToPlayer { enabled } => *enabled,
            _ => true,
        }
    }

    fn name(&self, lang: &Lang) -> String {
        match self {
            Item::Empty => String::new(),
            Item::Close => lang.get("spectatorMenu.close").to_owned(),
            Item::ScrollLeft => lang.get("spectatorMenu.previous_page").to_owned(),
            Item::ScrollRight { .. } => lang.get("spectatorMenu.next_page").to_owned(),
            Item::TeleportToPlayer { .. } => lang.get("spectatorMenu.teleport").to_owned(),
            Item::TeleportToTeam => lang.get("spectatorMenu.team_teleport").to_owned(),
            Item::Player { name, .. } => name.clone(),
        }
    }
}

/// `SpectatorMenuCategory`.
#[derive(Clone, Debug)]
enum Category {
    Root { players: Vec<(uuid::Uuid, String)> },
    Players(Vec<(uuid::Uuid, String)>),
}

impl Category {
    fn items(&self) -> Vec<Item> {
        match self {
            Category::Root { players } => vec![
                Item::TeleportToPlayer {
                    enabled: !players.is_empty(),
                },
                Item::TeleportToTeam,
            ],
            Category::Players(players) => players
                .iter()
                .map(|(id, name)| Item::Player {
                    id: *id,
                    name: name.clone(),
                })
                .collect(),
        }
    }

    fn prompt(&self) -> &'static str {
        match self {
            Category::Root { .. } => "spectatorMenu.root.prompt",
            Category::Players(_) => "spectatorMenu.teleport.prompt",
        }
    }
}

struct Menu {
    category: Category,
    selected: Option<usize>,
    page: usize,
}

impl Menu {
    /// `SpectatorMenu.getItem`.
    fn item(&self, slot: usize) -> Item {
        let items = self.category.items();
        let index = slot + self.page * 6;
        if self.page > 0 && slot == 0 {
            Item::ScrollLeft
        } else if slot == 7 {
            Item::ScrollRight {
                enabled: index < items.len(),
            }
        } else if slot == 8 {
            Item::Close
        } else {
            items.get(index).cloned().unwrap_or(Item::Empty)
        }
    }
}

/// What picking an item asks for.
pub enum SpectatorAction {
    None,
    TeleportTo(uuid::Uuid),
}

#[derive(Default)]
pub struct SpectatorMenu {
    menu: Option<Menu>,
    last_selection: Option<Instant>,
}

impl SpectatorMenu {
    pub fn is_open(&self) -> bool {
        self.menu.is_some()
    }

    /// `getHotbarAlpha`: full for five seconds, then gone over two.
    fn alpha(&self) -> f32 {
        let Some(at) = self.last_selection else {
            return 0.0;
        };
        let left = FADE_DELAY.as_secs_f32() * 1000.0 - at.elapsed().as_secs_f32() * 1000.0;
        (left / FADE_TIME).clamp(0.0, 1.0)
    }

    /// `onHotbarSelected`: a number key.
    pub fn select_slot(
        &mut self,
        slot: usize,
        players: Vec<(uuid::Uuid, String)>,
    ) -> SpectatorAction {
        self.last_selection = Some(Instant::now());
        let Some(menu) = &mut self.menu else {
            self.menu = Some(Menu {
                category: Category::Root { players },
                selected: None,
                page: 0,
            });
            return SpectatorAction::None;
        };
        let item = menu.item(slot);
        if item == Item::Empty {
            return SpectatorAction::None;
        }
        if menu.selected != Some(slot) || !item.enabled() {
            menu.selected = Some(slot);
            return SpectatorAction::None;
        }
        match item {
            Item::Close => self.menu = None,
            Item::ScrollLeft => menu.page = menu.page.saturating_sub(1),
            Item::ScrollRight { .. } => menu.page += 1,
            Item::TeleportToPlayer { .. } => {
                let players = match &menu.category {
                    Category::Root { players } => players.clone(),
                    Category::Players(players) => players.clone(),
                };
                menu.category = Category::Players(players);
                menu.selected = None;
                menu.page = 0;
            }
            Item::Player { id, .. } => return SpectatorAction::TeleportTo(id),
            Item::Empty | Item::TeleportToTeam => {}
        }
        SpectatorAction::None
    }

    /// `onHotbarActionKeyPressed`.
    pub fn action_key(&mut self, players: Vec<(uuid::Uuid, String)>) -> SpectatorAction {
        match self.menu.as_ref().and_then(|m| m.selected) {
            Some(slot) => self.select_slot(slot, players),
            None if self.menu.is_some() => {
                self.last_selection = Some(Instant::now());
                SpectatorAction::None
            }
            None => self.select_slot(0, players),
        }
    }

    /// `onMouseScrolled`: to the next enabled slot that way.
    pub fn scroll(&mut self, step: i32) {
        let Some(menu) = &mut self.menu else { return };
        let mut slot = menu.selected.map_or(-1, |s| s as i32) + step;
        while (0..=8).contains(&slot) {
            let item = menu.item(slot as usize);
            if item != Item::Empty && item.enabled() {
                break;
            }
            slot += step;
        }
        if (0..=8).contains(&slot) {
            menu.selected = Some(slot as usize);
            self.last_selection = Some(Instant::now());
        }
    }

    /// `extractHotbar` and `extractAction`.
    pub fn draw(
        &mut self,
        draw_list: &mut DrawList,
        (textures, hotbar): (&SpectatorTextures, (GuiSprite, GuiSprite)),
        skin_of: &dyn Fn(uuid::Uuid) -> TextureId,
        painter: ItemPainter,
        lang: &Lang,
        (width, height): (f32, f32),
    ) {
        let alpha = self.alpha();
        let Some(menu) = &self.menu else { return };
        if alpha <= 0.0 {
            self.menu = None;
            return;
        }
        let center = (width / 2.0).floor();
        let y = (height - 22.0 * alpha).floor();
        let white = [1.0, 1.0, 1.0, alpha];
        hotbar
            .0
            .draw_tinted(draw_list, center - 91.0, y, 182.0, 22.0, white);
        if let Some(selected) = menu.selected {
            hotbar.1.draw_tinted(
                draw_list,
                center - 91.0 - 1.0 + selected as f32 * 20.0,
                y - 1.0,
                24.0,
                23.0,
                white,
            );
        }
        for slot in 0..9 {
            let item = menu.item(slot);
            if item == Item::Empty {
                continue;
            }
            let x = center - 90.0 + slot as f32 * 20.0 + 2.0;
            let top = y + 3.0;
            let b = if item.enabled() { 1.0 } else { 0.25 };
            let tint = [b, b, b, alpha];
            let sprite = match &item {
                Item::Close => Some(textures.close),
                Item::ScrollLeft => Some(textures.scroll_left),
                Item::ScrollRight { .. } => Some(textures.scroll_right),
                Item::TeleportToPlayer { .. } => Some(textures.teleport_to_player),
                Item::TeleportToTeam => Some(textures.teleport_to_team),
                _ => None,
            };
            match sprite {
                Some(sprite) => {
                    sprite.draw_tinted(draw_list, x, top, 16.0, 16.0, tint);
                }
                None => {
                    if let Item::Player { id, .. } = &item {
                        crate::face::draw_face(
                            draw_list,
                            skin_of(*id),
                            [x + 2.0, top + 2.0],
                            12.0,
                            true,
                            false,
                            tint,
                        );
                    }
                }
            }
            if item.enabled() {
                let key = (slot + 1).to_string();
                let mut paint = painter.paint(colors::WHITE);
                paint.alpha = alpha;
                let text = StyledText::plain(&key);
                let w = text::width(painter.font, &text);
                text::draw(
                    draw_list,
                    &paint,
                    &text,
                    x + 19.0 - 2.0 - w,
                    top + 6.0 + 3.0,
                );
            }
        }
        let action = match menu.selected.map(|s| menu.item(s)) {
            Some(item) if item != Item::Empty => item.name(lang),
            _ => lang.get_dynamic(menu.category.prompt()).to_owned(),
        };
        let text = StyledText::plain(&action);
        let w = text::width(painter.font, &text);
        let mut paint = painter.paint(colors::WHITE);
        paint.alpha = alpha;
        text::draw(
            draw_list,
            &paint,
            &text,
            ((width - w) / 2.0).floor(),
            height - 35.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_presses_pick_a_player() {
        let id = uuid::Uuid::from_u128(7);
        let players = || vec![(id, "Alex".to_owned())];
        let mut gui = SpectatorMenu::default();
        gui.select_slot(0, players());
        assert!(gui.is_open());
        gui.select_slot(0, players());
        gui.select_slot(0, players());
        gui.select_slot(0, players());
        assert!(matches!(gui.select_slot(0, players()), SpectatorAction::TeleportTo(t) if t == id));
        gui.select_slot(8, players());
        gui.select_slot(8, players());
        assert!(!gui.is_open());
    }
}
