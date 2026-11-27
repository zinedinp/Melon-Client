//! Styled text.

pub mod font;
mod layout;
pub mod objects;
pub mod splash;

pub use layout::{TextPaint, draw, is_active, style_at, width, wrap};

use std::sync::Arc;

use crate::component::{ClickEvent, Component, Contents, HoverEvent, ObjectInfo, Style};
use crate::i18n::Lang;

/// `ChatFormatting`'s sixteen colours, by `§` code.
pub const FORMATTING_COLORS: [[u8; 3]; 16] = [
    [0, 0, 0],
    [0, 0, 170],
    [0, 170, 0],
    [0, 170, 170],
    [170, 0, 0],
    [170, 0, 170],
    [255, 170, 0],
    [170, 170, 170],
    [85, 85, 85],
    [85, 85, 255],
    [85, 255, 85],
    [85, 255, 255],
    [255, 85, 85],
    [255, 85, 255],
    [255, 255, 85],
    [255, 255, 255],
];

/// `Style.applyLegacyFormat` for the `§` code `code`, `§r` going back to `base`.
pub(crate) fn apply_code(style: &mut TextStyle, code: char, base: &TextStyle) {
    match code.to_ascii_lowercase() {
        digit @ ('0'..='9' | 'a'..='f') => {
            let index = digit.to_digit(16).unwrap_or(15) as usize;
            *style = TextStyle {
                color: Some(rgb(FORMATTING_COLORS[index])),
                ..base.clone()
            };
            style.bold = false;
            style.italic = false;
            style.underlined = false;
            style.strikethrough = false;
            style.obfuscated = false;
        }
        'k' => style.obfuscated = true,
        'l' => style.bold = true,
        'm' => style.strikethrough = true,
        'n' => style.underlined = true,
        'o' => style.italic = true,
        'r' => *style = base.clone(),
        _ => {}
    }
}

pub fn rgb(color: [u8; 3]) -> [f32; 3] {
    color.map(|c| f32::from(c) / 255.0)
}

/// `ChatFormatting.GRAY` and friends, as RGB.
pub mod colors {
    pub const BLACK: [f32; 3] = [0.0, 0.0, 0.0];
    pub const DARK_PURPLE: [f32; 3] = [170.0 / 255.0, 0.0, 170.0 / 255.0];
    pub const GOLD: [f32; 3] = [1.0, 170.0 / 255.0, 0.0];
    pub const GRAY: [f32; 3] = [170.0 / 255.0; 3];
    pub const DARK_GRAY: [f32; 3] = [85.0 / 255.0; 3];
    pub const BLUE: [f32; 3] = [85.0 / 255.0, 85.0 / 255.0, 1.0];
    pub const GREEN: [f32; 3] = [85.0 / 255.0, 1.0, 85.0 / 255.0];
    pub const DARK_GREEN: [f32; 3] = [0.0, 170.0 / 255.0, 0.0];
    pub const AQUA: [f32; 3] = [85.0 / 255.0, 1.0, 1.0];
    pub const RED: [f32; 3] = [1.0, 85.0 / 255.0, 85.0 / 255.0];
    pub const YELLOW: [f32; 3] = [1.0, 1.0, 85.0 / 255.0];
    pub const LIGHT_PURPLE: [f32; 3] = [1.0, 85.0 / 255.0, 1.0];
    pub const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
}

/// What a hover shows (`HoverEvent`), already turned into text.
#[derive(Clone, Debug, PartialEq)]
pub enum Hover {
    Text(StyledText),
    /// `ShowItem`.
    Item {
        id: String,
        count: i32,
        components: Option<serde_json::Value>,
    },
    Entity(StyledText),
}

/// One run's look and events (`Style`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    /// `None`: the colour the text is drawn in.
    pub color: Option<[f32; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
    pub obfuscated: bool,
    /// `Style.shadowColor`.
    pub shadow_color: Option<[f32; 4]>,
    /// `Style.font`: the font the run is drawn in, `None` for the default.
    pub font: Option<Arc<str>>,
    pub click: Option<Arc<ClickEvent>>,
    pub hover: Option<Arc<Hover>>,
    /// Shift-click inserts it into the chat box.
    pub insertion: Option<Arc<str>>,
    /// The sprite an `object` component's placeholder ([`OBJECT_CHAR`])
    pub sprite: Option<Arc<ObjectInfo>>,
}

/// `ObjectContents.PLACEHOLDER`: the character of an `object` component.
pub const OBJECT_CHAR: char = '\u{FFFC}';

impl TextStyle {
    pub fn colored(color: [f32; 3]) -> Self {
        Self {
            color: Some(color),
            ..Self::default()
        }
    }

    /// `Style.applyTo`: this component's own style over its parent's.
    fn apply(&self, style: &Style, lang: &Lang) -> Self {
        let mut out = self.clone();
        if let Some(color) = style.color {
            out.color = Some(color.rgb());
        }
        let flag = |own: Option<bool>, inherited: bool| own.unwrap_or(inherited);
        out.bold = flag(style.bold, out.bold);
        out.italic = flag(style.italic, out.italic);
        out.underlined = flag(style.underlined, out.underlined);
        out.strikethrough = flag(style.strikethrough, out.strikethrough);
        out.obfuscated = flag(style.obfuscated, out.obfuscated);
        if let Some(c) = style.shadow_color {
            out.shadow_color = Some(c.map(|v| f32::from(v) / 255.0));
        }
        if let Some(font) = &style.font {
            out.font = (font.0 != font::DEFAULT_FONT && font.0 != "default")
                .then(|| font.0.as_str().into());
        }
        if let Some(click) = &style.click_event {
            out.click = Some(Arc::new(click.clone()));
        }
        if let Some(hover) = &style.hover_event {
            out.hover = Some(Arc::new(match hover {
                HoverEvent::ShowText(value) => Hover::Text(StyledText::from_component(value, lang)),
                HoverEvent::ShowItem {
                    id,
                    count,
                    components,
                } => Hover::Item {
                    id: id.clone(),
                    count: *count,
                    components: components.clone(),
                },
                HoverEvent::ShowEntity { name, id, .. } => Hover::Entity(match name {
                    Some(name) => StyledText::from_component(name, lang),
                    None => StyledText::plain(id),
                }),
            }));
        }
        if let Some(insertion) = &style.insertion {
            out.insertion = Some(insertion.as_str().into());
        }
        out
    }
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: TextStyle,
}

/// A line (or paragraph) of styled runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyledText {
    pub spans: Vec<Span>,
}

impl From<&str> for StyledText {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

impl From<String> for StyledText {
    fn from(text: String) -> Self {
        Self::plain(&text)
    }
}

impl StyledText {
    /// Unstyled text.
    pub fn plain(text: &str) -> Self {
        Self::formatted(text)
    }

    /// Text in `color`; its `§` codes override it, `§r` returns to it.
    pub fn colored(text: &str, color: [f32; 3]) -> Self {
        let mut out = Self::default();
        out.push_formatted(text, &TextStyle::colored(color));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// `Component.getString`: the text without its styles.
    pub fn to_plain(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            match &span.style.sprite {
                Some(info) => {
                    for _ in span.text.chars() {
                        out.push_str(&info.default_fallback());
                    }
                }
                None => out.push_str(&span.text),
            }
        }
        out
    }

    /// Appends `text`, merged into the last run when the style matches.
    pub fn push(&mut self, text: &str, style: &TextStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.style == *style
        {
            last.text.push_str(text);
            return;
        }
        self.spans.push(Span {
            text: text.to_owned(),
            style: style.clone(),
        });
    }

    pub fn append(&mut self, other: &StyledText) {
        for span in &other.spans {
            self.push(&span.text, &span.style);
        }
    }

    /// `Component.withStyle` for all runs that don't set their own colour.
    pub fn with_default_color(mut self, color: [f32; 3]) -> Self {
        for span in &mut self.spans {
            span.style.color.get_or_insert(color);
        }
        self
    }

    /// Appends `text`, reading `§` codes in it the way vanilla's legacy formatting does.
    pub fn push_formatted(&mut self, text: &str, base: &TextStyle) {
        let mut style = base.clone();
        let mut chars = text.chars();
        let mut run = String::new();
        while let Some(c) = chars.next() {
            if c != '§' {
                run.push(c);
                continue;
            }
            let Some(code) = chars.next() else { break };
            self.push(&run, &style);
            run.clear();
            apply_code(&mut style, code, base);
        }
        self.push(&run, &style);
    }

    pub fn formatted(text: &str) -> Self {
        let mut out = Self::default();
        out.push_formatted(text, &TextStyle::default());
        out
    }

    /// A component, translated with `lang`.
    pub fn from_component(component: &Component, lang: &Lang) -> Self {
        let mut out = Self::default();
        out.push_base(component, &TextStyle::default(), lang);
        out
    }

    /// Appends `component` with `parent` as the style it inherits.
    pub fn push_component(&mut self, component: &Component, parent: &TextStyle, lang: &Lang) {
        self.push_base(component, parent, lang);
    }

    /// `Component.visit`.
    fn push_base(&mut self, base: &Component, parent: &TextStyle, lang: &Lang) {
        let style = parent.apply(&base.style, lang);
        match &*base.contents {
            Contents::Text(text) => self.push_formatted(text, &style),
            Contents::Translatable {
                key,
                fallback,
                args,
            } => self.push_translation(key, fallback.as_deref(), args, &style, lang),
            Contents::Keybind(keybind) => self.push_formatted(&keybind_name(keybind, lang), &style),
            Contents::Selector { selector, .. } => self.push_formatted(selector, &style),
            Contents::Score { .. } | Contents::Nbt { .. } => {
                if crate::probe::enabled() {
                    let kind = if matches!(&*base.contents, Contents::Score { .. }) {
                        "score"
                    } else {
                        "nbt"
                    };
                    crate::probe::count(
                        "text.unsupported",
                        format!("{kind} component (the server resolves it)"),
                    );
                }
            }
            Contents::Object { info, .. } => {
                let mut sprite_style = style.clone();
                sprite_style.sprite = Some(Arc::new(info.clone()));
                let mut text = [0; 4];
                self.push(OBJECT_CHAR.encode_utf8(&mut text), &sprite_style);
            }
        }
        for child in &base.extra {
            self.push_base(child, &style, lang);
        }
    }

    /// `TranslatableContents.decompose`.
    fn push_translation(
        &mut self,
        key: &str,
        fallback: Option<&str>,
        with: &[Component],
        style: &TextStyle,
        lang: &Lang,
    ) {
        let pattern = lang.raw_dynamic(key).or(fallback).unwrap_or(key);
        if crate::probe::enabled() && lang.raw_dynamic(key).is_none() {
            crate::probe::note("lang.missing", key, || {
                format!("fallback: {}", fallback.unwrap_or("(none)"))
            });
        }
        let mut next = 0;
        let mut rest = pattern;
        while let Some(at) = rest.find('%') {
            self.push_formatted(&rest[..at], style);
            rest = &rest[at + 1..];
            let index = if let Some(after) = rest.strip_prefix('%') {
                self.push("%", style);
                rest = after;
                continue;
            } else if let Some(after) = rest.strip_prefix('s') {
                rest = after;
                next += 1;
                next - 1
            } else if let Some((n, after)) = rest
                .split_once("$s")
                .and_then(|(n, after)| Some((n.parse::<usize>().ok()?, after)))
            {
                rest = after;
                n.wrapping_sub(1)
            } else {
                self.push("%", style);
                continue;
            };
            if let Some(arg) = with.get(index) {
                self.push_base(arg, style, lang);
            }
        }
        self.push_formatted(rest, style);
    }
}

/// `KeybindContents`: the key a binding is on, by Melon's fixed defaults.
fn keybind_name(keybind: &str, lang: &Lang) -> String {
    let key = match keybind {
        "key.forward" => "W",
        "key.left" => "A",
        "key.back" => "S",
        "key.right" => "D",
        "key.inventory" => "E",
        "key.drop" => "Q",
        "key.chat" => "T",
        "key.command" => "/",
        "key.swapOffhand" => "F",
        "key.jump" => "key.keyboard.space",
        "key.sneak" => "key.keyboard.left.shift",
        "key.sprint" => "key.keyboard.left.control",
        "key.playerlist" => "key.keyboard.tab",
        "key.attack" => "key.mouse.left",
        "key.use" => "key.mouse.right",
        "key.pickItem" => "key.mouse.middle",
        "key.togglePerspective" => "key.keyboard.f5",
        "key.screenshot" => "key.keyboard.f2",
        "key.fullscreen" => "key.keyboard.f11",
        other => {
            if let Some(n) = other.strip_prefix("key.hotbar.") {
                return n.to_owned();
            }
            other
        }
    };
    lang.raw_dynamic(key).unwrap_or(key).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_codes_split_runs() {
        let text = StyledText::formatted("a§cb§lc§rd");
        assert_eq!(text.spans.len(), 4);
        assert_eq!(text.spans[1].style.color, Some(rgb([255, 85, 85])));
        assert!(text.spans[2].style.bold);
        assert_eq!(text.spans[2].style.color, Some(rgb([255, 85, 85])));
        assert_eq!(text.spans[3].style, TextStyle::default());
        assert_eq!(text.to_plain(), "abcd");
    }

    #[test]
    fn translations_fill_in_their_arguments() {
        let lang = Lang::empty();
        let component = Component::translatable(
            "chat.type.text",
            vec![Component::text("Steve"), Component::text("hi")],
        );
        assert_eq!(
            StyledText::from_component(&component, &lang).to_plain(),
            "chat.type.text"
        );
        let lang = Lang::from_pairs(&[("chat.type.text", "<%s> %s")]);
        assert_eq!(
            StyledText::from_component(&component, &lang).to_plain(),
            "<Steve> hi"
        );
    }

    #[test]
    fn a_missing_translation_shows_its_fallback() {
        let component = Component::new(Contents::Translatable {
            key: "no.such.key".into(),
            fallback: Some("Hallo %s".into()),
            args: vec![Component::text("Welt")],
        });
        assert_eq!(
            StyledText::from_component(&component, &Lang::empty()).to_plain(),
            "Hallo Welt"
        );
        let lang = Lang::from_pairs(&[("no.such.key", "Hi %s")]);
        assert_eq!(
            StyledText::from_component(&component, &lang).to_plain(),
            "Hi Welt"
        );
    }

    #[test]
    fn an_object_is_one_placeholder_with_its_sprite() {
        let component = crate::component::decode::component(&serde_json::json!(
            ["a", {"object": "atlas", "sprite": "block/stone", "bold": true}]
        ));
        let text = StyledText::from_component(&component, &Lang::empty());
        assert_eq!(text.to_plain(), "a[block/stone]");
        assert_eq!(text.spans[1].text, "\u{FFFC}");
        assert!(text.spans[0].style.sprite.is_none());
        assert!(text.spans[1].style.sprite.is_some());
        assert!(text.spans[1].style.bold);
    }
}
