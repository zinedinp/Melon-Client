#version 450

// Vanilla `core/terrain.fsh` / `core/entity.fsh`: atlas sample, cutout,
// then fog. Combined image sampler at set 0 (the world pipelines' texture
// set). The lightmap is in the vertex colour already, taken per vertex as
// vanilla does (`lightmap.glsl`); meshes lit by `pc.ambient` get it here.
// `TERRAIN` (the solid and cutout terrain) knows no `pc.ambient` light and
// reads no `frag_light`, which `terrain.vert` leaves out.

#include "fog.glsl"
#include "world_push.glsl"
#ifdef GLINT
#include "glint.glsl"
layout(set = 1, binding = 0) uniform sampler2D glint_sampler;
#endif

layout(location = 0) in vec2 frag_uv;
layout(location = 1) in vec4 frag_color;
#ifndef TERRAIN
// Negative for meshes lit by `pc.ambient` (see world.vert).
layout(location = 2) in vec2 frag_light;
#endif
layout(location = 3) in vec2 frag_fog;

layout(location = 0) out vec4 out_color;

layout(set = 0, binding = 0) uniform sampler2D atlas_sampler;

void main() {
#ifdef GLINT
    // Overlay pass: the item atlas at set 0 masks holes (`ALPHA_CUTOUT`
    // 0.1, as `item.fsh` / `entity.fsh` do), then `core/glint.fsh` samples
    // the scrolling glint at set 1. Blended `BlendFunction.GLINT`.
    vec4 item = texture(atlas_sampler, frag_uv);
    if (item.a * frag_color.a < 0.1) {
        discard;
    }
    vec4 glint_tex = texture(glint_sampler, glint_uv(frag_uv, pc.view.w, g.glint.x)) * frag_color;
    if (glint_tex.a < 0.1) {
        discard;
    }
    float fog = 0.0;
    if (frag_fog.x >= 0.0) {
        fog = max(
            linear_fog_value(frag_fog.x, g.fog_env.x, g.fog_env.y),
            linear_fog_value(frag_fog.y, g.fog_env.z, g.fog_env.w));
    }
    float fade = (1.0 - fog * g.fog_color.a) * g.glint.y;
    out_color = vec4(glint_tex.rgb * fade, glint_tex.a);
    return;
#endif
#ifdef OUTLINE
    // Vanilla `core/rendertype_outline`: the outline colour wherever the
    // texture isn't clear, as the entity outline target holds it.
    if (texture(atlas_sampler, frag_uv).a == 0.0) {
        discard;
    }
    out_color = vec4(pc.ambient.rgb, 1.0);
    return;
#endif
#ifdef WATER_MASK
    // Vanilla `core/rendertype_water_mask`: depth only, no texture. The
    // boat patch's UVs sit on clear texels of the hull, so a cutout
    // discard would leave holes the water shows through.
    out_color = vec4(0.0);
    return;
#endif
#ifdef WEATHER
    // Rain and snow: the lightmap is in the vertex colour already, one
    // value for the whole column.
    vec4 drop = texture(atlas_sampler, frag_uv) * frag_color;
    // Nearly all of rain.png and snow.png is clear (97 and 99 %): a
    // fragment that would add nothing skips the blend and the write.
    if (drop.a < 0.004) {
        discard;
    }
    out_color = apply_fog(drop, frag_fog);
    return;
#endif
#ifdef SCROLL
    // `APPLY_TEXTURE_MATRIX` with an `OffsetTextureTransform` (energy swirl,
    // breeze wind). The offset rides in `pc.params.xy`: these meshes are
    // sentinel-lit and read no lightmap. Wrapped by hand, the sampler clamps;
    // the unwrapped UV's derivatives keep `fract`'s jump from picking the
    // smallest mip (a seam line once the texture is filtered).
    vec4 slid = textureGrad(
        atlas_sampler, fract(frag_uv + pc.params.xy), dFdx(frag_uv), dFdy(frag_uv));
    if (slid.a < 0.1) {
        discard;
    }
    slid *= frag_color;
    out_color = apply_fog(vec4(slid.rgb * pc.ambient.rgb, slid.a), frag_fog);
    return;
#endif
#ifdef CRUMBLING
    // Vanilla `core/rendertype_crumbling.fsh`: the texture times the vertex
    // colour, its clear parts dropped; the blend multiplies it in.
    vec4 crack = texture(atlas_sampler, frag_uv) * frag_color;
    if (crack.a < 0.1) {
        discard;
    }
    out_color = crack;
    return;
#endif
#ifdef CLOUDS
    // Vanilla `core/clouds.fsh`: the face's shaded colour times the clouds'
    // colour of the moment (`CloudColor`, here `pc.ambient`; world.vert has
    // multiplied its alpha in already), with no texture or lightmap, and
    // the fog only fades it out by the clouds' end instead of mixing in its
    // colour.
    out_color = vec4(
        frag_color.rgb * pc.ambient.rgb,
        frag_color.a * (1.0 - linear_fog_value(frag_fog.x, 0.0, g.fog_sky.y)));
    return;
#endif
    vec4 tex = texture(atlas_sampler, frag_uv);
#ifndef SOLID
    // Vanilla's `ALPHA_CUTOUT`: 0.1 for entities and items, 0.5 for cutout
    // terrain (`-DALPHA_CUTOUT=0.5`, `RenderPipelines.CUTOUT_TERRAIN`).
    // Built without it for the solid layer (`-DSOLID`), whose textures have
    // no holes: a shader that can discard makes many GPUs test depth only
    // after shading.
#ifndef ALPHA_CUTOUT
#define ALPHA_CUTOUT 0.1
#endif
    if (tex.a < ALPHA_CUTOUT) {
        discard;
    }
#endif
    // The opaque pipeline doesn't blend, so alpha only counts in the
    // translucent one. Celestial and blob shadows write vertex alpha too.
#ifdef TERRAIN
    out_color = apply_fog(tex * frag_color, frag_fog);
#else
    vec3 lighting = frag_light.x < 0.0 ? pc.ambient.rgb : vec3(1.0);
    vec3 albedo = tex.rgb * frag_color.rgb;
    // Vanilla `core/entity.fsh`'s overlay, before the light: a hurt
    // entity mixed toward red, a flashing creeper toward white. Only
    // entities (sentinel-lit) read it; terrain keeps other data there.
    if (frag_light.x < 0.0 && pc.offset.w != 0.0) {
        vec3 overlay = pc.offset.w > 0.0 ? vec3(1.0, 0.0, 0.0) : vec3(1.0);
        albedo = mix(albedo, overlay, abs(pc.offset.w));
    }
    out_color = apply_fog(vec4(albedo * lighting, tex.a * frag_color.a), frag_fog);
#endif
}
