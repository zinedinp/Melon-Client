//! `Style` and the events it carries.

use super::Component;
use super::contents::Profile;
use serde_json::Value;
use uuid::Uuid;

/// `TextColor`: an RGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextColor(pub [u8; 3]);

impl TextColor {
    /// `ChatFormatting`'s colours by their `§` code order, with their names.
    pub const NAMED: [(&'static str, TextColor); 16] = [
        ("black", Self([0, 0, 0])),
        ("dark_blue", Self([0, 0, 170])),
        ("dark_green", Self([0, 170, 0])),
        ("dark_aqua", Self([0, 170, 170])),
        ("dark_red", Self([170, 0, 0])),
        ("dark_purple", Self([170, 0, 170])),
        ("gold", Self([255, 170, 0])),
        ("gray", Self([170, 170, 170])),
        ("dark_gray", Self([85, 85, 85])),
        ("blue", Self([85, 85, 255])),
        ("green", Self([85, 255, 85])),
        ("aqua", Self([85, 255, 255])),
        ("red", Self([255, 85, 85])),
        ("light_purple", Self([255, 85, 255])),
        ("yellow", Self([255, 255, 85])),
        ("white", Self([255, 255, 255])),
    ];

    pub const YELLOW: Self = Self::NAMED[14].1;
    pub const RED: Self = Self::NAMED[12].1;

    /// `TextColor.parseColor`: `#RRGGBB` or a colour's name.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(hex) = text.strip_prefix('#') {
            let value = u32::from_str_radix(hex, 16)
                .ok()
                .filter(|v| *v <= 0xFF_FFFF)?;
            return Some(Self([(value >> 16) as u8, (value >> 8) as u8, value as u8]));
        }
        Self::NAMED
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, color)| *color)
    }

    /// The colour as RGB floats.
    pub fn rgb(self) -> [f32; 3] {
        self.0.map(|c| f32::from(c) / 255.0)
    }
}

/// `FontDescription.Resource`: the id of a font (`Style.font`).
#[derive(Clone, Debug, PartialEq)]
pub struct FontDescription(pub String);

/// `ClickEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum ClickEvent {
    OpenUrl {
        url: String,
    },
    OpenFile {
        path: String,
    },
    RunCommand {
        command: String,
    },
    SuggestCommand {
        command: String,
    },
    ChangePage {
        page: i32,
    },
    CopyToClipboard {
        value: String,
    },
    /// A dialog: its id or the inline definition.
    ShowDialog {
        dialog: Value,
    },
    Custom {
        id: String,
        payload: Option<Value>,
    },
}

/// `HoverEvent`.
#[derive(Clone, Debug, PartialEq)]
pub enum HoverEvent {
    ShowText(Box<Component>),
    /// `ItemStackTemplate`: the item's id, count and component patch (JSON).
    ShowItem {
        id: String,
        count: i32,
        components: Option<Value>,
    },
    /// `EntityTooltipInfo`: the entity type, its uuid and its name.
    ShowEntity {
        id: String,
        uuid: Option<Uuid>,
        name: Option<Box<Component>>,
    },
}

/// `Style`: what a component sets; what it leaves out it inherits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub color: Option<TextColor>,
    /// `shadowColor`: ARGB, as red, green, blue, alpha.
    pub shadow_color: Option<[u8; 4]>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub click_event: Option<ClickEvent>,
    pub hover_event: Option<HoverEvent>,
    pub insertion: Option<String>,
    pub font: Option<FontDescription>,
}

impl Style {
    pub fn colored(color: TextColor) -> Self {
        Self {
            color: Some(color),
            ..Self::default()
        }
    }
}

impl Profile {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.id.is_none()
            && self.textures.is_none()
            && self.texture.is_none()
    }
}
