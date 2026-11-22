//! `ClientboundTakeItemEntityPacket`.

use super::*;

/// `ItemPickupParticle`: it lives this many ticks.
pub const PICKUP_TICKS: u8 = 3;

/// An item on its way into someone's hands.
pub struct Pickup {
    /// What it showed, with its place and its spin when it was taken.
    pub stack: ItemStack,
    pub bob_offs: f32,
    pub age: f32,
    pub from: DVec3,
    taker: i32,
    life: u8,
    prev_target: DVec3,
    target: DVec3,
}

impl Pickup {
    pub fn taker(&self) -> i32 {
        self.taker
    }

    /// Where it is drawn.
    pub fn position(&self, partial: f32) -> DVec3 {
        let t = f64::from((f32::from(self.life) + partial) / f32::from(PICKUP_TICKS));
        let target = self.prev_target.lerp(self.target, f64::from(partial));
        self.from.lerp(target, t * t)
    }
}

/// `ITEM_PICKUP` or `EXPERIENCE_ORB_PICKUP`, to be played.
pub struct PickupSound {
    pub at: DVec3,
    /// A random draw in `0..1`, for the pitch's jitter.
    pub jitter: f32,
}

impl Entities {
    /// `ClientPacketListener.handleTakeItemEntity`.
    pub(super) fn take_item(&mut self, item: i32, taker: i32, amount: i32) {
        let Some(entity) = self.by_id.get_mut(&item) else {
            return;
        };
        self.pickup_sounds.push(PickupSound {
            at: entity.pos,
            jitter: ((entity.age as f32 * 0.618 + item as f32 * 0.37).fract()).abs(),
        });
        if entity.kind != EntityKind::Item {
            self.remove(item);
            return;
        }
        self.pickups.push(Pickup {
            stack: (*entity.item).clone(),
            bob_offs: entity.bob_offs,
            age: entity.age_in_ticks(1.0),
            from: entity.pos,
            taker,
            life: 0,
            prev_target: entity.pos,
            target: entity.pos,
        });
        let left = i32::from(entity.item.item_count) - amount.max(0);
        if left <= 0 {
            self.remove(item);
        } else {
            entity.item.set_count(left as u8);
        }
    }

    /// Moves the flying items on.
    pub fn tick_pickups(&mut self, taker_at: impl Fn(i32) -> Option<DVec3>) {
        for pickup in &mut self.pickups {
            if let Some(at) = taker_at(pickup.taker) {
                pickup.target = at;
                if pickup.life == 0 {
                    pickup.prev_target = at;
                }
            }
            pickup.life += 1;
            if pickup.life < PICKUP_TICKS {
                pickup.prev_target = pickup.target;
            }
        }
        self.pickups.retain(|p| p.life < PICKUP_TICKS);
    }

    pub fn pickups(&self) -> &[Pickup] {
        &self.pickups
    }

    /// The pickup sounds since the last call.
    pub fn take_pickup_sounds(&mut self) -> Vec<PickupSound> {
        std::mem::take(&mut self.pickup_sounds)
    }

    /// The middle of an entity's body, where an item flies to.
    pub fn body_middle(&self, id: i32) -> Option<DVec3> {
        let e = self.by_id.get(&id)?;
        Some(e.pos + DVec3::new(0.0, f64::from(e.ty.eye_height * e.scale()) / 2.0, 0.0))
    }
}
