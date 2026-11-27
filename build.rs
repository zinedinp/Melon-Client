//! The executable's Windows icon. The renderer's shaders are built by
//! `crates/melon-render/build.rs`, `MELON_RUSTC` by `crates/melon-gui/build.rs`.

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
    embed_windows_icon(&out_dir);
}
