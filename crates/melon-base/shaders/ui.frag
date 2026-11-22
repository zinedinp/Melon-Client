#version 450

// Vanilla `core/position_tex_color.fsh`: atlas * vertex colour.

layout(location = 0) in vec2 frag_uv;
layout(location = 1) in vec4 frag_color;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D tex_sampler;

void main() {
    out_color = texture(tex_sampler, frag_uv) * frag_color;
}
