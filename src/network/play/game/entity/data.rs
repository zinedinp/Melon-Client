//! `ClientboundSetEntityDataPacket`.

use crate::component::Component;
use anyhow::{Result, bail};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::meta_data_type::MetaDataType as T;

use crate::network::protocol::packet::Reader;
use crate::network::protocol::reader::initial_capacity;

/// A value Melon keeps.
#[derive(Clone)]
pub enum DataValue {
    Byte(u8),
    Int(i32),
    Long(i64),
    Float(f32),
    Bool(bool),
    Item(Box<ItemStack>),
    /// `VillagerData`: type, profession and level, as registry ids.
    Villager(i32, i32, i32),
    /// An optional text component (`DATA_CUSTOM_NAME`).
    Name(Option<Box<Component>>),
    /// `PARTICLE` and `PARTICLES`.
    Particles(Vec<crate::particle::ParticleSpec>),
    /// `Rotations`: an armor stand's limb angles, in degrees (x, y, z).
    Rotations([f32; 3]),
    /// `Vector3f`: a display's translation and scale.
    Vec3([f32; 3]),
    /// `Quaternionf` (x, y, z, w): a display's rotations.
    Quat([f32; 4]),
    /// A text component (`DATA_TEXT_ID` of a text display).
    Text(Box<Component>),
    /// `OPTIONAL_BLOCK_POS`: an end crystal's beam target.
    BlockPos(Option<[i32; 3]>),
    /// `RESOLVABLE_PROFILE`: a mannequin's skin.
    Profile(Box<crate::world::head_profile::AvatarProfile>),
}

impl std::fmt::Debug for DataValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Byte(b) => write!(f, "Byte({b})"),
            Self::Int(i) => write!(f, "Int({i})"),
            Self::Long(l) => write!(f, "Long({l})"),
            Self::Float(v) => write!(f, "Float({v})"),
            Self::Bool(b) => write!(f, "Bool({b})"),
            Self::Item(stack) => write!(f, "Item({})", stack.item.registry_key),
            Self::Villager(kind, job, level) => write!(f, "Villager({kind}, {job}, {level})"),
            Self::Name(name) => write!(f, "Name({name:?})"),
            Self::Particles(list) => write!(f, "Particles({list:?})"),
            Self::Rotations(r) => write!(f, "Rotations({r:?})"),
            Self::Vec3(v) => write!(f, "Vec3({v:?})"),
            Self::Quat(q) => write!(f, "Quat({q:?})"),
            Self::Text(t) => write!(f, "Text({:?})", t.plain_text()),
            Self::BlockPos(p) => write!(f, "BlockPos({p:?})"),
            Self::Profile(p) => write!(f, "Profile({p:?})"),
        }
    }
}

/// Item stacks compare by item and count only.
impl PartialEq for DataValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Byte(a), Self::Byte(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Long(a), Self::Long(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Villager(a, b, c), Self::Villager(x, y, z)) => (a, b, c) == (x, y, z),
            (Self::Name(a), Self::Name(b)) => a == b,
            (Self::Particles(a), Self::Particles(b)) => a == b,
            (Self::Rotations(a), Self::Rotations(b)) => a == b,
            (Self::Vec3(a), Self::Vec3(b)) => a == b,
            (Self::Quat(a), Self::Quat(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::BlockPos(a), Self::BlockPos(b)) => a == b,
            (Self::Profile(a), Self::Profile(b)) => a == b,
            (Self::Item(a), Self::Item(b)) => {
                a.item.id == b.item.id && a.item_count == b.item_count
            }
            _ => false,
        }
    }
}

impl DataValue {
    pub fn vec3(&self) -> Option<[f32; 3]> {
        match *self {
            Self::Vec3(v) => Some(v),
            _ => None,
        }
    }

    pub fn quat(&self) -> Option<[f32; 4]> {
        match *self {
            Self::Quat(q) => Some(q),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&Component> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// An optional block position; `None` for another type.
    pub fn block_pos(&self) -> Option<Option<[i32; 3]>> {
        match self {
            Self::BlockPos(p) => Some(*p),
            _ => None,
        }
    }

    pub fn long(&self) -> Option<i64> {
        match *self {
            Self::Long(l) => Some(l),
            _ => None,
        }
    }

    pub fn byte(&self) -> Option<u8> {
        match *self {
            Self::Byte(b) => Some(b),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i32> {
        match *self {
            Self::Int(i) => Some(i),
            _ => None,
        }
    }

    pub fn float(&self) -> Option<f32> {
        match *self {
            Self::Float(f) => Some(f),
            _ => None,
        }
    }

    /// The custom name, if this is one: `Some(None)` for no name.
    pub fn rotations(&self) -> Option<[f32; 3]> {
        match *self {
            Self::Rotations(r) => Some(r),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<Option<&Component>> {
        match self {
            Self::Name(name) => Some(name.as_deref()),
            _ => None,
        }
    }

    pub fn villager(&self) -> Option<(i32, i32, i32)> {
        match *self {
            Self::Villager(kind, job, level) => Some((kind, job, level)),
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(b) => Some(b),
            _ => None,
        }
    }
}

/// The values of one packet's list, by index, as far as they can be read.
pub fn read_entity_data(r: &mut Reader<'_>) -> Vec<(u8, DataValue)> {
    let mut values = Vec::new();
    while let Ok(index) = r.u8() {
        if index == 0xFF {
            break;
        }
        let Ok(ty) = r.var_int() else {
            break;
        };
        match read_value(r, ty) {
            Ok(Some(value)) => values.push((index, value)),
            Ok(None) => {}
            Err(e) => {
                crate::probe::count("entity.data", format!("index {index} serializer {ty}: {e}"));
                break;
            }
        }
    }
    values
}

fn read_value(r: &mut Reader<'_>, ty: i32) -> Result<Option<DataValue>> {
    const BYTE: i32 = T::BYTE.id;
    const BOOLEAN: i32 = T::BOOLEAN.id;
    const FLOAT: i32 = T::FLOAT.id;
    const ITEM: i32 = T::ITEM_STACK.id;
    const LONG: i32 = T::LONG.id;
    const STRING: i32 = T::STRING.id;
    const COMPONENT: i32 = T::COMPONENT.id;
    const OPT_COMPONENT: i32 = T::OPTIONAL_COMPONENT.id;
    const ROTATIONS: i32 = T::ROTATIONS.id;
    const VECTOR3: i32 = T::VECTOR3.id;
    const QUATERNION: i32 = T::QUATERNION.id;
    const BLOCK_POS: i32 = T::BLOCK_POS.id;
    const OPT_BLOCK_POS: i32 = T::OPTIONAL_BLOCK_POS.id;
    const OPT_ENTITY: i32 = T::OPTIONAL_LIVING_ENTITY_REFERENCE.id;
    const VILLAGER: i32 = T::VILLAGER_DATA.id;
    const GLOBAL_POS: i32 = T::OPTIONAL_GLOBAL_POS.id;
    const PAINTING: i32 = T::PAINTING_VARIANT.id;
    const PARTICLE: i32 = T::PARTICLE.id;
    const PARTICLES: i32 = T::PARTICLES.id;
    const PROFILE: i32 = T::RESOLVABLE_PROFILE.id;
    Ok(Some(match ty {
        BYTE => DataValue::Byte(r.u8()?),
        BOOLEAN => DataValue::Bool(r.bool()?),
        FLOAT => DataValue::Float(r.f32_be()?),
        ITEM => DataValue::Item(Box::new(r.item_stack()?)),
        LONG => DataValue::Long(r.var_long()?),
        STRING => {
            r.string()?;
            return Ok(None);
        }
        COMPONENT => DataValue::Text(Box::new(r.component()?)),
        OPT_COMPONENT => {
            let name = if r.bool()? {
                Some(Box::new(r.component()?))
            } else {
                None
            };
            DataValue::Name(name)
        }
        ROTATIONS => DataValue::Rotations([r.f32_be()?, r.f32_be()?, r.f32_be()?]),
        VECTOR3 => DataValue::Vec3([r.f32_be()?, r.f32_be()?, r.f32_be()?]),
        QUATERNION => DataValue::Quat([r.f32_be()?, r.f32_be()?, r.f32_be()?, r.f32_be()?]),
        BLOCK_POS => {
            r.i64_be()?;
            return Ok(None);
        }
        OPT_BLOCK_POS => DataValue::BlockPos(if r.bool()? {
            Some(unpack_block_pos(r.i64_be()?))
        } else {
            None
        }),
        OPT_ENTITY => {
            if r.bool()? {
                r.uuid()?;
            }
            return Ok(None);
        }
        VILLAGER => DataValue::Villager(r.var_int()?, r.var_int()?, r.var_int()?),
        GLOBAL_POS => {
            if r.bool()? {
                r.string()?;
                r.i64_be()?;
            }
            return Ok(None);
        }
        PAINTING => {
            // A holder: 0 for an inline variant, else the registry id + 1.
            let id = r.var_int()?;
            if id == 0 {
                bail!("inline painting variant");
            }
            DataValue::Int(id - 1)
        }
        PARTICLE => DataValue::Particles(vec![r.particle()?]),
        PARTICLES => {
            let count = r.list_count()?;
            let mut list = Vec::with_capacity(initial_capacity(count));
            for _ in 0..count {
                list.push(r.particle()?);
            }
            DataValue::Particles(list)
        }
        PROFILE => DataValue::Profile(Box::new(read_profile(r)?)),
        _ if (0..64).contains(&ty) => DataValue::Int(r.var_int()?),
        _ => bail!("entity data type {ty}"),
    }))
}

/// `ResolvableProfile.STREAM_CODEC`: a whole `GameProfile` or a partial
/// one (either), then the `PlayerSkin.Patch`.
fn read_profile(r: &mut Reader<'_>) -> Result<crate::world::head_profile::AvatarProfile> {
    use crate::world::head_profile::{AvatarProfile, HeadProfile, SkinPatch};
    let (id, name) = if r.bool()? {
        (Some(r.uuid()?), Some(r.string_max(16)?))
    } else {
        let name = if r.bool()? {
            Some(r.string_max(16)?)
        } else {
            None
        };
        let id = if r.bool()? { Some(r.uuid()?) } else { None };
        (id, name)
    };
    // `GAME_PROFILE_PROPERTIES`: name, value and an optional signature.
    let count = r.var_int()?;
    if !(0..=16).contains(&count) {
        bail!("{count} profile properties");
    }
    let mut textures = None;
    for _ in 0..count {
        let key = r.string_max(64)?;
        let value = r.string_max(32767)?;
        if r.bool()? {
            r.string_max(1024)?;
        }
        if key == "textures" {
            textures = Some(value.into_boxed_str());
        }
    }
    let asset = |r: &mut Reader<'_>| -> Result<Option<Box<str>>> {
        Ok(if r.bool()? {
            Some(r.string()?.into_boxed_str())
        } else {
            None
        })
    };
    let body = asset(r)?;
    let _cape = asset(r)?;
    let _elytra = asset(r)?;
    let slim = if r.bool()? { Some(r.bool()?) } else { None };
    let profile = HeadProfile {
        id: id.filter(|id| !id.is_nil()),
        name: name.filter(|n| !n.is_empty()).map(String::into_boxed_str),
        textures,
    };
    let named = profile.id.is_some() || profile.name.is_some() || profile.textures.is_some();
    Ok(AvatarProfile {
        profile: named.then_some(profile),
        patch: SkinPatch { body, slim },
    })
}

/// `BlockPos.of`: 26 bits of x, 26 of z, 12 of y.
fn unpack_block_pos(packed: i64) -> [i32; 3] {
    [
        (packed >> 38) as i32,
        ((packed << 52) >> 52) as i32,
        ((packed << 26) >> 38) as i32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_particle_list_does_not_end_the_values() {
        use crate::particle::{Kind, ParticleSpec};
        let mut body = vec![10, T::PARTICLES.id as u8, 2, Kind::Flame.to_id() as u8];
        body.push(Kind::EntityEffect.to_id() as u8);
        body.extend((-1i32).to_be_bytes());
        body.extend([0, 0, 0x01, 0xFF]);
        let values = read_entity_data(&mut Reader::new(&body));
        assert_eq!(values.len(), 2, "{values:?}");
        let DataValue::Particles(list) = &values[0].1 else {
            panic!("{values:?}");
        };
        assert_eq!(list[0], ParticleSpec::simple(Kind::Flame));
        assert_eq!(values[1], (0, DataValue::Byte(1)));
    }

    #[test]
    fn rotations_are_read_as_three_floats() {
        let mut body = vec![16, T::ROTATIONS.id as u8];
        for v in [-10.0_f32, 0.0, 45.5] {
            body.extend(v.to_be_bytes());
        }
        body.push(0xFF);
        let values = read_entity_data(&mut Reader::new(&body));
        assert_eq!(values, vec![(16, DataValue::Rotations([-10.0, 0.0, 45.5]))]);
    }

    #[test]
    fn reads_values_past_a_custom_name() {
        let body = [2, T::OPTIONAL_COMPONENT.id as u8, 0, 17, 0, 0x14, 0xFF];
        let values = read_entity_data(&mut Reader::new(&body));
        assert_eq!(
            values,
            vec![(2, DataValue::Name(None)), (17, DataValue::Byte(0x14))]
        );
    }

    #[test]
    fn a_custom_name_is_read_as_plain_text() {
        let mut body = vec![2, T::OPTIONAL_COMPONENT.id as u8, 1, 8, 0, 10];
        body.extend_from_slice(b"Dinnerbone");
        body.extend_from_slice(&[17, 0, 7, 0xFF]);
        let values = read_entity_data(&mut Reader::new(&body));
        let name = values[0].1.name().flatten().expect("a name");
        assert_eq!(name.plain_text(), "Dinnerbone");
        assert_eq!(values[1], (17, DataValue::Byte(7)));
    }

    #[test]
    fn an_unreadable_value_keeps_what_came_before() {
        let body = [0, 0, 1, 9, 200, 5, 0xFF];
        let values = read_entity_data(&mut Reader::new(&body));
        assert_eq!(values, vec![(0, DataValue::Byte(1))]);
    }

    #[test]
    fn a_mannequin_profile_is_read_with_the_values_after_it() {
        // A partial profile named "Notch" with no id or properties, a slim
        // patch, then a byte.
        let mut body = vec![17, T::RESOLVABLE_PROFILE.id as u8, 0, 1, 5];
        body.extend_from_slice(b"Notch");
        body.extend_from_slice(&[0, 0, 0, 0, 0, 1, 1, 16, 0, 0x7F, 0xFF]);
        let values = read_entity_data(&mut Reader::new(&body));
        let DataValue::Profile(profile) = &values[0].1 else {
            panic!("{values:?}");
        };
        let named = profile.profile.as_ref().unwrap();
        assert_eq!(named.name.as_deref(), Some("Notch"));
        assert_eq!(profile.patch.slim, Some(true));
        assert_eq!(values[1], (16, DataValue::Byte(0x7F)));
    }
}
