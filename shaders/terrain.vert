#version 460

// Terrain block faces from 20-byte `TerrainQuad`s, which the shader reads
// from the mesh buffer itself (set 4): drawn through the shared quad index
// buffer (0 1 2 0 2 3 per quad) with a `vertexOffset` of four per quad, so
// `gl_VertexIndex` is quad * 4 + corner. Each corner is shaded once, where
// an instance per quad shaded six vertices and left most of a GPU's
// vertex batch empty. The quad's corners, UVs and colours come from the
// quad table, which holds every baked block quad once; light and ambient
// occlusion per corner come from the quad. The push constants of
// `world.vert`, its outputs but `frag_light`: the lightmap is in the colour,
// as vanilla's `terrain.vsh` puts it (`world.frag`'s `TERRAIN` variants).
//
// `gl_VertexIndex` and `gl_InstanceIndex` are the Vulkan builtins; the
// latter counts from the draw's `firstInstance`
// (`drawIndirectFirstInstance`).

#include "fog.glsl"
#include "lightmap.glsl"
#include "world_push.glsl"

// [0]: quad index (0-20), offset kind (21-23). [1]: x (0-3), z (4-7),
// y (8-19), offset nibbles (20-31). [2], [3]: two corners' light each, 12
// bits apiece (block 0-5, sky 6-11, quarter levels). The top bytes of
// [0], [2], [3]: 6-bit AO per corner, in sixtieths. [4]: biome tint
// (unorm8 RGB) multiplied onto the baked shade.
layout(std430, set = 4, binding = 0) readonly buffer Quads {
    uint words[];
} quads;

layout(location = 0) out vec2 frag_uv;
layout(location = 1) out vec4 frag_color;
layout(location = 3) out vec2 frag_fog;

// The same depth in the solid and cutout pipelines: a cutout overlay (the
// grass block's side) lies exactly on its face, as in vanilla, no bias.
invariant gl_Position;

// A `QuadCorner`: position, packed UV (unorm16 x2), colour (unorm8 x4;
// the face's directional shade times its tint), and the direction (0-5, in
// `Direction` order) that shade was taken for. Light comes per quad.
struct Corner {
    float x, y, z;
    uint uv;
    uint color;
    uint face;
};

layout(std430, set = 1, binding = 0) readonly buffer QuadTable {
    Corner corners[];
} table;

// The 16-byte corner, left off (see `QuadCorner` in `renderer/frame.rs`):
// [0] x and y, [1] z and the direction, each a 16-bit field, the position
// signed fixed point in 1/4096 blocks; [2] UV; [3] colour. One load each.
//
// layout(std430, set = 1, binding = 0) readonly buffer QuadTable {
//     uvec4 corners[];
// } table;
//
// const float CORNER_POS_STEP = 1.0 / 4096.0;

#ifdef INDIRECT
// Multi-draw: the section's origin relative to the camera per draw. Each
// draw's `firstInstance` is its place in the list (one instance each), so
// the occlusion culling can leave draws out of a run without moving them.
layout(std430, set = 2, binding = 0) readonly buffer DrawOffsets {
    vec4 offsets[];
} draws;

vec3 chunk_offset() {
    return draws.offsets[gl_InstanceIndex].xyz;
}
#else
vec3 chunk_offset() {
    return pc.offset.xyz;
}
#endif

// `world::block::offset::KINDS`: max horizontal, max vertical offset
// (0 for `OffsetType.XZ`).
const vec2 OFFSET_LIMIT[4] = vec2[](
    vec2(0.25, 0.0),  // plants, XZ
    vec2(0.25, 0.2),  // grass and ferns, XYZ
    vec2(0.125, 0.0), // pointed dripstone, sulfur spike
    vec2(0.25, 0.1)   // small dripleaf
);

void main() {
    uint q = uint(gl_VertexIndex) >> 2;
    uint vertex = uint(gl_VertexIndex) & 3u;
    uint word = q * 5u;
    uvec4 in_quad = uvec4(quads.words[word], quads.words[word + 1u],
        quads.words[word + 2u], quads.words[word + 3u]);
    // Biome tint, unorm8 RGB.
    vec4 in_tint = unpackUnorm4x8(quads.words[word + 4u]);
    uint index = in_quad.x & 0x1FFFFFu;
    // 16-byte corner:
    // uvec4 corner = table.corners[index * 4u + vertex];
    // uint face = corner.y >> 16;
    // vec3 corner_pos = vec3(
    //     bitfieldExtract(int(corner.x), 0, 16),
    //     bitfieldExtract(int(corner.x), 16, 16),
    //     bitfieldExtract(int(corner.y), 0, 16)) * CORNER_POS_STEP;
    Corner corner = table.corners[index * 4u + vertex];
    uint face = corner.face;
    vec3 corner_pos = vec3(corner.x, corner.y, corner.z);
    uint at = in_quad.y;
    vec3 pos = corner_pos
        + vec3(float(at & 15u), float((at >> 8) & 0xFFFu), float((at >> 4) & 15u));
    uint kind = (in_quad.x >> 21) & 7u;
    if (kind != 0u) {
        // `BlockState.getOffset`: each nibble of the column's seed / 15.
        vec2 limit = OFFSET_LIMIT[kind - 1u];
        vec3 n = vec3(float((at >> 20) & 15u), float((at >> 24) & 15u), float(at >> 28)) / 15.0;
        vec2 xz = clamp((n.xz - 0.5) * 0.5, -limit.x, limit.x);
        pos += vec3(xz.x, (n.y - 1.0) * limit.y, xz.y);
    }
    vec3 relative = pos + chunk_offset();
    gl_Position = pc.view_proj * vec4(relative, 1.0);
    frag_fog = fog_distances(relative);
    frag_uv = unpackUnorm2x16(corner.uv); // 16-byte: corner.z

    uint light = (vertex < 2u ? in_quad.z : in_quad.w) >> (12u * (vertex & 1u));
    // Quarter levels to levels: vanilla's smooth light coordinate / 16.
    vec2 levels = vec2(float(light & 63u), float((light >> 6) & 63u)) / 4.0;
    uint ao_field = (in_quad.x >> 24) | ((in_quad.z >> 24) << 8) | ((in_quad.w >> 24) << 16);
    // `ARGB.gray(ao)`: floor(ao * 255), with ao in sixtieths.
    uint gray = (((ao_field >> (6u * vertex)) & 63u) * 255u) / 60u;
    vec4 color = unpackUnorm4x8(corner.color); // 16-byte: corner.w
    // The dimension's own cardinal lighting (the nether's lit undersides),
    // as a factor on the default one the colour was baked with, then the
    // biome tint (`ClientLevel.calculateBlockTint`), then the lightmap
    // (`Color * sample_lightmap(Sampler2, UV2)`).
    float cardinal = face < 4u ? g.cardinal_a[face] : g.cardinal_b[face - 4u];
    vec3 shaded = color.rgb * in_tint.rgb * (float(gray) / 255.0 * cardinal);
    frag_color = vec4(shaded * sample_lightmap(levels), color.a * pc.ambient.a);
}
