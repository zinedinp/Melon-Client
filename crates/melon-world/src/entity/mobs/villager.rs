//! `VillagerModel` and what is built on it (`WitchModel`, the wandering trader).

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::entity::humanoid;
use crate::entity::living::LivingRenderState;
use crate::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::math::angle::{self, HALF_PI, PI, Rotation, THIRD_PI};
use crate::model_part::{Cube, Pose};
use glam::{Mat4, Vec3};

/// `VillagerMetadataSection.Hat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hat {
    #[default]
    None,
    Partial,
    Full,
}

impl Hat {
    /// The section of a texture's `.mcmeta` (the whole file).
    pub fn from_meta(meta: &serde_json::Value) -> Self {
        match meta["villager"]["hat"].as_str() {
            Some("partial") => Self::Partial,
            Some("full") => Self::Full,
            _ => Self::None,
        }
    }

    /// `VillagerProfessionLayer`.
    pub fn type_hat_visible(self, type_hat: Self) -> bool {
        self == Self::None || self == Self::Partial && type_hat != Self::Full
    }
}

/// `VillagerModel.createBodyModel` as a part tree.
fn adult_root() -> PartDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0))],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((32.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0)).grown(0.51)],
        Pose::ZERO,
    )
    .add_or_replace_child(
        "hat_rim",
        vec![Cube::new(
            (30.0, 47.0),
            (-8.0, -8.0, -6.0),
            (16.0, 16.0, 1.0),
        )],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-HALF_PI, 0.0, 0.0)),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((24.0, 0.0), (-1.0, -1.0, -6.0), (2.0, 4.0, 2.0))],
        Pose::offset(0.0, -2.0, 0.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((16.0, 20.0), (-4.0, 0.0, -3.0), (8.0, 12.0, 6.0))],
        Pose::ZERO,
    )
    .add_or_replace_child(
        "jacket",
        vec![Cube::new((0.0, 38.0), (-4.0, 0.0, -3.0), (8.0, 20.0, 6.0)).grown(0.5)],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "arms",
        vec![
            Cube::new((44.0, 22.0), (-8.0, -2.0, -2.0), (4.0, 8.0, 4.0)),
            Cube::new((44.0, 22.0), (4.0, -2.0, -2.0), (4.0, 8.0, 4.0)).mirrored(),
            Cube::new((40.0, 38.0), (-4.0, 2.0, -2.0), (8.0, 4.0, 4.0)),
        ],
        Pose::offset_rotation((0.0, 3.0, -1.0), (-0.75, 0.0, 0.0)),
    );
    for (name, x, mirror) in [("right_leg", -2.0, false), ("left_leg", 2.0, true)] {
        let cube = Cube::new((0.0, 22.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0));
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 12.0, 0.0),
        );
    }
    root
}

pub fn adult() -> LayerDefinition {
    LayerDefinition::new(adult_root(), 64.0, 64.0)
}

/// `VillagerModel.createNoHatModel`: the profession layers' body.
pub fn adult_no_hat() -> LayerDefinition {
    let mut root = adult_root();
    root.child_mut("head").clear_cubes();
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyVillagerModel.createBodyModel`.
fn baby_root() -> PartDefinition {
    let mut root = PartDefinition::root();
    let arms = root.add_or_replace_child("arms", Vec::new(), Pose::offset(0.0, 17.5, 0.0));
    arms.add_or_replace_child(
        "right_hand",
        vec![
            Cube::new((36.0, 15.0), (-1.0, -2.4925, -1.8401), (2.0, 4.0, 2.0)),
            Cube::new((16.0, 15.0), (5.0, -2.4925, -1.8401), (2.0, 4.0, 2.0)),
        ],
        Pose::offset_rotation((-3.0, 1.4025, -0.9599), (-THIRD_PI, 0.0, 0.0)),
    );
    arms.add_or_replace_child(
        "middlearm_r1",
        vec![Cube::new(
            (24.0, 17.0),
            (-2.0, -0.9924, -0.9825),
            (4.0, 2.0, 2.0),
        )],
        Pose::offset_rotation((0.0, 0.9024, -1.8175), (-THIRD_PI, 0.0, 0.0)),
    );
    for (name, uv, x) in [
        ("right_leg", (8.0, 23.0), -1.0),
        ("left_leg", (0.0, 23.0), 1.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, -0.5, -1.0), (2.0, 3.0, 2.0))],
            Pose::offset(x, 21.5, 0.0),
        );
    }
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -3.5), (8.0, 8.0, 7.0))],
        Pose::offset(0.0, 16.0, 0.0),
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((0.0, 30.0), (-4.0, -4.0, -3.5), (8.0, 8.0, 7.0)).grown(0.3)],
        Pose::offset(0.0, -4.0, 0.0),
    );
    head.add_or_replace_child(
        "hat_rim",
        vec![Cube::new(
            (0.0, 45.0),
            (-7.0, -0.5, -6.0),
            (14.0, 1.0, 12.0),
        )],
        Pose::offset(0.0, -4.5, 0.0),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((23.0, 0.0), (-1.0, 0.0, -0.5), (2.0, 2.0, 1.0))],
        Pose::offset(0.0, -2.0, -4.0),
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((0.0, 15.0), (-2.0, -2.75, -1.5), (4.0, 5.0, 3.0))],
        Pose::offset(0.0, 18.75, 0.0),
    );
    root.add_or_replace_child(
        "bb_main",
        vec![Cube::new((16.0, 21.0), (-2.5, -8.0, -1.5), (4.0, 6.0, 3.0)).grown(0.2)],
        Pose::offset(0.5, 24.0, 0.0),
    );
    root
}

pub fn baby() -> LayerDefinition {
    LayerDefinition::new(baby_root(), 64.0, 64.0)
}

pub fn baby_no_hat() -> LayerDefinition {
    let mut root = baby_root();
    root.child_mut("head").clear_cubes();
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `WitchModel.createBodyLayer`.
pub fn witch() -> LayerDefinition {
    let mut root = adult_root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0))],
        Pose::ZERO,
    );
    let hat = head.add_or_replace_child(
        "hat",
        vec![Cube::new((0.0, 64.0), (0.0, 0.0, 0.0), (10.0, 2.0, 10.0))],
        Pose::offset(-5.0, -10.03125, -5.0),
    );
    let hat2 = hat.add_or_replace_child(
        "hat2",
        vec![Cube::new((0.0, 76.0), (0.0, 0.0, 0.0), (7.0, 4.0, 7.0))],
        Pose::offset_rotation((1.75, -4.0, 2.0), (-0.052_359_88, 0.0, 0.026_179_94)),
    );
    let hat3 = hat2.add_or_replace_child(
        "hat3",
        vec![Cube::new((0.0, 87.0), (0.0, 0.0, 0.0), (4.0, 4.0, 4.0))],
        Pose::offset_rotation((1.75, -4.0, 2.0), (-0.104_719_76, 0.0, 0.052_359_88)),
    );
    hat3.add_or_replace_child(
        "hat4",
        vec![Cube::new((0.0, 95.0), (0.0, 0.0, 0.0), (1.0, 2.0, 1.0)).grown(0.25)],
        Pose::offset_rotation((1.75, -2.0, 2.0), (-PI / 15.0, 0.0, 0.104_719_76)),
    );
    head.child_mut("nose").add_or_replace_child(
        "mole",
        vec![Cube::new((0.0, 0.0), (0.0, 3.0, -6.75), (1.0, 1.0, 1.0)).grown(-0.25)],
        Pose::offset(0.0, -2.0, 0.0),
    );
    LayerDefinition::new(root, 64.0, 128.0)
}

/// `ZombieVillagerModel.createBodyLayer`.
fn zombie_root() -> PartDefinition {
    let mut root = humanoid::create_mesh(0.0, 0.0);
    let head = root.add_or_replace_child(
        "head",
        vec![
            Cube::new((0.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0)),
            Cube::new((24.0, 0.0), (-1.0, -3.0, -6.0), (2.0, 4.0, 2.0)),
        ],
        Pose::ZERO,
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((32.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 10.0, 8.0)).grown(0.5)],
        Pose::ZERO,
    )
    .add_or_replace_child(
        "hat_rim",
        vec![Cube::new(
            (30.0, 47.0),
            (-8.0, -8.0, -6.0),
            (16.0, 16.0, 1.0),
        )],
        Pose::offset_rotation((0.0, 0.0, 0.0), (-HALF_PI, 0.0, 0.0)),
    );
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((16.0, 20.0), (-4.0, 0.0, -3.0), (8.0, 12.0, 6.0)),
            Cube::new((0.0, 38.0), (-4.0, 0.0, -3.0), (8.0, 20.0, 6.0)).grown(0.05),
        ],
        Pose::ZERO,
    );
    for (name, x, mirror) in [("right_arm", -5.0, false), ("left_arm", 5.0, true)] {
        let cube = Cube::new(
            (44.0, 22.0),
            if mirror {
                (-1.0, -2.0, -2.0)
            } else {
                (-3.0, -2.0, -2.0)
            },
            (4.0, 12.0, 4.0),
        );
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 2.0, 0.0),
        );
    }
    for (name, x, mirror) in [("right_leg", -2.0, false), ("left_leg", 2.0, true)] {
        let cube = Cube::new((0.0, 22.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0));
        root.add_or_replace_child(
            name,
            vec![if mirror { cube.mirrored() } else { cube }],
            Pose::offset(x, 12.0, 0.0),
        );
    }
    root
}

pub fn zombie_villager() -> LayerDefinition {
    LayerDefinition::new(zombie_root(), 64.0, 64.0)
}

pub fn zombie_villager_no_hat() -> LayerDefinition {
    let mut root = zombie_root();
    root.child_mut("head").clear_cubes();
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `BabyZombieVillagerModel.createBodyLayer`.
fn baby_zombie_root() -> PartDefinition {
    let mut root = PartDefinition::root();
    root.add_or_replace_child(
        "body",
        vec![
            Cube::new((0.0, 15.0), (-2.0, -2.75, -1.5), (4.0, 5.0, 3.0)),
            Cube::new((16.0, 22.0), (-2.0, -2.75, -1.5), (4.0, 6.0, 3.0)).grown(0.1),
        ],
        Pose::offset(0.0, 18.75, 0.0),
    );
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -3.5), (8.0, 8.0, 7.0))],
        Pose::offset(0.0, 16.0, 0.0),
    );
    head.add_or_replace_child(
        "hat",
        vec![Cube::new((0.0, 31.0), (-4.0, -4.0, -3.5), (8.0, 8.0, 7.0)).grown(0.3)],
        Pose::offset(0.0, -4.0, 0.0),
    );
    head.add_or_replace_child(
        "hat_rim",
        vec![Cube::new(
            (0.0, 46.0),
            (-7.0, -0.5, -6.0),
            (14.0, 1.0, 12.0),
        )],
        Pose::offset(0.0, -4.5, 0.0),
    );
    head.add_or_replace_child(
        "nose",
        vec![Cube::new((23.0, 0.0), (-1.0, -1.0, -0.5), (2.0, 2.0, 1.0))],
        Pose::offset(0.0, -1.0, -4.0),
    );
    for (name, uv, x) in [
        ("right_arm", (24.0, 15.0), -3.0),
        ("left_arm", (16.0, 15.0), 3.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, -0.5, -1.0), (2.0, 5.0, 2.0))],
            Pose::offset(x, 15.5, 0.0),
        );
    }
    for (name, uv, x) in [
        ("right_leg", (8.0, 23.0), -1.0),
        ("left_leg", (0.0, 23.0), 1.0),
    ] {
        root.add_or_replace_child(
            name,
            vec![Cube::new(uv, (-1.0, -0.5, -1.0), (2.0, 3.0, 2.0))],
            Pose::offset(x, 21.5, 0.0),
        );
    }
    root
}

pub fn baby_zombie_villager() -> LayerDefinition {
    LayerDefinition::new(baby_zombie_root(), 64.0, 64.0)
}

pub fn baby_zombie_villager_no_hat() -> LayerDefinition {
    let mut root = baby_zombie_root();
    root.child_mut("head").clear_cubes();
    LayerDefinition::new(root, 64.0, 64.0)
}

/// `ZombieVillagerModel.createArmorLayerSet`.
pub fn zombie_villager_armor() -> [LayerDefinition; 4] {
    humanoid::armor_layers(
        |g| {
            let mut root = humanoid::create_mesh(g, 0.0);
            root.add_or_replace_child(
                "head",
                vec![Cube::new((0.0, 0.0), (-4.0, -10.0, -4.0), (8.0, 8.0, 8.0)).grown(g)],
                Pose::ZERO,
            );
            root.add_or_replace_child(
                "body",
                vec![Cube::new((16.0, 16.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0)).grown(g + 0.1)],
                Pose::ZERO,
            );
            for (name, x, mirror) in [("right_leg", -2.0, false), ("left_leg", 2.0, true)] {
                let cube =
                    Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(g + 0.1);
                root.add_or_replace_child(
                    name,
                    vec![if mirror { cube.mirrored() } else { cube }],
                    Pose::offset(x, 12.0, 0.0),
                );
            }
            root
        },
        0.5,
        1.0,
    )
}

/// A villager's head, nose and legs.
#[derive(Clone, Copy, Debug)]
pub struct VillagerParts {
    head: PartId,
    nose: PartId,
    arms: PartId,
    right_leg: PartId,
    left_leg: PartId,
}

impl VillagerParts {
    /// The head, which a worn head or item sits on.
    pub fn head(&self) -> PartId {
        self.head
    }

    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            nose: model.part("nose"),
            arms: model.part("arms"),
            right_leg: model.part("right_leg"),
            left_leg: model.part("left_leg"),
        }
    }
}

/// `CrossedArmsItemLayer`: an item held in the folded arms.
pub fn crossed_arms_transform(
    model: &Model,
    parts: &VillagerParts,
    pose: &ModelPose,
    root: Mat4,
) -> Mat4 {
    let arms = pose[parts.arms]
        .pose
        .placed_in(model.parent_transform(pose, parts.arms, root));
    arms * Mat4::rot_x_deg(0.75f32.to_degrees())
        * Mat4::from_scale(Vec3::splat(1.07))
        * Mat4::from_translation(Vec3::new(0.0, 0.13, -0.34))
        * Mat4::rot_x_deg(180.0)
}

/// `WitchItemLayer`: a potion held up to the nose.
pub fn potion_transform(
    model: &Model,
    parts: &VillagerParts,
    pose: &ModelPose,
    root: Mat4,
) -> Mat4 {
    let nose = pose[parts.nose]
        .pose
        .placed_in(model.parent_transform(pose, parts.nose, root));
    nose * Mat4::from_translation(Vec3::new(0.0625, 0.25, 0.0))
        * Mat4::rot_z_deg(180.0)
        * Mat4::rot_x_deg(140.0)
        * Mat4::rot_z_deg(10.0)
        * Mat4::rot_x_deg(180.0)
}

/// What the pose adds to the walk: a shaking head, a witch's nose.
#[derive(Clone, Copy, Debug, Default)]
pub struct VillagerPose {
    /// `isUnhappy`: the head shakes "no".
    pub unhappy: bool,
    /// A witch holding something: the nose lifts (`WitchModel`).
    pub witch: Option<WitchPose>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WitchPose {
    pub entity_id: i32,
    pub holding_item: bool,
}

/// `VillagerModel.setupAnim` and `WitchModel.setupAnim`.
pub fn setup_anim(
    parts: &VillagerParts,
    pose: &mut ModelPose,
    living: &LivingRenderState,
    villager: VillagerPose,
) {
    let head = &mut pose[parts.head].pose.rotation;
    head.y = living.y_rot.to_radians();
    head.x = living.x_rot.to_radians();
    if villager.unhappy {
        head.z = 0.3 * angle::sin(0.45 * living.age_in_ticks);
        head.x = 0.4;
    } else {
        head.z = 0.0;
    }
    let phase = living.walk_position * 0.6662;
    let swing = angle::cos(phase) * 1.4 * living.walk_speed * 0.5;
    pose[parts.right_leg].pose.rotation.x = swing;
    pose[parts.left_leg].pose.rotation.x = angle::cos(phase + PI) * 1.4 * living.walk_speed * 0.5;
    pose[parts.right_leg].pose.rotation.y = 0.0;
    pose[parts.left_leg].pose.rotation.y = 0.0;
    if let Some(witch) = villager.witch {
        let speed = 0.01 * (witch.entity_id % 10) as f32;
        let nose = &mut pose[parts.nose].pose;
        nose.rotation.x = angle::sin(living.age_in_ticks * speed) * 4.5 * (PI / 180.0);
        nose.rotation.z = angle::cos(living.age_in_ticks * speed) * 2.5 * (PI / 180.0);
        if witch.holding_item {
            nose.offset = glam::Vec3::new(0.0, 1.0, -1.5);
            nose.rotation.x = -0.9;
        }
    }
}

/// The villager renderers and `WitchRenderer`.
pub(super) fn push_layers(
    entity: &ClientEntity,
    hat: &dyn Fn(&str) -> Hat,
    out: &mut Vec<LayerSpec>,
) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Villager | EntityKind::ZombieVillager => villager_layers(entity, hat, out),
        EntityKind::WanderingTrader => out.push(plain(
            M::Villager,
            "minecraft:entity/wandering_trader/wandering_trader",
        )),
        EntityKind::Witch => out.push(plain(M::Witch, "minecraft:entity/witch/witch")),
        _ => {}
    }
}

macro_rules! textures {
    ($dir:literal, $kind:literal, [$($name:literal),*]) => {
        [$(concat!("minecraft:entity/", $dir, "/", $kind, "/", $name)),*]
    };
}

/// One renderer's clothes, by type, profession and level.
macro_rules! clothes {
    ($dir:literal) => {
        Clothes {
            base: concat!("minecraft:entity/", $dir, "/", $dir),
            baby_base: concat!("minecraft:entity/", $dir, "/", $dir, "_baby"),
            types: textures!(
                $dir,
                "type",
                [
                    "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"
                ]
            ),
            baby_types: textures!(
                $dir,
                "baby",
                [
                    "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"
                ]
            ),
            professions: textures!(
                $dir,
                "profession",
                [
                    "none",
                    "armorer",
                    "butcher",
                    "cartographer",
                    "cleric",
                    "farmer",
                    "fisherman",
                    "fletcher",
                    "leatherworker",
                    "librarian",
                    "mason",
                    "nitwit",
                    "shepherd",
                    "toolsmith",
                    "weaponsmith"
                ]
            ),
            levels: textures!(
                $dir,
                "profession_level",
                ["stone", "iron", "gold", "emerald", "diamond"]
            ),
        }
    };
}

struct Clothes {
    base: &'static str,
    baby_base: &'static str,
    types: [&'static str; 7],
    baby_types: [&'static str; 7],
    professions: [&'static str; 15],
    levels: [&'static str; 5],
}

const VILLAGER_CLOTHES: Clothes = clothes!("villager");

const ZOMBIE_VILLAGER_CLOTHES: Clothes = clothes!("zombie_villager");

/// `VillagerProfession`'s `none` and `nitwit` ids.
const PROFESSION_NONE: usize = 0;
const PROFESSION_NITWIT: usize = 11;

/// A villager's body and `VillagerProfessionLayer`.
fn villager_layers(entity: &ClientEntity, hat: &dyn Fn(&str) -> Hat, out: &mut Vec<LayerSpec>) {
    let zombie = entity.kind == EntityKind::ZombieVillager;
    let (clothes, full, no_hat, baby_full, baby_no_hat) = if zombie {
        (
            &ZOMBIE_VILLAGER_CLOTHES,
            M::ZombieVillager,
            M::ZombieVillagerNoHat,
            M::ZombieVillagerBaby,
            M::ZombieVillagerBabyNoHat,
        )
    } else {
        (
            &VILLAGER_CLOTHES,
            M::Villager,
            M::VillagerNoHat,
            M::VillagerBaby,
            M::VillagerBabyNoHat,
        )
    };
    let a = &entity.appearance;
    let baby = a.baby;
    let plain = LayerSpec::plain;
    let (kind, job, level) = a.villager;
    let kind = usize::try_from(kind).ok().filter(|&k| k < 7).unwrap_or(2);
    let job = usize::try_from(job).ok().filter(|&j| j < 15).unwrap_or(0);
    out.push(if baby {
        plain(baby_full, clothes.baby_base)
    } else {
        plain(full, clothes.base)
    });
    let hat_visible = hat(clothes.professions[job]).type_hat_visible(hat(clothes.types[kind]));
    out.push(match (baby, hat_visible) {
        (true, true) => plain(baby_full, clothes.baby_types[kind]),
        (true, false) => plain(baby_no_hat, clothes.baby_types[kind]),
        (false, true) => plain(full, clothes.types[kind]),
        (false, false) => plain(no_hat, clothes.types[kind]),
    });
    if job != PROFESSION_NONE && !baby {
        out.push(plain(full, clothes.professions[job]));
        if job != PROFESSION_NITWIT {
            let badge = usize::try_from(level.clamp(1, 5) - 1).unwrap_or(0);
            out.push(plain(full, clothes.levels[badge]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn villagers_legs_walk_and_an_unhappy_head_shakes() {
        for layer in [adult(), baby(), witch()] {
            let model = Model::bake(&layer);
            let parts = VillagerParts::find(&model);
            let living = LivingRenderState {
                walk_position: 1.0,
                walk_speed: 1.0,
                age_in_ticks: 3.0,
                ..Default::default()
            };
            let mut pose = model.initial_pose();
            setup_anim(
                &parts,
                &mut pose,
                &living,
                VillagerPose {
                    unhappy: true,
                    witch: None,
                },
            );
            assert!(pose[parts.right_leg].pose.rotation.x.abs() > 0.1);
            assert_eq!(pose[parts.head].pose.rotation.x, 0.4);
            assert!(pose[parts.head].pose.rotation.z != 0.0);
        }
    }

    #[test]
    fn a_profession_hides_the_biomes_hat_as_far_as_its_texture_says() {
        use Hat::{Full, None as Nothing, Partial};
        assert!(Nothing.type_hat_visible(Full));
        assert!(Partial.type_hat_visible(Partial));
        assert!(!Partial.type_hat_visible(Full));
        assert!(!Full.type_hat_visible(Nothing));
        let meta: serde_json::Value = serde_json::json!({"villager": {"hat": "partial"}});
        assert_eq!(Hat::from_meta(&meta), Partial);
        assert_eq!(Hat::from_meta(&serde_json::json!({})), Nothing);
    }

    #[test]
    fn a_zombie_villagers_armor_sits_on_its_taller_head() {
        let top = |layers: [LayerDefinition; 4]| {
            let model = Model::bake(&layers[0]);
            model
                .part_vertices(model.part("head"))
                .iter()
                .map(|v| v.pos[1])
                .fold(f32::MAX, f32::min)
        };
        let villager = top(zombie_villager_armor());
        let plain = top(humanoid::create_armor_layers());
        assert!(villager < plain, "{villager} {plain}");
    }

    #[test]
    fn a_witch_lifts_its_nose_to_hold_a_potion() {
        let model = Model::bake(&witch());
        let parts = VillagerParts::find(&model);
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &LivingRenderState::default(),
            VillagerPose {
                unhappy: false,
                witch: Some(WitchPose {
                    entity_id: 7,
                    holding_item: true,
                }),
            },
        );
        assert_eq!(pose[parts.nose].pose.rotation.x, -0.9);
        assert_eq!(pose[parts.nose].pose.offset.y, 1.0);
    }
}
