//! Vanilla `OptionsScreen`.

pub mod accessibility;
pub mod chat;
pub mod controls;
pub mod debug;
pub mod keybinds;
pub mod language;
pub mod melon;
pub mod mouse;
pub mod skin;
pub mod sounds;
pub mod video;

use melon_assets::i18n::Lang;
use crate::screen::{ConfigChange, CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::placeholder::placeholder_screen;
use crate::screens::options::accessibility::accessibility_options_screen;
use crate::screens::options::chat::chat_options_screen;
use crate::screens::options::controls::controls_screen;
use crate::screens::options::language::LanguageScreen;
use crate::screens::options::melon::melon_settings_screen;
use crate::screens::options::skin::skin_customization_screen;
use crate::screens::options::sounds::sound_options_screen;
use crate::screens::options::video::video_settings_screen;
use melon_settings::options::{self, Config};
use crate::widgets::button::Button;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, WIDE_BUTTON_W};
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::slider::Slider;
use crate::widgets::{Ui, Widget};

/// `OptionsScreen`'s `HeaderAndFooterLayout(this, 61, 33)`.
const LAYOUT: HeaderFooter = HeaderFooter::new(61.0, 33.0);
/// Header contents: the title, 8 below it the FOV/Online row.
const HEADER_SPACING: f32 = 8.0;
const TITLE_H: f32 = 9.0;
/// `GridLayout.defaultCellSetting().paddingHorizontal(4).paddingBottom(4)`.
const CELL_PADDING: f32 = 4.0;

/// The sub-menu grid, row by row: (label key, what it opens).
const SUB_MENUS: [(&str, SubMenu); 11] = [
    ("options.skinCustomisation", SubMenu::Skin),
    ("options.sounds", SubMenu::Sounds),
    ("options.video", SubMenu::Video),
    ("options.controls", SubMenu::Controls),
    ("options.language", SubMenu::Language),
    ("options.chat", SubMenu::Chat),
    ("options.resourcepack", SubMenu::ResourcePacks),
    ("options.accessibility", SubMenu::Accessibility),
    ("options.telemetry", SubMenu::Placeholder),
    ("options.credits_and_attribution", SubMenu::Placeholder),
    ("melon.options.title", SubMenu::Melon),
];

#[derive(Clone, Copy)]
enum SubMenu {
    Skin,
    Accessibility,
    Video,
    Chat,
    Language,
    Controls,
    Sounds,
    Melon,
    ResourcePacks,
    Placeholder,
}

/// `Options.genericValueLabel`: `"<caption>: <value>"`.
pub fn value_label(lang: &Lang, caption: &'static str, value: &str) -> String {
    lang.format("options.generic_value", &[lang.get(caption), value])
}

/// `Options.fov`'s caption: "Normal" at 70, "Quake Pro" at 110.
pub fn fov_label(lang: &Lang, fov: u8) -> String {
    let value = match fov {
        options::FOV_DEFAULT => lang.get("options.fov.min").to_owned(),
        110 => lang.get("options.fov.max").to_owned(),
        _ => fov.to_string(),
    };
    value_label(lang, "options.fov", &value)
}

/// `Options.renderDistance`'s caption: "Render Distance: 12 Chunks".
pub fn render_distance_label(lang: &Lang, chunks: u8) -> String {
    chunks_label(lang, "options.renderDistance", chunks)
}

/// `Options.simulationDistance`'s caption: "Simulation Distance: 12 Chunks".
pub fn simulation_distance_label(lang: &Lang, chunks: u8) -> String {
    chunks_label(lang, "options.simulationDistance", chunks)
}

fn chunks_label(lang: &Lang, key: &'static str, chunks: u8) -> String {
    let value = lang.format("options.chunks", &[&chunks.to_string()]);
    value_label(lang, key, &value)
}

/// `Options.percentValueLabel` for `entityDistanceScaling`.
pub fn entity_distance_label(lang: &Lang, scale: f32) -> String {
    let percent = ((scale * 100.0).round() as i32).to_string();
    lang.format(
        "options.percent_value",
        &[lang.get("options.entityDistanceScaling"), &percent],
    )
}

/// `Options.gamma`'s caption: Moody at 0, Default at 50, Bright at 100.
pub fn gamma_label(lang: &Lang, gamma: f64) -> String {
    let percent = (gamma * 100.0) as i32;
    let value = match percent {
        0 => lang.get("options.gamma.min").to_owned(),
        50 => lang.get("options.gamma.default").to_owned(),
        100 => lang.get("options.gamma.max").to_owned(),
        n => n.to_string(),
    };
    value_label(lang, "options.gamma", &value)
}

/// `Options.biomeBlendRadius`'s caption: "Biome Blend: 5x5 (Normal)".
pub fn biome_blend_label(lang: &Lang, radius: u8) -> String {
    let side = u32::from(radius) * 2 + 1;
    let key = format!("options.biomeBlendRadius.{side}");
    value_label(lang, "options.biomeBlendRadius", lang.get_dynamic(&key))
}

/// `Options.percentValueLabel` of a `0..=1` option: "FOV Effects: 100%".
pub fn percent_label(lang: &Lang, caption: &'static str, value: f64) -> String {
    let percent = ((value * 100.0).round() as i32).to_string();
    lang.format("options.percent_value", &[lang.get(caption), &percent])
}

/// `CycleButton.onOffBuilder`'s caption: `<caption>: ON/OFF`.
pub fn on_off_label(lang: &Lang, caption: &'static str, on: bool) -> String {
    let state = lang.get(if on { "options.on" } else { "options.off" });
    value_label(lang, caption, state)
}

/// The FOV slider for a config: 30..=110, one step per degree.
fn fov_slider(config: &Config) -> Slider {
    let (min, max) = options::FOV_RANGE;
    Slider::new(
        u32::from(max - min),
        u32::from(config.fov.saturating_sub(min)),
        BUTTON_W,
        BUTTON_H,
    )
}

/// The FOV slider moved to `step`.
fn fov_change(step: u32) -> ConfigChange {
    Box::new(move |config| config.fov = options::FOV_RANGE.0 + step as u8)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Online,
    /// An index into [`SUB_MENUS`].
    SubMenu(usize),
    Done,
}

pub struct OptionsScreen {
    fov: Slider,
    /// Changes not yet handed out through [`Screen::poll`].
    pending: Vec<ConfigChange>,
    widgets: WidgetSet<Id>,
}

impl OptionsScreen {
    pub fn new(ctx: &ScreenContext) -> Self {
        Self {
            fov: fov_slider(ctx.config),
            pending: Vec::new(),
            widgets: SUB_MENUS
                .iter()
                .enumerate()
                .fold(
                    WidgetSet::new().with_button(
                        Id::Online,
                        Button::sized("options.online", BUTTON_W, BUTTON_H),
                    ),
                    |set, (i, &(key, _))| {
                        set.with_button(Id::SubMenu(i), Button::sized(key, BUTTON_W, BUTTON_H))
                    },
                )
                .with_button(Id::Done, Button::sized("gui.done", WIDE_BUTTON_W, BUTTON_H)),
        }
    }

    fn press(id: Id, ctx: &ScreenContext) -> ScreenAction {
        ctx.click();
        match id {
            Id::Online => Self::open(SubMenu::Placeholder, "options.online", ctx),
            Id::SubMenu(i) => {
                let (key, menu) = SUB_MENUS[i];
                Self::open(menu, key, ctx)
            }
            Id::Done => ScreenAction::Back,
        }
    }

    fn open(menu: SubMenu, key: &'static str, ctx: &ScreenContext) -> ScreenAction {
        match menu {
            SubMenu::Skin => ScreenAction::open(skin_customization_screen(ctx)),
            SubMenu::Video => ScreenAction::open(video_settings_screen(ctx)),
            SubMenu::Chat => ScreenAction::open(chat_options_screen(ctx)),
            SubMenu::Accessibility => ScreenAction::open(accessibility_options_screen(ctx)),
            SubMenu::Language => ScreenAction::open(LanguageScreen::new(ctx)),
            SubMenu::Controls => ScreenAction::open(controls_screen(ctx)),
            SubMenu::Sounds => ScreenAction::open(sound_options_screen(ctx)),
            SubMenu::Melon => ScreenAction::open(melon_settings_screen(ctx)),
            SubMenu::ResourcePacks => {
                ScreenAction::open(crate::screens::packs::PackSelectionScreen::new(ctx))
            }
            SubMenu::Placeholder => ScreenAction::open(placeholder_screen(key)),
        }
    }
}

impl Screen for OptionsScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        let center = width / 2.0;

        let header_contents = TITLE_H + HEADER_SPACING + BUTTON_H;
        let row_y = ((LAYOUT.header - header_contents) / 2.0).floor() + TITLE_H + HEADER_SPACING;
        self.fov.x = center - BUTTON_W - HEADER_SPACING / 2.0;
        self.fov.y = row_y;
        self.widgets
            .set_position(Id::Online, center + HEADER_SPACING / 2.0, row_y);

        let cell_w = BUTTON_W + CELL_PADDING * 2.0;
        let cell_h = BUTTON_H + CELL_PADDING;
        let rows = SUB_MENUS.len().div_ceil(2) as f32;
        let grid_y = (LAYOUT.header + 30.0).min(LAYOUT.bottom(height) - rows * cell_h);
        let grid_x = (center - cell_w).floor();
        for i in 0..SUB_MENUS.len() {
            self.widgets.set_position(
                Id::SubMenu(i),
                grid_x + CELL_PADDING + (i % 2) as f32 * cell_w,
                grid_y + (i / 2) as f32 * cell_h,
            );
        }

        self.widgets.set_position(
            Id::Done,
            (center - WIDE_BUTTON_W / 2.0).floor(),
            LAYOUT.footer_button_y(height),
        );
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.widgets.hover(x, y);
        self.fov.update_hover(x, y);
        if self.fov.drag(x) {
            self.pending.push(fov_change(self.fov.step()));
        }
    }

    fn on_mouse_up(&mut self, _x: f32, _y: f32, ctx: &ScreenContext) {
        if self.fov.release() {
            ctx.click();
        }
    }

    fn cursor(&self) -> CursorKind {
        self.widgets
            .cursor()
            .or(Widget::cursor(&self.fov))
            .unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if let Some(changed) = self.fov.press(x, y) {
            if changed {
                self.pending.push(fov_change(self.fov.step()));
            }
            return self.poll();
        }
        match self.widgets.click(x, y) {
            Some(id) => Self::press(id, ctx),
            None => ScreenAction::None,
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => Self::press(id, ctx),
            _ => ScreenAction::None,
        }
    }

    fn poll(&mut self) -> ScreenAction {
        ScreenAction::change_config(std::mem::take(&mut self.pending))
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        ui.menu_background();
        let title_y = ((LAYOUT.header - (TITLE_H + HEADER_SPACING + BUTTON_H)) / 2.0).floor();
        let title = ui.tr("options.title");
        ui.title(title, title_y);
        let fov = fov_label(ui.lang, ctx.config.fov);
        self.fov.draw_labeled(ui, &fov);
        self.widgets.draw(ui);
    }

    fn refresh_config(&mut self, config: &Config) {
        self.fov
            .set_step(u32::from(config.fov.saturating_sub(options::FOV_RANGE.0)));
    }
}
