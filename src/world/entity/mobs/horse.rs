//! `AbstractEquineModel`: horses, donkeys, mules and the undead horses,

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, HALF_PI, PI, QUARTER_PI, SIXTH_PI, THIRD_PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// `AbstractEquineModel.createBodyMesh(g)` at 64x64. The horse's armor
fn adult_root(g: f32) -> PartDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 32.0), (-5.0, -8.0, -17.0), (10.0, 10.0, 22.0)).grown(0.05)],
        Pose::offset(0.0, 11.0, 5.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((42.0, 36.0), (-1.5, 0.0, 0.0), (3.0, 14.0, 4.0)).grown(g)],
        Pose::offset_rotation((0.0, -5.0, 2.0), (SIXTH_PI, 0.0, 0.0)),
    );
    let head_parts = root.add_or_replace_child(
        "head_parts",
        vec![Cube::new(
            (0.0, 35.0),
            (-2.05, -6.0, -2.0),
            (4.0, 12.0, 7.0),
        )],
        Pose::offset_rotation((0.0, 4.0, -12.0), (SIXTH_PI, 0.0, 0.0)),
    );
    let head = head_parts.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 13.0), (-3.0, -11.0, -2.0), (6.0, 5.0, 7.0)).grown(g)],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((19.0, 16.0), (0.55, -13.0, 4.0), (2.0, 3.0, 1.0)).grown(-0.001)],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((19.0, 16.0), (-2.55, -13.0, 4.0), (2.0, 3.0, 1.0)).grown(-0.001)],
        Pose::ZERO,
    );
    head_parts.add_or_replace_child(
        "mane",
        vec![Cube::new((56.0, 36.0), (-1.0, -11.0, 5.01), (2.0, 16.0, 2.0)).grown(g)],
        Pose::ZERO,
    );
    head_parts.add_or_replace_child(
        "upper_mouth",
        vec![Cube::new((0.0, 25.0), (-2.0, -11.0, -7.0), (4.0, 5.0, 5.0)).grown(g)],
        Pose::ZERO,
    );
    for (name, x, z, front, mirror) in [
        ("left_hind_leg", 4.0, 7.0, false, true),
        ("right_hind_leg", -4.0, 7.0, false, false),
        ("left_front_leg", 4.0, -10.0, true, true),
        ("right_front_leg", -4.0, -10.0, true, false),
    ] {
        let min = (
            if mirror { -3.0 } else { -1.0 },
            -1.01,
            if front { -1.9 } else { -1.0 },
        );
        let cube = Cube::new((48.0, 21.0), min, (4.0, 11.0, 4.0)).grown(g);
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 14.0, z),
        );
    }
    root
}

/// A horse, scaled by 1.1 when placed (`livingHorseScale`).
pub fn horse() -> LayerDefinition {
    LayerDefinition::new(adult_root(0.0), 64.0, 64.0)
}

/// `HORSE_ARMOR`: the equine mesh grown by a tenth of a pixel.
pub fn armor() -> LayerDefinition {
    LayerDefinition::new(adult_root(0.1), 64.0, 64.0)
}

/// `EquineSaddleModel.createSaddleLayer`.
pub fn saddle() -> LayerDefinition {
    let mut root = adult_root(0.0);
    root.child_mut("body").add_or_replace_child(
        "saddle",
        vec![Cube::new((26.0, 0.0), (-5.0, -8.0, -9.0), (10.0, 9.0, 9.0)).grown(0.5)],
        Pose::ZERO,
    );
    let head_parts = root.child_mut("head_parts");
    head_parts.add_or_replace_child(
        "left_saddle_mouth",
        vec![Cube::new((29.0, 5.0), (2.0, -9.0, -6.0), (1.0, 2.0, 2.0))],
        Pose::ZERO,
    );
    head_parts.add_or_replace_child(
        "right_saddle_mouth",
        vec![Cube::new((29.0, 5.0), (-3.0, -9.0, -6.0), (1.0, 2.0, 2.0))],
        Pose::ZERO,
    );
    head_parts.add_or_replace_child(
        "left_saddle_line",
        vec![Cube::new((32.0, 2.0), (3.1, -6.0, -8.0), (0.0, 3.0, 16.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-SIXTH_PI, 0.0, 0.0)),
    );
    head_parts.add_or_replace_child(
        "right_saddle_line",
        vec![Cube::new((32.0, 2.0), (-3.1, -6.0, -8.0), (0.0, 3.0, 16.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-SIXTH_PI, 0.0, 0.0)),
    );
    head_parts.add_or_replace_child(
        "head_saddle",
        vec![Cube::new((1.0, 1.0), (-3.0, -11.0, -1.9), (6.0, 5.0, 6.0)).grown(0.22)],
        Pose::ZERO,
    );
    head_parts.add_or_replace_child(
        "mouth_saddle_wrap",
        vec![Cube::new((19.0, 0.0), (-2.0, -11.0, -4.0), (4.0, 5.0, 2.0)).grown(0.2)],
        Pose::ZERO,
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `DonkeyModel.createBodyLayer`: the horse's mesh with chests and long
pub fn donkey() -> LayerDefinition {
    let mut root = adult_root(0.0);
    let chest = || vec![Cube::new((26.0, 21.0), (-4.0, 0.0, -2.0), (8.0, 8.0, 3.0))];
    let body = root.child_mut("body");
    body.add_or_replace_child(
        "left_chest",
        chest(),
        Pose::offset_rotation((6.0, -8.0, 0.0), (0.0, -HALF_PI, 0.0)),
    );
    body.add_or_replace_child(
        "right_chest",
        chest(),
        Pose::offset_rotation((-6.0, -8.0, 0.0), (0.0, HALF_PI, 0.0)),
    );
    let head = root.child_mut("head_parts").child_mut("head");
    let ear = || vec![Cube::new((0.0, 12.0), (-1.0, -7.0, 0.0), (2.0, 7.0, 1.0))];
    head.add_or_replace_child(
        "left_ear",
        ear(),
        Pose::offset_rotation((1.25, -10.0, 4.0), (PI / 12.0, 0.0, PI / 12.0)),
    );
    head.add_or_replace_child(
        "right_ear",
        ear(),
        Pose::offset_rotation((-1.25, -10.0, 4.0), (PI / 12.0, 0.0, -PI / 12.0)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyHorseModel.createBabyMesh`.
#[allow(clippy::approx_constant)]
pub fn baby_horse() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 13.0), (-4.0, -3.5, -7.0), (8.0, 7.0, 14.0))],
        Pose::offset(0.0, 12.5, 0.0),
    );
    body.add_or_replace_child(
        "tail",
        vec![Cube::new((24.0, 34.0), (-1.5, -1.5, -1.0), (3.0, 3.0, 8.0))],
        Pose::offset_rotation((0.0, -1.0, 7.0), (-0.7418, 0.0, 0.0)),
    );
    for (name, uv, x, z) in [
        ("left_hind_leg", (12.0, 46.0), 2.4, 5.4),
        ("right_hind_leg", (0.0, 46.0), -2.4, 5.4),
        ("left_front_leg", (12.0, 34.0), 2.4, -5.4),
        ("right_front_leg", (0.0, 34.0), -2.4, -5.4),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.5, -1.0, -1.5), (3.0, 9.0, 3.0))],
            Pose::offset(x, 16.0, z),
        );
    }
    let neck = root.add_or_replace_child(
        "head_parts",
        vec![Cube::new((30.0, 0.0), (-2.0, -6.0, -2.0), (4.0, 8.0, 4.0))],
        Pose::offset_rotation((0.0, 10.0, -6.0), (0.6109, 0.0, 0.0)),
    );
    let head = neck.add_or_replace_child(
        "head",
        vec![Cube::new(
            (0.0, 0.0),
            (-3.0, -3.9484, -6.705),
            (6.0, 4.0, 9.0),
        )],
        Pose::offset(0.0, -6.0516, -0.2951),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((0.0, 4.0), (-1.0, -2.5, -0.8), (2.0, 3.0, 1.0))],
        Pose::offset_rotation((2.0, -4.2484, 1.9451), (0.0, 0.0, 0.2618)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((0.0, 0.0), (-1.0, -2.5, -0.5), (2.0, 3.0, 1.0))],
        Pose::offset_rotation((-2.0, -4.2484, 1.645), (0.0, 0.0, -0.2618)),
    );
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyDonkeyModel.createBabyLayer`: everything hangs off the body.
#[allow(clippy::approx_constant)]
pub fn baby_donkey() -> LayerDefinition {
    let mut root = PartDefinition::root();
    let body = root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 13.0), (-5.0, -3.0, -7.0), (8.0, 6.0, 14.0))],
        Pose::offset(1.0, 14.0, 0.0),
    );
    let tail = body.add_or_replace_child("tail", Vec::new(), Pose::offset(0.0, -1.5, 6.5));
    tail.add_or_replace_child(
        "tail_r1",
        vec![Cube::new((24.0, 33.0), (-2.5, -1.0, -0.5), (3.0, 3.0, 8.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-0.7418, 0.0, 0.0)),
    );
    for (name, uv, x, y, z) in [
        ("left_hind_leg", (12.0, 44.0), 2.25, 3.5, 5.25),
        ("right_hind_leg", (0.0, 44.0), -2.4, 3.5, 5.4),
        ("left_front_leg", (12.0, 33.0), 2.4, 3.5, -5.3),
        ("right_front_leg", (0.0, 33.0), -2.4, 3.5, -5.4),
    ] {
        body.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-2.5, -1.5, -1.5), (3.0, 8.0, 3.0))],
            Pose::offset(x, y, z),
        );
    }
    let neck = body.add_or_replace_child("head_parts", Vec::new(), Pose::offset(0.0, -3.0, -5.0));
    neck.add_or_replace_child(
        "neck_r1",
        vec![Cube::new((30.0, 9.0), (-3.0, -6.0, -3.0), (4.0, 8.0, 4.0))],
        Pose::offset_rotation((0.0, 0.0, 0.0), (0.3927, 0.0, 0.0)),
    );
    let head = neck.add_or_replace_child("head", Vec::new(), Pose::offset(0.0, -5.0, -3.0));
    head.add_or_replace_child(
        "head_r1",
        vec![Cube::new((0.0, 0.0), (-4.0, -3.6, -8.4), (6.0, 4.0, 9.0))],
        Pose::offset_rotation((0.0, -1.0, 1.0), (0.3927, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "left_ear",
        vec![Cube::new((0.0, 0.0), (-2.0, -6.5, -0.3), (2.0, 7.0, 1.0))],
        Pose::offset_rotation((2.0, -3.5, -1.0), (0.48, 0.0, 0.48)),
    );
    head.add_or_replace_child(
        "right_ear",
        vec![Cube::new((22.0, 0.0), (-2.0, -6.5, -0.3), (2.0, 7.0, 1.0)).mirrored()],
        Pose::offset_rotation((-2.0, -3.5, -1.0), (0.48, 0.0, -0.48)),
    );
    body.add_or_replace_child("right_chest", Vec::new(), Pose::offset(-1.0, 10.0, 0.0));
    body.add_or_replace_child("left_chest", Vec::new(), Pose::offset(-1.0, 10.0, 0.0));
    LayerDefinition::new(root, 64.0, 64.0)
}

/// Which equine a model is: they stand and graze by different amounts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    Adult,
    Foal,
    DonkeyFoal,
}

#[derive(Clone, Copy, Debug)]
pub struct EquineParts {
    body: PartId,
    head_parts: PartId,
    left_hind: PartId,
    right_hind: PartId,
    left_front: PartId,
    right_front: PartId,
    tail: PartId,
    chests: Option<[PartId; 2]>,
    /// The reins, only on the saddle mesh; hidden unless someone is on it.
    saddle_lines: Option<[PartId; 2]>,
    build: Build,
}

impl EquineParts {
    pub fn find(model: &Model, build: Build) -> Self {
        Self {
            body: model.part("body"),
            head_parts: model.part("head_parts"),
            left_hind: model.part("left_hind_leg"),
            right_hind: model.part("right_hind_leg"),
            left_front: model.part("left_front_leg"),
            right_front: model.part("right_front_leg"),
            tail: model.part("tail"),
            chests: model
                .find("left_chest")
                .zip(model.find("right_chest"))
                .map(|(l, r)| [l, r]),
            saddle_lines: model
                .find("left_saddle_line")
                .zip(model.find("right_saddle_line"))
                .map(|(l, r)| [l, r]),
            build,
        }
    }
}

/// `EquineRenderState`'s animation amounts and the donkey's chest.
#[derive(Clone, Copy, Debug, Default)]
pub struct EquinePose {
    pub eating: f32,
    pub standing: f32,
    pub feeding: f32,
    pub animate_tail: bool,
    pub has_chest: bool,
    pub in_water: bool,
}

/// `AbstractEquineModel.setupAnim`, with `BabyHorseModel`'s and
pub fn setup_anim(
    parts: &EquineParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    equine: EquinePose,
) {
    let build = parts.build;
    let age_scale = if build == Build::Adult { 1.0 } else { 0.5 };
    let age = living.age_in_ticks;
    let (position, speed) = (living.walk_position, living.walk_speed);
    let clamped_y = living.y_rot.clamp(-20.0, 20.0);
    let mut head_x = living.x_rot.to_radians();
    if speed > 0.2 {
        head_x += angle::cos(position * 0.8) * 0.15 * speed;
    }
    let EquinePose {
        eating,
        standing,
        feeding,
        ..
    } = equine;
    let not_standing = 1.0 - standing;
    let lerp = |t: f32, a: f32, b: f32| a + t * (b - a);

    let water = if equine.in_water { 0.2 } else { 1.0 };
    let leg_anim = angle::cos(water * position * 0.6662 + PI);
    let leg_swing = leg_anim * 0.8 * speed;
    let calm = 1.0 - standing.max(eating);
    let base_head = calm * (SIXTH_PI + head_x + feeding * angle::sin(age) * 0.05);
    let head = &mut pose[parts.head_parts].pose;
    let rest_y = clamped_y.to_radians();
    head.rotation.x = standing * (PI / 12.0 + head_x)
        + eating * (2.181_661_6 + angle::sin(age) * 0.05)
        + base_head;
    head.rotation.y = standing * rest_y + calm * rest_y;
    match build {
        Build::Adult => {
            head.offset.y += lerp(eating, lerp(standing, 0.0, -8.0), 7.0);
            head.offset.z = lerp(standing, head.offset.z, -4.0);
        }
        Build::Foal => {
            head.offset.y += lerp(eating, lerp(standing, 0.0, -2.0), 2.0);
            head.offset.z = lerp(standing, head.offset.z, -4.0);
        }
        Build::DonkeyFoal => {
            head.offset.y = lerp(eating, head.offset.y, -1.2);
            head.offset.z = lerp(standing, head.offset.z, -3.6);
            let low = (-30.0f32).to_radians();
            head.rotation.x = standing * (PI / 12.0 + low)
                + eating * (HALF_PI + angle::sin(age) * 0.05)
                + calm * (SIXTH_PI + low + feeding * angle::sin(age) * 0.05);
        }
    }

    let body = &mut pose[parts.body].pose;
    body.rotation.x = standing * (-QUARTER_PI) + not_standing * body.rotation.x;

    let (y_off, z_off, stand_angle, x_off, tail_off) = match build {
        Build::Adult => (12.0, 4.0, PI / 12.0, -THIRD_PI, 0.0),
        Build::Foal => (4.0, 0.0, PI / 12.0, -THIRD_PI, -HALF_PI),
        Build::DonkeyFoal => (1.0, 0.5, THIRD_PI, 0.0, -QUARTER_PI),
    };
    let left_front = &mut pose[parts.left_front].pose;
    left_front.offset.y -= y_off * standing;
    left_front.offset.z += z_off * standing;
    let (y, z) = (left_front.offset.y, left_front.offset.z);
    let right_front = &mut pose[parts.right_front].pose;
    right_front.offset.y = y;
    right_front.offset.z = z;

    let stand_angle = stand_angle * standing;
    let bob = angle::cos(age * 0.6 + PI);
    let right_rot = (x_off + bob) * standing + leg_swing * not_standing;
    let left_rot = (x_off - bob) * standing - leg_swing * not_standing;
    pose[parts.left_hind].pose.rotation.x = stand_angle - leg_anim * 0.5 * speed * not_standing;
    pose[parts.right_hind].pose.rotation.x = stand_angle + leg_anim * 0.5 * speed * not_standing;
    pose[parts.left_front].pose.rotation.x = right_rot;
    pose[parts.right_front].pose.rotation.x = left_rot;
    if build == Build::DonkeyFoal {
        let y = lerp(standing, pose[parts.left_hind].pose.offset.y, -0.3);
        pose[parts.left_hind].pose.offset.y = y;
        pose[parts.right_hind].pose.offset.y = lerp(standing, y, -0.3);
    }

    let tail = &mut pose[parts.tail].pose;
    tail.rotation.x = tail_off + SIXTH_PI + speed * 0.75;
    tail.offset.y += speed * age_scale;
    tail.offset.z += speed * 2.0 * age_scale;
    tail.rotation.y = if equine.animate_tail {
        angle::cos(age * 0.7)
    } else {
        0.0
    };

    if let Some(chests) = parts.chests {
        for chest in chests {
            pose[chest].visible = equine.has_chest;
        }
    }
    if let Some(lines) = parts.saddle_lines {
        for line in lines {
            pose[line].visible = living.riding;
        }
    }
}

/// `AbstractHorse.tick`'s three animation amounts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Amounts {
    pub eat: f32,
    pub stand: f32,
    pub mouth: f32,
}

impl Amounts {
    /// One tick of the eating, rearing and mouth flags. Rearing stops
    pub fn step(&mut self, eating: bool, standing: bool, mouth_open: bool) {
        if eating {
            self.eat = (self.eat + ((1.0 - self.eat) * 0.4 + 0.05)).min(1.0);
        } else {
            self.eat = (self.eat + ((0.0 - self.eat) * 0.4 - 0.05)).max(0.0);
        }
        if standing {
            self.eat = 0.0;
            self.stand = (self.stand + ((1.0 - self.stand) * 0.4 + 0.05)).min(1.0);
        } else {
            let cube = 0.8 * self.stand * self.stand * self.stand;
            self.stand = (self.stand + ((cube - self.stand) * 0.6 - 0.05)).max(0.0);
        }
        if mouth_open {
            self.mouth = (self.mouth + ((1.0 - self.mouth) * 0.7 + 0.05)).min(1.0);
        } else {
            self.mouth = (self.mouth + ((0.0 - self.mouth) * 0.7 - 0.05)).max(0.0);
        }
    }

    pub fn lerp(before: Self, now: Self, partial: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * partial;
        Self {
            eat: mix(before.eat, now.eat),
            stand: mix(before.stand, now.stand),
            mouth: mix(before.mouth, now.mouth),
        }
    }
}

/// `Variant` by id, adult and foal.
const COATS: [[&str; 2]; 7] = {
    macro_rules! coats {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/horse/horse_", $v),
            concat!("minecraft:entity/horse/horse_", $v, "_baby"),
        ]),*] };
    }
    coats!(
        "white",
        "creamy",
        "chestnut",
        "brown",
        "black",
        "gray",
        "darkbrown"
    )
};

/// `Markings` after `NONE`, adult and foal.
const MARKINGS: [[&str; 2]; 4] = {
    macro_rules! markings {
        ($($v:literal),*) => { [$([
            concat!("minecraft:entity/horse/horse_markings_", $v),
            concat!("minecraft:entity/horse/horse_markings_", $v, "_baby"),
        ]),*] };
    }
    markings!("white", "whitefield", "whitedots", "blackdots")
};

/// `HorseRenderer`, `DonkeyRenderer`, `UndeadHorseRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Horse => {
            let colour = usize::try_from(a.horse_variant & 0xFF).unwrap_or(0) % COATS.len();
            let markings = usize::try_from((a.horse_variant >> 8) & 0xFF).unwrap_or(0) % 5;
            out.push(plain(
                if baby { M::HorseBaby } else { M::Horse },
                COATS[colour][usize::from(baby)],
            ));
            if markings > 0 {
                out.push(
                    plain(
                        if baby { M::HorseBaby } else { M::Horse },
                        MARKINGS[markings - 1][usize::from(baby)],
                    )
                    .translucent(),
                );
            }
        }
        EntityKind::Donkey => out.push(LayerSpec::aged(
            baby,
            (M::Donkey, "minecraft:entity/horse/donkey"),
            (M::DonkeyBaby, "minecraft:entity/horse/donkey_baby"),
        )),
        EntityKind::Mule => out.push(LayerSpec::aged(
            baby,
            (M::Donkey, "minecraft:entity/horse/mule"),
            (M::DonkeyBaby, "minecraft:entity/horse/mule_baby"),
        )),
        EntityKind::SkeletonHorse => out.push(LayerSpec::aged(
            baby,
            (M::Horse, "minecraft:entity/horse/horse_skeleton"),
            (M::HorseBaby, "minecraft:entity/horse/horse_skeleton_baby"),
        )),
        EntityKind::ZombieHorse => out.push(LayerSpec::aged(
            baby,
            (M::Horse, "minecraft:entity/horse/horse_zombie"),
            (M::HorseBaby, "minecraft:entity/horse/horse_zombie_baby"),
        )),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builds() -> [(LayerDefinition, Build); 4] {
        [
            (horse(), Build::Adult),
            (donkey(), Build::Adult),
            (baby_horse(), Build::Foal),
            (baby_donkey(), Build::DonkeyFoal),
        ]
    }

    #[test]
    fn rearing_lifts_the_front_and_grazing_lowers_the_head() {
        for (layer, build) in builds() {
            let model = Model::bake(&layer);
            let parts = EquineParts::find(&model, build);
            let living = LivingRenderState::default();
            let mut calm = model.initial_pose();
            setup_anim(&parts, &mut calm, &living, EquinePose::default());
            let mut rearing = model.initial_pose();
            setup_anim(
                &parts,
                &mut rearing,
                &living,
                EquinePose {
                    standing: 1.0,
                    ..Default::default()
                },
            );
            assert!(rearing[parts.body].pose.rotation.x < calm[parts.body].pose.rotation.x);
            let mut grazing = model.initial_pose();
            setup_anim(
                &parts,
                &mut grazing,
                &living,
                EquinePose {
                    eating: 1.0,
                    ..Default::default()
                },
            );
            assert!(
                grazing[parts.head_parts].pose.rotation.x > calm[parts.head_parts].pose.rotation.x
            );
        }
    }

    #[test]
    fn only_a_donkey_with_a_chest_shows_its_bags() {
        let model = Model::bake(&donkey());
        let parts = EquineParts::find(&model, Build::Adult);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            EquinePose::default(),
        );
        let [left, right] = parts.chests.expect("a donkey has chests");
        assert!(!pose[left].visible && !pose[right].visible);
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            EquinePose {
                has_chest: true,
                ..Default::default()
            },
        );
        assert!(pose[left].visible && pose[right].visible);
    }

    #[test]
    fn the_amounts_rise_with_a_flag_and_settle_without() {
        let mut a = Amounts::default();
        for _ in 0..10 {
            a.step(false, true, false);
        }
        assert_eq!(a.stand, 1.0);
        for _ in 0..30 {
            a.step(false, false, false);
        }
        assert_eq!(a, Amounts::default());
    }
}
