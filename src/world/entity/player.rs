//! Vanilla `PlayerModel` and the parts of `AvatarRenderer` that place it:

use glam::{Mat4, Vec3};

use super::humanoid::{self, HumanoidParts, HumanoidRenderState, OVERLAY_SCALE};
use super::living::{self, LivingRenderState, Placement, Swim};
use super::magic_name::Magic;
use super::model::{LayerDefinition, Model, ModelPose, PartId};
use crate::math::angle::Rotation;
use crate::world::model_part::{Cube, Pose};

/// `PlayerModelType`: the arms' width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayerModelType {
    /// Four pixels wide, Steve's.
    Wide,
    /// Three pixels wide, Alex's.
    Slim,
}

impl PlayerModelType {
    /// The skin folder's name, `entity/player/<this>/`.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Slim => "slim",
        }
    }
}

/// `DefaultPlayerSkin.DEFAULT_SKINS`: the slim set, then the wide one.
pub const DEFAULT_SKINS: [(&str, PlayerModelType); 18] = {
    use PlayerModelType::{Slim, Wide};
    [
        ("alex", Slim),
        ("ari", Slim),
        ("efe", Slim),
        ("kai", Slim),
        ("makena", Slim),
        ("noor", Slim),
        ("steve", Slim),
        ("sunny", Slim),
        ("zuri", Slim),
        ("alex", Wide),
        ("ari", Wide),
        ("efe", Wide),
        ("kai", Wide),
        ("makena", Wide),
        ("noor", Wide),
        ("steve", Wide),
        ("sunny", Wide),
        ("zuri", Wide),
    ]
};

/// `DefaultPlayerSkin.get(UUID)`: `floorMod(uuid.hashCode(), 18)`, an
pub fn default_skin(uuid: uuid::Uuid) -> usize {
    let (high, low) = uuid.as_u64_pair();
    let both = high ^ low;
    let hash = ((both >> 32) as u32 ^ both as u32) as i32;
    hash.rem_euclid(DEFAULT_SKINS.len() as i32) as usize
}

/// The texture of default skin `index`, as an asset id.
pub fn default_skin_texture(index: usize) -> String {
    let (name, model) = DEFAULT_SKINS[index];
    format!("minecraft:entity/player/{}/{name}.png", model.folder())
}

/// `PlayerModel.createMesh(CubeDeformation.NONE, slim)` as a 64x64 layer.
pub fn create_layer(model: PlayerModelType) -> LayerDefinition {
    let mut root = humanoid::create_mesh(0.0, 0.0);
    let overlay = OVERLAY_SCALE;
    let arm_width = match model {
        PlayerModelType::Wide => 4.0,
        PlayerModelType::Slim => 3.0,
    };
    let right_x = match model {
        PlayerModelType::Wide => -3.0,
        PlayerModelType::Slim => -2.0,
    };
    let left_arm = root.add_or_replace_child(
        "left_arm",
        vec![Cube::new(
            (32.0, 48.0),
            (-1.0, -2.0, -2.0),
            (arm_width, 12.0, 4.0),
        )],
        Pose::offset(5.0, 2.0, 0.0),
    );
    left_arm.add_or_replace_child(
        "left_sleeve",
        vec![Cube::new((48.0, 48.0), (-1.0, -2.0, -2.0), (arm_width, 12.0, 4.0)).grown(overlay)],
        Pose::ZERO,
    );
    let right_arm = if model == PlayerModelType::Slim {
        root.add_or_replace_child(
            "right_arm",
            vec![Cube::new(
                (40.0, 16.0),
                (right_x, -2.0, -2.0),
                (arm_width, 12.0, 4.0),
            )],
            Pose::offset(-5.0, 2.0, 0.0),
        )
    } else {
        root.child_mut("right_arm")
    };
    right_arm.add_or_replace_child(
        "right_sleeve",
        vec![Cube::new((40.0, 32.0), (right_x, -2.0, -2.0), (arm_width, 12.0, 4.0)).grown(overlay)],
        Pose::ZERO,
    );
    let left_leg = root.add_or_replace_child(
        "left_leg",
        vec![Cube::new((16.0, 48.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0))],
        Pose::offset(1.9, 12.0, 0.0),
    );
    left_leg.add_or_replace_child(
        "left_pants",
        vec![Cube::new((0.0, 48.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(overlay)],
        Pose::ZERO,
    );
    root.child_mut("right_leg").add_or_replace_child(
        "right_pants",
        vec![Cube::new((0.0, 32.0), (-2.0, 0.0, -2.0), (4.0, 12.0, 4.0)).grown(overlay)],
        Pose::ZERO,
    );
    root.child_mut("body").add_or_replace_child(
        "jacket",
        vec![Cube::new((16.0, 32.0), (-4.0, 0.0, -2.0), (8.0, 12.0, 4.0)).grown(overlay)],
        Pose::ZERO,
    );
    LayerDefinition {
        root,
        texture_width: 64.0,
        texture_height: 64.0,
    }
}

/// `PlayerModelPart`, in its bit order: the cape and the skin's outer
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerModelPart {
    Cape,
    Jacket,
    LeftSleeve,
    RightSleeve,
    LeftPantsLeg,
    RightPantsLeg,
    Hat,
}

impl PlayerModelPart {
    pub const ALL: [Self; 7] = [
        Self::Cape,
        Self::Jacket,
        Self::LeftSleeve,
        Self::RightSleeve,
        Self::LeftPantsLeg,
        Self::RightPantsLeg,
        Self::Hat,
    ];

    /// `getId`: `options.txt`'s `modelPart_<id>`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Cape => "cape",
            Self::Jacket => "jacket",
            Self::LeftSleeve => "left_sleeve",
            Self::RightSleeve => "right_sleeve",
            Self::LeftPantsLeg => "left_pants_leg",
            Self::RightPantsLeg => "right_pants_leg",
            Self::Hat => "hat",
        }
    }

    /// `getName`'s translation key.
    pub fn caption_key(self) -> &'static str {
        match self {
            Self::Cape => "options.modelPart.cape",
            Self::Jacket => "options.modelPart.jacket",
            Self::LeftSleeve => "options.modelPart.left_sleeve",
            Self::RightSleeve => "options.modelPart.right_sleeve",
            Self::LeftPantsLeg => "options.modelPart.left_pants_leg",
            Self::RightPantsLeg => "options.modelPart.right_pants_leg",
            Self::Hat => "options.modelPart.hat",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|part| part.id() == id)
    }

    fn mask(self) -> u8 {
        1 << self as u8
    }
}

/// Which [`PlayerModelPart`]s show, as the mask `ClientInformation` sends
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownParts(u8);

impl Default for ShownParts {
    fn default() -> Self {
        Self(0x7f)
    }
}

impl ShownParts {
    pub fn from_mask(mask: u8) -> Self {
        Self(mask & 0x7f)
    }

    pub fn mask(self) -> u8 {
        self.0
    }

    pub fn shows(self, part: PlayerModelPart) -> bool {
        self.0 & part.mask() != 0
    }

    pub fn set(&mut self, part: PlayerModelPart, shown: bool) {
        if shown {
            self.0 |= part.mask();
        } else {
            self.0 &= !part.mask();
        }
    }
}

/// What `PlayerModel.setupAnim` and `AvatarRenderer` read of
#[derive(Clone, Copy, Debug, Default)]
pub struct AvatarRenderState {
    pub humanoid: HumanoidRenderState,
    /// Feet, in the world.
    pub position: Vec3,
    /// `bodyRot` in Melon's convention (0 faces north), degrees.
    pub body_yaw: f32,
    /// Spectators show only their head.
    pub spectator: bool,
    pub shown: ShownParts,
    /// `deathTime` with the partial tick, 0 while alive.
    pub death_time: f32,
    /// Swimming: how far down, and how the body lies in the water.
    pub swim: Swim,
    /// Drawn upside down, with the body's height (see
    pub upside_down: Option<f32>,
    /// `LivingEntity.getScale`.
    pub scale: f32,
}

impl AvatarRenderState {
    /// `AvatarRenderer.isEntityUpsideDown`: a player named `Dinnerbone` or
    pub fn wants_upside_down(name: &str, shown: ShownParts) -> bool {
        shown.shows(PlayerModelPart::Cape) && Magic::of(name) == Magic::UpsideDown
    }

    /// Turns the body over, its head's angles inverted
    pub fn turn_upside_down(&mut self, height: f32) {
        self.upside_down = Some(height);
        self.humanoid.x_rot = -self.humanoid.x_rot;
        self.humanoid.y_rot = -self.humanoid.y_rot;
    }
}

/// `HumanoidArm`: which arm, for the hand-held items, the first-person arm
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Arm {
    #[default]
    Right,
    Left,
}

impl Arm {
    pub fn opposite(self) -> Self {
        match self {
            Self::Right => Self::Left,
            Self::Left => Self::Right,
        }
    }

    /// `getSerializedName`, as `options.txt` stores it.
    pub fn id(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "right" => Some(Self::Right),
            "left" => Some(Self::Left),
            _ => None,
        }
    }

    /// `caption`'s translation key.
    pub fn caption_key(self) -> &'static str {
        match self {
            Self::Right => "options.mainHand.right",
            Self::Left => "options.mainHand.left",
        }
    }

    /// The id `ClientInformation` sends: 0 left, 1 right.
    pub fn network_id(self) -> i32 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

/// A baked `PlayerModel` of one arm width.
#[derive(Clone, Debug)]
pub struct PlayerModel {
    pub model: Model,
    pub kind: PlayerModelType,
    pub humanoid: HumanoidParts,
    pub jacket: PartId,
    pub left_sleeve: PartId,
    pub right_sleeve: PartId,
    pub left_pants: PartId,
    pub right_pants: PartId,
}

impl PlayerModel {
    pub fn new(kind: PlayerModelType) -> Self {
        let model = Model::bake(&create_layer(kind));
        Self {
            humanoid: HumanoidParts::find(&model),
            jacket: model.part("jacket"),
            left_sleeve: model.part("left_sleeve"),
            right_sleeve: model.part("right_sleeve"),
            left_pants: model.part("left_pants"),
            right_pants: model.part("right_pants"),
            model,
            kind,
        }
    }

    /// `PlayerModel.setupAnim`: a fresh pose for `state`.
    pub fn pose(&self, state: &AvatarRenderState) -> ModelPose {
        let mut pose = self.model.initial_pose();
        let h = &self.humanoid;
        let body = !state.spectator;
        for part in [h.body, h.right_arm, h.left_arm, h.right_leg, h.left_leg] {
            pose[part].visible = body;
        }
        let shown = state.shown;
        for (part, model_part) in [
            (h.hat, PlayerModelPart::Hat),
            (self.jacket, PlayerModelPart::Jacket),
            (self.left_pants, PlayerModelPart::LeftPantsLeg),
            (self.right_pants, PlayerModelPart::RightPantsLeg),
            (self.left_sleeve, PlayerModelPart::LeftSleeve),
            (self.right_sleeve, PlayerModelPart::RightSleeve),
        ] {
            pose[part].visible = shown.shows(model_part);
        }
        humanoid::setup_anim(h, &mut pose, &state.humanoid);
        pose
    }

    /// From model pixels to the world: [`living::root_transform`] with
    pub fn root_transform(state: &AvatarRenderState) -> Mat4 {
        let crouch = if state.humanoid.crouching {
            -2.0 / 16.0
        } else {
            0.0
        };
        let living = LivingRenderState {
            position: state.position + Vec3::new(0.0, crouch, 0.0),
            body_rot: state.body_yaw,
            x_rot: state.humanoid.x_rot,
            death_time: state.death_time,
            swim: state.swim,
            upside_down: state.upside_down,
            scale: state.scale,
            ..Default::default()
        };
        living::root_transform(
            &living,
            Placement {
                scale: Vec3::splat(0.9375),
                ..Default::default()
            },
        )
    }

    /// The skin's outer layer (`PlayerModelPart`s except the cape).
    pub fn overlay_parts(&self) -> [PartId; 6] {
        [
            self.humanoid.hat,
            self.jacket,
            self.left_sleeve,
            self.right_sleeve,
            self.left_pants,
            self.right_pants,
        ]
    }

    pub fn arm(&self, arm: Arm) -> PartId {
        self.humanoid.arm(arm)
    }

    /// `PlayerModel.translateToHand`: the arm's transform, a slim arm's
    pub fn hand_transform(&self, pose: &ModelPose, arm: Arm, root: Mat4) -> Mat4 {
        let outward = match self.kind {
            PlayerModelType::Slim => 0.5,
            PlayerModelType::Wide => 0.0,
        };
        humanoid::hand_transform(&self.model, &self.humanoid, pose, arm, outward, root)
    }

    /// `AvatarRenderer.renderHand`: the arm alone, reset to its
    pub fn first_person_arm_pose(&self, arm: Arm, sleeve: bool) -> ModelPose {
        let mut pose = self.model.initial_pose();
        pose[self.left_sleeve].visible = sleeve;
        pose[self.right_sleeve].visible = sleeve;
        let part = self.arm(arm);
        pose[part].pose.rotation.z = match arm {
            Arm::Right => 0.1,
            Arm::Left => -0.1,
        };
        pose
    }
}

/// `ItemInHandLayer.submitArmWithItem` from the hand: where a held item's
pub fn item_in_hand(hand: Mat4, arm: Arm) -> Mat4 {
    let side = match arm {
        Arm::Right => 1.0,
        Arm::Left => -1.0,
    };
    hand * Mat4::rot_x_deg(-90.0)
        * Mat4::rot_y_deg(180.0f32)
        * Mat4::from_translation(Vec3::new(side / 16.0, 0.125, -0.625))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steve_and_alex_differ_only_in_their_arms() {
        let wide = PlayerModel::new(PlayerModelType::Wide);
        let slim = PlayerModel::new(PlayerModelType::Slim);
        assert_eq!(wide.model.len(), slim.model.len());
        let width = |m: &PlayerModel, part| {
            let vertices = m.model.part_vertices(part);
            let xs = vertices.iter().map(|v| v.pos[0]);
            let (min, max) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
            ((max - min) * 16.0).round()
        };
        assert_eq!(width(&wide, wide.humanoid.right_arm), 4.0);
        assert_eq!(width(&slim, slim.humanoid.right_arm), 3.0);
        assert_eq!(width(&slim, slim.humanoid.left_arm), 3.0);
        assert_eq!(width(&slim, slim.humanoid.body), 8.0);
    }

    #[test]
    fn the_outer_layer_hangs_off_the_inner_parts() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let m = &player.model;
        assert!(m.is_within(player.right_sleeve, player.humanoid.right_arm));
        assert!(m.is_within(player.left_pants, player.humanoid.left_leg));
        assert!(m.is_within(player.jacket, player.humanoid.body));
        assert!(m.is_within(player.humanoid.hat, player.humanoid.head));
    }

    #[test]
    fn a_standing_player_has_feet_on_the_ground_and_head_at_1_8() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let state = AvatarRenderState::default();
        let pose = player.pose(&state);
        let matrices = player
            .model
            .part_matrices(&pose, PlayerModel::root_transform(&state));
        let ys = |part: PartId| {
            let m = matrices[part.0].unwrap();
            player
                .model
                .part_vertices(part)
                .iter()
                .map(|v| m.transform_point3(Vec3::from_array(v.pos)).y)
                .fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)))
        };
        let (feet, _) = ys(player.humanoid.right_leg);
        let (_, top) = ys(player.humanoid.head);
        assert!(feet.abs() < 0.01, "{feet}");
        assert!((top - 1.875).abs() < 0.01, "{top}");
    }

    #[test]
    fn the_hand_is_the_arm_half_a_pixel_further_out_on_slim_models() {
        let living = LivingRenderState {
            walk_position: 3.0,
            walk_speed: 0.8,
            x_rot: 20.0,
            ..Default::default()
        };
        let state = AvatarRenderState {
            humanoid: HumanoidRenderState::from_living(&living),
            body_yaw: 30.0,
            ..Default::default()
        };
        for (kind, outward) in [(PlayerModelType::Wide, 0.0), (PlayerModelType::Slim, 0.5)] {
            let player = PlayerModel::new(kind);
            let pose = player.pose(&state);
            let root = PlayerModel::root_transform(&state);
            let matrices = player.model.part_matrices(&pose, root);
            for (arm, side) in [(Arm::Right, 1.0), (Arm::Left, -1.0)] {
                let part = player.humanoid.arm(arm);
                let parent = player.model.parent_transform(&pose, part, root);
                let mut expected = matrices[part.0].unwrap();
                expected.w_axis += parent.x_axis * (side * outward / 16.0);
                let hand = player.hand_transform(&pose, arm, root);
                assert!(hand.abs_diff_eq(expected, 1e-6), "{kind:?} {arm:?}");
            }
        }
    }

    #[test]
    fn the_body_faces_where_the_player_looks() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let state = AvatarRenderState {
            body_yaw: 90.0,
            ..Default::default()
        };
        let pose = player.pose(&state);
        let root = PlayerModel::root_transform(&state);
        let head = player
            .model
            .part_transform(&pose, player.humanoid.head, root);
        let face = head.transform_point3(Vec3::new(0.0, -4.0, -4.0) / 16.0);
        let back = head.transform_point3(Vec3::new(0.0, -4.0, 4.0) / 16.0);
        assert!(face.x > back.x, "{face} {back}");
    }

    #[test]
    fn spectators_show_only_the_head() {
        let player = PlayerModel::new(PlayerModelType::Slim);
        let pose = player.pose(&AvatarRenderState {
            spectator: true,
            ..Default::default()
        });
        let matrices = player.model.part_matrices(&pose, Mat4::IDENTITY);
        assert!(matrices[player.humanoid.head.0].is_some());
        assert!(matrices[player.humanoid.hat.0].is_some());
        assert!(matrices[player.humanoid.body.0].is_none());
        assert!(matrices[player.right_sleeve.0].is_none());
    }

    #[test]
    fn hidden_parts_leave_the_model() {
        let player = PlayerModel::new(PlayerModelType::Wide);
        let mut shown = ShownParts::default();
        shown.set(PlayerModelPart::Jacket, false);
        shown.set(PlayerModelPart::LeftSleeve, false);
        let pose = player.pose(&AvatarRenderState {
            shown,
            ..Default::default()
        });
        assert!(!pose[player.jacket].visible);
        assert!(!pose[player.left_sleeve].visible);
        assert!(pose[player.right_sleeve].visible);
        assert!(pose[player.humanoid.hat].visible);
        assert_eq!(shown.mask(), 0b111_1001);
        assert_eq!(
            player.overlay_parts(),
            [
                player.humanoid.hat,
                player.jacket,
                player.left_sleeve,
                player.right_sleeve,
                player.left_pants,
                player.right_pants,
            ]
        );
    }

    #[test]
    fn default_skins_follow_the_uuid_hash() {
        assert_eq!(DEFAULT_SKINS[default_skin(uuid::Uuid::nil())].0, "alex");
        let id = uuid::Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        assert_eq!(default_skin(id), default_skin(id));
        assert!(default_skin(id) < 18);
        assert_eq!(
            default_skin_texture(15),
            "minecraft:entity/player/wide/steve.png"
        );
    }
}
