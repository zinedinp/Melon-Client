//! The in-game HUD (vanilla `client/gui/Hud.java`).

pub mod bars;
pub mod boss;
pub mod debug;
pub mod debug_entries;
pub mod effects;
pub mod icon_atlas;
pub mod item_icon;
pub mod item_slot;
pub mod player_tab;
pub mod sidebar;
pub mod subtitles;

use pumpkin_data::item_stack::ItemStack;

use crate::hud::item_slot::ItemPainter;
use crate::sprite::GuiSprite;
use crate::text::{self, StyledText, TextPaint, colors};
use melon_assets::assets::Assets;
use melon_base::gfx::quad::{BlendMode, DrawList};
use melon_core::math::argb;
use melon_render::Renderer;
use melon_settings::options::AttackIndicator;
use melon_world::entity::player::Arm;
use melon_world::game_mode::GameMode;
use melon_world::inventory::{Inventory, OFFHAND_SLOT};
pub use melon_world::java_random::JavaRandom;

/// `Hud.setOverlayMessage`: 3 seconds.
const OVERLAY_TICKS: u32 = 60;
/// `Hud.CROSSHAIR_SPRITE` is a 15x15 sprite.
const CROSSHAIR_SIZE: f32 = 15.0;
/// `Hud` status icons (hearts, food) are 9x9, 8 apart.
const ICON: f32 = 9.0;

/// `Hud`'s sprites.
pub struct HudTextures {
    pub boss: boss::BossSprites,
    crosshair: GuiSprite,
    hotbar: GuiSprite,
    hotbar_selection: GuiSprite,
    hotbar_offhand_left: GuiSprite,
    hotbar_offhand_right: GuiSprite,
    attack_crosshair_full: GuiSprite,
    attack_crosshair_background: GuiSprite,
    attack_crosshair_progress: GuiSprite,
    attack_hotbar_background: GuiSprite,
    attack_hotbar_progress: GuiSprite,
    bars: bars::BarSprites,
    experience_background: GuiSprite,
    experience_progress: GuiSprite,
    jump_background: GuiSprite,
    jump_progress: GuiSprite,
    pub effects: effects::EffectTextures,
}

impl HudTextures {
    /// The hotbar and its selection frame, for the spectator menu.
    pub fn hotbar_sprites(&self) -> (GuiSprite, GuiSprite) {
        (self.hotbar, self.hotbar_selection)
    }

    /// The heart sprites, for the tab list's health scores.
    pub fn hearts(&self) -> &bars::hearts::Sprites {
        &self.bars.hearts
    }

    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let boss = boss::BossSprites::load(renderer, assets)?;
        let mut load = |name: &str| {
            GuiSprite::load(
                renderer,
                assets,
                &format!("minecraft:gui/sprites/hud/{name}.png"),
            )
        };
        Ok(Self {
            boss,
            crosshair: load("crosshair")?,
            hotbar: load("hotbar")?,
            hotbar_selection: load("hotbar_selection")?,
            hotbar_offhand_left: load("hotbar_offhand_left")?,
            hotbar_offhand_right: load("hotbar_offhand_right")?,
            attack_crosshair_full: load("crosshair_attack_indicator_full")?,
            attack_crosshair_background: load("crosshair_attack_indicator_background")?,
            attack_crosshair_progress: load("crosshair_attack_indicator_progress")?,
            attack_hotbar_background: load("hotbar_attack_indicator_background")?,
            attack_hotbar_progress: load("hotbar_attack_indicator_progress")?,
            bars: bars::BarSprites::load(&mut load)?,
            experience_background: load("experience_bar_background")?,
            experience_progress: load("experience_bar_progress")?,
            jump_background: load("jump_bar_background")?,
            jump_progress: load("jump_bar_progress")?,
            effects: effects::EffectTextures::load(renderer, assets)?,
        })
    }
}

/// What the HUD shows about the player, from the server.
#[derive(Clone, Copy, Debug)]
pub struct PlayerStatus {
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub experience_progress: f32,
    pub experience_level: i32,
    pub game_mode: GameMode,
    pub hardcore: bool,
    /// The jump bar's fill (0 to 1) while riding a mount that jumps.
    pub jump_bar: Option<f32>,
    /// `Attributes.MAX_HEALTH` and `ARMOR`, as the server set them, and the absorption.
    pub max_health: f32,
    pub armor: i32,
    pub absorption: f32,
    /// The air supply in ticks, and whether the eyes are under water.
    pub air: i32,
    pub under_water: bool,
    /// What the effects and the cold make of the bars.
    pub hunger: bool,
    pub regeneration: bool,
    pub heart_kind: bars::hearts::HeartKind,
    /// The health and maximum of the mount ridden, if it has hearts.
    pub mount: Option<(f32, f32)>,
}

impl Default for PlayerStatus {
    fn default() -> Self {
        Self {
            health: 20.0,
            food: 20,
            saturation: 5.0,
            experience_progress: 0.0,
            experience_level: 0,
            game_mode: GameMode::Survival,
            hardcore: false,
            jump_bar: None,
            max_health: 20.0,
            armor: 0,
            absorption: 0.0,
            air: bars::air::MAX_AIR,
            under_water: false,
            hunger: false,
            regeneration: false,
            heart_kind: bars::hearts::HeartKind::Normal,
            mount: None,
        }
    }
}

impl PlayerStatus {
    /// `MultiPlayerGameMode.canHurtPlayer`: survival and adventure.
    pub fn can_hurt(&self) -> bool {
        self.game_mode.can_hurt()
    }
}

/// HUD state that lives across frames.
#[derive(Default)]
pub struct Hud {
    /// `Hud.tickCount`.
    ticks: u32,
    /// `Hud.toolHighlightTimer`: ticks the selected item's name still shows.
    tool_highlight_ticks: u32,
    /// `Hud.lastToolHighlight`: which item's name that is.
    highlighted: Option<(u16, StyledText)>,
    /// The action bar text and the ticks it still shows.
    overlay_message: Option<(StyledText, u32)>,
    /// The hearts' blink.
    bars: bars::BarState,
    /// The boss bars.
    pub bosses: boss::BossBars,
    title: Option<StyledText>,
    subtitle: Option<StyledText>,
    /// `Hud.titleTime`: ticks the title still shows.
    title_ticks: i32,
    /// `titleFadeInTime`, `titleStayTime`, `titleFadeOutTime`.
    title_times: Option<[i32; 3]>,
    /// `Gui.subtitleOverlay`.
    pub subtitles: subtitles::SubtitleOverlay,
}

/// `Hud.DEFAULT_TITLE_*`: 10, 70 and 20 ticks.
const DEFAULT_TITLE_TIMES: [i32; 3] = [10, 70, 20];

impl Hud {
    /// `Hud.tick`: once a game tick.
    pub fn tick(
        &mut self,
        selected: &ItemStack,
        status: &PlayerStatus,
        selected_name: impl FnOnce() -> StyledText,
    ) {
        self.ticks = self.ticks.wrapping_add(1);
        self.bars.tick(status, self.ticks);
        if self.title_ticks > 0 {
            self.title_ticks -= 1;
            if self.title_ticks == 0 {
                self.title = None;
                self.subtitle = None;
            }
        }
        if let Some((_, ticks)) = &mut self.overlay_message {
            *ticks = ticks.saturating_sub(1);
            if *ticks == 0 {
                self.overlay_message = None;
            }
        }
        if selected.is_empty() {
            self.tool_highlight_ticks = 0;
            self.highlighted = None;
            return;
        }
        let name = selected_name();
        if self.highlighted.as_ref().is_some_and(|(id, shown)| {
            *id == selected.item.id && shown.to_plain() == name.to_plain()
        }) {
            self.tool_highlight_ticks = self.tool_highlight_ticks.saturating_sub(1);
        } else {
            self.tool_highlight_ticks = 40;
            self.highlighted = Some((selected.item.id, name));
        }
    }

    /// `Hud.getGuiTicks`.
    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    /// `Hud.setOverlayMessage`: the action bar text, shown for three seconds.
    pub fn set_overlay_message(&mut self, content: StyledText) {
        self.overlay_message = Some((content, OVERLAY_TICKS));
    }

    fn times(&self) -> [i32; 3] {
        self.title_times.unwrap_or(DEFAULT_TITLE_TIMES)
    }

    /// `Hud.setTitle`.
    pub fn set_title(&mut self, title: StyledText) {
        let [a, b, c] = self.times();
        self.title = Some(title);
        self.title_ticks = a + b + c;
    }

    /// `Hud.setSubtitle`.
    pub fn set_subtitle(&mut self, subtitle: StyledText) {
        self.subtitle = Some(subtitle);
    }

    /// `Hud.setTimes`: a negative time keeps the old one.
    pub fn set_title_times(&mut self, fade_in: i32, stay: i32, fade_out: i32) {
        let old = self.times();
        let new = [fade_in, stay, fade_out];
        let times = [0, 1, 2].map(|i| if new[i] >= 0 { new[i] } else { old[i] });
        self.title_times = Some(times);
        if self.title_ticks > 0 {
            self.title_ticks = times.iter().sum();
        }
    }

    /// `Hud.clearTitles` and `resetTitleTimes`.
    pub fn clear_titles(&mut self, reset: bool) {
        self.title = None;
        self.subtitle = None;
        self.title_ticks = 0;
        if reset {
            self.title_times = None;
        }
    }

    /// `Hud.extractTitle`.
    pub fn draw_title(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        size: (f32, f32),
        partial: f32,
    ) {
        let (Some(title), true) = (&self.title, self.title_ticks > 0) else {
            return;
        };
        let [fade_in, stay, fade_out] = self.times();
        let t = self.title_ticks as f32 - partial;
        let alpha = if self.title_ticks > fade_out + stay {
            ((fade_in + stay + fade_out) as f32 - t) * 255.0 / fade_in.max(1) as f32
        } else if self.title_ticks <= fade_out {
            t * 255.0 / fade_out.max(1) as f32
        } else {
            255.0
        }
        .clamp(0.0, 255.0);
        if alpha <= 8.0 {
            return;
        }
        let paint = TextPaint {
            alpha: alpha / 255.0,
            obfuscation_seed: u64::from(self.ticks),
            ..painter.paint(colors::WHITE)
        };
        let (cx, cy) = (size.0 / 2.0, size.1 / 2.0);
        let mut line = |content: &StyledText, scale: f32, y: f32| {
            let start = draw_list.vertices_len();
            let x = cx / scale - text::width(painter.font, content) / 2.0;
            text::draw(draw_list, &paint, content, x, cy / scale + y);
            draw_list.scale_since(start, scale);
        };
        line(title, 4.0, -10.0);
        if let Some(subtitle) = &self.subtitle {
            line(subtitle, 2.0, 5.0);
        }
    }

    /// `Hud.extractOverlayMessage`.
    pub fn draw_overlay_message(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        size: (f32, f32),
        partial: f32,
    ) {
        let Some((content, ticks)) = &self.overlay_message else {
            return;
        };
        let t = *ticks as f32 - partial;
        let alpha = (t * 255.0 / 20.0).min(255.0);
        if alpha <= 8.0 {
            return;
        }
        let width = text::width(painter.font, content);
        let (screen_width, screen_height) = size;
        let x = (screen_width / 2.0).floor() - (width / 2.0).floor();
        let y = screen_height - 68.0 - 4.0;
        let paint = TextPaint {
            alpha: alpha / 255.0,
            obfuscation_seed: u64::from(self.ticks),
            ..painter.paint(colors::WHITE)
        };
        text::draw(draw_list, &paint, content, x, y);
    }

    /// Draws the HUD onto a `size` GUI screen.
    pub fn draw(
        &self,
        draw_list: &mut DrawList,
        textures: &HudTextures,
        painter: ItemPainter,
        player: HudPlayer,
        size: (f32, f32),
    ) {
        let HudPlayer {
            inventory,
            status,
            attack,
            first_person,
        } = player;
        let (width, height) = size;
        if first_person && !status.game_mode.is_spectator() {
            push_crosshair(draw_list, textures.crosshair, width, height);
            if attack.indicator == AttackIndicator::Crosshair {
                push_crosshair_cooldown(draw_list, textures, attack, width, height);
            }
        }
        if status.game_mode.is_spectator() {
            return;
        }
        let center = (width / 2.0).floor();
        push_hotbar(
            draw_list, textures, painter, inventory, attack, center, height,
        );
        if status.can_hurt() {
            match status.jump_bar {
                Some(charge) => push_jump_bar(draw_list, textures, charge, center, height),
                None => push_experience(draw_list, textures, painter, status, center, height),
            }
            bars::draw(
                draw_list,
                &textures.bars,
                &self.bars,
                status,
                self.ticks,
                center,
                height,
            );
        }
        self.push_selected_name(draw_list, painter, status, size);
    }

    /// `Hud.extractSelectedItemName`: fades out over its last 10 ticks.
    fn push_selected_name(
        &self,
        draw_list: &mut DrawList,
        painter: ItemPainter,
        status: &PlayerStatus,
        (width, height): (f32, f32),
    ) {
        let Some((_, name)) = &self.highlighted else {
            return;
        };
        let alpha = (self.tool_highlight_ticks as f32 * 256.0 / 10.0).min(255.0) / 255.0;
        if alpha <= 0.0 {
            return;
        }
        let x = ((width - text::width(painter.font, name)) / 2.0).floor();
        let mut y = height - 59.0;
        if !status.can_hurt() {
            y += 14.0;
        }
        let mut paint = painter.paint(colors::WHITE);
        paint.alpha = alpha;
        text::draw(draw_list, &paint, name, x, y);
    }
}

/// `Hud.extractItemHotbar`.
fn push_hotbar(
    draw_list: &mut DrawList,
    textures: &HudTextures,
    painter: ItemPainter,
    inventory: &Inventory,
    attack: AttackCooldown,
    center: f32,
    height: f32,
) {
    textures
        .hotbar
        .draw(draw_list, center - 91.0, height - 22.0, 182.0, 22.0);
    textures.hotbar_selection.draw(
        draw_list,
        center - 91.0 - 1.0 + f32::from(inventory.selected()) * 20.0,
        height - 22.0 - 1.0,
        24.0,
        23.0,
    );
    let item_y = height - 16.0 - 3.0;
    for i in 0..9 {
        let x = center - 90.0 + i as f32 * 20.0 + 2.0;
        painter.stack(draw_list, inventory.hotbar(i), x, item_y);
    }
    let offhand_left = attack.main_arm == Arm::Right;
    let offhand = &inventory.slots()[OFFHAND_SLOT];
    if !offhand.is_empty() {
        if offhand_left {
            textures.hotbar_offhand_left.draw(
                draw_list,
                center - 91.0 - 29.0,
                height - 23.0,
                29.0,
                24.0,
            );
            painter.stack(draw_list, offhand, center - 91.0 - 26.0, item_y);
        } else {
            textures
                .hotbar_offhand_right
                .draw(draw_list, center + 91.0, height - 23.0, 29.0, 24.0);
            painter.stack(draw_list, offhand, center + 91.0 + 10.0, item_y);
        }
    }
    if attack.indicator == AttackIndicator::Hotbar && attack.strength < 1.0 {
        let y = height - 20.0;
        let x = if offhand_left {
            center + 91.0 + 6.0
        } else {
            center - 91.0 - 22.0
        };
        let progress = (attack.strength * 19.0).floor().clamp(0.0, 18.0);
        textures
            .attack_hotbar_background
            .draw(draw_list, x, y, 18.0, 18.0);
        textures.attack_hotbar_progress.draw_region(
            draw_list,
            [18.0, 18.0],
            [0.0, 18.0 - progress],
            [x, y + 18.0 - progress, 18.0, progress],
        );
    }
}

/// What the HUD shows of the player.
#[derive(Clone, Copy)]
pub struct HudPlayer<'a> {
    pub inventory: &'a Inventory,
    pub status: &'a PlayerStatus,
    pub attack: AttackCooldown,
    /// The crosshair shows only in first person.
    pub first_person: bool,
}

/// What the attack cooldown shows: `Player.getAttackStrengthScale(0)`
#[derive(Clone, Copy, Debug)]
pub struct AttackCooldown {
    pub strength: f32,
    pub indicator: AttackIndicator,
    pub main_arm: Arm,
    /// A creature that can be hurt is under the crosshair and the weapon is charged.
    pub target_ready: bool,
}

/// `Hud.extractCrosshair`'s attack indicator.
fn push_crosshair_cooldown(
    draw_list: &mut DrawList,
    textures: &HudTextures,
    attack: AttackCooldown,
    width: f32,
    height: f32,
) {
    let strength = attack.strength;
    let y = ((height as i32) / 2 - 7 + 16) as f32;
    let x = ((width as i32) / 2 - 8) as f32;
    if strength >= 1.0 && attack.target_ready {
        draw_list.set_blend(BlendMode::Invert);
        textures
            .attack_crosshair_full
            .draw(draw_list, x, y, 16.0, 16.0);
        draw_list.set_blend(BlendMode::Alpha);
        return;
    }
    if strength >= 1.0 {
        return;
    }
    let progress = (strength * 17.0).floor().clamp(0.0, 16.0);
    draw_list.set_blend(BlendMode::Invert);
    textures
        .attack_crosshair_background
        .draw(draw_list, x, y, 16.0, 4.0);
    textures.attack_crosshair_progress.draw_region(
        draw_list,
        [16.0, 4.0],
        [0.0, 0.0],
        [x, y, progress, 4.0],
    );
    draw_list.set_blend(BlendMode::Alpha);
}

/// `JumpableVehicleBar.extractBackground`.
fn push_jump_bar(
    draw_list: &mut DrawList,
    textures: &HudTextures,
    charge: f32,
    center: f32,
    height: f32,
) {
    let left = center - 91.0;
    let top = height - 24.0 - 5.0;
    textures
        .jump_background
        .draw(draw_list, left, top, 182.0, 5.0);
    let progress = (charge * 181.0).floor() + if charge > 0.0 { 1.0 } else { 0.0 };
    if progress > 0.0 {
        textures.jump_progress.draw_region(
            draw_list,
            [182.0, 5.0],
            [0.0, 0.0],
            [left, top, progress.min(182.0), 5.0],
        );
    }
}

/// `ExperienceBar.extractBackground` and `extractRenderState`.
fn push_experience(
    draw_list: &mut DrawList,
    textures: &HudTextures,
    painter: ItemPainter,
    status: &PlayerStatus,
    center: f32,
    height: f32,
) {
    let left = center - 91.0;
    let top = height - 24.0 - 5.0;
    textures
        .experience_background
        .draw(draw_list, left, top, 182.0, 5.0);
    let progress = (status.experience_progress.clamp(0.0, 1.0) * 183.0)
        .floor()
        .min(182.0);
    if progress > 0.0 {
        textures.experience_progress.draw_region(
            draw_list,
            [182.0, 5.0],
            [0.0, 0.0],
            [left, top, progress, 5.0],
        );
    }
    if status.experience_level > 0 {
        let (font, pages) = (painter.font, painter.font_textures);
        let text = status.experience_level.to_string();
        let x = ((center * 2.0 - font.text_width(&text)) / 2.0).floor();
        let y = height - 24.0 - 9.0 - 2.0;
        let black = argb(0xFF00_0000);
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            font.push_text_flat(draw_list, pages, &text, x + dx, y + dy, black);
        }
        font.push_text_flat(draw_list, pages, &text, x, y, argb(0xFF80_FF20));
    }
}

/// `Hud.extractCrosshair`: `blitSprite(RenderPipelines.CROSSHAIR, ...)` in the middle.
pub fn push_crosshair(draw_list: &mut DrawList, crosshair: GuiSprite, width: f32, height: f32) {
    let x = ((width - CROSSHAIR_SIZE) as i32 / 2) as f32;
    let y = ((height - CROSSHAIR_SIZE) as i32 / 2) as f32;
    draw_list.set_blend(BlendMode::Invert);
    crosshair.draw(draw_list, x, y, CROSSHAIR_SIZE, CROSSHAIR_SIZE);
    draw_list.set_blend(BlendMode::Alpha);
}
