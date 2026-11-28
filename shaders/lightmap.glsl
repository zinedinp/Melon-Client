#ifndef MELON_LIGHTMAP_GLSL
#define MELON_LIGHTMAP_GLSL

// Vanilla `include/sample_lightmap.glsl`, read per vertex as `terrain.vsh`
// does: the lightmap is a 16 x 16 RGBA8 texture `lightmap.fsh` fills once a
// frame, here `g.lightmap` (filled by the CPU, `globals.rs`), and the
// vertex takes it bilinearly between the four levels around its own.
#include "globals.glsl"

// The lightmap's colour at whole levels.
vec3 lightmap_entry(uint block, uint sky) {
    uint i = sky * 16u + block;
    return unpackUnorm4x8(g.lightmap[i >> 2][i & 3u]).rgb;
}

// `sample_lightmap` at block and sky levels `0..=15` (vanilla's `UV2 / 16`):
// the texture's `LINEAR` filter between whole levels, clamped to the edge
// texels.
vec3 sample_lightmap(vec2 levels) {
    vec2 at = clamp(levels, 0.0, 15.0);
    vec2 low = floor(at);
    vec2 t = at - low;
    uvec2 lo = uvec2(low);
    uvec2 hi = min(lo + 1u, uvec2(15u));
    vec3 below = mix(lightmap_entry(lo.x, lo.y), lightmap_entry(hi.x, lo.y), t.x);
    vec3 above = mix(lightmap_entry(lo.x, hi.y), lightmap_entry(hi.x, hi.y), t.x);
    return mix(below, above, t.y);
}

#endif
