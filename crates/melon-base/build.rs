//! Compiles the UI shaders to SPIR-V with `glslc`.

use std::path::Path;
use std::process::Command;

/// `shaders/<name>` compiled to `<out_dir>/<name>.spv` and checked with `spirv-val`.
fn compile_shader(name: &str, out_dir: &str, stage: &str) {
    let src = Path::new("shaders").join(name);
    let out_path = format!("{out_dir}/{name}.spv");
    let status = Command::new("glslc")
        .arg("--target-env=vulkan1.1")
        .arg(format!("-fshader-stage={stage}"))
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .status()
        .unwrap_or_else(|e| panic!("failed to run glslc (is it installed?): {e}"));
    if !status.success() {
        panic!("glslc failed to compile {}", src.display());
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
    println!("cargo:rerun-if-changed={}", src.display());
}

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!("cargo:rerun-if-env-changed=MELON_MS_CLIENT_ID");
    println!("cargo:rerun-if-changed=shaders/glint.glsl");
    compile_shader("ui.vert", &out_dir, "vertex");
    compile_shader("ui.frag", &out_dir, "fragment");
    compile_shader("ui_glint.frag", &out_dir, "fragment");
}
