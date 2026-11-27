//! The screen interface the app's screen stack drives, and the GUI scale.

use std::path::Path;

use crate::assets::GuiAssets;
use crate::widgets::Ui;
use melon_assets::i18n::Lang;
use melon_assets::i18n::languages::Language;
use melon_audio::AudioEngine;
use melon_render::TextureId;
use melon_settings::options::Config;

/// A change to the options, applied and saved at once, as vanilla's options screens do.
pub type ConfigChange = Box<dyn FnOnce(&mut Config)>;

/// Click result for the app's screen stack.
pub enum ScreenAction {
    None,
    Quit,
    Back,
    Open(Box<dyn Screen>),
    /// Closes this screen and opens another in its place.
    Replace(Box<dyn Screen>),
    SetLanguage(Language),
    /// Joined a world: leave the menus and start playing it.
    EnterWorld(Box<melon_net::join::Joined>),
    /// The pause menu's Disconnect / Save and Quit to Title.
    LeaveWorld,
    /// The death screen's Respawn.
    Respawn,
    /// An options screen changed a setting.
    ChangeConfig(ConfigChange),
    /// `Util.getPlatform().openUri`, then back.
    OpenUrl(String),
    /// Puts text on the clipboard, then back.
    Copy(String),
    /// Puts text on the clipboard and leaves the screens as they are.
    Clipboard(String),
    /// The pack screen's Done.
    SetResourcePacks {
        selected: Vec<String>,
        incompatible: Vec<String>,
    },
    /// The answer to the server's resource pack prompt.
    AnswerServerPacks(bool),
    /// F3 + T: the resource packs read again.
    ReloadResources,
    /// A server dialog's button, link or Escape, or a dialog to open.
    Dialog(crate::screens::dialog::DialogRequest),
    /// The pause menu's World Options: opened by the app, which knows the world.
    OpenWorldOptions,
    /// World Options' Apply Changes, then back.
    WorldOptions(Box<crate::screens::ingame::world_options::WorldOptionsChange>),
}

impl ScreenAction {
    /// Opens a screen.
    pub fn open(screen: impl Screen + 'static) -> Self {
        ScreenAction::Open(Box::new(screen))
    }

    /// Several config changes as one.
    pub fn change_config(changes: Vec<ConfigChange>) -> Self {
        if changes.is_empty() {
            return ScreenAction::None;
        }
        ScreenAction::ChangeConfig(Box::new(move |config| {
            for change in changes {
                change(config);
            }
        }))
    }
}

/// `AbstractWidget.handleCursor`: `CursorTypes.POINTING_HAND`/`IBEAM` while hovered.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CursorKind {
    #[default]
    Default,
    Pointer,
    Text,
    /// `AbstractScrollArea.extractScrollbar`.
    ResizeNs,
    /// `AbstractSliderButton.handleCursor`: `RESIZE_EW` while dragging.
    ResizeEw,
}

/// Keyboard input for the focused screen. Subset of `EditBox.keyPressed` / `charTyped`.
pub enum TextEvent {
    Char(char),
    /// Text an input method is composing (`Ime::Preedit`).
    Preedit(String),
    /// Backspace and Delete; with Control a whole word.
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    /// The arrow keys.
    Left {
        shift: bool,
        word: bool,
    },
    Right {
        shift: bool,
        word: bool,
    },
    Home {
        shift: bool,
    },
    End {
        shift: bool,
    },
    /// Control+A, Control+C, Control+X.
    SelectAll,
    Copy,
    Cut,
    Enter,
    Up,
    Down,
    PageUp,
    PageDown,
    /// The Insert key.
    Insert,
    /// Tab, `shift` for backwards.
    Tab {
        shift: bool,
    },
    /// Control+V: the clipboard's text.
    Paste(String),
}

/// Everything a screen may need besides its own state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

pub struct ScreenContext<'a> {
    pub assets: &'a GuiAssets,
    pub lang: &'a Lang,
    pub audio: &'a AudioEngine,
    /// The options as they are now.
    pub config: &'a Config,
    /// The `assets/` folder.
    pub assets_root: &'a Path,
    /// The game folder: `options.txt`, `servers.json`, `saves/`, beside `assets/`.
    pub game_dir: &'a Path,
    /// The main game folder, where the accounts are whichever instance runs.
    pub account_dir: &'a Path,
    pub scale: GuiScale,
    pub modifiers: Modifiers,
}

impl ScreenContext<'_> {
    /// The GUI's size in GUI pixels.
    pub fn size(&self) -> (f32, f32) {
        self.scale.virtual_size()
    }

    pub fn width(&self) -> f32 {
        self.size().0
    }

    pub fn height(&self) -> f32 {
        self.size().1
    }

    /// `AbstractButton.playDownSound`.
    pub fn click(&self) {
        self.audio.play_click();
    }
}

pub trait Screen {
    /// Lays the screen out for the GUI's size (vanilla `Screen.init`).
    fn resize(&mut self, ctx: &ScreenContext);
    fn tick(&mut self) {}
    /// Called before each draw of the top menu screen.
    fn upload_images(&mut self, _upload: &mut dyn FnMut(&image::RgbaImage) -> Option<TextureId>) {}
    fn on_mouse_move(&mut self, x: f32, y: f32);
    fn on_click(&mut self, x: f32, y: f32, ctx: &ScreenContext) -> ScreenAction;
    /// Vanilla `AbstractScrollArea.onRelease`. Ends a scrollbar drag.
    fn on_mouse_up(&mut self, _x: f32, _y: f32, _ctx: &ScreenContext) {}
    /// Scroll wheel: positive = up/back.
    fn on_scroll(&mut self, _delta: f32) {}
    /// A key or typed text. Enter can submit, so it may act.
    fn on_text_event(&mut self, _event: TextEvent, _ctx: &ScreenContext) -> ScreenAction {
        ScreenAction::None
    }
    fn draw(&self, ui: &mut Ui, ctx: &ScreenContext);
    /// Rebuild text made from translations after a language change.
    fn refresh_lang(&mut self, _lang: &Lang) {}
    /// Options changed (here or in a sub-screen): show the new values.
    fn refresh_config(&mut self, _config: &Config) {}
    /// OS cursor for the last position reported to `on_mouse_move`.
    fn cursor(&self) -> CursorKind {
        CursorKind::Default
    }
    /// Checked once a frame for the top screen, for actions that aren't a click.
    fn poll(&mut self) -> ScreenAction {
        ScreenAction::None
    }
    /// Whether this is the screen shown while a new level loads.
    fn is_level_loading(&self) -> bool {
        false
    }
    /// `Screen.isPauseScreen`: a singleplayer world stands still behind it.
    fn is_pause_screen(&self) -> bool {
        !self.is_level_loading()
    }
    /// Whether leaving a world returns to this screen (`Minecraft.disconnect`'s next screen).
    fn is_world_exit(&self, _integrated: bool) -> bool {
        false
    }
    /// `Screen.extractBackground`.
    fn blurs_background(&self) -> bool {
        true
    }
    /// Escape on a screen that closes on it.
    fn on_escape(&mut self, _ctx: &ScreenContext) -> Option<ScreenAction> {
        None
    }
    /// `Screen.onFilesDrop`: files dropped on the window.
    fn on_files_dropped(
        &mut self,
        _paths: Vec<std::path::PathBuf>,
        _ctx: &ScreenContext,
    ) -> ScreenAction {
        ScreenAction::None
    }
    /// `Screen.shouldCloseOnEsc`: the death screen stays until a button.
    fn closes_on_escape(&self) -> bool {
        true
    }
    /// Top of the stack again after a sub-screen is popped (`Back`/Escape).
    fn on_resume(&mut self, _ctx: &ScreenContext) {}
    /// `KeyBindsScreen.selectedKey != null`: Escape unbinds instead of closing.
    fn capturing_bind(&self) -> bool {
        false
    }
    /// What a server's dialog showing up makes of this screen: a dialog or
    /// the wait for one is replaced, the warning keeps it behind.
    fn dialog_layer(&self) -> Option<crate::screens::dialog::DialogLayer> {
        None
    }
    /// The item stacks this screen draws, so their icons are made before the
    /// frame (in a world; see [`crate::widgets::Ui::items`]).
    fn visit_stacks(&self, _visit: &mut dyn FnMut(&dyn melon_world::item_model::StackKey) -> bool) {
    }
    /// A physical key or mouse button while [`Self::capturing_bind`].
    fn on_bind(
        &mut self,
        _bind: melon_settings::input::InputBind,
        _ctx: &ScreenContext,
    ) -> ScreenAction {
        ScreenAction::None
    }
}

/// The window's size in physical pixels and `Options.guiScale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuiScale {
    /// `0` for Auto (the largest scale that fits), otherwise the scale asked for.
    pub setting: u8,
    pub window_width: u32,
    pub window_height: u32,
}

impl GuiScale {
    pub fn new(setting: u8, window_width: u32, window_height: u32) -> Self {
        Self {
            setting,
            window_width: window_width.max(1),
            window_height: window_height.max(1),
        }
    }

    /// The scale the GUI is drawn at.
    pub fn scale(&self) -> u32 {
        calculate_scale(
            u32::from(self.setting),
            self.window_width,
            self.window_height,
        )
    }

    /// `ClampingLazyMaxIntRange`'s top.
    pub fn max_scale(&self) -> u8 {
        calculate_scale(0, self.window_width, self.window_height).min(255) as u8
    }

    /// `(virtual_width, virtual_height)`.
    pub fn virtual_size(&self) -> (f32, f32) {
        let scale = self.scale();
        (
            self.window_width.div_ceil(scale).max(1) as f32,
            self.window_height.div_ceil(scale).max(1) as f32,
        )
    }

    /// The GUI pixels the window shows, fractional (`GuiRenderer`'s ortho
    /// `width / guiScale`): drawn over this, one GUI pixel is exactly
    /// `scale` window pixels. [`Self::virtual_size`] rounds up for layout,
    /// so its last partial pixel runs off the edge, as in vanilla.
    pub fn projection_size(&self) -> (f32, f32) {
        let scale = self.scale() as f32;
        (
            self.window_width as f32 / scale,
            self.window_height as f32 / scale,
        )
    }

    /// GUI pixels to window pixels: the inverse of [`Self::to_virtual`].
    pub fn to_physical(&self, position: (f32, f32)) -> (f64, f64) {
        let (virtual_w, virtual_h) = self.virtual_size();
        (
            f64::from(position.0) * f64::from(self.window_width) / f64::from(virtual_w),
            f64::from(position.1) * f64::from(self.window_height) / f64::from(virtual_h),
        )
    }

    /// `MouseHandler.getScaledXPos`/`getScaledYPos`: a physical cursor position in GUI pixels.
    pub fn to_virtual(&self, position: (f64, f64)) -> (f32, f32) {
        let (virtual_w, virtual_h) = self.virtual_size();
        (
            (position.0 * f64::from(virtual_w) / f64::from(self.window_width)) as f32,
            (position.1 * f64::from(virtual_h) / f64::from(self.window_height)) as f32,
        )
    }
}

/// `Window.calculateScale(maxScale, false)`.
pub fn calculate_scale(max_scale: u32, window_width: u32, window_height: u32) -> u32 {
    let mut scale = 1;
    while scale != max_scale
        && scale < window_width
        && scale < window_height
        && window_width / (scale + 1) >= 320
        && window_height / (scale + 1) >= 240
    {
        scale += 1;
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_capped_by_the_setting_and_the_window() {
        assert_eq!(calculate_scale(0, 1920, 1080), 4);
        assert_eq!(calculate_scale(2, 1920, 1080), 2);
        assert_eq!(calculate_scale(9, 1920, 1080), 4);
        assert_eq!(calculate_scale(0, 854, 480), 2);
        assert_eq!(calculate_scale(0, 300, 200), 1);
    }

    #[test]
    fn virtual_size_rounds_up() {
        let scale = GuiScale::new(0, 1921, 1081);
        assert_eq!(scale.virtual_size(), (481.0, 271.0));
        assert_eq!(scale.to_virtual((1921.0, 0.0)).0, 481.0);
        assert_eq!(scale.max_scale(), 4);
    }

    #[test]
    fn the_projection_keeps_whole_window_pixels_per_gui_pixel() {
        let scale = GuiScale::new(0, 1921, 1081);
        assert_eq!(scale.projection_size(), (480.25, 270.25));
        let (w, h) = scale.projection_size();
        assert_eq!((1921.0 / w, 1081.0 / h), (4.0, 4.0));
    }
}
