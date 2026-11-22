#ifndef MELON_GLINT_GLSL
#define MELON_GLINT_GLSL

// Vanilla `TextureTransform.setupGlintTexturing`: translate by the two
// wrapping offsets, then rotate 10 degrees and scale. `millis` is
// `Util.getMillis() * glintSpeed * 8`.
vec2 glint_uv(vec2 uv, float scale, float millis) {
    float o0 = mod(millis, 110000.0) / 110000.0;
    float o1 = mod(millis, 30000.0) / 30000.0;
    float a = 3.14159265 / 18.0;
    float c = cos(a);
    float s = sin(a);
    vec2 p = uv * scale;
    return vec2(p.x * c - p.y * s, p.x * s + p.y * c) + vec2(-o0, o1);
}

#endif
