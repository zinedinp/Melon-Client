//! What pushes the local player.

use super::*;

/// `Entity.push(Entity)`'s strength per tick.
const PUSH_STRENGTH: f64 = 0.05;
/// A boat reaches this far (`inflate(0.2, -0.01, 0.2)`).
const BOAT_REACH: f64 = 0.2;
const BOAT_SQUEEZE: f64 = 0.01;

/// `Entity.push(Entity)`, as the impulse the one at `from` gives something at `at`.
pub fn push_impulse(from: DVec3, at: DVec3) -> DVec3 {
    let xa = at.x - from.x;
    let za = at.z - from.z;
    let dd = xa.abs().max(za.abs());
    if dd < f64::from(0.01_f32) {
        return DVec3::ZERO;
    }
    let dd = dd.sqrt();
    let pow = (1.0 / dd).min(1.0);
    DVec3::new(
        xa / dd * pow * PUSH_STRENGTH,
        0.0,
        za / dd * pow * PUSH_STRENGTH,
    )
}

impl ClientEntity {
    /// Whether it shoves the local player when their boxes meet.
    fn pushes_player(&self) -> bool {
        match self.kind {
            EntityKind::Bat | EntityKind::Parrot | EntityKind::Boat => false,
            _ => self.is_living() && self.ty != &EntityType::ARMOR_STAND,
        }
    }

    /// The box the entity pushes with: its own, a boat's a little wider.
    fn push_box(&self) -> Aabb {
        let (w, h) = self.size();
        let bounds = Aabb::at_feet(self.pos, f64::from(w), f64::from(h));
        if self.kind == EntityKind::Boat {
            Aabb {
                min: bounds.min + DVec3::new(-BOAT_REACH, BOAT_SQUEEZE, -BOAT_REACH),
                max: bounds.max + DVec3::new(BOAT_REACH, -BOAT_SQUEEZE, BOAT_REACH),
            }
        } else {
            bounds
        }
    }
}

impl Entities {
    /// The sum of the impulses the entities around give a player whose box is `own`.
    pub fn push_on_player(&self, own: &Aabb, team_ok: impl Fn(uuid::Uuid, bool) -> bool) -> DVec3 {
        let own_vehicle = self.own_vehicle();
        let own_feet = DVec3::new(
            f64::midpoint(own.min.x, own.max.x),
            own.min.y,
            f64::midpoint(own.min.z, own.max.z),
        );
        let mut total = DVec3::ZERO;
        for entity in self.by_id.values() {
            let boat = entity.kind == EntityKind::Boat;
            if !(boat || entity.pushes_player())
                || Some(entity.id) == own_vehicle
                || (own_vehicle.is_some() && entity.vehicle == own_vehicle)
                || !entity.push_box().intersects(own)
                || !team_ok(entity.uuid, entity.kind == EntityKind::Player)
            {
                continue;
            }
            if boat && own.min.y > entity.pos.y {
                continue;
            }
            total += push_impulse(entity.pos, own_feet);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_points_away_and_fades_with_distance() {
        let near = push_impulse(DVec3::ZERO, DVec3::new(0.25, 0.0, 0.0));
        assert!(near.x > 0.0 && near.z == 0.0);
        assert!((near.x - 0.5 * 0.05).abs() < 1e-12);
        let far = push_impulse(DVec3::ZERO, DVec3::new(0.0, 0.0, -4.0));
        assert!((far.z + 0.05).abs() < 1e-12);
    }

    #[test]
    fn standing_on_the_same_spot_does_not_push() {
        assert_eq!(
            push_impulse(DVec3::ZERO, DVec3::new(0.005, 0.0, 0.0)),
            DVec3::ZERO
        );
    }
}
