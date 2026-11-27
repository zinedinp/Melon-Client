//! `Options`' chat settings, grouped as `ChatScreen` and the chat HUD use them.

use super::*;

/// The options of vanilla's `ChatOptionsScreen` (the narrator aside).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatSettings {
    pub visibility: ChatVisibility,
    /// `chatColors`: show text colours, or everything in white.
    pub colors: bool,
    /// `chatLinks`: web links are clickable.
    pub links: bool,
    /// `chatLinksPrompt`: ask before opening one.
    pub links_prompt: bool,
    /// `chatOpacity`, `0..=1` (shown as `10%..=100%`).
    pub opacity: f64,
    /// `textBackgroundOpacity`, `0..=1`.
    pub background_opacity: f64,
    /// `backgroundForChatOnly`: off puts that opacity behind name tags and text displays too.
    pub background_for_chat_only: bool,
    /// `chatScale`, `0..=1`.
    pub scale: f64,
    /// `chatLineSpacing`, `0..=1`.
    pub line_spacing: f64,
    /// `chatDelay`, seconds `0..=6` in tenths.
    pub delay: f64,
    /// `chatWidth`, `0..=1` of `40..=320` pixels.
    pub width: f64,
    /// `chatHeightFocused`/`chatHeightUnfocused`, `0..=1` of `20..=180`.
    pub height_focused: f64,
    pub height_unfocused: f64,
    /// `autoSuggestions`: command suggestions pop up while typing.
    pub auto_suggestions: bool,
    /// `hideMatchedNames`.
    pub hide_matched_names: bool,
    /// `reducedDebugInfo`.
    pub reduced_debug_info: bool,
    /// `onlyShowSecureChat`.
    pub only_show_secure_chat: bool,
    /// `saveChatDrafts`: keep an unsent message when the chat is closed.
    pub save_drafts: bool,
}

impl ChatSettings {
    /// `Options.getBackgroundOpacity`: the text background outside the chat.
    pub fn world_background_opacity(&self, default: f32) -> f32 {
        if self.background_for_chat_only {
            default
        } else {
            self.background_opacity as f32
        }
    }
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            visibility: ChatVisibility::Full,
            colors: true,
            links: true,
            links_prompt: true,
            opacity: 1.0,
            background_opacity: 0.5,
            background_for_chat_only: true,
            scale: 1.0,
            line_spacing: 0.0,
            delay: 0.0,
            width: 1.0,
            height_focused: 1.0,
            height_unfocused: 70.0 / 160.0,
            auto_suggestions: true,
            hide_matched_names: true,
            reduced_debug_info: false,
            only_show_secure_chat: false,
            save_drafts: false,
        }
    }
}
