#version 450

// Lines and points a fixed number of pixels wide, however far away
// (vanilla `core/rendertype_lines.vsh` and `core/debug_point.vsh`): the
// block outline and the debug gizmos. One `LineSegment` per instance, a
// quad of the shared index buffer: two corners at each end, each moved
// sideways on screen by half the width.

#include "fog.glsl"
#include "world_push.glsl"
#include "world_fog.glsl"

// Where the line starts, or the point.
layout(location = 0) in vec3 in_a;
layout(location = 1) in vec4 in_color;
// Where the line ends; the point again.
layout(location = 2) in vec3 in_b;
// The width (a point's size) in sixteenths of a pixel.
layout(location = 3) in uint in_width;
// 1 for a point.
layout(location = 4) in uint in_point;

layout(location = 0) out vec2 frag_uv;
layout(location = 1) out vec4 frag_color;
layout(location = 2) out vec2 frag_light;
layout(location = 3) out vec2 frag_fog;

// Vanilla's `VIEW_SHRINK`: everything a 256th closer to the eye, so a line
// on a face it outlines lies in front of it.
const float VIEW_SHRINK = 1.0 - (1.0 / 256.0);

// A point's corners, in the index buffer's order.
const vec2 CORNERS[4] = vec2[](vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));

void main() {
    // Corners 0 and 1 at `a`, 2 and 3 at `b`; 0 and 3 on the left side.
    uint corner = uint(gl_VertexIndex) & 3u;
    vec3 pos = corner < 2u ? in_a : in_b;
    // `pc.offset.xyz` is the eye in the mesh's space for these draws: the
    // point vanilla shrinks the view space toward.
    vec3 eye = pc.offset.xyz;
    vec4 clip = pc.view_proj * vec4(eye + (pos - eye) * VIEW_SHRINK, 1.0);
    float width = float(in_width) / 16.0;
    // Half the width in NDC: the NDC's 2 units span the screen's pixels.
    vec2 half_width = vec2(width) / g.screen.xy;
    vec2 offset;
    if (in_point != 0u) {
        offset = CORNERS[corner] * half_width;
    } else {
        // The line's direction on screen where this end is: how its NDC
        // position changes along it (the derivative of xy / w), in pixels.
        // The same at both ends, so their sides match.
        vec4 along = pc.view_proj * vec4(normalize(in_b - in_a), 0.0);
        vec2 screen = (along.xy * clip.w - clip.xy * along.w) * g.screen.xy;
        vec2 dir = dot(screen, screen) > 1.0e-20 ? normalize(screen) : vec2(1.0, 0.0);
        float side = corner == 0u || corner == 3u ? 1.0 : -1.0;
        offset = vec2(-dir.y, dir.x) * half_width * side;
    }
    // TODO(parity): an end behind the eye (w < 0) turns the offset around,
    // so a line crossing the near plane is drawn twisted. Vanilla's shader
    // does the same for the block outline (its gizmos are cut on the CPU,
    // as `GizmoMesh` does); cutting the outline's lines at the near plane
    // as well would be the fix, if standing in a block shows it.
    gl_Position = vec4(clip.xy + offset * clip.w, clip.zw);
    frag_uv = vec2(0.5);
    frag_light = vec2(-1.0);
    frag_color = vec4(in_color.rgb, in_color.a * pc.ambient.a);
    frag_fog = pc.view.z == 2.0
        ? model_fog_distances(pc.view_proj * vec4(pos, 1.0))
        : vec2(-1.0);
}
