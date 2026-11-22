//! What the camera can't see isn't drawn. Three layers, cheapest first:

pub mod frustum;
pub mod occlusion;
pub mod section_cull;
pub mod visgraph;

use glam::{DVec3, IVec3, Vec3};

use crate::math::{SECTION_SIZE, section_coord};
use frustum::Frustum;
use occlusion::{OcclusionMask, SectionPos};

/// Which culling runs. All on is vanilla.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CullSettings {
    pub frustum: bool,
    pub occlusion: bool,
    /// `Options.entityDistanceScaling`.
    pub entity_distance: f32,
}

impl Default for CullSettings {
    fn default() -> Self {
        Self {
            frustum: true,
            occlusion: true,
            entity_distance: 1.0,
        }
    }
}

/// `Entity.shouldRenderAtSqrDistance`.
const ENTITY_RANGE_PER_BLOCK: f32 = 64.0;
/// `EntityRenderer.shouldRender` grows the box by this much before the frustum test.
const ENTITY_BOX_MARGIN: f32 = 0.5;

/// One frame's culling.
pub struct Culler<'a> {
    /// Relative to the eye (`Frustum.prepare`): boxes are moved by the
    /// eye in doubles first, so culling is as exact far out as near.
    frustum: Frustum,
    occlusion: Option<&'a OcclusionMask>,
    eye: DVec3,
    /// The block the eye is in and where in it: a section's corners,
    /// whole blocks, come out relative to the eye without a double.
    eye_block: IVec3,
    eye_fraction: Vec3,
    /// The world's bottom, where section 0 starts.
    min_y: i32,
    /// Sections in the world, bottom to top.
    height: i32,
    settings: CullSettings,
    /// Beyond this distance from the eye everything is fog.
    fog_end: Option<f32>,
}

impl<'a> Culler<'a> {
    /// `frustum` relative to `eye` (the camera's [`Frustum`]).
    pub fn new(
        frustum: Frustum,
        occlusion: Option<&'a OcclusionMask>,
        eye: DVec3,
        (min_y, height): (i32, i32),
        settings: CullSettings,
    ) -> Self {
        let block = eye.floor();
        Self {
            frustum,
            occlusion: occlusion.filter(|_| settings.occlusion),
            eye,
            eye_block: block.as_ivec3(),
            eye_fraction: (eye - block).as_vec3(),
            min_y,
            height,
            settings,
            fog_end: None,
        }
    }

    /// The same culler, now also hiding what lies wholly beyond `end`
    pub fn with_fog(self, end: f32) -> Self {
        Self {
            fog_end: Some(end.max(0.0)),
            ..self
        }
    }

    /// The same culler, now also hiding what the occlusion walk didn't reach.
    pub fn with_occlusion(self, mask: &'a OcclusionMask) -> Self {
        Self {
            occlusion: self.settings.occlusion.then_some(mask),
            ..self
        }
    }

    /// Where the camera is.
    pub fn eye(&self) -> DVec3 {
        self.eye
    }

    /// A world box relative to the eye.
    #[inline]
    fn relative(&self, min: DVec3, max: DVec3) -> (Vec3, Vec3) {
        ((min - self.eye).as_vec3(), (max - self.eye).as_vec3())
    }

    /// Whether any of the box is in the view volume.
    #[inline]
    pub fn sees_box(&self, min: DVec3, max: DVec3) -> bool {
        let (min, max) = self.relative(min, max);
        self.sees_relative_box(min, max)
    }

    /// [`Self::sees_box`] for a box already relative to the eye.
    #[inline]
    pub fn sees_relative_box(&self, min: Vec3, max: Vec3) -> bool {
        self.in_fog_range(min, max) && (!self.settings.frustum || self.frustum.sees_box(min, max))
    }

    /// Whether the box's (relative to the eye) nearest point is closer
    /// than the fog's end.
    #[inline]
    fn in_fog_range(&self, min: Vec3, max: Vec3) -> bool {
        self.fog_end
            .is_none_or(|end| Vec3::ZERO.clamp(min, max).length_squared() < end * end)
    }

    /// Whether the occlusion walk reached the section.
    #[inline]
    pub fn reaches(&self, pos: SectionPos) -> bool {
        !(0..self.height).contains(&pos.1) || self.occlusion.is_none_or(|mask| mask.contains(pos))
    }

    /// A section of chunk `(x, z)`, by index from the world's bottom.
    pub fn sees_section(&self, pos: SectionPos) -> bool {
        self.sees_section_within(pos, false)
    }

    /// [`Self::sees_section`] for a section of a box [`Self::frames_box`]
    #[inline]
    pub fn sees_section_within(&self, pos: SectionPos, framed: bool) -> bool {
        self.reaches(pos) && {
            let corner = IVec3::new(
                pos.0 * SECTION_SIZE,
                self.min_y + pos.1 * SECTION_SIZE,
                pos.2 * SECTION_SIZE,
            );
            let min = (corner - self.eye_block).as_vec3() - self.eye_fraction;
            let max = min + Vec3::splat(SECTION_SIZE as f32);
            if framed {
                self.in_fog_range(min, max)
            } else {
                self.sees_relative_box(min, max)
            }
        }
    }

    /// The frustum and fog test of whole chunk columns' sections at once.
    pub fn section_cull(&self) -> section_cull::SectionCull {
        section_cull::SectionCull::new(
            self.settings.frustum.then(|| self.frustum.planes()),
            self.fog_end,
            self.eye_block,
            self.eye_fraction,
            self.min_y,
        )
    }

    /// [`Self::reaches`] for the sections `mask` (bit per section) of chunk
    /// `(x, z)`: the bits of those the occlusion walk reached.
    #[inline]
    pub fn reached_of(&self, (x, z): (i32, i32), mask: u32) -> u32 {
        let Some(occlusion) = self.occlusion else {
            return mask;
        };
        let mut rest = mask;
        let mut reached = 0;
        while rest != 0 {
            let y = rest.trailing_zeros();
            rest &= rest - 1;
            let y = y as i32;
            if !(0..self.height).contains(&y) || occlusion.contains((x, y, z)) {
                reached |= 1 << y;
            }
        }
        reached
    }

    /// Whether all of the box is in the view volume, fog aside.
    #[inline]
    pub fn frames_box(&self, min: DVec3, max: DVec3) -> bool {
        let (min, max) = self.relative(min, max);
        !self.settings.frustum || self.frustum.contains_box(min, max)
    }

    /// `EntityRenderer.shouldRender`.
    pub fn sees_entity(&self, min: DVec3, max: DVec3) -> bool {
        let (min, max) = self.relative(min, max);
        let size = max - min;
        let range = (size.x + size.y + size.z) / 3.0
            * ENTITY_RANGE_PER_BLOCK
            * self.settings.entity_distance;
        let center = (min + max) * 0.5;
        center.length_squared() < range * range
            && self.sees_relative_box(
                min - Vec3::splat(ENTITY_BOX_MARGIN),
                max + Vec3::splat(ENTITY_BOX_MARGIN),
            )
    }

    /// `LevelExtractor.isEntityVisible`'s second half.
    pub fn in_compiled_section(
        &self,
        min: DVec3,
        max: DVec3,
        compiled: impl Fn(SectionPos) -> bool,
    ) -> bool {
        let center = (min + max) * 0.5;
        let section = self.section_of(DVec3::new(center.x, min.y, center.z));
        !(0..self.height).contains(&section.1) || compiled(section)
    }

    /// The section holding a world position.
    pub fn section_of(&self, position: DVec3) -> SectionPos {
        let block = position.floor().as_ivec3();
        (
            section_coord(block.x),
            section_coord(block.y - self.min_y),
            section_coord(block.z),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Mat4;

    fn culler(occlusion: Option<&OcclusionMask>, settings: CullSettings) -> Culler<'_> {
        let projection = crate::renderer::world_projection(70.0, 1.0, 512.0);
        Culler::new(
            Frustum::from_view_projection(projection * Mat4::IDENTITY),
            occlusion,
            DVec3::ZERO,
            (-64, 24),
            settings,
        )
    }

    #[test]
    fn the_fog_hides_what_it_has_swallowed() {
        let fogged = culler(None, CullSettings::default()).with_fog(96.0);
        let ahead = |z: f64| {
            (
                DVec3::new(-1.0, -1.0, z - 1.0),
                DVec3::new(1.0, 1.0, z + 1.0),
            )
        };
        let (min, max) = ahead(-50.0);
        assert!(fogged.sees_box(min, max));
        let (min, max) = ahead(-100.0);
        assert!(!fogged.sees_box(min, max));
        assert!(fogged.sees_box(DVec3::new(-1.0, -1.0, -150.0), DVec3::new(1.0, 1.0, -90.0)));
        let (min, max) = ahead(-100.0);
        assert!(culler(None, CullSettings::default()).sees_box(min, max));
    }

    #[test]
    fn entities_fade_out_with_distance_and_scale() {
        let culler = culler(None, CullSettings::default());
        let player = |z: f64| {
            (
                DVec3::new(-0.3, 0.0, z - 0.3),
                DVec3::new(0.3, 1.8, z + 0.3),
            )
        };
        let (min, max) = player(-60.0);
        assert!(culler.sees_entity(min, max));
        let (min, max) = player(-70.0);
        assert!(!culler.sees_entity(min, max));
        let far = culler_with_scale(2.0);
        assert!(far.sees_entity(min, max));
        let (min, max) = player(5.0);
        assert!(!culler.sees_entity(min, max));
    }

    fn culler_with_scale(entity_distance: f32) -> Culler<'static> {
        culler(
            None,
            CullSettings {
                entity_distance,
                ..CullSettings::default()
            },
        )
    }

    #[test]
    fn an_entity_shows_in_any_section_meshed_before_the_walk_reached_it() {
        let mask = occlusion::walk((0, 4, 0), 24, 2, None, |pos| {
            occlusion::SectionState::Loaded(if pos == (0, 4, 0) {
                visgraph::VisibilitySet::ALL
            } else {
                visgraph::VisibilitySet::NONE
            })
        });
        let culler = culler(Some(&mask), CullSettings::default());
        let (min, max) = (DVec3::new(4.0, 0.0, -30.0), DVec3::new(4.6, 1.8, -29.4));
        assert!(!culler.sees_section((0, 4, -2)));
        assert!(culler.in_compiled_section(min, max, |pos| pos == (0, 4, -2)));
        assert!(!culler.in_compiled_section(min, max, |_| false));
    }
}
