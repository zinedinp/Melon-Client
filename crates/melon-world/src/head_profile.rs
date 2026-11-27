//! Vanilla `ResolvableProfile` as far as a player head needs it.

use pumpkin_data::data_component_impl::ProfileImpl;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::tag::NbtTag;
use uuid::Uuid;

/// Keys for the skins known only by their `textures` value or a name.
const KEY_NAMESPACE: Uuid = Uuid::from_u128(0x6d65_6c6f_6e2d_6865_6164_2d73_6b69_6e00);

/// Whether `state` is a player head, standing or on a wall.
pub fn is_player_head(state: u16) -> bool {
    use crate::block_entity::{Kind, kind_of, models::SkullType};
    matches!(
        kind_of(state),
        Some(Kind::Skull {
            skull: SkullType::Player,
            ..
        })
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeadProfile {
    pub id: Option<Uuid>,
    pub name: Option<Box<str>>,
    /// The `textures` property: Base64 JSON naming the skin.
    pub textures: Option<Box<str>>,
}

impl HeadProfile {
    /// `None` for a profile that names nobody.
    pub fn from_component(profile: &ProfileImpl) -> Option<Self> {
        let id = profile.id.map(|[a, b, c, d]| {
            let part = |v: i32| u128::from(v as u32);
            Uuid::from_u128(part(a) << 96 | part(b) << 64 | part(c) << 32 | part(d))
        });
        let textures = profile
            .properties
            .iter()
            .find(|p| p.name == "textures")
            .map(|p| p.value.as_str().into());
        let out = Self {
            id,
            name: profile.name.as_deref().map(Into::into),
            textures,
        };
        (out.id.is_some() || out.name.is_some() || out.textures.is_some()).then_some(out)
    }

    /// A skull block entity's `profile`.
    pub fn from_nbt(tag: &NbtTag) -> Option<Self> {
        Self::from_component(&ProfileImpl::read_data(tag)?)
    }

    /// The `minecraft:profile` of a player head item.
    pub fn of_stack(stack: &ItemStack) -> Option<Self> {
        if stack.item.registry_key != "player_head" {
            return None;
        }
        Self::from_component(stack.get_data_component::<ProfileImpl>()?)
    }

    /// What the skin is kept under.
    pub fn skin_key(&self) -> Uuid {
        match (&self.textures, self.id, &self.name) {
            (Some(textures), ..) => Uuid::new_v5(&KEY_NAMESPACE, textures.as_bytes()),
            (None, Some(id), _) => id,
            (None, None, Some(name)) => {
                let name = format!("name:{}", name.to_ascii_lowercase());
                Uuid::new_v5(&KEY_NAMESPACE, name.as_bytes())
            }
            (None, None, None) => Uuid::nil(),
        }
    }
}

/// `PlayerSkin.Patch`: what a profile's skin is overridden with. Only the
/// body texture and the arms are drawn; the cape and elytra aren't.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkinPatch {
    /// `texture`: an asset id, `minecraft:entity/player/wide/steve`.
    pub body: Option<Box<str>>,
    /// `model`: `true` for slim arms.
    pub slim: Option<bool>,
}

/// `Mannequin.DATA_PROFILE`: whose skin it wears, and the patch on it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AvatarProfile {
    /// `None` for `ResolvableProfile.Static.EMPTY`, a nameless nobody.
    pub profile: Option<HeadProfile>,
    pub patch: SkinPatch,
}

impl AvatarProfile {
    /// `DefaultPlayerSkin.get(partialProfile)`: the default skin is picked
    /// by the profile's id, the nil one when it has none.
    pub fn default_skin_id(&self) -> Uuid {
        self.profile
            .as_ref()
            .and_then(|p| p.id)
            .unwrap_or(Uuid::nil())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_nbt::compound::NbtCompound;

    #[test]
    fn a_profile_is_read_from_its_name_or_its_compound() {
        let named = HeadProfile::from_nbt(&NbtTag::String("Notch".into())).unwrap();
        assert_eq!(named.name.as_deref(), Some("Notch"));
        assert_eq!(named.skin_key(), {
            let other = HeadProfile::from_nbt(&NbtTag::String("notch".into())).unwrap();
            other.skin_key()
        });

        let mut compound = NbtCompound::new();
        compound.put("id", NbtTag::IntArray(vec![1, 2, 3, -1]));
        let mut property = NbtCompound::new();
        property.put_string("name", "textures".to_owned());
        property.put_string("value", "e30=".to_owned());
        compound.put("properties", NbtTag::List(vec![NbtTag::Compound(property)]));
        let full = HeadProfile::from_nbt(&NbtTag::Compound(compound)).unwrap();
        assert_eq!(
            full.id,
            Some(Uuid::from_u128(0x0000_0001_0000_0002_0000_0003_ffff_ffff))
        );
        assert_eq!(full.textures.as_deref(), Some("e30="));
        assert_ne!(full.skin_key(), full.id.unwrap());
        assert_eq!(full.skin_key().get_version_num(), 5);

        assert!(HeadProfile::from_nbt(&NbtTag::Compound(NbtCompound::new())).is_none());
    }
}
