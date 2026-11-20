//! Building the block entities' geometry: those at rest once, the moving

use super::anim::pot_duration;
use super::models::{ChestHalf, SkullType};
use super::{
    Anim, BANNER_BASE, BANNER_SPRITES, BELL_BODY, BOOK, BlockEntities, Book, CONDUIT_SHELL,
    END_PORTAL, Kind, POT_BASE, POT_SIDE, Placement, VIEW_DISTANCE, kind_of, models, wrap_radians,
};
use crate::math::angle::{self, HALF_PI, PI, Rotation, TAU};
use crate::renderer::WorldVertex as Vertex;
use crate::world::block::direction::Direction;
use crate::world::block_models::BlockModels;
use crate::world::culling::Culler;
use crate::world::entity::skull;
use crate::world::level::Level;
use crate::world::model_part::{Sprite, emit};
use glam::{DVec3, Mat4, Vec2, Vec3};

/// `BuiltInBlockModels`' special models of a block drawn away from its
/// block entity (in a minecart, as a falling block or a display): a chest
/// or shulker box shut, a banner's plain flag, a pot without sherds, at
/// the origin. `false` for a state without one of these.
pub fn at_rest(state: u16, models: &BlockModels, out: &mut Vec<Vertex>) -> bool {
    let Some(kind) = kind_of(state) else {
        return false;
    };
    if !matches!(
        kind,
        Kind::Chest { .. } | Kind::Shulker { .. } | Kind::Banner { .. } | Kind::DecoratedPot { .. }
    ) {
        return false;
    }
    BlockEntities::default().build_one([0, 0, 0], kind, models, 0.0, out);
    true
}

/// The middle of the block at `pos`.
fn block_center(pos: [i32; 3]) -> DVec3 {
    DVec3::from_array(pos.map(f64::from)) + DVec3::splat(0.5)
}

impl BlockEntities {
    /// Builds the block entities at rest into `out` (world positions):
    pub fn build_still(
        &self,
        level: &Level,
        models: &BlockModels,
        center: DVec3,
        out: &mut Vec<Vertex>,
    ) -> usize {
        let mut built = 0;
        for pos in level.block_entities() {
            let Some(kind) = level.block_state(pos[0], pos[1], pos[2]).and_then(kind_of) else {
                continue;
            };
            let middle = block_center(pos);
            if middle.distance_squared(center) as f32 > VIEW_DISTANCE * VIEW_DISTANCE
                || self.is_moving(pos, kind)
                || drawn_with_skin(level, pos, kind)
            {
                continue;
            }
            built += 1;
            build_lit(level, pos, out, |out| {
                self.build_one(pos, kind, models, 0.0, out)
            });
        }
        built
    }

    /// Builds the moving block entities `culler` lets through into `out`
    pub fn build_moving(
        &self,
        level: &Level,
        models: &BlockModels,
        culler: &Culler,
        eye: DVec3,
        partial: f32,
        out: &mut Vec<Vertex>,
    ) -> usize {
        let mut drawn = 0;
        for pos in level.block_entities() {
            let Some(kind) = level.block_state(pos[0], pos[1], pos[2]).and_then(kind_of) else {
                continue;
            };
            let center = block_center(pos);
            if !self.is_moving(pos, kind)
                || drawn_with_skin(level, pos, kind)
                || center.distance_squared(eye) as f32 > VIEW_DISTANCE * VIEW_DISTANCE
                || !culler.sees_box(center - DVec3::ONE, center + DVec3::ONE)
                || !culler.reaches(culler.section_of(center))
            {
                continue;
            }
            drawn += 1;
            build_lit(level, pos, out, |out| {
                self.build_one(pos, kind, models, partial, out);
            });
        }
        drawn
    }

    /// Whether the block entity at `pos` looks different from one frame to
    pub(super) fn is_moving(&self, pos: [i32; 3], kind: &Kind) -> bool {
        let runs = |pos| {
            self.anims
                .get(&pos)
                .is_some_and(|anim| anim.is_running(self.ticks))
        };
        match kind {
            Kind::Banner { .. } | Kind::EnchantingTable => true,
            Kind::Chest { half, facing, .. } => {
                runs(pos) || chest_partner(pos, *half, *facing).is_some_and(runs)
            }
            Kind::Conduit | Kind::EndPortal | Kind::EndGateway => false,
            Kind::Shulker { .. } | Kind::Skull { .. } | Kind::Bell | Kind::DecoratedPot { .. } => {
                runs(pos)
            }
        }
    }

    /// One block entity at `pos` into `out`.
    fn build_one(
        &self,
        pos: [i32; 3],
        kind: &Kind,
        models: &BlockModels,
        partial: f32,
        out: &mut Vec<Vertex>,
    ) {
        let sprite = |id: &str, size: Vec2| Sprite {
            rect: models.sprite(id).unwrap_or([0.0; 4]),
            size,
        };
        let min = Vec3::new(pos[0] as f32, pos[1] as f32, pos[2] as f32);
        let at = Mat4::from_translation(min);
        match kind {
            Kind::Chest {
                half,
                facing,
                sprite: texture,
                ..
            } => {
                let mut open = self.lid(pos, partial);
                if let Some(partner) = chest_partner(pos, *half, *facing) {
                    open = open.max(self.lid(partner, partial));
                }
                let open = 1.0 - (1.0 - open).powi(3);
                let mut model = models::chest(*half);
                model.lid.pose.rotation.x = -(open * HALF_PI);
                model.lock.pose.rotation.x = model.lid.pose.rotation.x;
                let transform = at
                    * around(
                        Vec3::new(0.5, 0.0, 0.5),
                        Mat4::rot_y(-facing.y_rot().to_radians()),
                    );
                let sprite = sprite(texture, models::CHEST_TEXTURE);
                for part in [&model.bottom, &model.lid, &model.lock] {
                    emit(part, transform, &sprite, [1.0; 3], out);
                }
            }
            Kind::Shulker { texture, facing } => {
                let progress = self.lid(pos, partial);
                let mut model = models::shulker_box();
                model.lid.pose.offset.y = 24.0 - progress * 0.5 * 16.0;
                model.lid.pose.rotation.y = 270.0f32.to_radians() * progress;
                let transform = at
                    * Mat4::from_translation(Vec3::splat(0.5))
                    * Mat4::from_scale(Vec3::splat(0.9995))
                    * facing_rotation(*facing)
                    * Mat4::from_scale(Vec3::new(1.0, -1.0, -1.0))
                    * Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0));
                let sprite = sprite(texture, models::SHULKER_TEXTURE);
                emit(&model.base, transform, &sprite, [1.0; 3], out);
                emit(&model.lid, transform, &sprite, [1.0; 3], out);
            }
            Kind::Banner { color, placement } => {
                let standing = matches!(placement, Placement::Ground(_));
                let angle = match placement {
                    Placement::Ground(segment) => f32::from(*segment) * 22.5,
                    Placement::Wall(facing) => facing.y_rot(),
                };
                let transform = at
                    * Mat4::from_translation(Vec3::new(0.5, 0.0, 0.5))
                    * Mat4::rot_y(-angle.to_radians())
                    * Mat4::from_scale(Vec3::new(2.0, -2.0, -2.0) / 3.0);
                let mut model = models::banner(standing);
                let clock = (pos[0] as i64 * 7
                    + pos[1] as i64 * 9
                    + pos[2] as i64 * 13
                    + self.ticks as i64)
                    .rem_euclid(100) as f32;
                let phase = (clock + partial) / 100.0;
                model.flag.pose.rotation.x = (-0.0125 + 0.01 * angle::cos(TAU * phase)) * PI;
                emit(
                    &model.frame,
                    transform,
                    &sprite(BANNER_BASE, models::BANNER_TEXTURE),
                    [1.0; 3],
                    out,
                );
                emit(
                    &model.flag,
                    transform,
                    &sprite(&BANNER_SPRITES[*color], models::BANNER_TEXTURE),
                    [1.0; 3],
                    out,
                );
            }
            Kind::Skull {
                skull, placement, ..
            } => {
                let transform = at * skull_transform(*placement);
                let mut part = models::skull(*skull);
                let animation = match self.anims.get(&pos) {
                    Some(&Anim::Skull { ticks, animating }) => {
                        skull::placed_animation(ticks, animating, partial)
                    }
                    _ => 0.0,
                };
                skull::animate_part(*skull, &mut part, animation);
                emit(
                    &part,
                    transform,
                    &sprite(skull.texture(), skull.texture_size()),
                    [1.0; 3],
                    out,
                );
            }
            Kind::Bell => {
                let mut body = models::bell();
                if let Some(Anim::Bell {
                    ticks,
                    shaking: true,
                    direction,
                }) = self.anims.get(&pos)
                {
                    let t = *ticks as f32 + partial;
                    let swing = angle::sin(t / PI) / (4.0 + t / 3.0);
                    match direction {
                        Direction::North => body.pose.rotation.x = -swing,
                        Direction::South => body.pose.rotation.x = swing,
                        Direction::East => body.pose.rotation.z = -swing,
                        Direction::West => body.pose.rotation.z = swing,
                        _ => {}
                    }
                }
                emit(
                    &body,
                    at,
                    &sprite(BELL_BODY, models::BELL_TEXTURE),
                    [1.0; 3],
                    out,
                );
            }
            Kind::EnchantingTable => {
                let book = match self.anims.get(&pos) {
                    Some(Anim::Book(book)) => *book,
                    _ => Book::default(),
                };
                self.build_book(&book, at, partial, &sprite(BOOK, models::BOOK_TEXTURE), out);
            }
            Kind::DecoratedPot { facing } => {
                let mut transform =
                    at * around(Vec3::splat(0.5), Mat4::rot_y_deg(180.0 - facing.y_rot()));
                if let Some(Anim::Pot { started, negative }) = self.anims.get(&pos) {
                    transform *= self.pot_wobble(*started, *negative, partial);
                }
                let base = sprite(POT_BASE, models::POT_BASE_TEXTURE);
                for part in models::pot_base() {
                    emit(&part, transform, &base, [1.0; 3], out);
                }
                let side = sprite(POT_SIDE, models::POT_SIDE_TEXTURE);
                for part in models::pot_sides() {
                    emit(&part, transform, &side, [1.0; 3], out);
                }
            }
            Kind::Conduit => {
                let transform = at * Mat4::from_translation(Vec3::splat(0.5));
                emit(
                    &models::conduit_shell(),
                    transform,
                    &sprite(CONDUIT_SHELL, models::CONDUIT_TEXTURE),
                    [1.0; 3],
                    out,
                );
            }
            Kind::EndPortal | Kind::EndGateway => {
                let rect = models.sprite(END_PORTAL).unwrap_or([0.0; 4]);
                end_cube(min, matches!(kind, Kind::EndGateway), rect, out);
            }
        }
    }

    /// `EnchantTableRenderer.submit` with `BookModel.setupAnim`.
    pub(super) fn build_book(
        &self,
        book: &Book,
        at: Mat4,
        partial: f32,
        sprite: &Sprite,
        out: &mut Vec<Vertex>,
    ) {
        let lerp = |a: f32, b: f32| a + (b - a) * partial;
        let time = book.time as f32 + partial;
        let flip = lerp(book.old_flip, book.flip);
        let open = lerp(book.old_open, book.open);
        let y_rot = book.old_rot + wrap_radians(book.rot - book.old_rot) * partial;
        let transform =
            at * Mat4::from_translation(Vec3::new(
                0.5,
                0.75 + 0.1 + angle::sin(time * 0.1) * 0.01,
                0.5,
            )) * Mat4::rot_y(-y_rot)
                * Mat4::rot_z_deg(80.0f32);
        let frac = |v: f32| v - v.floor();
        let flip1 = (frac(flip + 0.25) * 1.6 - 0.3).clamp(0.0, 1.0);
        let flip2 = (frac(flip + 0.75) * 1.6 - 0.3).clamp(0.0, 1.0);
        let openness = (angle::sin(time * 0.02) * 0.1 + 1.25) * open;
        let mut parts = models::book();
        parts[0].pose.rotation.y = PI + openness;
        parts[1].pose.rotation.y = -openness;
        parts[3].pose.rotation.y = openness;
        parts[4].pose.rotation.y = -openness;
        parts[5].pose.rotation.y = openness - openness * 2.0 * flip1;
        parts[6].pose.rotation.y = openness - openness * 2.0 * flip2;
        for part in &mut parts[3..] {
            part.pose.offset.x = angle::sin(openness);
        }
        for part in &parts {
            emit(part, transform, sprite, [1.0; 3], out);
        }
    }

    /// `DecoratedPotRenderer.submit`'s wobble.
    pub(super) fn pot_wobble(&self, started: u64, negative: bool, partial: f32) -> Mat4 {
        let duration = pot_duration(negative) as f32;
        let progress = ((self.ticks - started) as f32 + partial) / duration;
        if !(0.0..=1.0).contains(&progress) {
            return Mat4::IDENTITY;
        }
        let pivot = Vec3::new(0.5, 0.0, 0.5);
        if negative {
            let turn = angle::sin(-progress * 3.0 * PI) * 0.125 * (1.0 - progress);
            around(pivot, Mat4::rot_y(turn))
        } else {
            let t = progress * TAU;
            let tilt_x = -1.5 * (angle::cos(t) + 0.5) * angle::sin(t / 2.0);
            around(pivot, Mat4::rot_x(tilt_x * 0.015625))
                * around(pivot, Mat4::rot_z(angle::sin(t) * 0.015625))
        }
    }
}

/// The other half of a double chest.
fn build_lit(
    level: &Level,
    pos: [i32; 3],
    out: &mut Vec<Vertex>,
    build: impl FnOnce(&mut Vec<Vertex>),
) {
    let start = out.len();
    build(out);
    let (block, sky) = level.light_at(pos[0], pos[1], pos[2]);
    let emission = level
        .block_state(pos[0], pos[1], pos[2])
        .and_then(pumpkin_data::BlockStateId::new)
        .map_or(0, |state| state.to_state().luminance);
    for vertex in &mut out[start..] {
        *vertex = vertex.with_light(block.max(emission), sky);
    }
}

fn chest_partner(pos: [i32; 3], half: ChestHalf, facing: Direction) -> Option<[i32; 3]> {
    let toward = match half {
        ChestHalf::Single => return None,
        ChestHalf::Left => facing.clockwise(),
        ChestHalf::Right => facing.clockwise().clockwise().clockwise(),
    };
    let [dx, _, dz] = toward.step();
    Some([pos[0] + dx, pos[1], pos[2] + dz])
}

/// `rotationAround`: `rotation` about `pivot` (blocks).
pub(super) fn around(pivot: Vec3, rotation: Mat4) -> Mat4 {
    Mat4::from_translation(pivot) * rotation * Mat4::from_translation(-pivot)
}

/// `Direction.getRotation`: turns `up` to face `direction`.
pub(super) fn facing_rotation(direction: Direction) -> Mat4 {
    let x = Mat4::rot_x;
    let z = Mat4::rot_z;
    match direction {
        Direction::Down => x(PI),
        Direction::Up => Mat4::IDENTITY,
        Direction::North => x(HALF_PI) * z(PI),
        Direction::South => x(HALF_PI),
        Direction::West => x(HALF_PI) * z(HALF_PI),
        Direction::East => x(HALF_PI) * z(-HALF_PI),
    }
}

/// Whether the block entity at `pos` is a player head with a profile, drawn
fn drawn_with_skin(level: &Level, pos: [i32; 3], kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Skull {
            skull: SkullType::Player,
            ..
        }
    ) && level.head_profile(pos).is_some()
}

/// `SkullBlockRenderer.createGroundTransformation` / `...Wall...`.
pub fn skull_transform(placement: Placement) -> Mat4 {
    let flip = Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0));
    match placement {
        Placement::Ground(segment) => {
            Mat4::from_translation(Vec3::new(0.5, 0.0, 0.5))
                * Mat4::rot_y(-(f32::from(segment) * 22.5).to_radians())
                * flip
        }
        Placement::Wall(facing) => {
            let [sx, _, sz] = facing.step();
            Mat4::from_translation(Vec3::new(
                0.5 - sx as f32 * 0.25,
                0.25,
                0.5 - sz as f32 * 0.25,
            )) * Mat4::rot_y(-facing.opposite().y_rot().to_radians())
                * flip
        }
    }
}

/// `TheEndPortalRenderer`: the portal's top (at 0.75) and bottom (at
pub(super) fn end_cube(min: Vec3, gateway: bool, rect: [f32; 4], out: &mut Vec<Vertex>) {
    let (bottom, top) = if gateway { (0.0, 1.0) } else { (0.375, 0.75) };
    let uv = [
        [rect[0], rect[1]],
        [rect[0], rect[3]],
        [rect[2], rect[3]],
        [rect[2], rect[1]],
    ];
    let color = [0.35, 0.35, 0.45];
    let mut quad = |corners: [Vec3; 4]| {
        let corners = std::array::from_fn(|i| ((min + corners[i]).to_array(), uv[i]));
        crate::world::geometry::push_quad(out, corners, color, false);
    };
    let v = Vec3::new;
    quad([
        v(0.0, top, 0.0),
        v(0.0, top, 1.0),
        v(1.0, top, 1.0),
        v(1.0, top, 0.0),
    ]);
    quad([
        v(0.0, bottom, 1.0),
        v(0.0, bottom, 0.0),
        v(1.0, bottom, 0.0),
        v(1.0, bottom, 1.0),
    ]);
    if gateway {
        quad([
            v(1.0, 1.0, 0.0),
            v(1.0, 0.0, 0.0),
            v(0.0, 0.0, 0.0),
            v(0.0, 1.0, 0.0),
        ]);
        quad([
            v(0.0, 1.0, 1.0),
            v(0.0, 0.0, 1.0),
            v(1.0, 0.0, 1.0),
            v(1.0, 1.0, 1.0),
        ]);
        quad([
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 0.0),
            v(0.0, 0.0, 1.0),
            v(0.0, 1.0, 1.0),
        ]);
        quad([
            v(1.0, 1.0, 1.0),
            v(1.0, 0.0, 1.0),
            v(1.0, 0.0, 0.0),
            v(1.0, 1.0, 0.0),
        ]);
    }
}
