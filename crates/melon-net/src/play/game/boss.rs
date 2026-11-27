//! `ClientboundBossEventPacket`: the boss bars at the top of the screen.

use crate::component::Component;
use anyhow::{Result, bail};
use uuid::Uuid;

use crate::protocol::packet::Reader;

/// `BossEvent.BossBarColor`, in ordinal order.
pub const COLORS: usize = 7;
/// `BossEvent.BossBarOverlay`: 0 is `PROGRESS`, 1 to 4 the notched ones.
pub const OVERLAYS: usize = 5;

/// What a boss bar packet does to the bar of `id`.
pub enum BossEvent {
    Add {
        id: Uuid,
        name: Component,
        progress: f32,
        color: u8,
        overlay: u8,
        flags: u8,
    },
    Remove(Uuid),
    Progress(Uuid, f32),
    Name(Uuid, Component),
    Style(Uuid, u8, u8),
    Properties(Uuid, u8),
}

/// Reads a color or overlay ordinal; one that does not exist is an error.
fn ordinal(r: &mut Reader, count: usize) -> Result<u8> {
    let value = r.var_int()?;
    match usize::try_from(value) {
        Ok(v) if v < count => Ok(v as u8),
        _ => bail!("boss bar: unknown ordinal {value}"),
    }
}

pub fn decode(r: &mut Reader) -> Result<BossEvent> {
    let id = r.uuid()?;
    Ok(match r.var_int()? {
        0 => BossEvent::Add {
            id,
            name: r.component()?,
            progress: r.f32_be()?,
            color: ordinal(r, COLORS)?,
            overlay: ordinal(r, OVERLAYS)?,
            flags: r.u8()?,
        },
        1 => BossEvent::Remove(id),
        2 => BossEvent::Progress(id, r.f32_be()?),
        3 => BossEvent::Name(id, r.component()?),
        4 => BossEvent::Style(id, ordinal(r, COLORS)?, ordinal(r, OVERLAYS)?),
        5 => BossEvent::Properties(id, r.u8()?),
        other => bail!("boss bar: unknown operation {other}"),
    })
}
