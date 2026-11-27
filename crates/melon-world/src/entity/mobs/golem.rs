//! `IronGolemModel`: a broad body on long arms and short legs.

use super::{ClientEntity, LayerSpec, ModelId as M};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::entity::quadruped::look;
use crate::world::model_part::{Cube, Pose};

/// `EntityEvent.IRON_GOLEM_ATTACK`: the arms swing for ten ticks.
pub const EVENT_ATTACK: u8 = 4;
/// `EntityEvent.OFFER_FLOWER` and `STOP_OFFER_FLOWER`.
pub const EVENT_OFFER_FLOWER: u8 = 11;
pub const EVENT_STOP_OFFER_FLOWER: u8 = 34;
pub const ATTACK_TICKS: i32 = 10;
/// `IronGolem.OFFER_FLOWER_TICKS`.
pub const OFFER_FLOWER_TICKS: i32 = 400;

/// `Crackiness.GOLEM.byFraction(health / maxHealth)`: the cracks of a
/// hurt golem, by its synced `MAX_HEALTH` (100 unless changed).
pub fn crackiness(health: f32, max_health: f32) -> Option<&'static str> {
    let fraction = health / max_health;
    if fraction < 0.25 {
        Some("minecraft:entity/iron_golem/iron_golem_crackiness_high")
    } else if fraction < 0.5 {
        Some("minecraft:entity/iron_golem/iron_golem_crackiness_medium")
    } else if fraction < 0.75 {
        Some("minecraft:entity/iron_golem/iron_golem_crackiness_low")
    } else {
        None
    }
}

/// The block it offers (`IronGolemFlowerLayer`).
pub const FLOWER_MODEL: &str = "minecraft:block/poppy";

pub fn layer() -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -12.0, -5.5), (8.0, 10.0, 8.0)),
            Cube::new((24.0, 0.0), (-1.0, -5.0, -7.5), (2.0, 4.0, 2.0)),
        ],
        Pose::offset(0.0, -7.0, -2.0),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 40.0), (-9.0, -2.0, -6.0), (18.0, 12.0, 11.0)),
            Cube::new((0.0, 70.0), (-4.5, 10.0, -3.0), (9.0, 5.0, 6.0)).grown(0.5),
        ],
        Pose::offset(0.0, -7.0, 0.0),
    );
    root.add_or_replace_child(
        "right_arm",
        vec![Cube::new(
            (60.0, 21.0),
            (-13.0, -2.5, -3.0),
            (4.0, 30.0, 6.0),
        )],
        Pose::offset(0.0, -7.0, 0.0),
    );
    root.add_or_replace_child(
        "left_arm",
        vec![Cube::new((60.0, 58.0), (9.0, -2.5, -3.0), (4.0, 30.0, 6.0))],
        Pose::offset(0.0, -7.0, 0.0),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((37.0, 0.0), (-3.5, -3.0, -3.0), (6.0, 16.0, 5.0))],
        Pose::offset(-4.0, 11.0, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((60.0, 0.0), (-3.5, -3.0, -3.0), (6.0, 16.0, 5.0)).mirrored()],
        Pose::offset(5.0, 11.0, 0.0),
    );
    LayerDefinition::new(root, 128.0, 128.0)
}

/// What the golem's pose reads besides the walk.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GolemPose {
    pub attack: f32,
    pub offer_flower: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct GolemParts {
    head: PartId,
    right_arm: PartId,
    left_arm: PartId,
    right_leg: PartId,
    left_leg: PartId,
}

impl GolemParts {
    /// The arm that holds the flower (`getFlowerHoldingArm`).
    pub fn flower_arm(&self) -> PartId {
        self.right_arm
    }

    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            right_arm: model.part("right_arm"),
            left_arm: model.part("left_arm"),
            right_leg: model.part("right_leg"),
            left_leg: model.part("left_leg"),
        }
    }
}

/// `Mth.triangleWave`: -1 to 1 and back every `period`.
pub fn triangle_wave(index: f32, period: f32) -> f32 {
    ((index % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

/// `IronGolemRenderer.setupRotations`.
pub fn roll(walk_position: f32, walk_speed: f32) -> f32 {
    if walk_speed < 0.01 {
        0.0
    } else {
        6.5 * triangle_wave(walk_position + 6.0, 13.0)
    }
}

/// `IronGolemModel.setupAnim`.
pub fn setup_anim(
    parts: &GolemParts,
    pose: &mut ModelPose,
    state: &LivingRenderState,
    golem: GolemPose,
) {
    let (position, speed) = (state.walk_position, state.walk_speed);
    let wave = triangle_wave(position, 13.0);
    let (right, left) = if golem.attack > 0.0 {
        let up = -2.0 + 1.5 * triangle_wave(golem.attack, 10.0);
        (up, up)
    } else if golem.offer_flower > 0 {
        (
            -0.8 + 0.025 * triangle_wave(golem.offer_flower as f32, 70.0),
            0.0,
        )
    } else {
        ((-0.2 + 1.5 * wave) * speed, (-0.2 - 1.5 * wave) * speed)
    };
    pose[parts.right_arm].pose.rotation.x = right;
    pose[parts.left_arm].pose.rotation.x = left;
    look(&mut pose[parts.head].pose, state);
    pose[parts.right_leg].pose.rotation.x = -1.5 * wave * speed;
    pose[parts.left_leg].pose.rotation.x = 1.5 * wave * speed;
}

/// `IronGolemRenderer` and `IronGolemCrackinessLayer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let plain = LayerSpec::plain;
    out.push(plain(
        M::IronGolem,
        "minecraft:entity/iron_golem/iron_golem",
    ));
    if let Some(cracks) = crackiness(a.health, entity.max_health) {
        out.push(plain(M::IronGolem, cracks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_triangle_wave_runs_between_minus_one_and_one() {
        assert_eq!(triangle_wave(0.0, 13.0), 1.0);
        assert!((triangle_wave(6.5, 13.0) + 1.0).abs() < 1e-6);
        assert!(triangle_wave(3.25, 13.0).abs() < 1e-6);
    }

    #[test]
    fn cracks_show_as_the_golem_loses_health() {
        assert_eq!(crackiness(100.0, 100.0), None);
        assert_eq!(crackiness(75.0, 100.0), None);
        assert!(crackiness(74.0, 100.0).unwrap().ends_with("low"));
        assert!(crackiness(49.0, 100.0).unwrap().ends_with("medium"));
        assert!(crackiness(24.0, 100.0).unwrap().ends_with("high"));
        assert!(crackiness(0.0, 100.0).unwrap().ends_with("high"));
    }

    #[test]
    fn a_standing_golem_does_not_rock() {
        assert_eq!(roll(3.0, 0.0), 0.0);
        assert!(roll(0.0, 1.0).abs() > 0.1);
    }
}
