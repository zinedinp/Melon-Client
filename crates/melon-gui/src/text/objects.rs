//! The pictures `object` components put in text (`AtlasGlyphProvider`).

use std::collections::HashMap;

use super::font::Font;
use crate::component::{AtlasSprite, ObjectInfo, PlayerSprite};
use crate::renderer::TextureId;

/// A rectangle of a texture, `[u0, v0, u1, v1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectQuad {
    pub texture: TextureId,
    pub uv: [f32; 4],
}

/// What an object is drawn as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectQuads {
    pub face: ObjectQuad,
    pub hat: Option<ObjectQuad>,
}

impl ObjectQuads {
    /// `PlayerGlyphProvider.Instance`.
    pub fn head(texture: TextureId, hat: bool) -> Self {
        let layer = |u: f32| ObjectQuad {
            texture,
            uv: [u / 64.0, 8.0 / 64.0, (u + 8.0) / 64.0, 16.0 / 64.0],
        };
        Self {
            face: layer(8.0),
            hat: hat.then(|| layer(40.0)),
        }
    }

    /// A whole texture.
    pub fn whole(texture: TextureId) -> Self {
        Self {
            face: ObjectQuad {
                texture,
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            hat: None,
        }
    }
}

/// What the font knows of its objects. `None` is one that couldn't be made.
#[derive(Default)]
pub struct ObjectSprites {
    pub(super) ready: HashMap<ObjectInfo, Option<ObjectQuads>>,
    pub(super) wanted: Vec<ObjectInfo>,
}

impl Font {
    /// The picture of `info` if it is made; else it is asked for.
    pub fn object_quads(&self, info: &ObjectInfo) -> Option<ObjectQuads> {
        let mut objects = self.objects.borrow_mut();
        if let Some(made) = objects.ready.get(info) {
            return *made;
        }
        if !objects.wanted.contains(info) {
            objects.wanted.push(info.clone());
        }
        None
    }

    /// The atlas sprites asked for that aren't made, once.
    pub fn take_wanted_objects(&self) -> Vec<ObjectInfo> {
        let mut objects = self.objects.borrow_mut();
        let (atlas, heads) = std::mem::take(&mut objects.wanted)
            .into_iter()
            .partition(|info| matches!(info, ObjectInfo::Atlas(_)));
        objects.wanted = heads;
        atlas
    }

    /// Records what `info` is drawn as.
    pub fn set_object(&self, info: ObjectInfo, quads: Option<ObjectQuads>) {
        let mut objects = self.objects.borrow_mut();
        objects.wanted.retain(|w| *w != info);
        objects.ready.insert(info, quads);
    }

    /// The heads text has shown, for the skins that arrive later.
    pub fn player_objects(&self) -> Vec<PlayerSprite> {
        let objects = self.objects.borrow();
        let known = objects.ready.keys().chain(&objects.wanted);
        known
            .filter_map(|info| match info {
                ObjectInfo::Player(player) => Some(player.clone()),
                ObjectInfo::Atlas(_) => None,
            })
            .collect()
    }
}

/// The texture ids an atlas sprite is looked for under (`AtlasIds`' sources).
fn folders(atlas: &str) -> &'static [&'static str] {
    match atlas {
        "minecraft:gui" => &["gui/sprites/", ""],
        "minecraft:particles" => &["particle/", ""],
        "minecraft:paintings" => &["painting/", ""],
        "minecraft:mob_effects" => &["mob_effect/", ""],
        _ => &[""],
    }
}

/// The image of an atlas sprite.
pub fn sprite_image(root: &std::path::Path, sprite: &AtlasSprite) -> Option<image::RgbaImage> {
    let (namespace, path) = sprite.sprite.split_once(':')?;
    folders(&sprite.atlas).iter().find_map(|folder| {
        let image =
            melon_assets::assets::texture_image(root, &format!("{namespace}:{folder}{path}"))?;
        let side = image.width();
        Some(if image.height() > side {
            image::imageops::crop_imm(&image, 0, 0, side, side).to_image()
        } else {
            image
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{PlayerSprite, Profile};

    fn font() -> Font {
        Font::from_bitmaps(Default::default(), Vec::new())
    }

    #[test]
    fn an_object_is_asked_for_once_and_known_when_made() {
        let font = font();
        let info = ObjectInfo::Atlas(AtlasSprite {
            atlas: "minecraft:blocks".into(),
            sprite: "minecraft:block/stone".into(),
        });
        assert_eq!(font.object_quads(&info), None);
        assert_eq!(font.object_quads(&info), None);
        assert_eq!(font.take_wanted_objects(), vec![info.clone()]);
        assert!(font.take_wanted_objects().is_empty());
        let quads = ObjectQuads::whole(TextureId(3));
        font.set_object(info.clone(), Some(quads));
        assert_eq!(font.object_quads(&info), Some(quads));
        assert!(font.take_wanted_objects().is_empty());
    }

    #[test]
    fn a_head_is_the_face_and_the_hat_of_the_skin() {
        let quads = ObjectQuads::head(TextureId(1), true);
        assert_eq!(quads.face.uv, [0.125, 0.125, 0.25, 0.25]);
        assert_eq!(quads.hat.unwrap().uv, [0.625, 0.125, 0.75, 0.25]);
        assert!(ObjectQuads::head(TextureId(1), false).hat.is_none());
        let font = font();
        let player = PlayerSprite {
            profile: Profile::default(),
            hat: true,
        };
        font.object_quads(&ObjectInfo::Player(player.clone()));
        assert_eq!(font.player_objects(), vec![player]);
    }
}
