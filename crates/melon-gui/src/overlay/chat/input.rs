//! Vanilla `ChatScreen` (T, or / for a command).

use melon_protocol::component::ClickEvent;

use crate::hud::item_slot::ItemPainter;
use crate::overlay::chat::Chat;
use crate::overlay::container_input::MouseButtonKind;
use crate::overlay::{ChatExit, Overlay, OverlayAction, OverlayContext, OverlayDraw, OverlayKind};
use crate::screen::TextEvent;
use crate::text::font::Font;
use crate::text::{self, Hover, StyledText, TextStyle, colors};
use crate::widgets::text_input::{Edit, TextInput};
use melon_assets::i18n::Lang;
use melon_core::math::argb;
use melon_net::play::chat::Suggestions;
use melon_net::play::chat::command_suggestions::{self, SELECTOR_LEVEL, Source};
use melon_net::play::chat::commands::{CommandTree, SpanKind};
use melon_render::quad::DrawList;
use melon_settings::input::InputBind;
use melon_settings::options::{ChatSettings, ChatVisibility};

/// `ChatScreen.input.setMaxLength(256)`.
const MAX_LENGTH: usize = 256;
/// `CommandSuggestions`' `lineCount`.
const SUGGESTION_LINES: usize = 10;
/// `EditBox.DEFAULT_TEXT_COLOR`.
const TEXT_COLOR: [f32; 3] = [224.0 / 255.0; 3];
const SUGGESTION_COLOR: [f32; 3] = [170.0 / 255.0; 3];
const SUGGESTION_SELECTED: [f32; 3] = [1.0, 1.0, 0.0];
/// `CommandSuggestions.ARGUMENT_STYLES`, one argument after the other.
const ARGUMENT_COLORS: [[f32; 3]; 5] = [
    colors::AQUA,
    colors::YELLOW,
    colors::GREEN,
    colors::LIGHT_PURPLE,
    colors::GOLD,
];
/// `CommandSuggestions.USAGE_OFFSET_FROM_BOTTOM`.
const USAGE_OFFSET: f32 = 27.0;

mod draw;
mod suggest;

use suggest::{Completer, SuggestionList};

pub struct ChatInput {
    input: TextInput,
    history_pos: usize,
    history_buffer: String,
    /// Restored from a draft: grey and italic until edited.
    is_draft: bool,
    suggestions: Option<SuggestionList>,
    transaction: i32,
    /// Whether the server's answer may open the list (`allowSuggestions`).
    allow_suggestions: bool,
    mouse: (f32, f32),
    /// The left button went down on the input line and is still held.
    selecting: bool,
    /// `formatChat`.
    colors: Option<(String, Vec<[f32; 3]>)>,
    /// `commandUsage`.
    usage: Vec<StyledText>,
    /// The character the usage lines start over.
    usage_start: usize,
}

impl ChatInput {
    /// `ChatScreen(initial, isDraft)`.
    pub fn new(initial: &str, is_draft: bool, history_len: usize) -> Self {
        let mut input = TextInput::new(MAX_LENGTH);
        input.set_value(initial);
        Self {
            input,
            history_pos: history_len,
            history_buffer: String::new(),
            is_draft,
            suggestions: None,
            transaction: 0,
            allow_suggestions: false,
            mouse: (0.0, 0.0),
            selecting: false,
            colors: None,
            usage: Vec::new(),
            usage_start: 0,
        }
    }

    pub fn value(&self) -> String {
        self.input.value()
    }

    fn set_value(&mut self, value: &str) {
        self.input.set_value(value);
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
    }

    /// A key or typed text.
    fn on_text(
        &mut self,
        event: &TextEvent,
        chat: &mut Chat,
        settings: &ChatSettings,
        completer: &Completer,
    ) -> OverlayAction {
        self.input.restart_blink();
        if self.suggestions.is_some() {
            match event {
                TextEvent::Up => {
                    self.cycle(-1);
                    return OverlayAction::None;
                }
                TextEvent::Down => {
                    self.cycle(1);
                    return OverlayAction::None;
                }
                TextEvent::Tab { shift } => {
                    if self.suggestions.as_ref().is_some_and(|l| l.cycled) {
                        self.cycle(if *shift { -1 } else { 1 });
                    }
                    self.use_suggestion();
                    return OverlayAction::None;
                }
                _ => {}
            }
        }
        if self.is_draft && matches!(event, TextEvent::Backspace { .. }) {
            self.set_value("");
            self.is_draft = false;
            return OverlayAction::None;
        }
        match self.input.handle(event) {
            Edit::Changed => return self.edited(settings, completer),
            Edit::Moved => return OverlayAction::None,
            Edit::Ignored => {}
        }
        match event {
            TextEvent::Up => self.move_in_history(-1, chat),
            TextEvent::Down => self.move_in_history(1, chat),
            TextEvent::PageUp => {
                chat.scroll_by(chat.lines_per_page(true) as i32 - 1);
            }
            TextEvent::PageDown => {
                chat.scroll_by(-(chat.lines_per_page(true) as i32) + 1);
            }
            TextEvent::Tab { .. } => {
                self.allow_suggestions = true;
                if self.value().starts_with('/') {
                    return self.request(settings, true, completer);
                }
                self.suggest_names(completer.source.players);
                self.use_suggestion();
            }
            TextEvent::Enter => {
                let message = normalize(&self.value());
                if !may_send(&message, settings) {
                    return OverlayAction::None;
                }
                if !message.is_empty() {
                    chat.add_recent(&message);
                }
                return OverlayAction::Send(message);
            }
            _ => {}
        }
        OverlayAction::None
    }

    /// `moveInHistory`.
    fn move_in_history(&mut self, step: i32, chat: &Chat) {
        let max = chat.history.len();
        let pos = (self.history_pos as i32 + step).clamp(0, max as i32) as usize;
        if pos == self.history_pos {
            return;
        }
        if pos == max {
            self.history_pos = max;
            let buffer = self.history_buffer.clone();
            self.set_value(&buffer);
        } else {
            if self.history_pos == max {
                self.history_buffer = self.value();
            }
            let entry = chat.history.get(pos).unwrap_or_default().to_owned();
            self.set_value(&entry);
            self.allow_suggestions = false;
            self.suggestions = None;
            self.history_pos = pos;
        }
    }

    /// `mouseScrolled`.
    fn on_scroll(&mut self, lines: f32, shift: bool, chat: &mut Chat) {
        let step = lines.clamp(-1.0, 1.0);
        if self.suggestions.is_some() {
            self.cycle(if step > 0.0 { -1 } else { 1 });
            return;
        }
        let amount = if shift { step } else { step * 7.0 };
        chat.scroll_by(amount as i32);
    }

    /// `mouseClicked`: a completion, the queue line, or chat text.
    fn on_click(
        &mut self,
        chat: &mut Chat,
        font: &Font,
        size: (f32, f32),
        shift: bool,
        settings: &ChatSettings,
        completer: &Completer,
    ) -> OverlayAction {
        let (mx, my) = self.mouse;
        if let Some(index) = self.suggestion_at(font, size) {
            if let Some(list) = &mut self.suggestions {
                list.selected = index;
            }
            self.use_suggestion();
            return OverlayAction::None;
        }
        if mx >= 4.0 && my >= size.1 - 12.0 && my < size.1 {
            self.input.click(mx - Self::text_x(), shift);
            self.selecting = true;
            return OverlayAction::None;
        }
        if chat.is_over_queue(size.1, mx, my) {
            chat.accept_next_delayed(font);
            return OverlayAction::None;
        }
        let Some(style) = chat.style_at(font, size.1, mx, my) else {
            return OverlayAction::None;
        };
        if shift {
            if let Some(insertion) = &style.insertion {
                self.insert(insertion, settings, completer);
            }
            return OverlayAction::None;
        }
        match style.click.as_deref() {
            Some(ClickEvent::OpenUrl { url }) => {
                if settings.links {
                    return OverlayAction::OpenUrl(url.to_string());
                }
            }
            Some(ClickEvent::RunCommand { command }) => {
                return OverlayAction::RunCommand(command.to_string());
            }
            Some(ClickEvent::SuggestCommand { command }) => {
                let command = command.to_string();
                self.set_value(&command);
                return self.edited(settings, completer);
            }
            Some(ClickEvent::CopyToClipboard { value }) => {
                return OverlayAction::Copy(value.to_string());
            }
            // Only the client makes these (a saved screenshot), never a server.
            Some(ClickEvent::OpenFile { path }) => {
                return OverlayAction::OpenFile(path.to_string());
            }
            // `ChatScreen.handleClickEvent` → `defaultHandleGameClickEvent`.
            Some(event @ (ClickEvent::ShowDialog { .. } | ClickEvent::Custom { .. })) => {
                return OverlayAction::Dialog(melon_net::dialog::DialogClick::from_event(
                    event.clone(),
                ));
            }
            _ => {}
        }
        OverlayAction::None
    }

    fn insert(&mut self, text: &str, settings: &ChatSettings, completer: &Completer) {
        self.input.insert(text);
        let _ = self.edited(settings, completer);
    }

    /// The unsent text, for the next opening (`removed`'s draft).
    pub fn draft(&self) -> Option<String> {
        let value = self.value();
        (!value.trim().is_empty() && !self.is_draft).then_some(value)
    }
}

/// `normalizeChatMessage`.
pub fn normalize(message: &str) -> String {
    let joined = message.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(MAX_LENGTH).collect()
}

/// `ChatAbilities` for the chat visibility.
fn may_send(message: &str, settings: &ChatSettings) -> bool {
    match settings.visibility {
        ChatVisibility::Full => true,
        ChatVisibility::System => message.starts_with('/') || message.is_empty(),
        ChatVisibility::Hidden => message.is_empty(),
    }
}

impl Overlay for ChatInput {
    fn kind(&self) -> OverlayKind {
        OverlayKind::Chat
    }

    fn mouse_moved(&mut self, x: f32, y: f32, dragging: bool, _ctx: &mut OverlayContext) {
        self.on_mouse_move(x, y);
        self.selecting &= dragging;
        if self.selecting {
            self.input.drag(x - Self::text_x());
        }
    }

    fn mouse_button(
        &mut self,
        button: MouseButtonKind,
        pressed: bool,
        ctx: &mut OverlayContext,
    ) -> OverlayAction {
        if !pressed || button != MouseButtonKind::Left {
            return OverlayAction::None;
        }
        let players = (ctx.players)();
        let completer = Completer::new(ctx, &players);
        let action = self.on_click(
            ctx.chat,
            ctx.font,
            ctx.size,
            ctx.shift,
            ctx.chat_settings,
            &completer,
        );
        self.update_command_info(&completer);
        action
    }

    fn scroll(&mut self, lines: f32, ctx: &mut OverlayContext) {
        self.on_scroll(lines, ctx.shift, ctx.chat);
    }

    fn key(
        &mut self,
        _bind: Option<&InputBind>,
        text: &[TextEvent],
        ctx: &mut OverlayContext,
    ) -> Vec<OverlayAction> {
        let players = (ctx.players)();
        let mut actions = Vec::new();
        for event in text {
            let completer = Completer::new(ctx, &players);
            let action = self.on_text(event, ctx.chat, ctx.chat_settings, &completer);
            self.update_command_info(&completer);
            let sent = matches!(action, OverlayAction::Send(_));
            if !matches!(action, OverlayAction::None) {
                actions.push(action);
            }
            if sent {
                break;
            }
        }
        actions
    }

    fn escape(&mut self) -> OverlayAction {
        if self.close_suggestions() {
            OverlayAction::None
        } else {
            OverlayAction::Close(ChatExit::Intentional)
        }
    }

    /// `ChatScreen.removed`.
    fn closed(&mut self, exit: ChatExit, ctx: &mut OverlayContext) {
        ctx.chat.reset_scroll();
        let keep = match exit {
            ChatExit::Interrupted => true,
            ChatExit::Intentional => ctx.chat_settings.save_drafts,
            ChatExit::Done => false,
        };
        ctx.chat.draft = if keep { self.draft() } else { None };
    }

    fn receive_suggestions(&mut self, suggestions: Suggestions, lang: &Lang) {
        ChatInput::receive_suggestions(self, suggestions, lang);
    }

    fn draw(&mut self, ui: &mut OverlayDraw) {
        self.draw_screen(
            ui.draw_list,
            ui.painter,
            ui.chat,
            ui.chat_settings,
            ui.lang,
            ui.size,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_normalized() {
        assert_eq!(normalize("  hallo   welt  "), "hallo welt");
        assert_eq!(normalize(&"x".repeat(300)).len(), 256);
    }

    #[test]
    fn a_suggestion_replaces_its_range() {
        let mut screen = ChatInput::new("/gamemode cr", false, 0);
        screen.allow_suggestions = true;
        screen.transaction = 1;
        screen.receive_suggestions(
            Suggestions {
                transaction: 1,
                start: 10,
                length: 2,
                matches: vec![("creative".into(), None)],
            },
            &Lang::empty(),
        );
        screen.use_suggestion();
        assert_eq!(screen.value(), "/gamemode creative");
    }
}
