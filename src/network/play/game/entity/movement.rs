//! How the server moves entities in 26.3.

use anyhow::{Result, bail};

use super::spawn::unpack_degrees;
use crate::network::protocol::packet::Reader;

/// `ClientboundMoveEntityPacket.Pos`, `PosRot` and `Rot`.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveEntity {
    pub id: i32,
    /// `VecDelta`.
    pub steps: Vec<[i16; 3]>,
    /// Yaw and pitch, degrees.
    pub rotation: Option<[f32; 2]>,
    pub on_ground: bool,
}

/// An absolute position with its rotation: `ClientboundEntityPositionSyncPacket`
#[derive(Clone, Debug, PartialEq)]
pub struct SetPosition {
    pub id: i32,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `Relative` bits of a teleport (0 for a sync).
    pub relative: i32,
    pub on_ground: bool,
    /// The velocity a teleport carries (`PositionMoveRotation.deltaMovement`);
    pub velocity: Option<[f64; 3]>,
}

/// `Relative`'s bits, as `ClientboundPlayerPositionPacket` has them too.
pub mod relative {
    pub const X: i32 = 1 << 0;
    pub const Y: i32 = 1 << 1;
    pub const Z: i32 = 1 << 2;
    pub const Y_ROT: i32 = 1 << 3;
    pub const X_ROT: i32 = 1 << 4;
    pub const DELTA_X: i32 = 1 << 5;
    pub const DELTA_Y: i32 = 1 << 6;
    pub const DELTA_Z: i32 = 1 << 7;
}

/// The packed var-int in front of a delta: on ground, and how many steps.
fn properties(r: &mut Reader<'_>) -> Result<(bool, usize)> {
    let properties = r.var_int()? as u32;
    Ok((properties & 1 != 0, (properties >> 1) as usize))
}

/// `VecDelta.read`.
fn vec_delta(r: &mut Reader<'_>, steps: usize) -> Result<Vec<[i16; 3]>> {
    let short3 =
        |r: &mut Reader<'_>| Ok::<_, anyhow::Error>([r.i16_be()?, r.i16_be()?, r.i16_be()?]);
    if steps == 0 {
        return Ok(vec![short3(r)?]);
    }
    if steps > r.remaining().len() / 7 {
        bail!("VecDelta of {steps} steps");
    }
    (0..steps)
        .map(|_| {
            r.var_int()?;
            short3(r)
        })
        .collect()
}

pub fn read_move_pos(r: &mut Reader<'_>, with_rotation: bool) -> Result<MoveEntity> {
    let id = r.var_int()?;
    let (on_ground, steps) = properties(r)?;
    let steps = vec_delta(r, steps)?;
    let rotation = if with_rotation {
        let yaw = unpack_degrees(r.u8()?);
        Some([yaw, unpack_degrees(r.u8()?)])
    } else {
        None
    };
    Ok(MoveEntity {
        id,
        steps,
        rotation,
        on_ground,
    })
}

pub fn read_move_rot(r: &mut Reader<'_>) -> Result<MoveEntity> {
    let id = r.var_int()?;
    let on_ground = r.bool()?;
    let yaw = unpack_degrees(r.u8()?);
    let pitch = unpack_degrees(r.u8()?);
    Ok(MoveEntity {
        id,
        steps: Vec::new(),
        rotation: Some([yaw, pitch]),
        on_ground,
    })
}

/// `ClientboundEntityPositionSyncPacket`: a `PositionPath` (linear.
pub fn read_position_sync(r: &mut Reader<'_>) -> Result<SetPosition> {
    let id = r.var_int()?;
    let pos = match r.var_int()? {
        0 => r.vec3_f64()?,
        1 => {
            let count = r.var_int()?.clamp(0, 4096);
            let mut end = None;
            for _ in 0..count {
                end = Some(r.vec3_f64()?);
                r.var_int()?;
            }
            end.ok_or_else(|| anyhow::anyhow!("empty position path"))?
        }
        other => bail!("position path type {other}"),
    };
    Ok(SetPosition {
        id,
        pos,
        yaw: r.f32_be()?,
        pitch: r.f32_be()?,
        relative: 0,
        on_ground: r.bool()?,
        velocity: None,
    })
}

/// `ClientboundTeleportEntityPacket`.
pub fn read_teleport(r: &mut Reader<'_>) -> Result<SetPosition> {
    let id = r.var_int()?;
    let pos = r.vec3_f64()?;
    let delta = r.vec3_f64()?;
    let yaw = r.f32_be()?;
    let pitch = r.f32_be()?;
    let relative = r.i32_be()?;
    Ok(SetPosition {
        id,
        pos,
        yaw,
        pitch,
        relative,
        on_ground: r.bool()?,
        velocity: Some(delta),
    })
}

/// `ClientboundRotateHeadPacket`: the head's yaw, degrees.
pub fn read_rotate_head(r: &mut Reader<'_>) -> Result<(i32, f32)> {
    Ok((r.var_int()?, unpack_degrees(r.u8()?)))
}
