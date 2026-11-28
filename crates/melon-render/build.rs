//! The renderer's shaders, compiled to SPIR-V into `OUT_DIR` by `glslc` and
//! checked by `spirv-val`.

use std::path::Path;
use std::process::Command;

/// Shader includes the UI shaders in `melon-base` use too (`glint.glsl`).
const SHARED_SHADERS: &str = "../melon-base/shaders";

fn compile_shader(src: &str, out_dir: &str, stage: &str) {
    let file_name = Path::new(src).file_name().unwrap().to_str().unwrap();
    compile_variant(src, out_dir, stage, file_name, &[]);
}

/// `src` compiled with `defines` set, to `<out_dir>/<out_name>.spv`.
fn compile_variant(src: &str, out_dir: &str, stage: &str, out_name: &str, defines: &[&str]) {
    let out_path = format!("{out_dir}/{out_name}.spv");
    let status = Command::new("glslc")
        // SPIR-V 1.0: every Vulkan version from 1.0 on takes it.
        .arg("--target-env=vulkan1.0")
        .arg(format!("-fshader-stage={stage}"))
        .arg(format!("-I{SHARED_SHADERS}"))
        .args(defines.iter().map(|define| format!("-D{define}")))
        .arg(src)
        .arg("-o")
        .arg(&out_path)
        .status()
        .unwrap_or_else(|e| panic!("failed to run glslc (is it installed?): {e}"));
    if !status.success() {
        panic!("glslc failed to compile {src}");
    }
    let status = Command::new("spirv-val")
        .arg("--target-env")
        .arg("vulkan1.0")
        .arg(&out_path)
        .status()
        .unwrap_or_else(|e| panic!("failed to run spirv-val (is it installed?): {e}"));
    if !status.success() {
        panic!("spirv-val rejected {out_path}");
    }
    println!("cargo:rerun-if-changed={src}");
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    // UI (2D quads) and sky (3D cubemap) are separate pipelines.
    println!("cargo:rerun-if-changed={SHARED_SHADERS}/glint.glsl");
    compile_shader("../../shaders/sky.vert", &out_dir, "vertex");
    compile_shader("../../shaders/sky.frag", &out_dir, "fragment");
    // The menu background blur: a full-screen triangle, one axis per run.
    compile_shader("../../shaders/blur.vert", &out_dir, "vertex");
    compile_shader("../../shaders/blur.frag", &out_dir, "fragment");
    println!("cargo:rerun-if-changed=../../shaders/lightmap.glsl");
    println!("cargo:rerun-if-changed=../../shaders/globals.glsl");
    println!("cargo:rerun-if-changed=../../shaders/fog.glsl");
    println!("cargo:rerun-if-changed=../../shaders/world_push.glsl");
    println!("cargo:rerun-if-changed=../../shaders/world_fog.glsl");
    // The sky's flat colour, faded into the fog.
    compile_shader("../../shaders/skydisc.vert", &out_dir, "vertex");
    compile_shader("../../shaders/skydisc.frag", &out_dir, "fragment");
    compile_shader("../../shaders/world.vert", &out_dir, "vertex");
    // The same, each draw's offset read per `gl_InstanceIndex`: the
    // translucent terrain's multi-draws.
    compile_variant(
        "../../shaders/world.vert",
        &out_dir,
        "vertex",
        "world_indirect.vert",
        &["INDIRECT"],
    );
    // Rain and snow: the texture's drop streaks placed per column.
    compile_shader("../../shaders/weather.vert", &out_dir, "vertex");
    // Particles: a quad per instance.
    compile_shader("../../shaders/particle.vert", &out_dir, "vertex");
    // Lines and points of a fixed width in pixels: outlines and gizmos.
    compile_shader("../../shaders/lines.vert", &out_dir, "vertex");
    // Terrain faces from `TerrainQuad`s and the quad table.
    compile_shader("../../shaders/terrain.vert", &out_dir, "vertex");
    // The same, its section offsets read per `gl_DrawIndex` for multi-draws.
    compile_variant(
        "../../shaders/terrain.vert",
        &out_dir,
        "vertex",
        "terrain_indirect.vert",
        &["INDIRECT"],
    );
    // Those multi-draws, expanded per visible section from its record.
    compile_shader("../../shaders/expand.comp", &out_dir, "compute");
    // Occlusion culling of those multi-draws: the depth pyramid, the phase 1
    // selection and the phase 2 test, and packing what each phase keeps.
    println!("cargo:rerun-if-changed=../../shaders/occlusion.glsl");
    compile_shader("../../shaders/hiz.comp", &out_dir, "compute");
    compile_shader("../../shaders/compact.comp", &out_dir, "compute");
    compile_variant(
        "../../shaders/occlusion.comp",
        &out_dir,
        "compute",
        "occlusion_select.comp",
        &["SELECT"],
    );
    compile_shader("../../shaders/occlusion.comp", &out_dir, "compute");
    compile_shader("../../shaders/world.frag", &out_dir, "fragment");
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_glint.frag",
        &["GLINT"],
    );
    // Fully opaque terrain: no `discard`, so the depth test can run before
    // the fragment shader.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_solid.frag",
        &["SOLID"],
    );
    // Cutout terrain (`RenderPipelines.CUTOUT_TERRAIN`): alpha cutoff 0.5.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_cutout_terrain.frag",
        &["ALPHA_CUTOUT=0.5"],
    );
    // Rain and snow: the texture times a colour with the light in it.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_weather.frag",
        &["WEATHER"],
    );
    // The cracks of a block being broken: the texture as it is, multiplied
    // onto the block by the blend.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_crumbling.frag",
        &["CRUMBLING"],
    );
    // The clouds: shaded colour only, faded by the clouds' fog end.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_clouds.frag",
        &["CLOUDS"],
    );
    // The boat's water patch: depth only, no cutout.
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_water_mask.frag",
        &["WATER_MASK"],
    );
    // A texture slid by an offset (`APPLY_TEXTURE_MATRIX`).
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_scroll.frag",
        &["SCROLL"],
    );
    // A glowing entity's flat outline colour (`core/rendertype_outline`).
    compile_variant(
        "../../shaders/world.frag",
        &out_dir,
        "fragment",
        "world_outline.frag",
        &["OUTLINE"],
    );
}
