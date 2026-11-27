#version 450

// GUI item icons with vanilla's item-glint (`core/item.fsh` + `GLINT`):
// the baked icon, plus `enchanted_glint_item` scrolling over it.

#include "glint.glsl"

layout(location = 0) in vec2 frag_uv;
layout(location = 1) in vec4 frag_color;
layout(location = 2) in vec2 frag_corner;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D tex_sampler;
layout(set = 1, binding = 3) uniform sampler2D glint_sampler;

// Matches `gui_uniforms::DynamicTransforms`; `ModelOffset`'s padding carries
// the glint's clock, where no vanilla shader looks.
layout(std140, set = 1, binding = 0) uniform DynamicTransforms {
    mat4 model_view;
    mat4 texture_mat;
    vec4 color_modulator;
    vec4 model_offset_and_clock;
} t;

// Matches `gui_uniforms::Globals` / vanilla `include/globals.glsl`.
layout(std140, set = 1, binding = 2) uniform Globals {
    ivec3 camera_block_pos;
    float glint_alpha;
    vec3 camera_offset;
    float game_time;
    vec2 screen_size;
    int menu_blur_radius;
    int use_rgss;
} g;

void main() {
    vec4 color = texture(tex_sampler, frag_uv) * frag_color;
    if (color.a < 0.1) {
        discard;
    }
    // `ITEM_SPRITE_SCALE`: vanilla's 8.0 is for atlas UVs, a 16-texel sprite
    // of a 1024 atlas; here over the icon's own quad, 0..1.
    vec4 glint_color = g.glint_alpha * texture(glint_sampler, glint_uv(frag_corner, 8.0 * 16.0 / 1024.0, t.model_offset_and_clock.w));
    color.rgb += glint_color.rgb * glint_color.rgb;
    out_color = color;
}
