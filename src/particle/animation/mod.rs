//! How particles look over their life.

use std::path::Path;

use super::Kind;
use super::kind;

pub mod fade;

/// An atlas rectangle `[u0, v0, u1, v1]`.
pub type Rect = [f32; 4];

/// The sprites of every type, by type id: what each `particles/*.json`
#[derive(Clone, Debug, Default)]
pub struct SpriteSets {
    sets: Vec<Vec<Rect>>,
    /// The missing texture, for a type whose JSON names nothing found.
    missing: Rect,
}

impl SpriteSets {
    /// The atlas ids every set needs (`minecraft:particle/generic_0`...).
    pub fn texture_ids(assets_root: &Path) -> Vec<String> {
        let mut ids: Vec<String> = kind::all()
            .flat_map(|kind| textures_of(assets_root, kind))
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// The sets, with each texture's place from `sprite` (an atlas lookup), `missing` for one
    /// not found.
    pub fn load(assets_root: &Path, sprite: impl Fn(&str) -> Option<Rect>, missing: Rect) -> Self {
        let sets = (0..kind::COUNT)
            .map(|id| {
                Kind::from_id(id).map_or_else(Vec::new, |kind| {
                    textures_of(assets_root, kind)
                        .iter()
                        .map(|id| sprite(id).unwrap_or(missing))
                        .collect()
                })
            })
            .collect();
        Self { sets, missing }
    }

    /// The sprites of `kind`; empty for a type drawn without any.
    pub fn set(&self, kind: Kind) -> &[Rect] {
        self.sets
            .get(usize::from(kind.to_id()))
            .map_or(&[], Vec::as_slice)
    }

    /// `SpriteSet.first`.
    pub fn first(&self, kind: Kind) -> Rect {
        self.set(kind).first().copied().unwrap_or(self.missing)
    }

    /// `SpriteSet.get(age, lifetime)`.
    pub fn from_age(&self, kind: Kind, age: i32, lifetime: i32) -> Rect {
        let set = self.set(kind);
        match set.len() {
            0 => self.missing,
            n => set[sprite_index(age, lifetime, n)],
        }
    }

    /// `SpriteSet.get(random)`: one of the sprites, `roll` in `0..1`.
    pub fn random(&self, kind: Kind, roll: f32) -> Rect {
        let set = self.set(kind);
        match set.len() {
            0 => self.missing,
            n => set[((roll * n as f32) as usize).min(n - 1)],
        }
    }
}

/// `MutableSpriteSet.get(index, max)`: `index * (n - 1) / max`.
pub fn sprite_index(age: i32, lifetime: i32, n: usize) -> usize {
    if n <= 1 || lifetime <= 0 {
        return 0;
    }
    let last = n as i32 - 1;
    (age.clamp(0, lifetime) * last / lifetime).clamp(0, last) as usize
}

/// Particle types registered without a sprite set (`ParticleEngine` gives them none).
const SPRITELESS: &[&str] = &[
    "block",
    "block_crumble",
    "block_marker",
    "dust_pillar",
    "elder_guardian",
    "explosion_emitter",
    "gust_emitter_large",
    "gust_emitter_small",
    "item",
    "item_cobweb",
    "item_slime",
    "item_snowball",
    "noxious_gas_cloud",
    "geyser",
];

/// The atlas ids `particles/<kind>.json` lists: `minecraft:generic_0`
fn textures_of(assets_root: &Path, kind: Kind) -> Vec<String> {
    if SPRITELESS.contains(&kind.to_name()) {
        return Vec::new();
    }
    let Some(json) = crate::assets::json_resource(assets_root, "particles", kind.to_name()) else {
        return Vec::new();
    };
    json["textures"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str())
        .map(|id| {
            let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
            format!("{namespace}:particle/{path}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sprite_follows_the_age() {
        assert_eq!(sprite_index(0, 10, 8), 0);
        assert_eq!(sprite_index(10, 10, 8), 7);
        assert_eq!(sprite_index(5, 10, 8), 3);
        assert_eq!(sprite_index(20, 10, 8), 7);
        assert_eq!(sprite_index(3, 0, 8), 0);
        assert_eq!(sprite_index(3, 10, 1), 0);
    }

    #[test]
    fn the_sets_name_their_textures() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        if crate::assets::json_resource(&root, "particles", "smoke").is_none() {
            return;
        }
        let smoke = textures_of(&root, Kind::Smoke);
        assert_eq!(smoke.len(), 8);
        assert_eq!(smoke[0], "minecraft:particle/generic_7");
        assert!(textures_of(&root, Kind::Block).is_empty());
        let sets = SpriteSets::load(&root, |_| Some([0.0, 0.0, 1.0, 1.0]), [0.0; 4]);
        assert_eq!(sets.set(Kind::Smoke).len(), 8);
        assert_eq!(sets.first(Kind::Block), [0.0; 4]);
    }
}
