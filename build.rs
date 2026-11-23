use std::path::Path;
use std::process::Command;

/// Shader includes the UI shaders in `melon-base` use too (`glint.glsl`).
const SHARED_SHADERS: &str = "crates/melon-base/shaders";

fn compile_shader(src: &str, out_dir: &str, stage: &str) {
    let file_name = Path::new(src).file_name().unwrap().to_str().unwrap();
    compile_variant(src, out_dir, stage, file_name, &[]);
}

/// `src` compiled with `defines` set, to `<out_dir>/<out_name>.spv`.
fn compile_variant(src: &str, out_dir: &str, stage: &str, out_name: &str, defines: &[&str]) {
    let out_path = format!("{out_dir}/{out_name}.spv");
    let status = Command::new("glslc")
        .arg("--target-env=vulkan1.1")
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
        .arg("vulkan1.1")
        .arg(&out_path)
        .status()
        .unwrap_or_else(|e| panic!("failed to run spirv-val (is it installed?): {e}"));
    if !status.success() {
        panic!("spirv-val rejected {out_path}");
    }
    println!("cargo:rerun-if-changed={src}");
}

/// `MELON_RUSTC`: the compiler that built this binary, for the F3 overlay's
/// system-specs group (`DebugEntrySystemSpecs` prints `Java: <version>`
/// there; Melon's equivalent runtime fact is the rustc version, since there
/// is no VM version to report).
fn emit_rustc_version() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MELON_RUSTC={version}");
}

/// The icons the Windows executable carries, smallest first: what Explorer,
/// a pinned taskbar entry and the shortcut show before any window opens.
const WINDOWS_ICONS: [&str; 3] = [
    "packaging/melon-48.png",
    "packaging/melon-128.png",
    "crates/melon-base/src/icon.png",
];

/// Puts [`WINDOWS_ICONS`] into the executable as its icon group: a `.res`
/// file written here, which MSVC's linker takes as it is. The images stay
/// PNGs, as Windows reads icons since Vista, so no resource compiler is needed.
fn embed_windows_icon(out_dir: &str) {
    let target = |key: &str| std::env::var(key).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let images: Vec<Vec<u8>> = WINDOWS_ICONS
        .iter()
        .map(|path| {
            println!("cargo:rerun-if-changed={path}");
            std::fs::read(path).unwrap_or_else(|e| panic!("icon {path}: {e}"))
        })
        .collect();
    let mut res = Vec::new();
    // A `.res` file opens with an empty entry.
    push_resource(&mut res, 0, 0, &[]);
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (i, png) in images.iter().enumerate() {
        let id = i as u16 + 1;
        push_resource(&mut res, RT_ICON, id, png);
        // `IHDR`'s width and height; 256 is written as 0.
        let side = |at: usize| {
            let v = u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
            assert!(v <= 256, "icon wider than 256");
            (v % 256) as u8
        };
        group.extend_from_slice(&[side(16), side(20), 0, 0]);
        group.extend_from_slice(&1u16.to_le_bytes());
        group.extend_from_slice(&32u16.to_le_bytes());
        group.extend_from_slice(&(png.len() as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, 1, &group);
    let path = format!("{out_dir}/melon_icon.res");
    std::fs::write(&path, res).expect("icon resource");
    println!("cargo:rustc-link-arg-bins={path}");
}

/// One `.res` entry with ordinal type and name: the 32-byte header, then
/// the data padded to four bytes.
fn push_resource(out: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for ordinal in [kind, name] {
        out.extend_from_slice(&0xFFFFu16.to_le_bytes());
        out.extend_from_slice(&ordinal.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes()); // data version
    let (flags, language): (u16, u16) = if kind == 0 { (0, 0) } else { (0x1010, 0x0409) };
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&language.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // version
    out.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    emit_rustc_version();
    embed_windows_icon(&out_dir);
    // UI (2D quads) and sky (3D cubemap) are separate pipelines.
    println!("cargo:rerun-if-changed={SHARED_SHADERS}/glint.glsl");
    compile_shader("shaders/sky.vert", &out_dir, "vertex");
    compile_shader("shaders/sky.frag", &out_dir, "fragment");
    // The menu background blur: a full-screen triangle, one axis per run.
    compile_shader("shaders/blur.vert", &out_dir, "vertex");
    compile_shader("shaders/blur.frag", &out_dir, "fragment");
    println!("cargo:rerun-if-changed=shaders/lightmap.glsl");
    println!("cargo:rerun-if-changed=shaders/globals.glsl");
    println!("cargo:rerun-if-changed=shaders/fog.glsl");
    println!("cargo:rerun-if-changed=shaders/world_push.glsl");
    println!("cargo:rerun-if-changed=shaders/world_fog.glsl");
    // The sky's flat colour, faded into the fog.
    compile_shader("shaders/skydisc.vert", &out_dir, "vertex");
    compile_shader("shaders/skydisc.frag", &out_dir, "fragment");
    compile_shader("shaders/world.vert", &out_dir, "vertex");
    // Rain and snow: the texture's drop streaks placed per column.
    compile_shader("shaders/weather.vert", &out_dir, "vertex");
    // Lines and points of a fixed width in pixels: outlines and gizmos.
    compile_shader("shaders/lines.vert", &out_dir, "vertex");
    // Terrain faces from `TerrainQuad`s and the quad table.
    compile_shader("shaders/terrain.vert", &out_dir, "vertex");
    // The same, its section offsets read per `gl_DrawIndex` for multi-draws.
    compile_variant(
        "shaders/terrain.vert",
        &out_dir,
        "vertex",
        "terrain_indirect.vert",
        &["INDIRECT"],
    );
    // Occlusion culling of those multi-draws: the depth pyramid, the phase 1
    // selection and the phase 2 test, and packing what each phase keeps.
    println!("cargo:rerun-if-changed=shaders/occlusion.glsl");
    compile_shader("shaders/hiz.comp", &out_dir, "compute");
    compile_shader("shaders/compact.comp", &out_dir, "compute");
    compile_variant(
        "shaders/occlusion.comp",
        &out_dir,
        "compute",
        "occlusion_select.comp",
        &["SELECT"],
    );
    compile_shader("shaders/occlusion.comp", &out_dir, "compute");
    compile_shader("shaders/world.frag", &out_dir, "fragment");
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_glint.frag",
        &["GLINT"],
    );
    // Fully opaque terrain: no `discard`, so the depth test can run before
    // the fragment shader.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_solid.frag",
        &["SOLID"],
    );
    // Cutout terrain (`RenderPipelines.CUTOUT_TERRAIN`): alpha cutoff 0.5.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_cutout_terrain.frag",
        &["ALPHA_CUTOUT=0.5"],
    );
    // Rain and snow: the texture times a colour with the light in it.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_weather.frag",
        &["WEATHER"],
    );
    // The cracks of a block being broken: the texture as it is, multiplied
    // onto the block by the blend.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_crumbling.frag",
        &["CRUMBLING"],
    );
    // The clouds: shaded colour only, faded by the clouds' fog end.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_clouds.frag",
        &["CLOUDS"],
    );
    // The boat's water patch: depth only, no cutout.
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_water_mask.frag",
        &["WATER_MASK"],
    );
    // A texture slid by an offset (`APPLY_TEXTURE_MATRIX`).
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_scroll.frag",
        &["SCROLL"],
    );
    // A glowing entity's flat outline colour (`core/rendertype_outline`).
    compile_variant(
        "shaders/world.frag",
        &out_dir,
        "fragment",
        "world_outline.frag",
        &["OUTLINE"],
    );
}
