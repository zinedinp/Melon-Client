//! Vanilla's chat on the client.

mod draw;
pub mod history;
pub mod input;

use std::collections::VecDeque;
use std::path::PathBuf;

use melon_protocol::component::{Component, Contents};

use melon_assets::i18n::Lang;
use melon_net::play::chat::{BoundChatType, ChatEvent, ChatTypes};
use melon_settings::options::{ChatSettings, ChatVisibility};
use crate::text::font::Font;
use crate::text::{self, StyledText, TextStyle, colors};
use history::CommandHistory;

/// `ChatComponent.MAX_CHAT_HISTORY`.
const MAX_MESSAGES: usize = 100;
/// `ChatComponent.BOTTOM_MARGIN`.
const BOTTOM_MARGIN: f32 = 40.0;
/// `LastSeenMessagesTracker`: acknowledge once this many are pending.
const ACK_THRESHOLD: i32 = 64;
/// `GuiMessageTag`'s indicator colours.
const SYSTEM_INDICATOR: [f32; 3] = [208.0 / 255.0; 3];

/// `GuiMessageSource`: what the chat visibility option filters on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Player,
    SystemServer,
    SystemClient,
}

/// `GuiMessageTag`: the bar left of a message and its tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub indicator: [f32; 3],
    /// Translation key of the tooltip.
    pub text: &'static str,
}

impl Tag {
    fn system() -> Self {
        Self {
            indicator: SYSTEM_INDICATOR,
            text: "chat.tag.system_single_player",
        }
    }

    fn not_secure() -> Self {
        Self {
            indicator: SYSTEM_INDICATOR,
            text: "chat.tag.not_secure",
        }
    }
}

/// `GuiMessage`.
#[derive(Clone, Debug)]
struct Message {
    added: u32,
    content: StyledText,
    signature: Option<Box<[u8; 256]>>,
    source: Source,
    tag: Option<Tag>,
}

/// `GuiMessage.Line`: one wrapped line of a message.
#[derive(Clone, Debug)]
pub struct Line {
    pub content: StyledText,
    pub added: u32,
    pub tag: Option<Tag>,
    pub end_of_entry: bool,
}

/// A message held back by the chat delay (`ChatListener.Message`).
struct Delayed {
    content: StyledText,
    signature: Option<Box<[u8; 256]>>,
    source: Source,
    tag: Option<Tag>,
}

pub struct Chat {
    messages: VecDeque<Message>,
    lines: VecDeque<Line>,
    scroll: usize,
    new_since_scroll: bool,
    /// Sent messages and commands, for the up and down keys.
    pub history: CommandHistory,
    /// An unsent message kept for the next opening (`latestDraft`).
    pub draft: Option<String>,
    delayed: VecDeque<Delayed>,
    previous_message_ms: u128,
    /// `LastSeenMessagesTracker.offset`.
    pending_acks: i32,
    /// `Hud.getGuiTicks`.
    ticks: u32,
    types: ChatTypes,
    /// This player, whose own messages are trusted.
    local_player: uuid::Uuid,
    settings: ChatSettings,
}

impl Chat {
    pub fn new(types: ChatTypes, local_player: uuid::Uuid, game_dir: Option<PathBuf>) -> Self {
        Self {
            messages: VecDeque::new(),
            lines: VecDeque::new(),
            scroll: 0,
            new_since_scroll: false,
            history: CommandHistory::load(game_dir),
            draft: None,
            delayed: VecDeque::new(),
            previous_message_ms: 0,
            pending_acks: 0,
            ticks: 0,
            types,
            local_player,
            settings: ChatSettings::default(),
        }
    }

    pub fn ticks(&self) -> u32 {
        self.ticks
    }

    /// The options changed: filter and wrap again (`rescaleChat`).
    pub fn set_settings(&mut self, settings: &ChatSettings, font: &Font) {
        if *settings != self.settings {
            if settings.delay <= 0.0 && self.settings.delay > 0.0 {
                self.flush_delayed(font);
            }
            self.settings = settings.clone();
            self.reset_scroll();
            self.rewrap(font);
        }
    }

    /// `Hud.tick` and `ChatListener.tick`: once a game tick.
    pub fn tick(&mut self, font: &Font) {
        self.ticks = self.ticks.wrapping_add(1);
        let delay_ms = (self.settings.delay * 1000.0) as u128;
        if delay_ms == 0 {
            self.flush_delayed(font);
        } else if now_ms() >= self.previous_message_ms + delay_ms
            && let Some(message) = self.delayed.pop_front()
        {
            self.show(
                message.content,
                message.signature,
                message.source,
                message.tag,
                font,
            );
        }
    }

    fn flush_delayed(&mut self, font: &Font) {
        while let Some(message) = self.delayed.pop_front() {
            self.show(
                message.content,
                message.signature,
                message.source,
                message.tag,
                font,
            );
        }
        self.previous_message_ms = 0;
    }

    /// Delayed messages waiting, for the `chat.queue` line.
    pub fn queue_size(&self) -> usize {
        self.delayed.len()
    }

    /// `ChatScreen`'s queue line: shows the next delayed message now.
    pub fn accept_next_delayed(&mut self, font: &Font) {
        if let Some(message) = self.delayed.pop_front() {
            self.show(
                message.content,
                message.signature,
                message.source,
                message.tag,
                font,
            );
        }
    }

    /// Takes the pending acknowledgement count once it is due.
    pub fn take_ack(&mut self) -> Option<i32> {
        (self.pending_acks > ACK_THRESHOLD).then(|| std::mem::take(&mut self.pending_acks))
    }

    /// The acknowledgements to send along with a chat message.
    pub fn take_offset(&mut self) -> i32 {
        std::mem::take(&mut self.pending_acks)
    }

    /// A chat packet from the server.
    pub fn receive(&mut self, event: ChatEvent, lang: &Lang, font: &Font) -> Option<StyledText> {
        match event {
            ChatEvent::System { content, overlay } => {
                let content = StyledText::from_component(&content, lang);
                if overlay {
                    return Some(content);
                } else {
                    self.queue(
                        content,
                        None,
                        Source::SystemServer,
                        Some(Tag::system()),
                        font,
                    );
                }
            }
            ChatEvent::ActionBar(content) => {
                return Some(StyledText::from_component(&content, lang));
            }
            ChatEvent::Disguised { message, bound } => {
                let content = self.decorate(&bound, &message, lang);
                self.queue(content, None, Source::Player, Some(Tag::system()), font);
            }
            ChatEvent::Player {
                sender,
                signature,
                message,
                unsigned_content,
                fully_filtered,
                bound,
            } => {
                if signature.is_some() {
                    self.pending_acks += 1;
                }
                if fully_filtered {
                    return None;
                }
                let secure = sender == self.local_player || signature.is_some();
                if self.settings.only_show_secure_chat && !secure {
                    return None;
                }
                let body = match unsigned_content {
                    Some(content) if !self.settings.only_show_secure_chat => content,
                    _ => Component::text(message),
                };
                let content = self.decorate(&bound, &body, lang);
                let tag = (!secure).then(Tag::not_secure);
                self.queue(content, signature, Source::Player, tag, font);
            }
            ChatEvent::Delete(signature) => self.delete(&signature, lang, font),
            ChatEvent::Suggestions(_)
            | ChatEvent::Title(_)
            | ChatEvent::Subtitle(_)
            | ChatEvent::TitleTimes { .. }
            | ChatEvent::ClearTitles { .. } => {}
        }
        None
    }

    /// `ChatType.Bound.decorate`.
    fn decorate(&self, bound: &BoundChatType, content: &Component, lang: &Lang) -> StyledText {
        let decoration = self.types.get(bound.chat_type);
        let args: Vec<Component> = decoration
            .parameters
            .iter()
            .map(|param| match param.as_str() {
                "sender" => bound.sender_name.clone(),
                "target" => bound
                    .target_name
                    .clone()
                    .unwrap_or_else(|| Component::text("")),
                _ => content.clone(),
            })
            .collect();
        let component = Component::new(Contents::Translatable {
            key: decoration.translation_key.clone(),
            fallback: None,
            args,
        })
        .with_style(decoration.style.clone().unwrap_or_default());
        StyledText::from_component(&component, lang)
    }

    /// `ChatComponent.addClientSystemMessage`.
    pub fn add_client_message(&mut self, content: StyledText, font: &Font) {
        self.show(
            content,
            None,
            Source::SystemClient,
            Some(Tag::system()),
            font,
        );
    }

    /// `ChatListener.handleMessage`: straight in, or behind the delay.
    fn queue(
        &mut self,
        content: StyledText,
        signature: Option<Box<[u8; 256]>>,
        source: Source,
        tag: Option<Tag>,
        font: &Font,
    ) {
        if self.settings.delay > 0.0 {
            self.delayed.push_back(Delayed {
                content,
                signature,
                source,
                tag,
            });
        } else {
            self.show(content, signature, source, tag, font);
        }
    }

    /// `ChatComponent.addMessage`.
    fn show(
        &mut self,
        content: StyledText,
        signature: Option<Box<[u8; 256]>>,
        source: Source,
        tag: Option<Tag>,
        font: &Font,
    ) {
        if !self.visible(source) {
            return;
        }
        self.previous_message_ms = now_ms();
        println!("[CHAT] {}", content.to_plain());
        let message = Message {
            added: self.ticks,
            content,
            signature,
            source,
            tag,
        };
        self.add_lines(&message, font, true);
        self.messages.push_front(message);
        self.messages.truncate(MAX_MESSAGES);
    }

    /// `ChatAbilities.selectVisibleMessages` for the visibility option.
    fn visible(&self, source: Source) -> bool {
        match self.settings.visibility {
            ChatVisibility::Full => true,
            ChatVisibility::System => source != Source::Player,
            ChatVisibility::Hidden => source == Source::SystemClient,
        }
    }

    /// `ChatComponent.addMessageToDisplayQueue`.
    fn add_lines(&mut self, message: &Message, font: &Font, live: bool) {
        let max_width = self.max_line_width();
        let lines = text::wrap(font, &message.content, max_width);
        let count = lines.len();
        for (i, content) in lines.into_iter().enumerate() {
            if live && self.scroll > 0 {
                self.new_since_scroll = true;
                self.scroll_by(1);
            }
            self.lines.push_front(Line {
                content,
                added: message.added,
                tag: message.tag.clone(),
                end_of_entry: i == count - 1,
            });
        }
        self.lines.truncate(MAX_MESSAGES);
    }

    /// `ChatComponent.clearMessages(false)` (F3+D).
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.lines.clear();
    }

    /// `refreshTrimmedMessages`.
    fn rewrap(&mut self, font: &Font) {
        self.lines.clear();
        let messages = std::mem::take(&mut self.messages);
        for message in messages.iter().rev() {
            if self.visible(message.source) {
                self.add_lines(message, font, false);
            }
        }
        self.messages = messages;
    }

    /// `ChatComponent.deleteMessage`.
    fn delete(&mut self, signature: &[u8; 256], lang: &Lang, font: &Font) {
        self.delayed
            .retain(|m| m.signature.as_deref() != Some(signature));
        let marker = TextStyle {
            color: Some(colors::GRAY),
            italic: true,
            ..TextStyle::default()
        };
        let mut changed = false;
        for message in &mut self.messages {
            if message.signature.as_deref() == Some(signature) {
                message.content = StyledText::default();
                message
                    .content
                    .push(lang.get("chat.deleted_marker"), &marker);
                message.signature = None;
                message.tag = Some(Tag::system());
                changed = true;
            }
        }
        if changed {
            self.rewrap(font);
        }
    }

    /// `addRecentChat`.
    pub fn add_recent(&mut self, message: &str) {
        self.history.add(message);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.new_since_scroll = false;
    }

    /// `scrollChat`: positive scrolls back in time.
    pub fn scroll_by(&mut self, lines: i32) {
        let max = self.lines.len() as i32 - self.lines_per_page(true) as i32;
        let pos = (self.scroll as i32 + lines).min(max).max(0);
        self.scroll = pos as usize;
        if pos == 0 {
            self.new_since_scroll = false;
        }
    }

    fn scale(&self) -> f32 {
        self.settings.scale as f32
    }

    /// `getWidth`: 40 to 320 pixels.
    fn width(&self) -> f32 {
        (self.settings.width * 280.0 + 40.0).floor() as f32
    }

    /// `getHeight`: 20 to 180 pixels.
    fn height(&self, focused: bool) -> f32 {
        let pct = if focused {
            self.settings.height_focused
        } else {
            self.settings.height_unfocused
        };
        (pct * 160.0 + 20.0).floor() as f32
    }

    fn max_line_width(&self) -> f32 {
        let scale = self.scale();
        if scale <= 0.0 {
            return self.width();
        }
        (self.width() / scale).floor()
    }

    fn line_height(&self) -> f32 {
        (9.0 * (self.settings.line_spacing + 1.0)) as i32 as f32
    }

    pub fn lines_per_page(&self, focused: bool) -> usize {
        (self.height(focused) / self.line_height()).max(1.0) as usize
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
