#version 450

// Terrain and entity meshes: MVP, atlas UV, a per-vertex colour (the
// face's directional shade times its tint), and packed block/sky light.
// Vulkan GLSL: explicit locations, push-constant offsets, set 3 globals.
// Vanilla counterpart: `core/terrain.vsh` / `core/entity.vsh` (OpenGL 330).

#include "fog.glsl"
#include "world_push.glsl"
#include "world_fog.glsl"

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec4 in_color;
layout(location = 3) in uvec2 in_light;

layout(location = 0) out vec2 frag_uv;
layout(location = 1) out vec4 frag_color;
// Block and sky light levels for the lightmap, or negative for meshes lit
// by `pc.ambient`.
layout(location = 2) out vec2 frag_light;
// Spherical and cylindrical distance from the camera, for the fog; negative
// for meshes the fog doesn't touch.
layout(location = 3) out vec2 frag_fog;

// The same depth in every pipeline: the glint's `EQUAL` test meets its mesh.
invariant gl_Position;

void main() {
    gl_Position = pc.view_proj * vec4(in_pos + pc.offset.xyz, 1.0);
    frag_uv = in_uv;
    // Light coordinates are level * 16 (vanilla `UV2`); above 240 is the
    // sentinel for meshes lit by `pc.ambient`.
    frag_light = in_light.x > 240u ? vec2(-1.0) : vec2(in_light) / 16.0;
    frag_color = vec4(in_color.rgb, in_color.a * pc.ambient.a);
    if (pc.view.z == 1.0) {
        frag_fog = fog_distances(in_pos + pc.offset.xyz);
    } else if (pc.view.z == 2.0) {
        frag_fog = model_fog_distances(gl_Position);
    } else {
        frag_fog = vec2(-1.0);
    }
}
