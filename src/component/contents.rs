//! `ComponentContents`: what a component says.

use super::Component;
use uuid::Uuid;

/// Where an `nbt` component reads from (`DataSource`).
#[derive(Clone, Debug, PartialEq)]
pub enum NbtSource {
    /// `BlockDataSource`: coordinates, as written.
    Block(String),
    /// `EntityDataSource`: a selector.
    Entity(String),
    /// `StorageDataSource`: a storage id.
    Storage(String),
}

/// `ResolvableProfile`, as far as a head needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Profile {
    pub name: Option<String>,
    pub id: Option<Uuid>,
    /// The `textures` property's value (base64 JSON), if any.
    pub textures: Option<String>,
    /// `PlayerSkin.Patch.texture`: a resource-pack texture for the skin.
    pub texture: Option<String>,
}

/// `AtlasSprite`: a sprite of an atlas.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AtlasSprite {
    pub atlas: String,
    pub sprite: String,
}

/// `PlayerSprite`: a player's face, with or without the hat layer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerSprite {
    pub profile: Profile,
    pub hat: bool,
}

/// `ObjectInfo`: what an `object` component shows in the text.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ObjectInfo {
    Atlas(AtlasSprite),
    Player(PlayerSprite),
}

impl ObjectInfo {
    /// `AtlasSprite.DEFAULT_ATLAS`.
    pub const DEFAULT_ATLAS: &'static str = "minecraft:blocks";

    /// `ObjectInfo.defaultFallback`.
    pub fn default_fallback(&self) -> String {
        let short = |id: &str| id.strip_prefix("minecraft:").unwrap_or(id).to_owned();
        match self {
            Self::Atlas(AtlasSprite { atlas, sprite }) => {
                if atlas == Self::DEFAULT_ATLAS {
                    format!("[{}]", short(sprite))
                } else {
                    format!("[{}@{}]", short(sprite), short(atlas))
                }
            }
            Self::Player(PlayerSprite { profile, .. }) => match &profile.name {
                Some(name) => format!("[{name} head]"),
                None => "[unknown player head]".to_owned(),
            },
        }
    }
}

/// `ComponentContents`.
#[derive(Clone, Debug, PartialEq)]
pub enum Contents {
    /// `PlainTextContents`.
    Text(String),
    /// `TranslatableContents`.
    Translatable {
        key: String,
        fallback: Option<String>,
        args: Vec<Component>,
    },
    /// `KeybindContents`.
    Keybind(String),
    /// `SelectorContents`.
    Selector {
        selector: String,
        separator: Option<Box<Component>>,
    },
    /// `ScoreContents`: a holder (a name or a selector) and an objective.
    Score { name: String, objective: String },
    /// `NbtContents`.
    Nbt {
        path: String,
        interpreting: bool,
        plain: bool,
        separator: Option<Box<Component>>,
        source: NbtSource,
    },
    /// `ObjectContents`.
    Object {
        info: ObjectInfo,
        fallback: Option<Box<Component>>,
    },
}
