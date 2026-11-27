//! `net.minecraft.network.chat.Component`.

mod contents;
pub mod decode;
pub mod pumpkin;
mod wire;
mod style;

pub use contents::{AtlasSprite, Contents, NbtSource, ObjectInfo, PlayerSprite, Profile};
pub use style::{ClickEvent, FontDescription, HoverEvent, Style, TextColor};

/// `Util.parseAndValidateUntrustedUri`: a scheme, and it is http or https.
pub fn is_trusted_url(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, rest)| {
        !rest.is_empty()
            && matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
            && !url.chars().any(|c| c.is_whitespace() || c.is_control())
    })
}

/// A text component.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub contents: Box<Contents>,
    pub style: Box<Style>,
    pub extra: Vec<Component>,
}

impl Default for Component {
    fn default() -> Self {
        Self::text("")
    }
}

impl Component {
    pub fn new(contents: Contents) -> Self {
        Self {
            contents: Box::new(contents),
            style: Box::default(),
            extra: Vec::new(),
        }
    }

    /// `Component.literal`.
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(Contents::Text(text.into()))
    }

    /// `Component.translatable`.
    pub fn translatable(key: impl Into<String>, args: Vec<Component>) -> Self {
        Self::new(Contents::Translatable {
            key: key.into(),
            fallback: None,
            args,
        })
    }

    /// `Component.empty`.
    pub fn empty() -> Self {
        Self::text("")
    }

    /// `MutableComponent.withStyle`.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Box::new(style);
        self
    }

    /// `MutableComponent.withColor`.
    pub fn with_color(mut self, color: TextColor) -> Self {
        self.style.color = Some(color);
        self
    }

    /// `MutableComponent.append`.
    pub fn append(mut self, sibling: Component) -> Self {
        self.extra.push(sibling);
        self
    }

    /// `Component.getString` without a language.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        self.write_plain(&mut out);
        out
    }

    fn write_plain(&self, out: &mut String) {
        match &*self.contents {
            Contents::Text(text) => out.push_str(text),
            Contents::Translatable { key, fallback, .. } => {
                out.push_str(fallback.as_deref().unwrap_or(key));
            }
            Contents::Keybind(key) => out.push_str(key),
            Contents::Selector { selector, .. } => out.push_str(selector),
            Contents::Score { .. } | Contents::Nbt { .. } => {}
            Contents::Object { info, fallback } => match fallback {
                Some(fallback) => fallback.write_plain(out),
                None => out.push_str(&info.default_fallback()),
            },
        }
        for sibling in &self.extra {
            sibling.write_plain(out);
        }
    }
}

impl From<&str> for Component {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

impl From<String> for Component {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}
