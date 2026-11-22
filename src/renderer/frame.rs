//! Contract between the three layers: the only path world and UI take into the

use glam::Mat4;

use super::quad::DrawList;
use super::{CubeMapId, MeshId, QuadMeshId, TextureId};

/// One frame: scene first, then UI on top.
pub struct Frame<'a> {
    /// World. `None`: no 3D background, only [`Frame::clear_color`].
    pub scene: Option<&'a SceneFrame>,
    pub ui: &'a DrawList,
    /// GUI coordinate system of the `ui` vertices (`gui::screen::virtual_size`).
    pub ui_size: (f32, f32),
    pub clear_color: [f32; 4],
    /// Blur what was drawn before a point in the UI list, then draw the rest
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
    /// The scene and the UI calls before this index are blurred; the calls
    pub ui_split: usize,
}

/// World snapshot for one frame: sky first (background, no depth), then
pub struct SceneFrame {
    pub sky: Option<Sky>,
    pub world: Option<WorldDraw>,
    /// Extra meshes drawn with the same camera as `world` (terrain), each
    pub entities: Vec<EntityDraw>,
    /// Cloud boxes, camera-relative, white texture, vertex colour.
    pub clouds: Option<CloudDraw>,
    /// Rain then snow columns, camera-relative, wrapping environment textures.
    pub weather: Vec<WeatherDraw>,
    /// The block outline and the debug gizmos' lines.
    pub lines: Vec<LineDraw>,
    /// `textures/misc/enchanted_glint_{item,armor}.png`, for the overlay.
    pub glint_item: Option<TextureId>,
    pub glint_armor: Option<TextureId>,
    /// The entities (indices into [`Self::entities`]) that glow, with the
    pub glowing: Vec<(usize, [u8; 3])>,
    /// Post effect chains the scene asks for (`creeper`, `invert`...), by
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
}

/// One non-terrain mesh in world space: `model` places [`EntityDraw::mesh`]
#[derive(Clone)]
pub struct EntityDraw {
    pub mesh: MeshId,
    pub atlas: TextureId,
    /// From the mesh to world space less [`Self::origin`].
    pub model: Mat4,
    /// The block `model` is relative to. Far from the world's origin a
    pub origin: glam::IVec3,
    /// [`RenderLayer::Entities`] or [`RenderLayer::Hand`].
    pub layer: RenderLayer,
    pub blend: Blending,
    /// The light on this draw (`EntityRenderer.getPackedLightCoords` at
    pub light: Option<[f32; 3]>,
    /// Hurt red or creeper white, see [`DrawLight::overlay`](crate::renderer::pipeline::world::DrawLight::overlay).
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

/// Which enchanted-glint overlay a draw wants (`TextureTransform` scale
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
            Self::Item { texels } => per_sprite(crate::world::glint::ITEM_SPRITE_SCALE, texels),
            Self::Entity { texels } => per_sprite(crate::world::glint::ENTITY_SPRITE_SCALE, texels),
            Self::Armor => crate::world::glint::ARMOR_SCALE,
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

/// The world's draw order, vanilla's (`LevelRenderer.renderLevel`, then
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
    /// Cracks on blocks being broken (`RenderPipelines.CRUMBLING`):
    Crumbling,
    /// Depth only, before the water: what keeps it out of a boat.
    WaterMask,
    /// Water, glass and ice: blended over everything solid, far to near.
    Translucent,
    /// Particles (`ParticleEngine.extract`): after the translucent
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

    /// `renderItemInHand` clears the depth buffer first, so terrain the
    pub const fn clears_depth(self) -> bool {
        matches!(self, RenderLayer::Hand)
    }

    /// Whether the layer has a projection FOV of its own: the hand keeps
    pub const fn has_own_fov(self) -> bool {
        matches!(self, RenderLayer::Hand)
    }
}

/// Cubemap sky around the camera (vanilla `CubeMap.render`).
pub struct Sky {
    /// Title-screen panorama. In a world the sky colour is the clear colour
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
    /// The sky's flat colour (`tint`'s rgb), faded into the fog
    Disc,
    /// Over the sky colour, blended like `BlendFunction.TRANSLUCENT`: the
    Glow,
    /// Sun, moon and stars (`BlendFunction.OVERLAY`).
    Celestial,
}

/// One mesh of the sky, camera-local (`SkyRenderer`): the disc, the glow
pub struct CelestialDraw {
    pub kind: SkyKind,
    pub mesh: MeshId,
    pub atlas: TextureId,
    pub model: Mat4,
    /// Vertex colour multiplier: rain fade on the sun/moon, star
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
    /// Translate by `-cam_pos` then rotate (vanilla `Camera` order), unlike
    pub view: Mat4,
    /// The camera position the view translates by. The renderer moves each
    pub eye: glam::DVec3,
    pub fov_y_degrees: f32,
    /// The hand's: 70 degrees, narrowed while dying or in a fluid
    pub hand_fov_y_degrees: f32,
    /// Far plane distance in blocks (vanilla `GameRenderer.depthFar`).
    pub far: f32,
    /// The lightmap colour at the player's eye, multiplied onto entity
    pub ambient: [f32; 3],
    /// Time of day and brightness option, applied at draw time so day and
    pub lightmap: crate::world::lightmap::LightmapParams,
    /// The fog every drawn thing but the sky bodies is mixed into.
    pub fog: crate::world::fog::FogData,
    /// The dimension's shading of faces by direction (down, up, north,
    pub cardinal: [f32; 6],
    /// The warp the nether portal and nausea put on the view, applied to
    pub distortion: Mat4,
    /// `GameRenderer.bobView`: the walking sway, in view space, for the
    pub bob: Mat4,
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

    /// A chunk's `origin` relative to the camera's block, worked out in
    pub fn offset(&self, origin: glam::IVec3) -> glam::Vec3 {
        (origin.as_dvec3() - self.eye.floor()).as_vec3()
    }

    /// `model`, relative to the block `origin`, made relative to the
    pub fn relative_to(&self, model: Mat4, origin: glam::IVec3) -> Mat4 {
        let blocks = (origin - self.eye.floor().as_ivec3()).as_vec3();
        let mut relative = model;
        relative.w_axis += blocks.extend(0.0) * model.w_axis.w;
        relative
    }

    /// Where the camera is within its block: the part of the view's
    pub fn eye_fraction(&self) -> glam::Vec3 {
        (self.eye - self.eye.floor()).as_vec3()
    }

    /// This frame's light for a world draw, with `ambient` for its
    pub fn light(&self, ambient: [f32; 4]) -> crate::renderer::pipeline::world::DrawLight {
        crate::renderer::pipeline::world::DrawLight {
            ambient,
            lightmap: self.lightmap,
            fog: crate::renderer::pipeline::world::FogInput::None,
            overlay: 0.0,
            glint_scale: 0.0,
            scroll: None,
        }
    }
}

/// A run of one chunk mesh - one or more neighbouring sections the culling
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkDraw {
    pub mesh: TerrainMesh,
    /// Where the chunk-local mesh starts in the world, in blocks.
    pub origin: glam::IVec3,
    /// Quads of a [`TerrainMesh::Quads`] mesh, vertices of a
    pub range: std::ops::Range<u32>,
}

impl ChunkDraw {
    /// Vertices this draw shades: four per quad.
    pub fn vertex_count(&self) -> usize {
        let len = self.range.len();
        match self.mesh {
            TerrainMesh::Quads(_) => len * 4,
            TerrainMesh::Vertices(_) => len,
        }
    }
}

/// A chunk mesh: block faces as [`TerrainQuad`]s, or fluids as plain
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TerrainMesh {
    Quads(QuadMeshId),
    Vertices(MeshId),
}

/// Light and ambient occlusion at one corner of a [`TerrainQuad`], in the
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CornerLight {
    pub block: u8,
    pub sky: u8,
    pub ao: u8,
}

impl CornerLight {
    /// Quarter levels in a whole level.
    pub const STEPS_PER_LEVEL: u8 = 4;
    /// AO's unit: `ao / 60`.
    pub const AO_ONE: u8 = 60;

    /// Whole light levels `0..=15`, no occlusion: flat lighting.
    pub fn flat(block: u8, sky: u8) -> Self {
        Self {
            block: block.min(15) * Self::STEPS_PER_LEVEL,
            sky: sky.min(15) * Self::STEPS_PER_LEVEL,
            ao: Self::AO_ONE,
        }
    }
}

/// One block face of a terrain mesh: which baked quad it is (an index into
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainQuad(pub [u32; 5]);

impl TerrainQuad {
    /// Quads the 21-bit index reaches.
    pub const MAX_QUADS: u32 = 1 << 21;
    /// Block heights the 12-bit y reaches.
    pub const MAX_HEIGHT: u32 = 1 << 12;

    /// `index` into the quad table; `pos` chunk-local x, z (`0..16`) and y
    #[inline]
    pub fn new(index: u32, [x, y, z]: [u32; 3], corners: [CornerLight; 4], offset: u16) -> Self {
        debug_assert!(index < Self::MAX_QUADS && x < 16 && z < 16 && y < Self::MAX_HEIGHT);
        let light = |c: CornerLight| u32::from(c.block.min(60)) | u32::from(c.sky.min(60)) << 6;
        let ao = corners
            .iter()
            .enumerate()
            .fold(0u32, |ao, (i, c)| ao | u32::from(c.ao.min(60)) << (6 * i));
        let offset = u32::from(offset);
        Self([
            index | (offset >> 12) << 21 | (ao & 0xFF) << 24,
            x | z << 4 | y << 8 | (offset & 0xFFF) << 20,
            light(corners[0]) | light(corners[1]) << 12 | (ao >> 8 & 0xFF) << 24,
            light(corners[2]) | light(corners[3]) << 12 | (ao >> 16) << 24,
            u32::from_le_bytes([255, 255, 255, 255]),
        ])
    }

    /// [`Self::new`] with a biome colour the shader multiplies onto the
    pub fn tinted(
        index: u32,
        pos: [u32; 3],
        corners: [CornerLight; 4],
        offset: u16,
        tint: [u8; 3],
    ) -> Self {
        let mut quad = Self::new(index, pos, corners, offset);
        quad.0[4] = u32::from_le_bytes([tint[0], tint[1], tint[2], 255]);
        quad
    }

    /// RGB the shader multiplies onto the baked shade.
    pub fn tint(self) -> [u8; 3] {
        let [r, g, b, _] = self.0[4].to_le_bytes();
        [r, g, b]
    }

    pub fn index(self) -> u32 {
        self.0[0] & (Self::MAX_QUADS - 1)
    }

    /// Light and occlusion at corner `i` (`0..4`).
    pub fn corner(self, i: usize) -> CornerLight {
        let light = self.0[2 + i / 2] >> (12 * (i % 2)) & 0xFFF;
        let ao = (self.0[0] >> 24 | (self.0[2] >> 24) << 8 | (self.0[3] >> 24) << 16) >> (6 * i);
        CornerLight {
            block: (light & 63) as u8,
            sky: (light >> 6 & 63) as u8,
            ao: (ao & 63) as u8,
        }
    }

    /// Chunk-local x, y above the base, z.
    pub fn block(self) -> [u32; 3] {
        let w = self.0[1];
        [w & 15, w >> 8 & 0xFFF, w >> 4 & 15]
    }

    /// What [`Self::new`] got as `offset`.
    pub fn offset(self) -> u16 {
        ((self.0[0] >> 21 & 7) << 12 | self.0[1] >> 20) as u16
    }
}

const _: () = assert!(std::mem::size_of::<TerrainQuad>() == 20);

/// One vertex of a terrain or entity mesh: an atlas UV, a colour (the
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldVertex {
    pub pos: [f32; 3],
    /// Atlas or wrapping weather UV.
    pub uv: [f32; 2],
    /// RGB as unsigned normalized bytes; alpha is 255 for terrain, the
    pub color: [u8; 4],
    /// Block then sky in vanilla's light-coordinate units (`UV2`): level
    pub light: [u8; 2],
    /// The direction (`Direction` ordinal, `0..6`) a baked block quad's
    pub face: u8,
    pub _pad: u8,
}

impl WorldVertex {
    /// Above any light coordinate (240): the shader takes `pc.ambient`.
    pub const SENTINEL_LIGHT: [u8; 2] = [255, 255];

    pub fn new(pos: [f32; 3], uv: [f32; 2], color: [f32; 3]) -> Self {
        let unorm8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self {
            pos,
            uv,
            color: [unorm8(color[0]), unorm8(color[1]), unorm8(color[2]), 255],
            light: Self::SENTINEL_LIGHT,
            face: 0,
            _pad: 0,
        }
    }

    /// [`Self::new`] with an alpha other than opaque.
    pub fn rgba(pos: [f32; 3], uv: [f32; 2], color: [f32; 4]) -> Self {
        let mut v = Self::new(pos, uv, [color[0], color[1], color[2]]);
        v.color[3] = (color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        v
    }

    /// The direction its shade was taken for.
    pub fn with_face(mut self, face: u8) -> Self {
        self.face = face;
        self
    }

    /// Block and sky light levels `0..=15`.
    pub fn with_light(mut self, block: u8, sky: u8) -> Self {
        self.light = [block.min(15) << 4, sky.min(15) << 4];
        self
    }

    /// The quad of a line from `a` to `b`, `width` pixels wide on screen,
    /// for `shaders/lines.vert` (vanilla `RenderPipelines.LINES`): two
    /// vertices at each end, one per side. They carry the line's direction
    /// in `uv` (octahedron-encoded), the width in sixteenths of a pixel in
    /// `light` and the side in `face`. `None` for a line of no length.
    pub fn line(a: glam::Vec3, b: glam::Vec3, color: [f32; 4], width: f32) -> Option<[Self; 4]> {
        let dir = (b - a).try_normalize()?;
        let at = |pos: glam::Vec3, corner: u8| Self::line_corner(pos, dir, color, width, corner);
        Some([at(a, 0), at(a, 1), at(b, 1), at(b, 0)])
    }

    /// A square `size` pixels wide on screen around `at`, for
    /// `shaders/lines.vert` (vanilla `core/debug_point.vsh`'s points).
    pub fn point(at: glam::Vec3, color: [f32; 4], size: f32) -> [Self; 4] {
        [2, 3, 4, 5].map(|corner| Self::line_corner(at, glam::Vec3::X, color, size, corner))
    }

    fn line_corner(
        pos: glam::Vec3,
        dir: glam::Vec3,
        color: [f32; 4],
        width: f32,
        corner: u8,
    ) -> Self {
        let sixteenths = (width.max(0.0) * 16.0).round().min(f32::from(u16::MAX)) as u16;
        let mut v = Self::rgba(pos.to_array(), octahedron_encode(dir), color);
        v.light = sixteenths.to_le_bytes();
        v.face = corner;
        v
    }
}

/// A unit vector as a point of the octahedron's square, `-1..=1` each;
/// `shaders/lines.vert` decodes it.
fn octahedron_encode(v: glam::Vec3) -> [f32; 2] {
    let v = v / (v.x.abs() + v.y.abs() + v.z.abs());
    if v.z >= 0.0 {
        [v.x, v.y]
    } else {
        let sign = |x: f32| if x >= 0.0 { 1.0 } else { -1.0 };
        [(1.0 - v.y.abs()) * sign(v.x), (1.0 - v.x.abs()) * sign(v.y)]
    }
}

const _: () = assert!(std::mem::size_of::<WorldVertex>() == 28);

/// A baked quad corner in the quad table `terrain.vert` reads: a
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

/// The world camera's near plane, in blocks (vanilla `GameRenderer.PROJECTION_Z_NEAR`).
pub const WORLD_Z_NEAR: f32 = 0.05;

/// The world camera's projection (Vulkan clip space: depth `0..1`, Y down),
pub fn world_projection(fov_y_degrees: f32, aspect: f32, far: f32) -> Mat4 {
    glam::camera::rh::proj::vulkan::perspective(
        fov_y_degrees.to_radians(),
        aspect,
        far,
        WORLD_Z_NEAR,
    )
}

/// What the depth buffer is cleared to: the far plane, with reversed depth.
pub const DEPTH_CLEAR: f32 = 0.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// `lines.vert`'s `octahedron_decode`.
    fn octahedron_decode([x, y]: [f32; 2]) -> glam::Vec3 {
        let mut v = glam::Vec3::new(x, y, 1.0 - x.abs() - y.abs());
        if v.z < 0.0 {
            let sign = |x: f32| if x >= 0.0 { 1.0 } else { -1.0 };
            let (vx, vy) = (v.x, v.y);
            v.x = (1.0 - vy.abs()) * sign(vx);
            v.y = (1.0 - vx.abs()) * sign(vy);
        }
        v.normalize()
    }

    #[test]
    fn a_line_direction_survives_the_octahedron() {
        let dirs = [
            glam::Vec3::X,
            -glam::Vec3::Y,
            glam::Vec3::Z,
            -glam::Vec3::Z,
            glam::Vec3::new(1.0, -2.0, -3.0).normalize(),
            glam::Vec3::new(-0.3, 0.1, 0.9).normalize(),
        ];
        for dir in dirs {
            let back = octahedron_decode(octahedron_encode(dir));
            assert!(back.distance(dir) < 1.0e-6, "{dir} came back as {back}");
        }
    }

    #[test]
    fn a_line_is_one_quad_both_sides_at_both_ends() {
        let (a, b) = (glam::Vec3::ZERO, glam::Vec3::new(0.0, 2.0, 0.0));
        let quad = WorldVertex::line(a, b, [1.0, 0.0, 0.0, 0.4], 2.5).unwrap();
        let ends: Vec<_> = quad.iter().map(|v| v.pos).collect();
        assert_eq!(
            ends,
            [a.to_array(), a.to_array(), b.to_array(), b.to_array()]
        );
        assert_eq!(quad.map(|v| v.face), [0, 1, 1, 0]);
        assert!(quad.iter().all(|v| u16::from_le_bytes(v.light) == 40));
        assert!(quad.iter().all(|v| v.color == [255, 0, 0, 102]));
        assert!(WorldVertex::line(a, a, [1.0; 4], 1.0).is_none());
        assert_eq!(
            WorldVertex::point(a, [1.0; 4], 2.0).map(|v| v.face),
            [2, 3, 4, 5]
        );
    }

    #[test]
    fn depth_is_reversed_and_keeps_millimetres_apart_far_out() {
        let projection = world_projection(70.0, 1.0, 512.0);
        let depth = |distance: f32| {
            let clip = projection * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
            clip.z / clip.w
        };
        assert!((depth(WORLD_Z_NEAR) - 1.0).abs() < 1e-5);
        assert!(depth(512.0).abs() < 1e-5);
        assert!(depth(100.0) > depth(100.001));
    }

    #[test]
    fn neighbouring_chunks_share_their_corners_exactly() {
        let world = WorldDraw {
            chunks: Vec::new(),
            cutout: Vec::new(),
            translucent: Vec::new(),
            atlas: crate::renderer::TextureId(0),
            view: Mat4::IDENTITY,
            eye: glam::DVec3::new(1234.567, 70.31, -987.654),
            fov_y_degrees: 70.0,
            hand_fov_y_degrees: 70.0,
            far: 256.0,
            ambient: [1.0; 3],
            lightmap: crate::world::lightmap::LightmapParams::overworld([1.0; 3], 0.5),
            fog: crate::world::fog::FogData::NONE,
            cardinal: [1.0; 6],
            distortion: Mat4::IDENTITY,
            bob: Mat4::IDENTITY,
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

    #[test]
    fn terrain_quads_keep_every_field() {
        let corners = [
            CornerLight {
                block: 60,
                sky: 0,
                ao: 12,
            },
            CornerLight {
                block: 3,
                sky: 57,
                ao: 60,
            },
            CornerLight {
                block: 0,
                sky: 60,
                ao: 0,
            },
            CornerLight {
                block: 41,
                sky: 22,
                ao: 37,
            },
        ];
        let offset = 3 << 12 | 0xABC;
        let quad = TerrainQuad::new(TerrainQuad::MAX_QUADS - 1, [15, 4095, 9], corners, offset);
        assert_eq!(quad.index(), TerrainQuad::MAX_QUADS - 1);
        assert_eq!(quad.block(), [15, 4095, 9]);
        assert_eq!(quad.offset(), offset);
        for (i, corner) in corners.into_iter().enumerate() {
            assert_eq!(quad.corner(i), corner, "corner {i}");
        }
    }

    #[test]
    fn flat_light_is_whole_levels_without_occlusion() {
        let quad = TerrainQuad::new(7, [1, 2, 3], [CornerLight::flat(5, 15); 4], 0);
        for i in 0..4 {
            assert_eq!(
                quad.corner(i),
                CornerLight {
                    block: 20,
                    sky: 60,
                    ao: 60
                }
            );
        }
        assert_eq!(quad.offset(), 0);
    }
}
