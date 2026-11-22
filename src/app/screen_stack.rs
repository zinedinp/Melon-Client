//! Vanilla `minecraft.setScreen` as a plain stack.

use crate::gui::screen::{CursorKind, Screen, ScreenContext};
use crate::i18n::Lang;
use crate::settings::options::Config;

#[derive(Default)]
pub struct ScreenStack {
    screens: Vec<Box<dyn Screen>>,
    /// The menu stack a world was joined from, while it is played.
    menus: Vec<Box<dyn Screen>>,
}

impl ScreenStack {
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    pub fn top(&self) -> Option<&dyn Screen> {
        self.screens.last().map(Box::as_ref)
    }

    pub fn top_mut(&mut self) -> Option<&mut (dyn Screen + 'static)> {
        self.screens.last_mut().map(Box::as_mut)
    }

    /// Opens a screen on top, laid out for the GUI's size.
    pub fn push(&mut self, mut screen: Box<dyn Screen>, ctx: &ScreenContext) {
        screen.resize(ctx);
        self.screens.push(screen);
    }

    /// Closes the top screen and opens `screen` in its place.
    pub fn replace_top(&mut self, screen: Box<dyn Screen>, ctx: &ScreenContext) {
        self.screens.pop();
        self.push(screen, ctx);
    }

    /// `Screen::poll` for the top screen, and while server dialogs cover
    /// it, for the screen under them: a join goes on behind a dialog the
    /// server shows while configuring.
    pub fn poll(&mut self) -> Option<crate::gui::screen::ScreenAction> {
        let top = self.screens.len().checked_sub(1)?;
        let action = self.screens[top].poll();
        if !matches!(action, crate::gui::screen::ScreenAction::None) {
            return Some(action);
        }
        let under = self
            .screens
            .iter()
            .rposition(|screen| screen.dialog_layer().is_none())?;
        (under != top).then(|| self.screens[under].poll())
    }

    /// The screen under the top one, if any.
    pub fn below_top(&self) -> Option<&dyn Screen> {
        let len = self.screens.len();
        (len >= 2).then(|| self.screens[len - 2].as_ref())
    }

    /// Puts `screen` under the top one: in place of the one there with
    /// `replace`, else between them. A dialog the server shows while its
    /// warning is open goes behind the warning (`WarningScreen.updateReturnScreen`).
    pub fn set_below_top(
        &mut self,
        mut screen: Box<dyn Screen>,
        replace: bool,
        ctx: &ScreenContext,
    ) {
        let Some(top) = self.screens.pop() else {
            return self.push(screen, ctx);
        };
        if replace {
            self.screens.pop();
        }
        screen.resize(ctx);
        self.screens.push(screen);
        self.screens.push(top);
    }

    /// Drops the screen under the top one.
    pub fn remove_below_top(&mut self) {
        let len = self.screens.len();
        if len >= 2 {
            self.screens.remove(len - 2);
        }
    }

    /// Drops every open screen (death, respawn).
    pub fn clear(&mut self) {
        self.screens.clear();
    }

    /// Pops one screen and resumes the one below.
    pub fn close_top(&mut self, in_world: bool, ctx: &ScreenContext) {
        if in_world || self.screens.len() > 1 {
            self.screens.pop();
        }
        if let Some(top) = self.screens.last_mut() {
            top.on_resume(ctx);
        }
    }

    /// Closes the top screen after a language change, unless it is the last one.
    pub fn pop_unless_last(&mut self) {
        if self.screens.len() > 1 {
            self.screens.pop();
        }
    }

    /// Every screen, open or set aside, in its new language.
    pub fn refresh_lang(&mut self, lang: &Lang) {
        for screen in self.screens.iter_mut().chain(&mut self.menus) {
            screen.refresh_lang(lang);
        }
    }

    /// The options changed.
    pub fn refresh_config(&mut self, config: &Config) {
        for screen in self.screens.iter_mut().chain(&mut self.menus) {
            screen.refresh_config(config);
        }
    }

    /// The GUI's size changed.
    pub fn resize(&mut self, ctx: &ScreenContext) {
        for screen in self.screens.iter_mut().chain(&mut self.menus) {
            screen.resize(ctx);
        }
    }

    /// A world was joined: the menus wait aside, playing starts with none open.
    pub fn enter_world(&mut self) {
        if self.menus.is_empty() {
            self.menus = std::mem::take(&mut self.screens);
        } else {
            self.screens.clear();
        }
    }

    /// The world was left.
    pub fn leave_world(&mut self, integrated: bool) {
        self.screens = std::mem::take(&mut self.menus);
        while self.screens.len() > 1
            && !self
                .screens
                .last()
                .is_some_and(|screen| screen.is_world_exit(integrated))
        {
            self.screens.pop();
        }
    }

    /// The OS cursor the top screen asks for.
    pub fn cursor(&self) -> Option<CursorKind> {
        self.top().map(Screen::cursor)
    }

    /// `Screen.shouldCloseOnEsc` for the top screen; an empty stack has none to close.
    pub fn top_closes_on_escape(&self) -> bool {
        self.top().is_none_or(Screen::closes_on_escape)
    }

    /// `KeyBindsScreen` is waiting for a physical key or mouse button.
    pub fn capturing_bind(&self) -> bool {
        self.top().is_some_and(Screen::capturing_bind)
    }
}
