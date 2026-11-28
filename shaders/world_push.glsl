#ifndef MELON_WORLD_PUSH_GLSL
#define MELON_WORLD_PUSH_GLSL

// 128 bytes, the size Vulkan guarantees. Offsets match
// `pipeline::world::{OFFSET_AT, VIEW_AT}`.
layout(push_constant) uniform PushConstants {
    layout(offset = 0) mat4 view_proj;
    layout(offset = 64) vec4 ambient;
    layout(offset = 80) vec4 params; // xy: the texture's scroll (`OffsetTextureTransform`)
    layout(offset = 96) vec4 offset;
    layout(offset = 112) vec4 view;
} pc;

#endif
