//! `MELON_RUSTC` for the F3 overlay.

use std::process::Command;

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

fn main() {
    // Reruns with a new compiler too: cargo rebuilds build scripts for one.
    println!("cargo:rerun-if-changed=build.rs");
    emit_rustc_version();
}
