//! What the world hands the renderer: the vertex and quad formats its meshes
//! are built in, and the index lists that draw them.

use numerobis::elem::Pod;

/// `0, 1, 2, 0, 2, 3` per quad: both triangles counter-clockwise.
pub const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Whether a sorted mesh of `vertices` takes `u32` indices, not `u16`s.
pub fn wide_indices(vertices: u32) -> bool {
    vertices > 1 << 16
}

/// `MeshData.SortState.buildSortedIndexBuffer`: the index list that draws
/// `quads` (by index, in order) of a mesh of `vertices`, [`QUAD_INDICES`]
/// each. `u16`s two to a word while they reach every vertex
/// (`IndexType.least`), else `u32`s.
pub fn quad_index_list(
    quads: impl ExactSizeIterator<Item = u32>,
    vertices: u32,
    out: &mut Vec<u32>,
) {
    out.clear();
    let corners = |quad: u32| QUAD_INDICES.map(|i| quad * 4 + u32::from(i));
    if wide_indices(vertices) {
        out.reserve(quads.len() * 6);
        out.extend(quads.flat_map(corners));
    } else {
        out.reserve(quads.len() * 3);
        out.extend(quads.flat_map(|quad| {
            let [a, b, c, d, e, f] = corners(quad);
            [a | b << 16, c | d << 16, e | f << 16]
        }));
    }
}

/// The quads [`quad_index_list`] put in `list`, in its order.
pub fn quads_of_index_list(list: &[u32], vertices: u32) -> Vec<u32> {
    let (step, mask) = if wide_indices(vertices) {
        (6, u32::MAX)
    } else {
        (3, 0xFFFF)
    };
    list.iter()
        .step_by(step)
        .map(|&word| (word & mask) / 4)
        .collect()
}

/// Light and ambient occlusion at one corner of a [`TerrainQuad`].
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

/// One block face of a terrain mesh.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainQuad(pub [u32; 5]);

impl TerrainQuad {
    /// Quads the 21-bit index reaches.
    pub const MAX_QUADS: u32 = 1 << 21;
    /// Block heights the 12-bit y reaches.
    pub const MAX_HEIGHT: u32 = 1 << 12;

    /// `index` into the quad table; `pos` chunk-local x, z (`0..16`) and y.
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

    /// [`Self::new`] with a biome colour the shader multiplies onto the shade.
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
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for TerrainQuad {}

/// One vertex of a terrain or entity mesh.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldVertex {
    pub pos: [f32; 3],
    /// Atlas or wrapping weather UV.
    pub uv: [f32; 2],
    /// RGB as unsigned normalized bytes; alpha is 255 for terrain.
    pub color: [u8; 4],
    /// Block then sky in vanilla's light-coordinate units (`UV2`).
    pub light: [u8; 2],
    /// The direction (`Direction` ordinal, `0..6`) a baked block quad's shade was taken for.
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

    /// [`Self::rgba`] with the colour already as bytes (a GUI [`Vertex`](melon_base::gfx::quad::Vertex)'s).
    pub fn rgba8(pos: [f32; 3], uv: [f32; 2], color: [u8; 4]) -> Self {
        Self {
            color,
            ..Self::new(pos, uv, [1.0; 3])
        }
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
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for WorldVertex {}

#[cfg(test)]
mod tests {
    use numerobis::elem::as_bytes;

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

    /// The `u16`s read back as the shared quad indices would draw those
    /// quads; past 65536 vertices the list holds `u32`s.
    #[test]
    fn index_lists_draw_quads_in_the_order_given() {
        let mut list = Vec::new();
        quad_index_list([2, 0].into_iter(), 12, &mut list);
        let narrow: Vec<u16> = as_bytes(&list)
            .chunks(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(narrow, [8, 9, 10, 8, 10, 11, 0, 1, 2, 0, 2, 3]);
        assert_eq!(quads_of_index_list(&list, 12), [2, 0]);
        let last = (1 << 16) / 4;
        quad_index_list([last].into_iter(), (1 << 16) + 4, &mut list);
        let corner = last * 4;
        assert_eq!(
            list,
            [
                corner,
                corner + 1,
                corner + 2,
                corner,
                corner + 2,
                corner + 3
            ]
        );
        assert_eq!(quads_of_index_list(&list, (1 << 16) + 4), [last]);
    }
}
