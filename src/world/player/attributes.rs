//! The attributes the server sets on the local player and how movement

use pumpkin_data::attributes::Attributes as A;

/// `LivingEntity.SPEED_MODIFIER_SPRINTING_ID`.
const SPRINTING_MODIFIER: &str = "minecraft:sprinting";

/// One modifier: `(id, amount, operation)`.
type Modifier = (String, f64, i8);

#[derive(Clone, Debug, Default)]
struct Instance {
    base: f64,
    modifiers: Vec<Modifier>,
}

impl Instance {
    /// `AttributeInstance.calculateValue`, before clamping.
    fn value(&self, extra: Option<&Modifier>) -> f64 {
        let all = || self.modifiers.iter().chain(extra);
        let sum = |op: i8| -> f64 { all().filter(|m| m.2 == op).map(|m| m.1).sum() };
        let base = self.base + sum(0);
        let mut value = base + base * sum(1);
        for m in all().filter(|m| m.2 == 2) {
            value *= 1.0 + m.1;
        }
        value
    }
}

/// What the server set. An attribute it never sent reads as its vanilla
#[derive(Clone, Debug, Default)]
pub struct PlayerAttributes {
    instances: Vec<(u8, Instance)>,
}

/// `Attribute.sanitizeValue`'s range for the attributes movement reads.
fn range(id: u8) -> (f64, f64) {
    match id {
        x if x == A::STEP_HEIGHT.id => (0.0, 10.0),
        x if x == A::JUMP_STRENGTH.id => (0.0, 32.0),
        x if x == A::GRAVITY.id => (-1.0, 1.0),
        x if x == A::SCALE.id => (0.0625, 16.0),
        x if x == A::SNEAKING_SPEED.id => (0.0, 1.0),
        x if x == A::WATER_MOVEMENT_EFFICIENCY.id || x == A::MOVEMENT_EFFICIENCY.id => (0.0, 1.0),
        x if x == A::AIR_DRAG_MODIFIER.id || x == A::FRICTION_MODIFIER.id => (0.0, 2048.0),
        x if x == A::FLYING_SPEED.id => (0.0, 1024.0),
        _ => (0.0, 1024.0),
    }
}

impl PlayerAttributes {
    /// `AttributeMap.assignBaseValues`: the same base values, no modifiers.
    pub fn base_only(&self) -> Self {
        Self {
            instances: self
                .instances
                .iter()
                .map(|(id, instance)| {
                    (
                        *id,
                        Instance {
                            base: instance.base,
                            modifiers: Vec::new(),
                        },
                    )
                })
                .collect(),
        }
    }

    /// A snapshot from the server replaces the attribute whole.
    pub fn set(&mut self, attribute: u8, base: f64, modifiers: Vec<Modifier>) {
        let instance = Instance { base, modifiers };
        match self.instances.iter_mut().find(|(id, _)| *id == attribute) {
            Some((_, old)) => *old = instance,
            None => self.instances.push((attribute, instance)),
        }
    }

    /// Whether the server has set this attribute.
    pub fn has(&self, attribute: u8) -> bool {
        self.instances.iter().any(|(id, _)| *id == attribute)
    }

    /// `AttributeInstance.getValue` clamped, or `default` when unset.
    pub fn value(&self, attribute: u8, default: f64) -> f64 {
        self.value_with(attribute, default, None)
    }

    fn value_with(&self, attribute: u8, default: f64, extra: Option<&Modifier>) -> f64 {
        let (min, max) = range(attribute);
        let raw = match self.instances.iter().find(|(id, _)| *id == attribute) {
            Some((_, instance)) => instance.value(extra),
            None => Instance {
                base: default,
                modifiers: Vec::new(),
            }
            .value(extra),
        };
        if raw.is_nan() {
            min
        } else {
            raw.clamp(min, max)
        }
    }

    /// `Attributes.MOVEMENT_SPEED` with the sprint modifier added by the
    pub fn movement_speed(&self, sprinting: bool) -> Option<f32> {
        let id = A::MOVEMENT_SPEED.id;
        let (_, instance) = self.instances.iter().find(|(i, _)| *i == id)?;
        let kept = Instance {
            base: instance.base,
            modifiers: instance
                .modifiers
                .iter()
                .filter(|m| m.0 != SPRINTING_MODIFIER)
                .cloned()
                .collect(),
        };
        let sprint = (SPRINTING_MODIFIER.to_owned(), 0.3_f32 as f64, 2);
        let value = kept.value(sprinting.then_some(&sprint));
        let (min, max) = range(id);
        Some(value.clamp(min, max) as f32)
    }

    /// `MINING_EFFICIENCY` once the server sent it (Efficiency and tool
    /// modifiers); before that the miner reads the enchantment itself.
    pub fn mining_efficiency(&self) -> Option<f32> {
        let id = A::MINING_EFFICIENCY.id;
        self.has(id).then(|| self.value(id, 0.0) as f32)
    }

    /// `BLOCK_BREAK_SPEED`: a factor on the whole destroy speed.
    pub fn block_break_speed(&self) -> f32 {
        self.value(A::BLOCK_BREAK_SPEED.id, A::BLOCK_BREAK_SPEED.default_value) as f32
    }

    pub fn step_height(&self) -> f64 {
        self.value(A::STEP_HEIGHT.id, 0.6)
    }
    pub fn jump_strength(&self) -> f32 {
        self.value(A::JUMP_STRENGTH.id, 0.42_f32 as f64) as f32
    }
    pub fn gravity(&self) -> f64 {
        self.value(A::GRAVITY.id, 0.08)
    }
    pub fn scale(&self) -> f64 {
        self.value(A::SCALE.id, 1.0)
    }
    /// `Attributes.SNEAKING_SPEED`.
    pub fn sneaking_speed(&self) -> f32 {
        self.value(A::SNEAKING_SPEED.id, 0.3) as f32
    }
    pub fn water_movement_efficiency(&self) -> f32 {
        self.value(A::WATER_MOVEMENT_EFFICIENCY.id, 0.0) as f32
    }
    pub fn movement_efficiency(&self) -> f32 {
        self.value(A::MOVEMENT_EFFICIENCY.id, 0.0) as f32
    }
    pub fn air_drag_modifier(&self) -> f32 {
        self.value(A::AIR_DRAG_MODIFIER.id, 1.0) as f32
    }
    pub fn friction_modifier(&self) -> f32 {
        self.value(A::FRICTION_MODIFIER.id, 1.0) as f32
    }
    /// `Attributes.SAFE_FALL_DISTANCE`.
    pub fn safe_fall_distance(&self) -> f64 {
        self.value(A::SAFE_FALL_DISTANCE.id, 3.0)
    }
    /// `Attributes.FALL_DAMAGE_MULTIPLIER`.
    pub fn fall_damage_multiplier(&self) -> f64 {
        self.value(A::FALL_DAMAGE_MULTIPLIER.id, 1.0)
    }
}

/// `LivingEntity.computeModifiedFriction`.
pub fn modified_friction(friction: f32, modifier: f32) -> f32 {
    (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_only_drops_the_modifiers_as_assign_base_values_does() {
        let mut a = PlayerAttributes::default();
        a.set(A::GRAVITY.id, 0.08, vec![("plugin:low".into(), -0.04, 0)]);
        assert!((a.gravity() - 0.04).abs() < 1e-12);
        let base = a.base_only();
        assert_eq!(base.gravity(), 0.08);
        assert!(base.has(A::GRAVITY.id));
    }

    #[test]
    fn unset_attributes_read_as_vanilla_defaults() {
        let a = PlayerAttributes::default();
        assert_eq!(a.step_height(), 0.6);
        assert_eq!(a.gravity(), 0.08);
        assert_eq!(a.scale(), 1.0);
        assert!(a.movement_speed(false).is_none());
    }

    #[test]
    fn the_servers_sprint_modifier_is_not_counted_twice() {
        let mut a = PlayerAttributes::default();
        let id = A::MOVEMENT_SPEED.id;
        a.set(id, 0.1, vec![(SPRINTING_MODIFIER.to_owned(), 0.3, 2)]);
        assert!((a.movement_speed(false).unwrap() - 0.1).abs() < 1e-6);
        assert!((a.movement_speed(true).unwrap() - 0.13).abs() < 1e-6);
    }

    #[test]
    fn values_are_clamped_and_modifiers_apply_in_order() {
        let mut a = PlayerAttributes::default();
        a.set(A::SCALE.id, 100.0, vec![]);
        assert_eq!(a.scale(), 16.0);
        a.set(
            A::STEP_HEIGHT.id,
            0.6,
            vec![("x:y".into(), 0.4, 0), ("x:z".into(), 1.0, 1)],
        );
        assert!((a.step_height() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn friction_modifier_pulls_toward_one() {
        assert_eq!(modified_friction(0.6, 1.0), 0.6);
        assert_eq!(modified_friction(0.6, 0.0), 1.0);
        assert_eq!(modified_friction(0.6, 3.0), 0.0);
    }
}
