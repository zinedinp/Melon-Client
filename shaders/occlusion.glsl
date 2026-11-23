#ifndef MELON_OCCLUSION_GLSL
#define MELON_OCCLUSION_GLSL

// The set and push constants `occlusion.comp` and `compact.comp` share
// (`renderer/occlusion.rs`).

struct Command {
    uint index_count;
    uint instance_count;
    uint first_index;
    int vertex_offset;
    uint first_instance;
};

// The gathered draws at [0, count), then phase 1's and phase 2's as
// `compact.comp` packs them, each run at the same place as in the first.
layout(std430, set = 0, binding = 0) buffer Commands {
    Command commands[];
};

// Per draw: its section's lowest corner relative to the camera's block, and
// its visibility slot (as `uint` bits; NO_SLOT for none).
layout(std430, set = 0, binding = 1) readonly buffer Bounds {
    vec4 bounds[];
};

// By slot: whether the section was seen the last time it was tested.
layout(std430, set = 0, binding = 2) buffer Visible {
    uint visible[];
};

layout(set = 0, binding = 3) uniform sampler2D hiz;

// Per draw and phase: whether the phase draws it.
layout(std430, set = 0, binding = 4) buffer Keep {
    uint keep[];
};

// Each run: its first draw and how many it has.
layout(std430, set = 0, binding = 5) readonly buffer Runs {
    uvec2 runs[];
};

// Per run, phase 1's then phase 2's: how many draws were kept.
layout(std430, set = 0, binding = 6) buffer Counts {
    uint counts[];
};

layout(push_constant) uniform Push {
    mat4 view_proj;
    vec2 screen;
    // The gathered draws.
    uint count;
    // `occlusion.comp`: the pyramid's levels. `compact.comp`: the phase, 0 or 1.
    uint extra;
} pc;

#endif
