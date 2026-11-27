//! `ClientboundAddEntityPacket` and `ClientboundRemoveEntitiesPacket`.

use anyhow::Result;

use melon_protocol::packet::Reader;

/// `ClientboundAddEntityPacket`. Angles are vanilla's, in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct AddEntity {
    pub id: i32,
    pub uuid: uuid::Uuid,
    /// `EntityType` registry id.
    pub kind: i32,
    pub pos: [f64; 3],
    /// Per tick.
    pub velocity: [f64; 3],
    pub pitch: f32,
    pub yaw: f32,
    pub head_yaw: f32,
    /// Type-specific (a falling block's state, an item frame's facing).
    pub data: i32,
}

/// `Mth.unpackDegrees`: a byte angle in degrees.
pub fn unpack_degrees(byte: u8) -> f32 {
    f32::from(byte as i8) * 360.0 / 256.0
}

pub fn read_add_entity(r: &mut Reader<'_>) -> Result<AddEntity> {
    Ok(AddEntity {
        id: r.var_int()?,
        uuid: r.uuid()?,
        kind: r.var_int()?,
        pos: r.vec3_f64()?,
        velocity: r.lp_vec3()?,
        pitch: unpack_degrees(r.u8()?),
        yaw: unpack_degrees(r.u8()?),
        head_yaw: unpack_degrees(r.u8()?),
        data: r.var_int()?,
    })
}

pub fn read_remove_entities(r: &mut Reader<'_>) -> Result<Vec<i32>> {
    let count = r.list_count()?;
    (0..count).map(|_| r.var_int()).collect()
}
