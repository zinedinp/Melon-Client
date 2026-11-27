//! Contract between the three layers.

use glam::Mat4;
use numerobis::elem::Pod;

use super::{CubeMapId, IndexMeshId, MeshId, ParticleMeshId, QuadMeshId};
use melon_base::gfx::TextureId;
use melon_base::gfx::quad::DrawList;
use melon_world::mesh::WorldVertex;

/// One frame: scene first, then UI on top.
pub struct Frame<'a> {
    /// World. `None`: no 3D background, only [`Frame::clear_color`].
    pub scene: Option<&'a SceneFrame>,
    pub ui: &'a DrawList,
    /// The GUI pixels the window spans (`GuiScale::projection_size`): the
    /// `ui` vertices are drawn over this, whole window pixels per GUI pixel.
    pub ui_size: (f32, f32),
    pub clear_color: [f32; 4],
    /// Blur what was drawn before a point in the UI list, then draw the rest over it.
    pub blur: Option<Blur>,
    /// Drawn before everything else, into their own textures, which `ui`
    /// shows.
    pub pictures: &'a [super::PictureDraw],
}

/// `Screen.extractBlurredBackground`: the frame so far, blurred, under a menu.
#[derive(Clone, Copy)]
pub struct Blur {
    /// The box width in pixels (`Options.menuBackgroundBlurriness`).
    pub radius: f32,
    /// The scene and the UI calls before this index are blurred.
    pub ui_split: usize,
}

/// World snapshot for one frame.
pub struct SceneFrame {
    pub sky: Option<Sky>,
    pub world: Option<WorldDraw>,
    /// Extra meshes drawn with the same camera as `world` (terrain).
    pub entities: Vec<EntityDraw>,
    /// Drawn in [`RenderLayer::Particles`], cut out ones first.
    pub particles: Vec<ParticleDraw>,
    /// Cloud boxes, camera-relative, white texture, vertex colour.
    pub clouds: Option<CloudDraw>,
    /// Rain then snow columns, camera-relative, wrapping environment textures.
    pub weather: Vec<WeatherDraw>,
    /// The block outline and the debug gizmos' lines.
    pub lines: Vec<LineDraw>,
    /// `textures/misc/enchanted_glint_{item,armor}.png`, for the overlay.
    pub glint_item: Option<TextureId>,
    pub glint_armor: Option<TextureId>,
    /// The entities (indices into [`Self::entities`]) that glow, with their colour.
    pub glowing: Vec<(usize, [u8; 3])>,
    /// Post effect chains the scene asks for (`creeper`, `invert`...), by id.
    pub post_effects: Vec<String>,
}

/// Rain or snow: the texture's drop streaks (`template`, static) drawn once
/// per piece of a column in `pieces` (`world::weather::{template, pieces}`).
#[derive(Clone, Copy)]
pub struct WeatherDraw {
    pub template: MeshId,
    pub pieces: MeshId,
    /// Whether the template's second half, the wrapped copies, is drawn.
    pub wraps: bool,
    pub atlas: TextureId,
    pub model: Mat4,
    pub origin: glam::IVec3,
}

/// The cloud mesh and the colour it is drawn in this frame. The mesh holds
/// each cell's colour and face shade only, so it outlives the time of day
/// and the weather, which only change `color` (vanilla `CloudColor`).
pub struct CloudDraw {
    pub draw: EntityDraw,
    /// Multiplied onto the vertex colours, alpha included.
    pub color: [f32; 4],
}

/// A mesh of [`WorldVertex::line`]s and [`WorldVertex::point`]s: the block
/// outline or the debug gizmos.
#[derive(Clone)]
pub struct LineDraw {
    pub mesh: MeshId,
    /// From the mesh to world space less [`Self::origin`].
    pub model: Mat4,
    /// The block `model` is relative to.
    pub origin: glam::IVec3,
    pub style: LineStyle,
}

/// Which of vanilla's line pipelines a [`LineDraw`] takes, and when it is
/// drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    /// `RenderPipelines.LINES`: depth written, not blended. With the solid
    /// entities (the opaque gizmos, the high contrast outline's black).
    Opaque,
    /// `LINES_DEPTH_BIAS`: the same, a little nearer, so it wins over an
    /// [`Self::Opaque`] line it lies on (the high contrast outline).
    OpaqueDepthBias,
    /// `LINES_TRANSLUCENT`: blended, depth written. After the blob shadows,
    /// before the translucent terrain (the block outline).
    Translucent,
    /// The same after the translucent terrain: the outline of a block that
    /// is drawn in it (`BlockOutlineRenderState.isTranslucent`).
    TranslucentAfterTerrain,
    /// `LINES_TRANSLUCENT_NO_DEPTH_WRITE`: blended, depth left alone. After
    /// the blob shadows (the see-through gizmos).
    TranslucentNoDepthWrite,
    /// `LINES` in `render3dHud`: after the hand, on the depth cleared for
    /// it, so no block covers it (the F3 crosshair).
    Hud,
}

/// One non-terrain mesh in world space: `model` places [`EntityDraw::mesh`].
#[derive(Clone)]
pub struct EntityDraw {
    pub mesh: MeshId,
    pub atlas: TextureId,
    /// From the mesh to world space less [`Self::origin`].
    pub model: Mat4,
    /// The block `model` is relative to.
    pub origin: glam::IVec3,
    /// [`RenderLayer::Entities`] or [`RenderLayer::Hand`].
    pub layer: RenderLayer,
    pub blend: Blending,
    /// The light on this draw (`EntityRenderer.getPackedLightCoords` at the entity).
    pub light: Option<[f32; 3]>,
    /// Hurt red or creeper white, see [`DrawLight::overlay`](crate::pipeline::world::DrawLight::overlay).
    pub overlay: f32,
    /// Enchantment glint overlay (`ItemStack.hasFoil`).
    pub glint: Glint,
    /// Multiplies the texels' alpha: a model drawn with a colour whose
    /// alpha is below one (a shulker bullet's glow, an experience orb).
    pub alpha: f32,
    /// `OffsetTextureTransform`; additive draws are energy swirls.
    pub scroll: Option<[f32; 2]>,
}

/// How an [`EntityDraw`] meets what is drawn already. In the order they are
/// drawn within a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Blending {
    /// Depth written, texels below the cutout dropped.
    Opaque,
    /// A text display (`RenderPipelines.TEXT_POLYGON_OFFSET`): blended and
    /// depth written, so what lies behind and is drawn later stays behind.
    Text,
    /// Blended: an item or model with see-through texels. Depth is written
    /// in the entity and hand layers (vanilla `ITEM_TRANSLUCENT`,
    /// `ENTITY_TRANSLUCENT`), where they are drawn far to near; not for
    /// particles.
    Translucent,
    /// Added onto what is behind it (`BlendFunction.LIGHTNING`: source
    /// alpha and one), depth tested and written: lightning bolts.
    Additive,
}

impl Blending {
    pub fn of(translucent: bool) -> Self {
        if translucent {
            Self::Translucent
        } else {
            Self::Opaque
        }
    }
}

/// Which enchanted-glint overlay a draw wants (`TextureTransform` scale).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Glint {
    #[default]
    None,
    /// GUI and first-person items, over a texture `texels` wide per UV unit
    /// (an atlas's side; 16 for a sprite with a texture of its own).
    Item { texels: u32 },
    /// Items held in the world, likewise.
    Entity { texels: u32 },
    /// Worn armor: scale 0.16, armor texture.
    Armor,
}

impl Glint {
    /// Vanilla's `TextureTransform` scales are for its atlas UVs; the same
    /// glint per 16-texel sprite over any atlas size, or a sprite's own UVs.
    pub fn scale(self) -> f32 {
        let per_sprite = |scale: f32, texels: u32| scale * texels.max(1) as f32 / 16.0;
        match self {
            Self::None => 0.0,
            Self::Item { texels } => per_sprite(melon_world::glint::ITEM_SPRITE_SCALE, texels),
            Self::Entity { texels } => per_sprite(melon_world::glint::ENTITY_SPRITE_SCALE, texels),
            Self::Armor => melon_world::glint::ARMOR_SCALE,
        }
    }

    pub const fn on(self) -> bool {
        !matches!(self, Self::None)
    }

    pub const fn is_armor(self) -> bool {
        matches!(self, Self::Armor)
    }

    /// [`Self::Item`] when `foil`, else none.
    pub const fn item(foil: bool, texels: u32) -> Self {
        if foil {
            Self::Item { texels }
        } else {
            Self::None
        }
    }

    /// [`Self::Entity`] when `foil`, else none.
    pub const fn entity(foil: bool, texels: u32) -> Self {
        if foil {
            Self::Entity { texels }
        } else {
            Self::None
        }
    }

    /// [`Self::Armor`] when `foil`, else none.
    pub const fn armor(foil: bool) -> Self {
        if foil { Self::Armor } else { Self::None }
    }
}

/// The world's draw order, vanilla's (`LevelRenderer.renderLevel`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderLayer {
    /// The cubemap or sky, behind everything, no depth.
    Sky,
    /// Chunk meshes, depth-tested and alpha-cut.
    Terrain,
    /// Bodies in the world, depth-tested against the terrain.
    Entities,
    /// Entity blob shadows (`ShadowFeatureRenderer`): blended, no depth write.
    Shadows,
    /// Cracks on blocks being broken (`RenderPipelines.CRUMBLING`).
    Crumbling,
    /// Depth only, before the water: what keeps it out of a boat.
    WaterMask,
    /// Water, glass and ice: blended over everything solid, far to near.
    Translucent,
    /// Particles (`ParticleEngine.extract`).
    Particles,
    /// Clouds and rain/snow, over the translucent terrain.
    Atmosphere,
    /// The first-person hand: on top of the world, never inside a wall.
    Hand,
}

impl RenderLayer {
    pub const ORDER: [RenderLayer; 10] = [
        RenderLayer::Sky,
        RenderLayer::Terrain,
        RenderLayer::Entities,
        RenderLayer::Shadows,
        RenderLayer::Crumbling,
        RenderLayer::WaterMask,
        RenderLayer::Translucent,
        RenderLayer::Particles,
        RenderLayer::Atmosphere,
        RenderLayer::Hand,
    ];

    /// `renderItemInHand` clears the depth buffer first, so terrain never covers the hand.
    pub const fn clears_depth(self) -> bool {
        matches!(self, RenderLayer::Hand)
    }

    /// Whether the layer has a projection FOV of its own.
    pub const fn has_own_fov(self) -> bool {
        matches!(self, RenderLayer::Hand)
    }
}

/// Cubemap sky around the camera (vanilla `CubeMap.render`).
pub struct Sky {
    /// Title-screen panorama.
    pub cubemap: Option<CubeMapId>,
    /// Camera orientation, no translation: the cubemap sits at infinity.
    pub view: Mat4,
    pub fov_y_degrees: f32,
    /// Sun, moon and stars, in the same rotation-only view.
    pub celestials: Vec<CelestialDraw>,
}

/// How a [`CelestialDraw`] is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyKind {
    /// The sky's flat colour (`tint`'s rgb), faded into the fog.
    Disc,
    /// Over the sky colour, blended like `BlendFunction.TRANSLUCENT`.
    Glow,
    /// Sun, moon and stars (`BlendFunction.OVERLAY`).
    Celestial,
}

/// One mesh of the sky, camera-local (`SkyRenderer`).
pub struct CelestialDraw {
    pub kind: SkyKind,
    pub mesh: MeshId,
    pub atlas: TextureId,
    pub model: Mat4,
    /// Vertex colour multiplier.
    pub tint: [f32; 4],
}

/// The depth-tested terrain, drawn with a real (translating) camera.
pub struct WorldDraw {
    /// The visible parts of the chunk meshes' solid layer, near to far.
    pub chunks: Vec<ChunkDraw>,
    /// Their cutout layer (leaves, plants), near to far.
    pub cutout: Vec<ChunkDraw>,
    /// Their translucent layer, far to near.
    pub translucent: Vec<ChunkDraw>,
    pub atlas: TextureId,
    /// Translate by `-cam_pos` then rotate (vanilla `Camera` order).
    pub view: Mat4,
    /// The camera position the view translates by.
    pub eye: glam::DVec3,
    /// `Globals.GameTime`: `((gameTime % 24000) + partialTick) / 24000`.
    pub game_time: f32,
    pub fov_y_degrees: f32,
    /// The hand's.
    pub hand_fov_y_degrees: f32,
    /// Far plane distance in blocks (vanilla `GameRenderer.depthFar`).
    pub far: f32,
    /// The lightmap colour at the player's eye, multiplied onto entity draws.
    pub ambient: [f32; 3],
    /// Time of day and brightness option, applied at draw time.
    pub lightmap: melon_world::lightmap::LightmapParams,
    /// The fog every drawn thing but the sky bodies is mixed into.
    pub fog: melon_world::fog::FogData,
    /// The dimension's shading of faces by direction (down, up, north, south, west, east).
    pub cardinal: [f32; 6],
    /// The warp the nether portal and nausea put on the view.
    pub distortion: Mat4,
    /// `GameRenderer.bobView`.
    pub bob: Mat4,
    /// One past the highest [`SectionCull::slot`] in use.
    pub section_slots: u32,
}

impl WorldDraw {
    /// The vertical field of view `layer` is projected with.
    pub fn fov_for(&self, layer: RenderLayer) -> f32 {
        if layer.has_own_fov() {
            self.hand_fov_y_degrees
        } else {
            self.fov_y_degrees
        }
    }

    /// A chunk's `origin` relative to the camera's block, worked out in `f64`.
    pub fn offset(&self, origin: glam::IVec3) -> glam::Vec3 {
        (origin.as_dvec3() - self.eye.floor()).as_vec3()
    }

    /// `model`, relative to the block `origin`, made relative to the camera's block.
    pub fn relative_to(&self, model: Mat4, origin: glam::IVec3) -> Mat4 {
        let blocks = (origin - self.eye.floor().as_ivec3()).as_vec3();
        let mut relative = model;
        relative.w_axis += blocks.extend(0.0) * model.w_axis.w;
        relative
    }

    /// Where the camera is within its block.
    pub fn eye_fraction(&self) -> glam::Vec3 {
        (self.eye - self.eye.floor()).as_vec3()
    }

    /// This frame's light for a world draw, with `ambient` for its sentinel-lit vertices.
    pub fn light(&self, ambient: [f32; 4]) -> crate::pipeline::world::DrawLight {
        crate::pipeline::world::DrawLight {
            ambient,
            lightmap: self.lightmap,
            fog: crate::pipeline::world::FogInput::None,
            overlay: 0.0,
            glint_scale: 0.0,
            scroll: None,
        }
    }
}

/// A run of one chunk mesh: one or more neighbouring sections the culling let through.
/// `Copy`, so draw lists move as plain memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChunkDraw {
    pub mesh: TerrainMesh,
    /// Where the chunk-local mesh starts in the world, in blocks.
    pub origin: glam::IVec3,
    /// Quads of a [`TerrainMesh::Quads`] mesh, vertices of a vertex mesh
    /// (a sorted mesh draws all of them).
    pub range: core::range::Range<u32>,
    /// The section it belongs to, for occlusion culling; `None` is never culled.
    pub cull: Option<SectionCull>,
}

/// A section as the occlusion culling knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionCull {
    /// Where its visibility is kept from frame to frame. Its solid and
    /// cutout draws share it, so they always go in the same phase.
    pub slot: u32,
    /// Its bottom, in blocks above the draw's `origin`.
    pub bottom: i32,
}

impl ChunkDraw {
    /// Vertices this draw shades: four per quad.
    pub fn vertex_count(&self) -> usize {
        let len = self.range.end.saturating_sub(self.range.start) as usize;
        match self.mesh {
            TerrainMesh::Quads(_) => len * 4,
            TerrainMesh::Vertices(_) | TerrainMesh::Sorted { .. } => len,
        }
    }
}

/// A chunk mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerrainMesh {
    Quads(QuadMeshId),
    Vertices(MeshId),
    /// Vertices drawn through their own index list: the translucent
    /// terrain, whose order changes as the camera moves.
    Sorted {
        vertices: MeshId,
        indices: IndexMeshId,
    },
}

/// One texture's particles of a frame (`ParticleEngine.extract`).
#[derive(Clone, Copy)]
pub struct ParticleDraw {
    pub mesh: ParticleMeshId,
    pub atlas: TextureId,
    /// The block the quads' centres are relative to.
    pub origin: glam::IVec3,
    /// Blended, after the cut out ones.
    pub translucent: bool,
}

/// A baked quad corner in the quad table `terrain.vert` reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadCorner {
    pub pos: [f32; 3],
    pub uv: [u16; 2],
    pub color: [u8; 4],
    /// [`WorldVertex::face`], as the shader's `uint`.
    pub face: u32,
}

impl From<WorldVertex> for QuadCorner {
    fn from(v: WorldVertex) -> Self {
        let unorm16 = |t: f32| (t.clamp(0.0, 1.0) * 65535.0).round() as u16;
        Self {
            pos: v.pos,
            uv: v.uv.map(unorm16),
            color: v.color,
            face: u32::from(v.face),
        }
    }
}

const _: () = assert!(std::mem::size_of::<QuadCorner>() == 24);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for QuadCorner {}

// A 16-byte corner (one `uvec4` load in `terrain.vert`), tried 2026-11-13 and
// left off: on an RTX 3080 at 4K it made no measurable difference, at
// render distance 8 or 32 (solid and cutout terrain within ±1 %, see
// `performance/c16-*`, `fab-c16-*` against `c24-*`, `fab-c24-*`), and it
// rounds rotated elements to 1/4096 of a block where floats keep them
// exact. The commented-out shader half is in `terrain.vert`.
//
// pub struct QuadCorner {
//     /// x, y, z in fixed point, [`QuadCorner::POS_SCALE`] steps per block.
//     pub pos: [i16; 3],
//     /// [`WorldVertex::face`].
//     pub face: u16,
//     pub uv: [u16; 2],
//     pub color: [u8; 4],
// }
//
// impl QuadCorner {
//     /// Fixed-point steps per block: every 1/16 of a model is exact, and
//     /// rotated elements are off by at most 1/8192 of a block. Reaches
//     /// ±8 blocks; models reach -1..2 (`BlockElement`'s -16..32).
//     pub const POS_SCALE: f32 = 4096.0;
//
//     /// Whether `pos` fits the fixed point without clamping.
//     pub fn fits(pos: [f32; 3]) -> bool {
//         pos.iter()
//             .all(|&p| (p * Self::POS_SCALE).round().abs() <= f32::from(i16::MAX))
//     }
// }
//
// impl From<WorldVertex> for QuadCorner {
//     fn from(v: WorldVertex) -> Self {
//         let unorm16 = |t: f32| (t.clamp(0.0, 1.0) * 65535.0).round() as u16;
//         let fixed = |p: f32| {
//             (p * Self::POS_SCALE)
//                 .round()
//                 .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
//         };
//         Self {
//             pos: v.pos.map(fixed),
//             face: u16::from(v.face),
//             uv: v.uv.map(unorm16),
//             color: v.color,
//         }
//     }
// }
//
// const _: () = assert!(std::mem::size_of::<QuadCorner>() == 16);
//
// In `Renderer::set_quad_table`, before converting:
//     let outside = corners.iter().filter(|v| !QuadCorner::fits(v.pos)).count();
//     if outside > 0 {
//         eprintln!("Quad-Tabelle: {outside} Ecken ausserhalb von ±8 Bloecken, abgeschnitten");
//     }
//
// Its test:
//     #[test]
//     fn quad_corners_keep_sixteenths_exact_and_rotations_close() {
//         let back = |c: QuadCorner| c.pos.map(|p| f32::from(p) / QuadCorner::POS_SCALE);
//         let model = WorldVertex::new([-1.0, 2.0, 0.0625 * 7.0], [0.5, 0.25], [1.0; 3]);
//         assert_eq!(back(model.into()), [-1.0, 2.0, 0.4375]);
//         let rotated = 0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2;
//         let corner = QuadCorner::from(WorldVertex::new([rotated; 3], [0.0; 2], [1.0; 3]));
//         for p in back(corner) {
//             assert!((p - rotated).abs() <= 0.5 / QuadCorner::POS_SCALE);
//         }
//         assert!(QuadCorner::fits([7.9, -7.9, 0.0]));
//         assert!(!QuadCorner::fits([8.5, 0.0, 0.0]));
//     }

/// What the depth buffer is cleared to: the far plane, with reversed depth.
pub const DEPTH_CLEAR: f32 = 0.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_chunks_share_their_corners_exactly() {
        let world = WorldDraw {
            chunks: Vec::new(),
            cutout: Vec::new(),
            translucent: Vec::new(),
            atlas: melon_base::gfx::TextureId(0),
            view: Mat4::IDENTITY,
            eye: glam::DVec3::new(1234.567, 70.31, -987.654),
            game_time: 0.0,
            fov_y_degrees: 70.0,
            hand_fov_y_degrees: 70.0,
            far: 256.0,
            ambient: [1.0; 3],
            lightmap: melon_world::lightmap::LightmapParams::overworld([1.0; 3], 0.5),
            fog: melon_world::fog::FogData::NONE,
            cardinal: [1.0; 6],
            distortion: Mat4::IDENTITY,
            bob: Mat4::IDENTITY,
            section_slots: 0,
        };
        for x in [1216, 1232, -1008, -992] {
            let a = world.offset(glam::IVec3::new(x, 0, -992));
            let b = world.offset(glam::IVec3::new(x + 16, 0, -992));
            let corner = glam::Vec3::new(16.0, 62.888_89, 3.0);
            assert_eq!(corner + a, corner - glam::Vec3::X * 16.0 + b);
        }
        let fraction = world.eye_fraction();
        assert!(fraction.min_element() >= 0.0 && fraction.max_element() < 1.0);
    }
}
