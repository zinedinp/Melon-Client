//! `SquidModel` and the swimming that drives it (`Squid.aiStep`): the

use glam::DVec3;

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `EntityEvent` 19: the tentacles start a new stroke.
pub const EVENT_STROKE: u8 = 19;
const TENTACLES: usize = 8;
const TWO_PI: f32 = PI * 2.0;
/// `MeshTransformer.scaling(0.5)` of the baby.
const BABY_SCALE: f32 = 0.5;

fn tentacle_name(i: usize) -> &'static str {
    const NAMES: [&str; TENTACLES] = [
        "tentacle0",
        "tentacle1",
        "tentacle2",
        "tentacle3",
        "tentacle4",
        "tentacle5",
        "tentacle6",
        "tentacle7",
    ];
    NAMES[i]
}

pub fn layer(baby: bool) -> LayerDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 0.0), (-6.0, -8.0, -6.0), (12.0, 16.0, 12.0)).grown(0.02)],
        Pose::offset(0.0, 8.0, 0.0),
    );
    for i in 0..TENTACLES {
        let around = i as f64 * crate::math::angle::double::PI * 2.0 / TENTACLES as f64;
        let (x, z) = (around.cos() as f32 * 5.0, around.sin() as f32 * 5.0);
        let turn = (i as f64 * crate::math::angle::double::PI * -2.0 / TENTACLES as f64
            + crate::math::angle::double::HALF_PI) as f32;
        root.add_or_replace_child(
            tentacle_name(i),
            vec![Cube::new((48.0, 0.0), (-1.0, 0.0, -1.0), (2.0, 18.0, 2.0))],
            Pose::offset_rotation((x, 15.0, z), (0.0, turn, 0.0)),
        );
    }
    if baby {
        root.pose = super::ghast::scaling(Pose::ZERO, BABY_SCALE);
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

#[derive(Clone, Copy, Debug)]
pub struct SquidParts {
    tentacles: [PartId; TENTACLES],
}

impl SquidParts {
    pub fn find(model: &Model) -> Self {
        Self {
            tentacles: std::array::from_fn(|i| model.part(tentacle_name(i))),
        }
    }
}

/// `SquidModel.setupAnim`: every tentacle swings out by one angle.
pub fn setup_anim(parts: &SquidParts, pose: &mut ModelPose, tentacle_angle: f32) {
    for tentacle in parts.tentacles {
        pose[tentacle].pose.rotation.x = tentacle_angle;
    }
}

/// How the body lies, for the renderer: `SquidRenderer.setupRotations`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tilt {
    pub x_degrees: f32,
    pub z_degrees: f32,
    pub baby: bool,
}

impl Tilt {
    /// Up from the feet to where the body turns, and back down after.
    pub fn lift(&self) -> f32 {
        if self.baby { 0.25 } else { 0.5 }
    }

    pub fn drop(&self) -> f32 {
        if self.baby { -0.6 } else { -1.2 }
    }
}

/// The squid's swimming, a tick at a time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swim {
    tentacle_movement: f32,
    old_tentacle_movement: f32,
    tentacle_angle: f32,
    old_tentacle_angle: f32,
    tentacle_speed: f32,
    rotate_speed: f32,
    x_body: f32,
    old_x_body: f32,
    z_body: f32,
    old_z_body: f32,
    /// `yBodyRot`, which the body turns by.
    pub y_body: f32,
}

impl Swim {
    /// A squid of this `id`: `tentacleSpeed` starts as
    pub fn new(id: i32) -> Self {
        let hash = (id as u32).wrapping_mul(2_654_435_761) >> 8;
        let random = (hash & 0xFFFF) as f32 / 65_536.0;
        Self {
            tentacle_movement: 0.0,
            old_tentacle_movement: 0.0,
            tentacle_angle: 0.0,
            old_tentacle_angle: 0.0,
            tentacle_speed: 1.0 / (random + 1.0) * 0.2,
            rotate_speed: 0.0,
            x_body: 0.0,
            old_x_body: 0.0,
            z_body: 0.0,
            old_z_body: 0.0,
            y_body: 0.0,
        }
    }

    /// The tentacles start a new stroke (`handleEntityEvent(19)`).
    pub fn stroke(&mut self) {
        self.tentacle_movement = 0.0;
    }

    /// `Squid.aiStep` on the client: `moved` is how far it went this tick,
    pub fn tick(&mut self, in_water: bool, moved: DVec3) {
        self.old_x_body = self.x_body;
        self.old_z_body = self.z_body;
        self.old_tentacle_movement = self.tentacle_movement;
        self.old_tentacle_angle = self.tentacle_angle;
        self.tentacle_movement += self.tentacle_speed;
        if self.tentacle_movement > TWO_PI {
            self.tentacle_movement = TWO_PI;
        }
        if in_water {
            if self.tentacle_movement < PI {
                let stroke = self.tentacle_movement / PI;
                self.tentacle_angle = angle::sin(stroke * stroke * PI) * PI * 0.25;
                if stroke > 0.75 {
                    self.rotate_speed = 1.0;
                } else {
                    self.rotate_speed *= 0.8;
                }
            } else {
                self.tentacle_angle = 0.0;
                self.rotate_speed *= 0.99;
            }
            let horizontal = moved.x.hypot(moved.z);
            if horizontal * horizontal + moved.y * moved.y > 1.0e-12 {
                let toward_y = -(moved.x.atan2(moved.z) as f32).to_degrees();
                self.y_body += (toward_y - self.y_body) * 0.1;
                let toward_x = -(horizontal.atan2(moved.y) as f32).to_degrees();
                self.x_body += (toward_x - self.x_body) * 0.1;
            }
            self.z_body += PI * self.rotate_speed * 1.5;
        } else {
            self.tentacle_angle = angle::sin(self.tentacle_movement).abs() * PI * 0.25;
            self.x_body += (-90.0 - self.x_body) * 0.02;
        }
    }

    /// `SquidRenderState.tentacleAngle`.
    pub fn tentacle_angle(&self, partial: f32) -> f32 {
        self.old_tentacle_angle + (self.tentacle_angle - self.old_tentacle_angle) * partial
    }

    pub fn tilt(&self, partial: f32, baby: bool) -> Tilt {
        Tilt {
            x_degrees: self.old_x_body + (self.x_body - self.old_x_body) * partial,
            z_degrees: self.old_z_body + (self.z_body - self.old_z_body) * partial,
            baby,
        }
    }
}

/// `SquidRenderer` and `GlowSquidRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    match entity.kind {
        EntityKind::Squid => out.push(LayerSpec::aged(
            baby,
            (M::Squid, "minecraft:entity/squid/squid"),
            (M::SquidBaby, "minecraft:entity/squid/squid_baby"),
        )),
        EntityKind::GlowSquid => out.push(LayerSpec::aged(
            baby,
            (M::Squid, "minecraft:entity/squid/glow_squid"),
            (M::SquidBaby, "minecraft:entity/squid/glow_squid_baby"),
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_squid_out_of_water_hangs_and_its_tentacles_flap() {
        let mut swim = Swim::new(7);
        let mut angles = Vec::new();
        for _ in 0..40 {
            swim.tick(false, DVec3::ZERO);
            angles.push(swim.tentacle_angle(1.0));
        }
        assert!(
            (swim.x_body + 90.0).abs() < 60.0 && swim.x_body < -40.0,
            "{}",
            swim.x_body
        );
        assert!(angles.iter().any(|a| *a > 0.1));
        assert!(angles.iter().all(|a| (0.0..=PI * 0.25 + 1e-4).contains(a)));
    }

    #[test]
    fn a_stroke_in_water_opens_the_tentacles_and_spins_the_body() {
        let mut swim = Swim::new(3);
        let heading = DVec3::new(0.1, 0.0, 0.1);
        let mut peak: f32 = 0.0;
        for _ in 0..30 {
            swim.tick(true, heading);
            peak = peak.max(swim.tentacle_angle(1.0));
        }
        assert!(peak > 0.3, "{peak}");
        assert!(swim.y_body < 0.0, "{}", swim.y_body);
        assert!(swim.z_body > 0.0);
        assert!(swim.x_body < -10.0, "{}", swim.x_body);
    }

    #[test]
    fn the_server_ends_a_stroke_and_the_client_waits_for_it() {
        let mut swim = Swim::new(1);
        for _ in 0..200 {
            swim.tick(true, DVec3::new(0.0, 0.1, 0.0));
        }
        assert_eq!(swim.tentacle_angle(1.0), 0.0);
        swim.stroke();
        swim.tick(true, DVec3::new(0.0, 0.1, 0.0));
        assert!(swim.tentacle_movement < 1.0);
    }

    #[test]
    fn eight_tentacles_stand_around_the_body() {
        let model = Model::bake(&layer(false));
        let parts = SquidParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, 0.5);
        for tentacle in parts.tentacles {
            assert_eq!(pose[tentacle].pose.rotation.x, 0.5);
            let offset = pose[tentacle].pose.offset;
            assert!((offset.x.hypot(offset.z) - 5.0).abs() < 1e-4);
        }
    }
}
