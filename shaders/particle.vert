#version 450

// Particles (vanilla `SingleQuadParticle`): one `ParticleQuad` per instance,
// its corners made here. The centre is f32; the half axes are F16, which is
// fine relative to the quad. Same outputs as `world.vert`.

#include "fog.glsl"
#include "lightmap.glsl"
#include "world_push.glsl"
#include "world_fog.glsl"

layout(location = 0) in vec3 in_center;
// `right`, and `up`'s x; `up`'s y and z.
layout(location = 1) in vec4 in_axes;
layout(location = 2) in vec2 in_axes2;
// u0, v0, u1, v1: u1 and v1 at the corner `-right - up`.
layout(location = 3) in vec4 in_uv;
layout(location = 4) in vec4 in_color;
layout(location = 5) in uvec2 in_light;

layout(location = 0) out vec2 frag_uv;
layout(location = 1) out vec4 frag_color;
layout(location = 2) out vec2 frag_light;
layout(location = 3) out vec2 frag_fog;

// Counter-clockwise as the camera sees it, as the CPU built them before.
const vec2 CORNERS[4] = vec2[](vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));

void main() {
    vec2 corner = CORNERS[gl_VertexIndex & 3];
    vec3 right = in_axes.xyz;
    vec3 up = vec3(in_axes.w, in_axes2);
    vec3 pos = in_center + right * corner.x + up * corner.y;
    gl_Position = pc.view_proj * vec4(pos + pc.offset.xyz, 1.0);
    frag_uv = vec2(corner.x < 0.0 ? in_uv.z : in_uv.x, corner.y < 0.0 ? in_uv.w : in_uv.y);
    // Light coordinates are level * 16 (vanilla `UV2`), the lightmap taken
    // into the colour here; above 240 is the sentinel for meshes lit by
    // `pc.ambient`, which `world.frag` multiplies in.
    bool lit = in_light.x <= 240u;
    frag_light = lit ? vec2(0.0) : vec2(-1.0);
    vec3 light = lit ? sample_lightmap(vec2(in_light) / 16.0) : vec3(1.0);
    frag_color = vec4(in_color.rgb * light, in_color.a * pc.ambient.a);
    if (pc.view.z == 1.0) {
        frag_fog = fog_distances(pos + pc.offset.xyz);
    } else if (pc.view.z == 2.0) {
        frag_fog = model_fog_distances(gl_Position);
    } else {
        frag_fog = vec2(-1.0);
    }
}
