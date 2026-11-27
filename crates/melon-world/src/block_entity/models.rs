//! The block entity models, transcribed from their `createBodyLayer`s.

use glam::Vec2;

use crate::block::direction::{Direction, Directions};
use crate::math::angle::{HALF_PI, PI, SIXTH_PI};
use crate::model_part::{Cube, Part, Pose};

/// `ChestType`: which half of a double chest, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChestHalf {
    Single,
    Left,
    Right,
}

/// `ChestModel`: 64x64.
pub struct ChestModel {
    pub bottom: Part,
    pub lid: Part,
    pub lock: Part,
}

pub const CHEST_TEXTURE: Vec2 = Vec2::splat(64.0);

/// `createSingleBodyLayer` / `createDoubleBodyLeftLayer` / `...Right...`.
pub fn chest(half: ChestHalf) -> ChestModel {
    let all_but = |d: Direction| Directions(0b11_1111 & !d.bit());
    let (x, width, lock_x, lock_w, faces) = match half {
        ChestHalf::Single => (1.0, 14.0, 7.0, 2.0, Directions(0b11_1111)),
        ChestHalf::Right => (1.0, 15.0, 15.0, 1.0, all_but(Direction::East)),
        ChestHalf::Left => (0.0, 15.0, 0.0, 1.0, all_but(Direction::West)),
    };
    let hinge = Pose::offset(0.0, 9.0, 1.0);
    ChestModel {
        bottom: Part::new(
            Pose::ZERO,
            vec![Cube::new((0.0, 19.0), (x, 0.0, 1.0), (width, 10.0, 14.0)).only(faces)],
        ),
        lid: Part::new(
            hinge,
            vec![Cube::new((0.0, 0.0), (x, 0.0, 0.0), (width, 5.0, 14.0)).only(faces)],
        ),
        lock: Part::new(
            hinge,
            vec![Cube::new((0.0, 0.0), (lock_x, -2.0, 14.0), (lock_w, 4.0, 1.0)).only(faces)],
        ),
    }
}

/// `ShulkerBoxRenderer.ShulkerBoxModel` (`ShulkerModel.createBoxLayer`).
pub struct ShulkerModel {
    pub lid: Part,
    pub base: Part,
}

pub const SHULKER_TEXTURE: Vec2 = Vec2::splat(64.0);

pub fn shulker_box() -> ShulkerModel {
    ShulkerModel {
        lid: Part::new(
            Pose::offset(0.0, 24.0, 0.0),
            vec![Cube::new(
                (0.0, 0.0),
                (-8.0, -16.0, -8.0),
                (16.0, 12.0, 16.0),
            )],
        ),
        base: Part::new(
            Pose::offset(0.0, 24.0, 0.0),
            vec![Cube::new(
                (0.0, 28.0),
                (-8.0, -8.0, -8.0),
                (16.0, 8.0, 16.0),
            )],
        ),
    }
}

/// `BannerModel` and `BannerFlagModel`: 64x64.
pub struct BannerModel {
    /// Pole (standing only) and bar.
    pub frame: Part,
    pub flag: Part,
}

pub const BANNER_TEXTURE: Vec2 = Vec2::splat(64.0);

pub fn banner(standing: bool) -> BannerModel {
    let mut frame = Vec::new();
    if standing {
        frame.push(Cube::new(
            (44.0, 0.0),
            (-1.0, -42.0, -1.0),
            (2.0, 42.0, 2.0),
        ));
    }
    let (bar_y, bar_z) = if standing {
        (-44.0, -1.0)
    } else {
        (-20.5, 9.5)
    };
    frame.push(Cube::new(
        (0.0, 42.0),
        (-10.0, bar_y, bar_z),
        (20.0, 2.0, 2.0),
    ));
    let flag_pose = if standing {
        Pose::offset(0.0, -44.0, 0.0)
    } else {
        Pose::offset(0.0, -20.5, 10.5)
    };
    BannerModel {
        frame: Part::new(Pose::ZERO, frame),
        flag: Part::new(
            flag_pose,
            vec![Cube::new((0.0, 0.0), (-10.0, 0.0, -2.0), (20.0, 40.0, 1.0))],
        ),
    }
}

/// `SkullBlock.Types`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SkullType {
    Skeleton,
    WitherSkeleton,
    Zombie,
    Creeper,
    Dragon,
    Piglin,
    Player,
}

impl SkullType {
    /// `SkullBlockRenderer.SKIN_BY_TYPE`.
    pub fn texture(self) -> &'static str {
        match self {
            SkullType::Skeleton => "minecraft:entity/skeleton/skeleton",
            SkullType::WitherSkeleton => "minecraft:entity/skeleton/wither_skeleton",
            SkullType::Zombie => "minecraft:entity/zombie/zombie",
            SkullType::Creeper => "minecraft:entity/creeper/creeper",
            SkullType::Dragon => "minecraft:entity/enderdragon/dragon",
            SkullType::Piglin => "minecraft:entity/piglin/piglin",
            SkullType::Player => "minecraft:entity/player/wide/steve",
        }
    }

    /// The head an item is (`AbstractSkullBlock.getType` of its block).
    pub fn of_item(key: &str) -> Option<Self> {
        Some(match key {
            "skeleton_skull" => SkullType::Skeleton,
            "wither_skeleton_skull" => SkullType::WitherSkeleton,
            "zombie_head" => SkullType::Zombie,
            "creeper_head" => SkullType::Creeper,
            "dragon_head" => SkullType::Dragon,
            "piglin_head" => SkullType::Piglin,
            "player_head" => SkullType::Player,
            _ => return None,
        })
    }

    /// Every type.
    pub const ALL: [SkullType; 7] = [
        SkullType::Skeleton,
        SkullType::WitherSkeleton,
        SkullType::Zombie,
        SkullType::Creeper,
        SkullType::Dragon,
        SkullType::Piglin,
        SkullType::Player,
    ];

    /// The model's texture size (`LayerDefinitions`).
    pub fn texture_size(self) -> Vec2 {
        match self {
            SkullType::Skeleton | SkullType::WitherSkeleton | SkullType::Creeper => {
                Vec2::new(64.0, 32.0)
            }
            SkullType::Dragon => Vec2::splat(256.0),
            SkullType::Zombie | SkullType::Piglin | SkullType::Player => Vec2::splat(64.0),
        }
    }
}

/// A head's root part, with its animated children reachable by index.
pub fn skull(skull: SkullType) -> Part {
    match skull {
        SkullType::Skeleton | SkullType::WitherSkeleton | SkullType::Creeper => {
            Part::new(Pose::ZERO, vec![head_box()])
        }
        SkullType::Zombie | SkullType::Player => Part::new(
            Pose::ZERO,
            vec![
                head_box(),
                Cube::new((32.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0)).grown(0.25),
            ],
        ),
        SkullType::Dragon => dragon_head(),
        SkullType::Piglin => piglin_head(),
    }
}

fn head_box() -> Cube {
    Cube::new((0.0, 0.0), (-4.0, -8.0, -4.0), (8.0, 8.0, 8.0))
}

/// `DragonHeadModel.createHeadLayer`.
fn dragon_head() -> Part {
    let named =
        |u: f32, v: f32, min: (f32, f32, f32), size: (f32, f32, f32)| Cube::new((u, v), min, size);
    Part::new(
        Pose::offset(0.0, -7.986_666, 0.0).scaled(0.75),
        vec![
            named(176.0, 44.0, (-6.0, -1.0, -24.0), (12.0, 5.0, 16.0)),
            named(112.0, 30.0, (-8.0, -8.0, -10.0), (16.0, 16.0, 16.0)),
            named(0.0, 0.0, (-5.0, -12.0, -4.0), (2.0, 4.0, 6.0)).mirrored(),
            named(112.0, 0.0, (-5.0, -3.0, -22.0), (2.0, 2.0, 4.0)).mirrored(),
            named(0.0, 0.0, (3.0, -12.0, -4.0), (2.0, 4.0, 6.0)),
            named(112.0, 0.0, (3.0, -3.0, -22.0), (2.0, 2.0, 4.0)),
        ],
    )
    .with_children(vec![Part::new(
        Pose::offset(0.0, 4.0, -8.0),
        vec![named(176.0, 65.0, (-6.0, 0.0, -16.0), (12.0, 4.0, 16.0))],
    )])
}

/// `AbstractPiglinModel.addHead`: ears `children[0]` (left) and `[1]`.
fn piglin_head() -> Part {
    Part::new(
        Pose::ZERO,
        vec![
            Cube::new((0.0, 0.0), (-5.0, -8.0, -4.0), (10.0, 8.0, 8.0)),
            Cube::new((31.0, 1.0), (-2.0, -4.0, -5.0), (4.0, 4.0, 1.0)),
            Cube::new((2.0, 4.0), (2.0, -2.0, -5.0), (1.0, 2.0, 1.0)),
            Cube::new((2.0, 0.0), (-3.0, -2.0, -5.0), (1.0, 2.0, 1.0)),
        ],
    )
    .with_children(vec![
        Part::new(
            Pose::offset_rotation((4.5, -6.0, 0.0), (0.0, 0.0, -SIXTH_PI)),
            vec![Cube::new((51.0, 6.0), (0.0, 0.0, -2.0), (1.0, 5.0, 4.0))],
        ),
        Part::new(
            Pose::offset_rotation((-4.5, -6.0, 0.0), (0.0, 0.0, SIXTH_PI)),
            vec![Cube::new((39.0, 6.0), (-1.0, 0.0, -2.0), (1.0, 5.0, 4.0))],
        ),
    ])
}

/// `BellModel`: 32x32, the swinging body with its rim as a child.
pub const BELL_TEXTURE: Vec2 = Vec2::splat(32.0);

pub fn bell() -> Part {
    Part::new(
        Pose::offset(8.0, 12.0, 8.0),
        vec![Cube::new((0.0, 0.0), (-3.0, -6.0, -3.0), (6.0, 7.0, 6.0))],
    )
    .with_children(vec![Part::new(
        Pose::offset(-8.0, -12.0, -8.0),
        vec![Cube::new((0.0, 13.0), (4.0, 4.0, 4.0), (8.0, 2.0, 8.0))],
    )])
}

/// `BookModel`: 64x32.
pub const BOOK_TEXTURE: Vec2 = Vec2::new(64.0, 32.0);

pub fn book() -> Vec<Part> {
    let page = || vec![Cube::new((24.0, 10.0), (0.0, -4.0, 0.0), (5.0, 8.0, 0.005))];
    vec![
        Part::new(
            Pose::offset(0.0, 0.0, -1.0),
            vec![Cube::new(
                (0.0, 0.0),
                (-6.0, -5.0, -0.005),
                (6.0, 10.0, 0.005),
            )],
        ),
        Part::new(
            Pose::offset(0.0, 0.0, 1.0),
            vec![Cube::new(
                (16.0, 0.0),
                (0.0, -5.0, -0.005),
                (6.0, 10.0, 0.005),
            )],
        ),
        Part::new(
            Pose::offset_rotation((0.0, 0.0, 0.0), (0.0, HALF_PI, 0.0)),
            vec![Cube::new(
                (12.0, 0.0),
                (-1.0, -5.0, 0.0),
                (2.0, 10.0, 0.005),
            )],
        ),
        Part::new(
            Pose::ZERO,
            vec![Cube::new((0.0, 10.0), (0.0, -4.0, -0.99), (5.0, 8.0, 1.0))],
        ),
        Part::new(
            Pose::ZERO,
            vec![Cube::new((12.0, 10.0), (0.0, -4.0, -0.01), (5.0, 8.0, 1.0))],
        ),
        Part::new(Pose::ZERO, page()),
        Part::new(Pose::ZERO, page()),
    ]
}

/// `DecoratedPotRenderer.createBaseLayer` (32x32).
pub const POT_BASE_TEXTURE: Vec2 = Vec2::splat(32.0);
pub const POT_SIDE_TEXTURE: Vec2 = Vec2::splat(16.0);

pub fn pot_base() -> Vec<Part> {
    let plane = || vec![Cube::new((-14.0, 13.0), (0.0, 0.0, 0.0), (14.0, 0.0, 14.0))];
    vec![
        Part::new(
            Pose::offset_rotation((0.0, 37.0, 16.0), (PI, 0.0, 0.0)),
            vec![
                Cube::new((0.0, 0.0), (4.0, 17.0, 4.0), (8.0, 3.0, 8.0)).grown(-0.1),
                Cube::new((0.0, 5.0), (5.0, 20.0, 5.0), (6.0, 1.0, 6.0)).grown(0.2),
            ],
        ),
        Part::new(Pose::offset(1.0, 16.0, 1.0), plane()),
        Part::new(Pose::offset(1.0, 0.0, 1.0), plane()),
    ]
}

pub fn pot_sides() -> Vec<Part> {
    let side = || {
        vec![
            Cube::new((1.0, 0.0), (0.0, 0.0, 0.0), (14.0, 16.0, 0.0))
                .only(Directions(Direction::North.bit())),
        ]
    };
    vec![
        Part::new(
            Pose::offset_rotation((15.0, 16.0, 1.0), (0.0, 0.0, PI)),
            side(),
        ),
        Part::new(
            Pose::offset_rotation((1.0, 16.0, 1.0), (0.0, -HALF_PI, PI)),
            side(),
        ),
        Part::new(
            Pose::offset_rotation((15.0, 16.0, 15.0), (0.0, HALF_PI, PI)),
            side(),
        ),
        Part::new(
            Pose::offset_rotation((1.0, 16.0, 15.0), (PI, 0.0, 0.0)),
            side(),
        ),
    ]
}

/// `ConduitRenderer.createShellLayer`: 32x16.
pub const CONDUIT_TEXTURE: Vec2 = Vec2::new(32.0, 16.0);

pub fn conduit_shell() -> Part {
    Part::new(
        Pose::ZERO,
        vec![Cube::new((0.0, 0.0), (-3.0, -3.0, -3.0), (6.0, 6.0, 6.0))],
    )
}

pub const SHIELD_TEXTURE: Vec2 = Vec2::splat(64.0);

pub const TRIDENT_TEXTURE: Vec2 = Vec2::splat(32.0);

/// `TridentModel.createLayer`.
pub fn trident() -> [Part; 5] {
    let part = |cube: Cube| Part::new(Pose::ZERO, vec![cube]);
    [
        part(Cube::new((0.0, 6.0), (-0.5, 2.0, -0.5), (1.0, 25.0, 1.0))),
        part(Cube::new((4.0, 0.0), (-1.5, 0.0, -0.5), (3.0, 2.0, 1.0))),
        part(Cube::new((4.0, 3.0), (-2.5, -3.0, -0.5), (1.0, 4.0, 1.0))),
        part(Cube::new((0.0, 0.0), (-0.5, -4.0, -0.5), (1.0, 4.0, 1.0))),
        part(Cube::new((4.0, 3.0), (1.5, -3.0, -0.5), (1.0, 4.0, 1.0)).mirrored()),
    ]
}

/// `ShieldModel.createLayer`: the plate and the handle behind it.
pub fn shield() -> [Part; 2] {
    [
        Part::new(
            Pose::ZERO,
            vec![Cube::new(
                (0.0, 0.0),
                (-6.0, -11.0, -2.0),
                (12.0, 22.0, 1.0),
            )],
        ),
        Part::new(
            Pose::ZERO,
            vec![Cube::new((26.0, 0.0), (-1.0, -3.0, -1.0), (2.0, 6.0, 6.0))],
        ),
    ]
}
