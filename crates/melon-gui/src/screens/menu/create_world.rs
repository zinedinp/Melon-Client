//! Vanilla `CreateWorldScreen`.

use melon_assets::i18n::Lang;
use melon_net::join::Target;
use crate::screen::{CursorKind, Screen, ScreenAction, ScreenContext, TextEvent};
use crate::screens::menu::connecting::ConnectingScreen;
use crate::screens::menu::experiments::ExperimentsScreen;
use crate::screens::menu::game_rules::GameRulesScreen;
use crate::screens::options::{on_off_label, value_label};
use crate::screens::packs::PackSelectionScreen;
use melon_net::singleplayer::creation::{CreationState, GAME_MODES};
use melon_net::singleplayer::rules::{self, Changes};
use melon_net::singleplayer::{
    self, Difficulty, WorldCreationMode, WorldEntry, WorldSettings, WorldType, data_packs,
};
use crate::text::font::LINE_HEIGHT;
use crate::widgets::Ui;
use crate::widgets::button::Button;
use crate::widgets::edit_box::EditBox;
use crate::widgets::layout::{BUTTON_H, BUTTON_W, HeaderFooter, SPACING};
use crate::widgets::set::{KeyResult, WidgetSet};
use crate::widgets::tab_bar::{self, TabBar};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// `CreateWorldScreen.repositionElements` sets the header height from the tab bar.
const LAYOUT: HeaderFooter = HeaderFooter::new(tab_bar::HEIGHT, 33.0);

/// `CommonLayouts.labeledElement`: a 9px label, `LABEL_SPACING` (4), the field.
const LABEL_OFFSET: f32 = LINE_HEIGHT + 4.0;
const LABELED_H: f32 = LABEL_OFFSET + BUTTON_H;

/// `GameTab`/`MoreTab` widgets: `create(0, 0, 210, 20, ..)` / `.width(210)`.
const TAB_BUTTON_W: f32 = 210.0;
/// `GameTab.nameEdit`: `new EditBox(font, 208, 20, ..)`.
const NAME_W: f32 = 208.0;
const GAME_GRID_W: f32 = TAB_BUTTON_W;
const GAME_GRID_H: f32 = LABELED_H + (SPACING + BUTTON_H) * 3.0;

/// `WorldTab.WORLD_TAB_WIDTH`; its grid is 2 columns of 150 with 10 between.
const WORLD_GRID_W: f32 = 310.0;
const WORLD_COL_W: f32 = 150.0;
const WORLD_COL_SPACING: f32 = 10.0;
/// `WorldTab.seedEdit`: `new EditBox(font, 308, 20, ..)`.
const SEED_W: f32 = 308.0;
/// `SwitchGrid.DEFAULT_SWITCH_BUTTON_WIDTH` and its `rowSpacing` (4).
const SWITCH_W: f32 = 44.0;
const SWITCH_ROW_SPACING: f32 = 4.0;
const SWITCH_GRID_H: f32 = (SWITCH_ROW_SPACING + BUTTON_H) * 2.0;
const WORLD_GRID_H: f32 = BUTTON_H + SPACING + LABELED_H + SPACING + SWITCH_GRID_H;

const MORE_GRID_W: f32 = TAB_BUTTON_W;
const MORE_GRID_H: f32 = BUTTON_H * 3.0 + SPACING * 2.0;

const TAB_TITLES: [&str; 3] = [
    "createWorld.tab.game.title",
    "createWorld.tab.world.title",
    "createWorld.tab.more.title",
];
const GAME: usize = 0;
const WORLD: usize = 1;

const PUMPKIN_STRUCTURES_SWITCH: bool = false;
const PUMPKIN_BONUS_CHEST: bool = false;
const PUMPKIN_PRESET_EDITOR: bool = false;

/// `FrameLayout.alignInDimension`.
fn align_in(pos: f32, length: f32, widget: f32, align: f32) -> f32 {
    pos + (align * (length - widget)).trunc()
}

/// `CycleButton.onOffBuilder` without its caption: just "ON"/"OFF".
fn on_off_value(lang: &Lang, value: bool) -> &str {
    lang.get(if value { "options.on" } else { "options.off" })
}

/// `CycleButton.cycleValue`: the value `delta` places on in `list`, wrapping.
fn step<T: Copy + PartialEq>(list: &[T], current: T, delta: isize) -> T {
    let at = list.iter().position(|&v| v == current).unwrap_or(0) as isize;
    list[(at + delta).rem_euclid(list.len() as isize) as usize]
}

/// `GridLayoutTab.doLayout`.
fn grid_origin(width: f32, height: f32, grid_w: f32, grid_h: f32) -> (f32, f32) {
    let area_h = LAYOUT.bottom(height) - LAYOUT.top();
    (
        align_in(0.0, width, grid_w, 0.5),
        align_in(LAYOUT.top(), area_h, grid_h, 1.0 / 6.0),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Id {
    Name,
    GameMode,
    Difficulty,
    AllowCheats,
    MapType,
    Customize,
    Seed,
    Structures,
    BonusChest,
    More(usize),
    Save,
    Cancel,
}

const GAME_TAB: [Id; 4] = [Id::Name, Id::GameMode, Id::Difficulty, Id::AllowCheats];
const WORLD_TAB: [Id; 5] = [
    Id::MapType,
    Id::Customize,
    Id::Seed,
    Id::Structures,
    Id::BonusChest,
];
const MORE_TAB: [Id; 3] = [Id::More(0), Id::More(1), Id::More(2)];

const MORE_KEYS: [&str; 3] = [
    "selectWorld.gameRules",
    "selectWorld.experiments",
    "selectWorld.dataPacks",
];

/// A tab's widgets.
fn tab_ids(tab: usize) -> &'static [Id] {
    match tab {
        GAME => &GAME_TAB,
        WORLD => &WORLD_TAB,
        _ => &MORE_TAB,
    }
}

/// The buttons that step through values: the wheel and Shift work on them.
fn is_cycle(id: Id) -> bool {
    matches!(
        id,
        Id::GameMode
            | Id::Difficulty
            | Id::AllowCheats
            | Id::MapType
            | Id::Structures
            | Id::BonusChest
    )
}

pub struct CreateWorldScreen {
    state: CreationState,
    /// The game rules that differ from the defaults, shared with the Game Rules screen.
    rules: Rc<RefCell<Changes>>,
    /// The enabled data packs and experiments (without `vanilla`), shared with their screens.
    packs: Rc<RefCell<Vec<String>>>,
    /// `tempDataPackDir`.
    temp_packs: Option<PathBuf>,
    /// Packs of the world this one was made from, copied when it opens.
    source_packs: Option<PathBuf>,
    tabs: TabBar,
    widgets: WidgetSet<Id>,
    mouse: (f32, f32),
}

impl CreateWorldScreen {
    pub fn new_create(ctx: &ScreenContext) -> Self {
        let state = CreationState::new(ctx.lang.get("selectWorld.newWorld").to_owned());
        Self::build(state, Changes::new(), Vec::new(), None)
    }

    /// `CreateWorldScreen.openCreateWorldScreen` from `recreateWorld`.
    pub fn new_recreate(entry: &WorldEntry, ctx: &ScreenContext) -> Self {
        let mut settings = entry.settings();
        if let Some(seed) = singleplayer::read_seed(ctx.game_dir, &entry.folder) {
            settings.seed = seed.to_string();
        }
        let world = singleplayer::world_dir(ctx.game_dir, &entry.folder);
        let changes = rules::read_changes(&world);
        let mut state = CreationState::new(settings.name.clone());
        state.seed = settings.seed.clone();
        state.world_type = settings.world_type;
        state.set_game_mode(settings.game_mode);
        state.set_difficulty(settings.difficulty);
        state.set_allow_commands(settings.allow_cheats);
        let source = data_packs::folder(&world);
        Self::build(
            state,
            changes,
            settings.data_packs.clone(),
            source.is_dir().then_some(source),
        )
    }

    fn build(
        state: CreationState,
        rules: Changes,
        packs: Vec<String>,
        source_packs: Option<PathBuf>,
    ) -> Self {
        let save_key = "selectWorld.create";
        let mut name = EditBox::new("selectWorld.newWorld", NAME_W, BUTTON_H);
        name.set_value(&state.name);
        let mut seed = EditBox::new("selectWorld.seedInfo", SEED_W, BUTTON_H);
        seed.set_value(&state.seed);
        let cycle = |w| Button::sized("", w, BUTTON_H);
        let mut widgets = WidgetSet::new()
            .with_field(Id::Name, name)
            .with_button(Id::GameMode, cycle(TAB_BUTTON_W))
            .with_button(Id::Difficulty, cycle(TAB_BUTTON_W))
            .with_button(Id::AllowCheats, cycle(TAB_BUTTON_W))
            .with_button(Id::MapType, cycle(WORLD_COL_W))
            .with_button(
                Id::Customize,
                Button::sized("selectWorld.customizeType", WORLD_COL_W, BUTTON_H),
            )
            .with_field(Id::Seed, seed)
            .with_button(Id::Structures, cycle(SWITCH_W))
            .with_button(Id::BonusChest, cycle(SWITCH_W));
        for (id, key) in MORE_TAB.into_iter().zip(MORE_KEYS) {
            widgets = widgets.with_button(id, Button::sized(key, TAB_BUTTON_W, BUTTON_H));
        }
        let widgets = widgets
            .with_button(Id::Save, Button::sized(save_key, BUTTON_W, BUTTON_H))
            .with_button(Id::Cancel, Button::sized("gui.cancel", BUTTON_W, BUTTON_H));
        let mut screen = Self {
            state,
            rules: Rc::new(RefCell::new(rules)),
            packs: Rc::new(RefCell::new(packs)),
            temp_packs: None,
            source_packs,
            tabs: TabBar::new(&TAB_TITLES),
            widgets,
            mouse: (-1.0, -1.0),
        };
        screen.set_tab(GAME);
        screen.widgets.focus(Id::Name);
        screen.update_enabled();
        screen
    }

    /// `CreateWorldScreen`'s listeners.
    fn update_enabled(&mut self) {
        let state = &self.state;
        let enabled = [
            (Id::GameMode, state.game_mode_active()),
            (Id::Difficulty, state.difficulty_active()),
            (Id::AllowCheats, state.allow_commands_active()),
            (
                Id::Customize,
                state.customize_active() && PUMPKIN_PRESET_EDITOR,
            ),
            (
                Id::Structures,
                state.structures_active() && PUMPKIN_STRUCTURES_SWITCH,
            ),
            (
                Id::BonusChest,
                state.bonus_chest_active() && PUMPKIN_BONUS_CHEST,
            ),
            (Id::Save, state.world_type.pumpkin_supported()),
        ];
        for (id, on) in enabled {
            self.widgets.set_enabled(id, on);
        }
    }

    /// Shows the tab's widgets; the focus doesn't carry over.
    fn set_tab(&mut self, tab: usize) {
        self.tabs.select(tab);
        self.widgets.blur();
        for other in [GAME, WORLD, 2] {
            for &id in tab_ids(other) {
                self.widgets.set_visible(id, other == tab);
            }
        }
    }

    /// One column, `rowSpacing(8)`.
    fn layout_game_tab(&mut self, width: f32, height: f32) {
        let (x, y) = grid_origin(width, height, GAME_GRID_W, GAME_GRID_H);
        self.widgets.set_position(
            Id::Name,
            align_in(x, GAME_GRID_W, NAME_W, 0.5),
            y + LABEL_OFFSET,
        );
        self.widgets.column(
            &[Id::GameMode, Id::Difficulty, Id::AllowCheats],
            x,
            y + LABELED_H + SPACING,
            BUTTON_H + SPACING,
        );
    }

    /// Two 150 columns with `columnSpacing(10)`.
    fn layout_world_tab(&mut self, width: f32, height: f32) {
        let (x, y) = grid_origin(width, height, WORLD_GRID_W, WORLD_GRID_H);
        self.widgets
            .row(&[Id::MapType, Id::Customize], x, y, WORLD_COL_SPACING);
        self.widgets
            .set_position(Id::Seed, x, y + BUTTON_H + SPACING + LABEL_OFFSET);
        let switch_top = y + BUTTON_H + SPACING + LABELED_H + SPACING;
        self.widgets.column(
            &[Id::Structures, Id::BonusChest],
            x + WORLD_GRID_W - SWITCH_W,
            switch_top + SWITCH_ROW_SPACING,
            BUTTON_H + SWITCH_ROW_SPACING,
        );
    }

    /// One column of three 210 wide buttons.
    fn layout_more_tab(&mut self, width: f32, height: f32) {
        let (x, y) = grid_origin(width, height, MORE_GRID_W, MORE_GRID_H);
        self.widgets.column(&MORE_TAB, x, y, BUTTON_H + SPACING);
    }

    /// The text fields' values go into the state (`setResponder`).
    fn read_fields(&mut self) {
        self.state.name = self.widgets.field(Id::Name).value();
        self.state.seed = self.widgets.field(Id::Seed).value();
    }

    /// `CycleButton.cycleValue` on one of the cycle buttons; `alt` is the alternate step.
    fn cycle(&mut self, id: Id, delta: isize, alt: bool) {
        let state = &mut self.state;
        match id {
            Id::GameMode => {
                let current = state.game_mode().unwrap_or(WorldCreationMode::Survival);
                state.set_game_mode(step(&GAME_MODES, current, delta));
            }
            Id::Difficulty => {
                let next = step(&Difficulty::ALL, state.difficulty(), delta);
                state.set_difficulty(next);
            }
            Id::AllowCheats => state.set_allow_commands(!state.allow_commands()),
            Id::Structures => state.set_generate_structures(!state.generate_structures()),
            Id::BonusChest => state.set_bonus_chest(!state.bonus_chest()),
            Id::MapType => state.world_type = state.world_type.stepped(alt, delta),
            _ => return,
        }
        self.update_enabled();
    }

    /// A click or Enter on a button.
    fn press(&mut self, id: Id, ctx: &ScreenContext) -> ScreenAction {
        let delta = if ctx.modifiers.shift { -1 } else { 1 };
        match id {
            Id::More(0) => {
                ctx.click();
                return ScreenAction::open(GameRulesScreen::new(
                    self.rules.clone(),
                    &self.packs.borrow(),
                ));
            }
            Id::More(1) => {
                ctx.click();
                return ScreenAction::open(ExperimentsScreen::new(self.packs.clone()));
            }
            Id::More(_) => {
                ctx.click();
                let dir = self.pack_dir();
                return ScreenAction::open(PackSelectionScreen::new_data(
                    ctx,
                    dir,
                    self.packs.clone(),
                ));
            }
            Id::Save => {
                ctx.click();
                return self.save(ctx);
            }
            Id::Cancel => {
                ctx.click();
                return ScreenAction::Back;
            }
            Id::Name | Id::Seed | Id::Customize => return ScreenAction::None,
            _ => {}
        }
        ctx.click();
        self.cycle(id, delta, ctx.modifiers.alt);
        ScreenAction::None
    }

    /// `getOrCreateTempDataPackDir`.
    fn pack_dir(&mut self) -> PathBuf {
        if let Some(dir) = &self.temp_packs {
            return dir.clone();
        }
        let dir = std::env::temp_dir().join(format!("melon-datapacks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        if let Some(source) = &self.source_packs
            && let Err(e) = data_packs::copy_into(source, &dir)
        {
            melon_base::warn!("World data packs not copied: {e}");
        }
        let _ = std::fs::create_dir_all(&dir);
        self.temp_packs = Some(dir.clone());
        dir
    }

    /// A cycle button's or switch's label, from the state.
    fn label(&self, id: Id, lang: &Lang) -> Option<String> {
        let state = &self.state;
        Some(match id {
            Id::GameMode => {
                let key = match state.game_mode() {
                    Some(mode) => mode_key(mode),
                    None => "selectWorld.gameMode.spectator",
                };
                value_label(lang, "selectWorld.gameMode", lang.get(key))
            }
            Id::Difficulty => value_label(
                lang,
                "options.difficulty",
                lang.get(state.difficulty().translation_key()),
            ),
            Id::AllowCheats => {
                on_off_label(lang, "selectWorld.allowCommands", state.allow_commands())
            }
            Id::MapType => value_label(
                lang,
                "selectWorld.mapType",
                lang.get(state.world_type.translation_key()),
            ),
            Id::Structures => on_off_value(lang, state.generate_structures()).to_owned(),
            Id::BonusChest => on_off_value(lang, state.bonus_chest()).to_owned(),
            _ => return None,
        })
    }

    /// The tooltip of a widget.
    fn tooltip(&self, id: Id, ctx: &ScreenContext) -> Option<String> {
        let lang = ctx.lang;
        let state = &self.state;
        let unsupported = |key: &str| lang.get_dynamic(key).to_owned();
        match id {
            Id::Name => {
                let typed = self.widgets.field(Id::Name).value();
                let folder = singleplayer::folder_name_for(ctx.game_dir, &typed);
                Some(lang.format("selectWorld.targetFolder", &[folder.as_str()]))
            }
            Id::GameMode => {
                let key = match state.game_mode() {
                    Some(mode) => format!("{}.info", mode_key(mode)),
                    None => "selectWorld.gameMode.spectator.info".to_owned(),
                };
                Some(lang.get_dynamic(&key).to_owned())
            }
            Id::Difficulty => Some(
                lang.get_dynamic(&format!(
                    "options.difficulty.{}.info",
                    state.difficulty().id()
                ))
                .to_owned(),
            ),
            Id::AllowCheats => Some(lang.get("selectWorld.allowCommands.info").to_owned()),
            Id::MapType => {
                if !state.world_type.pumpkin_supported() {
                    Some(unsupported("melon.pumpkin.unsupported.world_type"))
                } else if state.world_type == WorldType::Amplified {
                    Some(lang.get("generator.minecraft.amplified.info").to_owned())
                } else {
                    None
                }
            }
            Id::Customize => (state.customize_active() && !PUMPKIN_PRESET_EDITOR)
                .then(|| unsupported("melon.pumpkin.unsupported.customize")),
            Id::Structures => {
                let info = lang.get("selectWorld.mapFeatures.info").to_owned();
                if state.structures_active() && !PUMPKIN_STRUCTURES_SWITCH {
                    Some(format!(
                        "{info}\n{}",
                        unsupported("melon.pumpkin.unsupported.structures")
                    ))
                } else {
                    Some(info)
                }
            }
            Id::BonusChest => (state.bonus_chest_active() && !PUMPKIN_BONUS_CHEST)
                .then(|| unsupported("melon.pumpkin.unsupported.bonus_chest")),
            Id::Save => (!state.world_type.pumpkin_supported())
                .then(|| unsupported("melon.pumpkin.unsupported.world_type")),
            _ => None,
        }
    }

    /// The labels the tabs draw beside their widgets.
    fn draw_labels(&self, ui: &mut Ui) {
        let lang = ui.lang;
        let label_above = |ui: &mut Ui, key: &'static str, id: Id| {
            let field = self.widgets.field(id);
            ui.text(lang.get(key), field.x, field.y - LABEL_OFFSET, [1.0; 4]);
        };
        match self.tabs.selected() {
            GAME => label_above(ui, "selectWorld.enterName", Id::Name),
            WORLD => {
                label_above(ui, "selectWorld.enterSeed", Id::Seed);
                for (key, id) in [
                    ("selectWorld.mapFeatures", Id::Structures),
                    ("selectWorld.bonusItems", Id::BonusChest),
                ] {
                    let button = self.widgets.button(id);
                    let x = button.x - (WORLD_GRID_W - SWITCH_W);
                    ui.text(lang.get(key), x, button.y + 6.0, [1.0; 4]);
                }
            }
            _ => {}
        }
    }

    /// Saves the world's settings, then plays the new world right away.
    fn save(&mut self, ctx: &ScreenContext) -> ScreenAction {
        if !self.state.world_type.pumpkin_supported() {
            return ScreenAction::None;
        }
        self.read_fields();
        let name = self.state.name.clone();
        let name = if name.trim().is_empty() {
            ctx.lang.get("selectWorld.newWorld").to_owned()
        } else {
            name
        };
        let folder = singleplayer::folder_name_for(ctx.game_dir, &name);
        let settings = WorldSettings {
            name,
            game_mode: self.state.world_game_mode(),
            seed: self.state.seed.clone(),
            difficulty: self.state.difficulty(),
            allow_cheats: self.state.allow_commands(),
            world_type: self.state.world_type,
            data_packs: self.packs.borrow().clone(),
        };
        if let Err(e) = singleplayer::save_world(ctx.game_dir, &folder, &settings) {
            melon_base::warn!("World not saved ({folder}): {e:#}");
            return ScreenAction::Back;
        }
        let world_dir = singleplayer::world_dir(ctx.game_dir, &folder);
        if let Err(e) = rules::write_changes(&world_dir, &self.rules.borrow()) {
            melon_base::warn!("Game rules not saved ({folder}): {e:#}");
        }
        if let Some(temp) = &self.temp_packs
            && let Err(e) = data_packs::copy_into(temp, &data_packs::folder(&world_dir))
        {
            melon_base::warn!("Data packs not copied ({folder}): {e:#}");
        }
        let target = Target::World {
            save_path: world_dir,
            game_dir: ctx.game_dir.to_path_buf(),
            settings,
        };
        ScreenAction::open(ConnectingScreen::new(target, ctx))
    }
}

/// `selectWorld.gameMode.<mode>`, the Game Mode button's value texts.
fn mode_key(mode: WorldCreationMode) -> &'static str {
    match mode {
        WorldCreationMode::Survival => "selectWorld.gameMode.survival",
        WorldCreationMode::Hardcore => "selectWorld.gameMode.hardcore",
        WorldCreationMode::Creative => "selectWorld.gameMode.creative",
        WorldCreationMode::Adventure => "selectWorld.gameMode.adventure",
    }
}

impl Drop for CreateWorldScreen {
    /// `removeTempDataPackDir`.
    fn drop(&mut self) {
        if let Some(dir) = &self.temp_packs {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

impl Screen for CreateWorldScreen {
    fn resize(&mut self, ctx: &ScreenContext) {
        let (width, height) = ctx.size();
        self.tabs.resize(width);
        self.layout_game_tab(width, height);
        self.layout_world_tab(width, height);
        self.layout_more_tab(width, height);
        let button_y = LAYOUT.bottom(height) + 7.0;
        self.widgets
            .centered_row(&[Id::Save, Id::Cancel], width, button_y, SPACING);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        self.tabs.on_mouse_move(x, y);
        self.widgets.hover(x, y);
    }

    fn cursor(&self) -> CursorKind {
        if self.tabs.hovered() {
            return CursorKind::Pointer;
        }
        self.widgets.cursor().unwrap_or_default()
    }

    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction {
        if let Some(tab) = self.tabs.on_click(x, y) {
            if tab != self.tabs.selected() {
                ctx.click();
                self.set_tab(tab);
            }
            return ScreenAction::None;
        }
        match self.widgets.click(x, y) {
            Some(id) => self.press(id, ctx),
            None => ScreenAction::None,
        }
    }

    /// `CycleButton.mouseScrolled`: up steps back, down forward.
    fn on_scroll(&mut self, delta: f32) {
        let Some((id, _)) = self.widgets.at(self.mouse.0, self.mouse.1) else {
            return;
        };
        if is_cycle(id) && self.widgets.button(id).enabled && delta != 0.0 {
            self.cycle(id, if delta > 0.0 { -1 } else { 1 }, false);
        }
    }

    fn on_text_event(&mut self, event: TextEvent, ctx: &ScreenContext) -> ScreenAction {
        match self.widgets.key(&event) {
            KeyResult::Pressed(id) => self.press(id, ctx),
            KeyResult::Ignored if matches!(event, TextEvent::Enter) => self.save(ctx),
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext) {
        crate::background::push_menu_overlay_rect(
            ui.draw_list,
            ui.assets.white,
            0.0,
            LAYOUT.top(),
            ui.width,
            ui.height - LAYOUT.top(),
        );
        self.tabs.draw(ui);

        let separator = ui.assets.footer_separator;
        let separator_y = LAYOUT.bottom(ui.height) - 2.0;
        let width = ui.width;
        ui.draw_list
            .push_rect_tiled_x(separator, [0.0, separator_y, width, 2.0], 32.0, [1.0; 4]);

        self.draw_labels(ui);
        let lang = ui.lang;
        self.widgets.draw_with(ui, |id| self.label(id, lang));

        if let Some((id, rect)) = self.widgets.at(self.mouse.0, self.mouse.1)
            && let Some(text) = self.tooltip(id, ctx)
        {
            ui.widget_tooltip(&text, self.mouse, rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_wraps_in_both_directions() {
        assert_eq!(
            step(&GAME_MODES, WorldCreationMode::Survival, -1),
            WorldCreationMode::Creative
        );
        assert_eq!(
            step(&GAME_MODES, WorldCreationMode::Creative, 1),
            WorldCreationMode::Survival
        );
        assert_eq!(
            step(&Difficulty::ALL, Difficulty::Peaceful, -1),
            Difficulty::Hard
        );
    }
}
