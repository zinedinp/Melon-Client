//! Vanilla `HumanoidModel`.

use glam::{Mat4, Vec3};

use super::animation::{animate_zombie_arms, bob_model_part, out_quart, rot_lerp_rad};
use super::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use super::player::Arm;
use crate::math::angle::{self, PI};
use crate::world::model_part::{Cube, Pose};

/// `HumanoidModel.OVERLAY_SCALE`: how far the outer layer stands off.
pub const OVERLAY_SCALE: f32 = 0.25;
/// `HumanoidModel.HAT_OVERLAY_SCALE`.
pub const HAT_OVERLAY_SCALE: f32 = 0.5;

/// `HumanoidModel.createMesh(g, yOffset)`.
pub fn create_mesh(grow: f32, y_offset: f32) -> PartDefinition {
    let mut root = PartDefinition::root();
    let head = root.add_or_replace_child(
        "head",
        vec![Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(grow)],
        Pose::offset(0.0, y_offset, 0.0),
    );
    head.add_or_replace_child(
        "hat",
        vec![
            Cube::new((32.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0))
                .grown(grow + HAT_OVERLAY_SCALE),
        ],
        Pose::ZERO,
    );
    root.add_or_replace_child(
        "body",
        vec![Cube::new((16.0, 16.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0)).grown(grow)],
        Pose::offset(0.0, y_offset, 0.0),
    );
    root.add_or_replace_child(
        "right_arm",
        vec![Cube::new((40.0, 16.0), (-3.0, -2.0, -2.0), (4.0, 12.0, 4.0)).grown(grow)],
        Pose::offset(-5.0, 2.0 + y_offset, 0.0),
    );
    root.add_or_replace_child(
        "left_arm",
        vec![
            Cube::new((40.0, 16.0), (-1.0, -2.0, -2.0), (4.0, 12.0, 4.0))
                .grown(grow)
                .mirrored(),
        ],
        Pose::offset(5.0, 2.0 + y_offset, 0.0),
    );
    root.add_or_replace_child(
        "right_leg",
        vec![Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(grow)],
        Pose::offset(-1.9, 12.0 + y_offset, 0.0),
    );
    root.add_or_replace_child(
        "left_leg",
        vec![
            Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0))
                .grown(grow)
                .mirrored(),
        ],
        Pose::offset(1.9, 12.0 + y_offset, 0.0),
    );
    root
}

/// `HumanoidModel`'s parts.
#[derive(Clone, Copy, Debug)]
pub struct HumanoidParts {
    pub head: PartId,
    pub hat: PartId,
    pub body: PartId,
    pub right_arm: PartId,
    pub left_arm: PartId,
    pub right_leg: PartId,
    pub left_leg: PartId,
}

impl HumanoidParts {
    pub fn find(model: &Model) -> Self {
        Self {
            head: model.part("head"),
            hat: model.part("hat"),
            body: model.part("body"),
            right_arm: model.part("right_arm"),
            left_arm: model.part("left_arm"),
            right_leg: model.part("right_leg"),
            left_leg: model.part("left_leg"),
        }
    }
}

/// `HumanoidModel.ArmPose`, the ones Melon sets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArmPose {
    #[default]
    Empty,
    /// Holding anything without a pose of its own.
    Item,
    /// Raising a shield.
    Block,
    /// Drawing a bow: both arms up along the look.
    BowAndArrow,
    /// Holding a trident back to throw it: the arm straight up.
    ThrowTrident,
}

impl ArmPose {
    /// `ArmPose.isTwoHanded`: the other hand holds on with it.
    pub fn is_two_handed(self) -> bool {
        self == Self::BowAndArrow
    }

    /// `AvatarRenderer.getArmPose`'s last step.
    pub fn offhand_of(self, main_hand: Self) -> Self {
        match (main_hand.is_two_handed(), self) {
            (true, Self::Empty) | (false, _) => self,
            (true, _) => Self::Item,
        }
    }
}

/// How the arms answer a swing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttackStyle {
    /// The player's and most mobs' one-armed swing.
    #[default]
    Swing,
    /// `ZombieModel`: both arms held out, higher when `aggressive`.
    Zombie {
        aggressive: bool,
        baby_holding: bool,
    },
}

/// The swing under way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingPose {
    pub arm: Arm,
    pub animation: f32,
    pub whack: bool,
}

/// What `HumanoidModel.setupAnim` reads of `HumanoidRenderState`.
#[derive(Clone, Copy, Debug)]
pub struct HumanoidRenderState {
    /// The head's pitch, degrees (`xRot`).
    pub x_rot: f32,
    /// The head's yaw relative to the body, degrees (`yRot`).
    pub y_rot: f32,
    /// `walkAnimationPos` and `walkAnimationSpeed`.
    pub walk_position: f32,
    pub walk_speed: f32,
    /// `speedValue`: 1 for players.
    pub speed_value: f32,
    /// `ageInTicks`, for the idle arm bob.
    pub age_in_ticks: f32,
    pub crouching: bool,
    /// `isPassenger`: seated, the legs out in front.
    pub riding: bool,
    /// `swimAmount`: 0 on land, 1 in the swimming pose.
    pub swim_amount: f32,
    /// `isUsingItem`: a swimmer keeps the arms of the item in use.
    pub using_item: bool,
    /// Which stroke.
    pub drowned_swim: bool,
    pub right_arm_pose: ArmPose,
    pub left_arm_pose: ArmPose,
    pub swing: Option<SwingPose>,
    pub attack: AttackStyle,
}

impl Default for HumanoidRenderState {
    fn default() -> Self {
        Self {
            x_rot: 0.0,
            y_rot: 0.0,
            walk_position: 0.0,
            walk_speed: 0.0,
            speed_value: 1.0,
            age_in_ticks: 0.0,
            crouching: false,
            riding: false,
            swim_amount: 0.0,
            using_item: false,
            drowned_swim: false,
            right_arm_pose: ArmPose::Empty,
            left_arm_pose: ArmPose::Empty,
            swing: None,
            attack: AttackStyle::Swing,
        }
    }
}

impl HumanoidRenderState {
    /// The living state's look, walk and age; the arms at rest.
    pub fn from_living(living: &super::living::LivingRenderState) -> Self {
        Self {
            x_rot: living.x_rot,
            y_rot: living.y_rot,
            walk_position: living.walk_position,
            walk_speed: living.walk_speed,
            age_in_ticks: living.age_in_ticks,
            riding: living.riding,
            ..Default::default()
        }
    }
}

impl HumanoidParts {
    /// The seven parts in a fixed order, for copying between models.
    fn all(&self) -> [PartId; 7] {
        [
            self.head,
            self.hat,
            self.body,
            self.right_arm,
            self.left_arm,
            self.right_leg,
            self.left_leg,
        ]
    }
}

/// `HumanoidModel.copyPropertiesTo`.
pub fn copy_pose(
    from: (&Model, &HumanoidParts, &ModelPose),
    to: (&Model, &HumanoidParts, &mut ModelPose),
) {
    let (from_model, from_parts, from_pose) = from;
    let (to_model, to_parts, to_pose) = to;
    for (src, dst) in from_parts.all().into_iter().zip(to_parts.all()) {
        let state = from_pose[src];
        let moved = state.pose.offset - from_model.initial(src).offset;
        let target = &mut to_pose[dst];
        target.pose.rotation = state.pose.rotation;
        target.pose.scale = state.pose.scale;
        target.pose.offset = to_model.initial(dst).offset + moved;
        target.visible = state.visible;
    }
}

/// `HumanoidModel.setupAnim`, on a pose fresh from the definition.
pub fn setup_anim(parts: &HumanoidParts, pose: &mut ModelPose, state: &HumanoidRenderState) {
    let head = &mut pose[parts.head].pose;
    head.rotation.x = state.x_rot.to_radians();
    head.rotation.y = state.y_rot.to_radians();

    let phase = state.walk_position * 0.6662;
    let speed = state.walk_speed / state.speed_value;
    pose[parts.right_arm].pose.rotation.x = angle::cos(phase + PI) * 2.0 * speed * 0.5;
    pose[parts.left_arm].pose.rotation.x = angle::cos(phase) * 2.0 * speed * 0.5;
    pose[parts.right_leg].pose.rotation = Vec3::new(angle::cos(phase) * 1.4 * speed, 0.005, 0.005);
    pose[parts.left_leg].pose.rotation =
        Vec3::new(angle::cos(phase + PI) * 1.4 * speed, -0.005, -0.005);
    if state.riding {
        pose[parts.right_arm].pose.rotation.x += -PI / 5.0;
        pose[parts.left_arm].pose.rotation.x += -PI / 5.0;
        sit(parts, pose);
    }

    let head_rotation = pose[parts.head].pose.rotation;
    for (arm, arm_pose, right) in [
        (parts.right_arm, state.right_arm_pose, true),
        (parts.left_arm, state.left_arm_pose, false),
    ] {
        let rotation = &mut pose[arm].pose.rotation;
        match arm_pose {
            ArmPose::Empty => rotation.y = 0.0,
            ArmPose::Item => {
                rotation.x = rotation.x * 0.5 - PI / 10.0;
                rotation.y = 0.0;
            }
            ArmPose::Block => {
                rotation.x = rotation.x * 0.5 - 0.942_477_9
                    + head_rotation.x.clamp(-PI * 4.0 / 9.0, 0.436_332_32);
                let side = if right { -30f32 } else { 30.0 };
                rotation.y = side.to_radians() + head_rotation.y.clamp(-PI / 6.0, PI / 6.0);
            }
            ArmPose::BowAndArrow => {}
            ArmPose::ThrowTrident => {
                rotation.x = rotation.x * 0.5 - PI;
                rotation.y = 0.0;
            }
        }
    }
    if let Some(right) = [(state.right_arm_pose, true), (state.left_arm_pose, false)]
        .into_iter()
        .find_map(|(arm_pose, right)| (arm_pose == ArmPose::BowAndArrow).then_some(right))
    {
        let (head_x, head_y) = (head_rotation.x, head_rotation.y);
        let (right_turn, left_turn) = if right { (0.0, 0.4) } else { (-0.4, 0.0) };
        let r = &mut pose[parts.right_arm].pose.rotation;
        r.y = -0.1 + head_y + right_turn;
        r.x = -PI / 2.0 + head_x;
        let l = &mut pose[parts.left_arm].pose.rotation;
        l.y = 0.1 + head_y + left_turn;
        l.x = -PI / 2.0 + head_x;
    }

    if let Some(swing) = state.swing {
        setup_attack_animation(parts, pose, swing);
    }
    if let AttackStyle::Zombie {
        aggressive,
        baby_holding,
    } = state.attack
    {
        let swing = state.swing.map_or(0.0, |s| s.animation);
        let [left, right] = pose.get_two_mut(parts.left_arm, parts.right_arm);
        animate_zombie_arms(
            &mut left.pose,
            &mut right.pose,
            aggressive,
            baby_holding,
            swing,
        );
    }

    if state.crouching {
        pose[parts.body].pose.rotation.x = 0.5;
        pose[parts.right_arm].pose.rotation.x += 0.4;
        pose[parts.left_arm].pose.rotation.x += 0.4;
        pose[parts.right_leg].pose.offset.z += 4.0;
        pose[parts.left_leg].pose.offset.z += 4.0;
        pose[parts.head].pose.offset.y += 4.2;
        pose[parts.body].pose.offset.y += 3.2;
        pose[parts.left_arm].pose.offset.y += 3.2;
        pose[parts.right_arm].pose.offset.y += 3.2;
    }

    bob_model_part(&mut pose[parts.right_arm].pose, state.age_in_ticks, 1.0);
    bob_model_part(&mut pose[parts.left_arm].pose, state.age_in_ticks, -1.0);

    if state.swim_amount > 0.0 {
        if state.drowned_swim {
            setup_drowned_swim(parts, pose, state);
        } else {
            setup_swim_animation(parts, pose, state);
        }
    }
}

/// The legs of a passenger: straight out ahead, a little apart.
pub fn sit(parts: &HumanoidParts, pose: &mut ModelPose) {
    pose[parts.right_leg].pose.rotation = Vec3::new(-1.413_716_7, PI / 10.0, 0.078_539_82);
    pose[parts.left_leg].pose.rotation = Vec3::new(-1.413_716_7, -PI / 10.0, -0.078_539_82);
}

/// `HumanoidModel.setupSwimAnimation`: the front crawl.
fn setup_swim_animation(parts: &HumanoidParts, pose: &mut ModelPose, state: &HumanoidRenderState) {
    let swim = state.swim_amount;
    let head = &mut pose[parts.head].pose.rotation;
    head.x = rot_lerp_rad(swim, head.x, -PI / 4.0);

    let swim_pos = state.walk_position % 26.0;
    let swinging = |arm: Arm| state.swing.is_some_and(|s| s.arm == arm);
    let right_amount = if swinging(Arm::Right) { 0.0 } else { swim };
    let left_amount = if swinging(Arm::Left) { 0.0 } else { swim };
    if !state.using_item {
        let quadratic = |x: f32| -65.0 * x + x * x;
        let (x, z_left, z_right) = if swim_pos < 14.0 {
            let k = 1.870_796_4 * quadratic(swim_pos) / quadratic(14.0);
            (0.0, PI + k, PI - k)
        } else if swim_pos < 22.0 {
            let t = (swim_pos - 14.0) / 8.0;
            (
                PI / 2.0 * t,
                5.012_389 - 1.870_796_4 * t,
                1.270_796_3 + 1.870_796_4 * t,
            )
        } else {
            let t = (swim_pos - 22.0) / 4.0;
            (PI / 2.0 - PI / 2.0 * t, PI, PI)
        };
        let left = &mut pose[parts.left_arm].pose.rotation;
        left.x = rot_lerp_rad(left_amount, left.x, x);
        left.y = rot_lerp_rad(left_amount, left.y, PI);
        left.z = rot_lerp_rad(left_amount, left.z, z_left);
        let right = &mut pose[parts.right_arm].pose.rotation;
        right.x += (x - right.x) * right_amount;
        right.y += (PI - right.y) * right_amount;
        right.z += (z_right - right.z) * right_amount;
    }
    let kick = |phase: f32| 0.3 * angle::cos(state.walk_position * 0.333_333_34 + phase);
    let left_leg = &mut pose[parts.left_leg].pose.rotation.x;
    *left_leg += (kick(PI) - *left_leg) * swim;
    let right_leg = &mut pose[parts.right_leg].pose.rotation.x;
    *right_leg += (kick(0.0) - *right_leg) * swim;
}

/// `DrownedModel.setupSwimAnimation`.
fn setup_drowned_swim(parts: &HumanoidParts, pose: &mut ModelPose, state: &HumanoidRenderState) {
    let swim = state.swim_amount;
    let paddle = angle::sin(0.1 * state.age_in_ticks);
    let reach = -PI * 4.0 / 5.0;
    let right = &mut pose[parts.right_arm].pose.rotation;
    right.x = rot_lerp_rad(swim, right.x, reach) + swim * 0.35 * paddle;
    right.z = rot_lerp_rad(swim, right.z, -0.15);
    let left = &mut pose[parts.left_arm].pose.rotation;
    left.x = rot_lerp_rad(swim, left.x, reach) - swim * 0.35 * paddle;
    left.z = rot_lerp_rad(swim, left.z, 0.15);
    pose[parts.left_leg].pose.rotation.x -= swim * 0.55 * paddle;
    pose[parts.right_leg].pose.rotation.x += swim * 0.55 * paddle;
    pose[parts.head].pose.rotation.x = 0.0;
}

/// `HumanoidModel.setupAttackAnimation`.
fn setup_attack_animation(parts: &HumanoidParts, pose: &mut ModelPose, swing: SwingPose) {
    let animation = swing.animation;
    if animation <= 0.0 {
        return;
    }
    let mut body_y = angle::sin(animation.sqrt() * PI * 2.0) * 0.2;
    if swing.arm == Arm::Left {
        body_y = -body_y;
    }
    pose[parts.body].pose.rotation.y = body_y;
    let (sin, cos) = angle::sin_cos(body_y);
    let right = &mut pose[parts.right_arm].pose;
    right.offset.z = sin * 5.0;
    right.offset.x = -cos * 5.0;
    right.rotation.y += body_y;
    let left = &mut pose[parts.left_arm].pose;
    left.offset.z = -sin * 5.0;
    left.offset.x = cos * 5.0;
    left.rotation.y += body_y;
    left.rotation.x += body_y;
    if swing.whack {
        let head_x = pose[parts.head].pose.rotation.x;
        let arc = angle::sin(out_quart(animation) * PI);
        let lift = angle::sin(animation * PI) * -(head_x - 0.7) * 0.75;
        let arm = match swing.arm {
            Arm::Right => parts.right_arm,
            Arm::Left => parts.left_arm,
        };
        let rotation = &mut pose[arm].pose.rotation;
        rotation.x -= arc * 1.2 + lift;
        rotation.y += body_y * 2.0;
        rotation.z += angle::sin(animation * PI) * -0.4;
    }
}

impl HumanoidParts {
    pub fn arm(&self, arm: Arm) -> PartId {
        match arm {
            Arm::Right => self.right_arm,
            Arm::Left => self.left_arm,
        }
    }
}

/// `HumanoidModel.translateToHand`.
pub fn hand_transform(
    model: &Model,
    parts: &HumanoidParts,
    pose: &ModelPose,
    arm: Arm,
    outward: f32,
    root: Mat4,
) -> Mat4 {
    let part = parts.arm(arm);
    let mut shoulder = pose[part].pose;
    shoulder.offset.x += match arm {
        Arm::Right => outward,
        Arm::Left => -outward,
    };
    shoulder.placed_in(model.parent_transform(pose, part, root))
}

/// `EquipmentSlot`'s armor slots, in the order armor is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArmorSlot {
    Head,
    Chest,
    Legs,
    Feet,
}

impl ArmorSlot {
    pub const ALL: [Self; 4] = [Self::Head, Self::Chest, Self::Legs, Self::Feet];
}

/// `HumanoidModel.createArmorMeshSet(inner 0.5, outer 1.0)` as 64x32 layers.
pub fn create_armor_layers() -> [LayerDefinition; 4] {
    armor_layers(base_armor_mesh, 0.5, 1.0)
}

/// `HumanoidModel.createBaseArmorMesh`.
pub fn base_armor_mesh(grow: f32) -> PartDefinition {
    let mut root = create_mesh(grow, 0.0);
    for (name, x, mirror) in [("right_leg", -1.9, false), ("left_leg", 1.9, true)] {
        let mut cube =
            Cube::new((0.0, 16.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(grow - 0.1);
        if mirror {
            cube = cube.mirrored();
        }
        root.add_or_replace_child(name, vec![cube], Pose::offset(x, 12.0, 0.0));
    }
    root
}

/// `HumanoidModel.createArmorMeshSet` over `base`.
pub fn armor_layers(
    base: impl Fn(f32) -> PartDefinition,
    inner: f32,
    outer: f32,
) -> [LayerDefinition; 4] {
    ArmorSlot::ALL.map(|slot| {
        let (grow, keep, children): (f32, &[&str], bool) = match slot {
            ArmorSlot::Head => (outer, &["head"], true),
            ArmorSlot::Chest => (outer, &["body", "left_arm", "right_arm"], false),
            ArmorSlot::Legs => (inner, &["left_leg", "right_leg", "body"], false),
            ArmorSlot::Feet => (outer, &["left_leg", "right_leg"], false),
        };
        let mut root = base(grow);
        root.retain_cubes(keep, children);
        LayerDefinition {
            root,
            texture_width: 64.0,
            texture_height: 32.0,
        }
    })
}

/// `createBabyArmorMeshSet` with `HumanoidMobRenderer`'s baby deformations.
pub fn create_baby_armor_layers() -> [LayerDefinition; 4] {
    baby_armor_layers(
        Vec3::new(-0.1, 0.3, 0.3),
        Vec3::new(-0.1, 0.5, 0.3),
        Vec3::ZERO,
    )
}

/// `createBabyArmorMeshSet` with these deformations and `arm_offset`.
pub fn baby_armor_layers(inner: Vec3, outer: Vec3, arm_offset: Vec3) -> [LayerDefinition; 4] {
    let mesh = |g: Vec3| {
        let less = g - Vec3::splat(0.1);
        let mut root = PartDefinition::root();
        let head = root.add_or_replace_child(
            "head",
            vec![Cube::new((0.0, 0.0), (-4.5, -7.0, -4.5), (9.0, 8.0, 8.0)).grown_by(g)],
            Pose::offset(0.0, 15.0, 0.0),
        );
        head.add_or_replace_child("hat", Vec::new(), Pose::ZERO);
        root.add_or_replace_child(
            "body",
            vec![Cube::new((0.0, 17.0), (-3.0, -3.0, -1.5), (6.0, 5.0, 3.0)).grown_by(g)],
            Pose::offset(0.0, 18.0, 0.0),
        );
        root.add_or_replace_child(
            "waist",
            vec![Cube::new((0.0, 36.0), (-3.0, -1.2, -1.49), (5.9, 2.0, 2.9)).grown_by(less)],
            Pose::offset(0.0, 19.0, 0.0),
        );
        root.add_or_replace_child(
            "right_arm",
            vec![Cube::new((30.0, 25.0), (-1.0, 0.0, -1.53), (2.0, 5.0, 3.0)).grown_by(g)],
            Pose::offset(-3.5 - arm_offset.x, 15.5 + arm_offset.y, arm_offset.z),
        );
        root.add_or_replace_child(
            "left_arm",
            vec![Cube::new((30.0, 17.0), (-1.0, 0.0, -1.53), (2.0, 5.0, 3.0)).grown_by(g)],
            Pose::offset(3.5 + arm_offset.x, 15.5 + arm_offset.y, arm_offset.z),
        );
        root.add_or_replace_child(
            "inner_body",
            vec![Cube::new((0.0, 17.0), (-3.0, -3.0, -1.5), (6.0, 5.0, 3.0)).grown_by(g)],
            Pose::offset(0.0, 18.0, 0.0),
        );
        root.add_or_replace_child(
            "left_leg",
            vec![Cube::new((18.0, 24.0), (-2.0, -0.2, -2.0), (3.0, 4.0, 3.0)).grown_by(less)],
            Pose::offset(1.5, 20.0, 0.5),
        )
        .add_or_replace_child(
            "right_foot",
            vec![Cube::new((0.0, 25.0), (-2.0, 2.9, -2.0), (3.0, 1.0, 3.0)).grown_by(g)],
            Pose::ZERO,
        );
        root.add_or_replace_child(
            "right_leg",
            vec![Cube::new((18.0, 17.0), (-1.0, -0.2, -2.0), (3.0, 4.0, 3.0)).grown_by(less)],
            Pose::offset(-1.5, 20.0, 0.5),
        )
        .add_or_replace_child(
            "left_foot",
            vec![
                Cube::new((0.0, 29.0), (-1.0, 2.9, -2.0), (3.0, 1.0, 3.0))
                    .grown_by(g)
                    .mirrored(),
            ],
            Pose::ZERO,
        );
        root
    };
    ArmorSlot::ALL.map(|slot| {
        let (g, keep, children): (Vec3, &[&str], bool) = match slot {
            ArmorSlot::Head => (outer, &["head"], true),
            ArmorSlot::Chest => (outer, &["body", "left_arm", "right_arm"], false),
            ArmorSlot::Legs => (inner, &["left_leg", "right_leg", "waist"], false),
            ArmorSlot::Feet => (outer, &["left_foot", "right_foot"], false),
        };
        let mut root = mesh(g);
        root.retain_cubes(keep, children);
        LayerDefinition {
            root,
            texture_width: 64.0,
            texture_height: 64.0,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::entity::model::LayerDefinition;

    fn model() -> (Model, HumanoidParts) {
        let model = Model::bake(&LayerDefinition {
            root: create_mesh(0.0, 0.0),
            texture_width: 64.0,
            texture_height: 64.0,
        });
        let parts = HumanoidParts::find(&model);
        (model, parts)
    }

    #[test]
    fn a_passenger_sits_with_its_legs_out_and_does_not_walk() {
        let (model, parts) = model();
        let state = HumanoidRenderState {
            riding: true,
            walk_position: 3.0,
            walk_speed: 0.0,
            ..Default::default()
        };
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, &state);
        let right = pose[parts.right_leg].pose.rotation;
        let left = pose[parts.left_leg].pose.rotation;
        assert!((right.x + 1.413_716_7).abs() < 1e-6 && (left.x + 1.413_716_7).abs() < 1e-6);
        assert!(right.y > 0.0 && left.y < 0.0 && right.z > 0.0 && left.z < 0.0);
        assert!(pose[parts.right_arm].pose.rotation.x < -0.5);
    }

    #[test]
    fn a_swimmer_reaches_forward_and_kicks() {
        let (model, parts) = model();
        let state = HumanoidRenderState {
            swim_amount: 1.0,
            walk_position: 2.0,
            x_rot: 20.0,
            ..Default::default()
        };
        let mut pose = model.initial_pose();
        setup_anim(&parts, &mut pose, &state);
        assert!((pose[parts.head].pose.rotation.x + PI / 4.0).abs() < 1e-4);
        assert!((pose[parts.right_arm].pose.rotation.y - PI).abs() < 1e-4);
        let (left, right) = (
            pose[parts.left_leg].pose.rotation.x,
            pose[parts.right_leg].pose.rotation.x,
        );
        assert!(
            (left + right).abs() < 1e-4 && left.abs() > 0.01,
            "{left} {right}"
        );
        let mut holding = model.initial_pose();
        setup_anim(
            &parts,
            &mut holding,
            &HumanoidRenderState {
                using_item: true,
                right_arm_pose: ArmPose::ThrowTrident,
                ..state
            },
        );
        assert!(holding[parts.right_arm].pose.rotation.y.abs() < 1e-4);
        assert!(holding[parts.right_arm].pose.rotation.x < -2.5);
    }

    #[test]
    fn a_drowned_paddles_with_its_arms_forward() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &HumanoidRenderState {
                swim_amount: 1.0,
                drowned_swim: true,
                x_rot: 30.0,
                ..Default::default()
            },
        );
        assert!(pose[parts.right_arm].pose.rotation.x < -2.0);
        assert_eq!(pose[parts.head].pose.rotation.x, 0.0);
    }

    #[test]
    fn walking_swings_opposite_limbs_together() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        let state = HumanoidRenderState {
            walk_position: 1.0,
            walk_speed: 1.0,
            ..Default::default()
        };
        setup_anim(&parts, &mut pose, &state);
        let right_arm = pose[parts.right_arm].pose.rotation.x;
        let left_leg = pose[parts.left_leg].pose.rotation.x;
        let right_leg = pose[parts.right_leg].pose.rotation.x;
        assert!(right_arm * left_leg > 0.0);
        assert!(right_arm * right_leg < 0.0);
    }

    #[test]
    fn crouching_lowers_the_head_and_bends_the_body() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &HumanoidRenderState {
                crouching: true,
                ..Default::default()
            },
        );
        assert_eq!(pose[parts.head].pose.offset.y, 4.2);
        assert_eq!(pose[parts.body].pose.rotation.x, 0.5);
        assert_eq!(pose[parts.right_leg].pose.offset.z, 4.0);
    }

    #[test]
    fn a_swing_brings_the_arm_down_and_turns_the_body() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &HumanoidRenderState {
                swing: Some(SwingPose {
                    arm: Arm::Right,
                    animation: 0.3,
                    whack: true,
                }),
                ..Default::default()
            },
        );
        assert!(pose[parts.right_arm].pose.rotation.x < -0.8);
        let mut early = model.initial_pose();
        setup_anim(
            &parts,
            &mut early,
            &HumanoidRenderState {
                swing: Some(SwingPose {
                    arm: Arm::Right,
                    animation: 0.1,
                    whack: true,
                }),
                ..Default::default()
            },
        );
        assert!(early[parts.body].pose.rotation.y > 0.1);
        let shoulder = pose[parts.right_arm].pose.offset;
        assert!(((shoulder.x * shoulder.x + shoulder.z * shoulder.z).sqrt() - 5.0).abs() < 1e-4);
    }

    #[test]
    fn a_raised_shield_holds_the_arm_up_and_in() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &HumanoidRenderState {
                left_arm_pose: ArmPose::Block,
                ..Default::default()
            },
        );
        let left = pose[parts.left_arm].pose.rotation;
        assert!(left.x < -0.8, "{left}");
        assert!((left.y - 30f32.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn a_held_item_raises_its_arm() {
        let (model, parts) = model();
        let mut pose = model.initial_pose();
        setup_anim(
            &parts,
            &mut pose,
            &HumanoidRenderState {
                right_arm_pose: ArmPose::Item,
                ..Default::default()
            },
        );
        let raised = pose[parts.right_arm].pose.rotation.x;
        let free = pose[parts.left_arm].pose.rotation.x;
        assert!(raised < free - 0.2, "{raised} {free}");
    }
}
