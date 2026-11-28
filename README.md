# Melon

Melon is a Rust rewrite of Minecraft: Java Edition, with [PumpkinMC](https://pumpkinmc.org/) as its backend.

- Website: <https://elbengeist.moe/melonmc/index.html>
- Install (launcher): <https://elbengeist.moe/melonmc/install.html>
- Discord: <https://discord.gg/xS4E9DeWqD>

## Building

On every system you need Rust 1.100 or newer from [rustup](https://rustup.rs/) (`rustup update`) and a GPU whose driver supports Vulkan 1.0 or newer.
The build compiles the shaders with `glslc` and checks them with `spirv-val`, so both must be on `PATH`.
The game links the shaderc library.

### Windows

1. Install the [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload. rustup's default `x86_64-pc-windows-msvc` toolchain uses them.
2. Install the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows). Its installer sets `VULKAN_SDK` and puts `glslc` and `spirv-val` on `PATH`; shaderc is found through `VULKAN_SDK`.
3. Open a new terminal, so the new `PATH` applies.

### Linux

Install a C toolchain, `lld` (the Linux linker in `.cargo/config.toml`), the shader tools and shaderc, the Vulkan loader, and ALSA's headers for sound.

Debian / Ubuntu:

```sh
sudo apt install build-essential pkg-config lld glslc spirv-tools libshaderc-dev libvulkan-dev libasound2-dev
```

Arch:

```sh
sudo pacman -S base-devel lld shaderc spirv-tools vulkan-icd-loader alsa-lib
```

Your GPU's Vulkan driver must be installed as well, for example `mesa-vulkan-drivers` (Debian / Ubuntu) or `vulkan-radeon` / `vulkan-intel` (Arch).
The [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#linux) works instead of the distribution's packages if `VULKAN_SDK` is set.

### Build and run

```sh
cargo build --release
cargo run --release
```

`cargo build --profile fast` builds faster and runs almost as fast as `--release`.
The game is `target/release/melon.exe` on Windows, `target/release/melon` on Linux.
On Linux, `packaging/install.sh` adds a desktop entry and the icon for it.

The game reads the vanilla assets that the launcher downloads.
- Install Melon once from the [install page](https://elbengeist.moe/melonmc/install.html), or put a prepared `assets/` folder next to the checkout.

Other commands:

```sh
cargo test --profile fast                   # tests
cargo run --release -- --join <address>     # connect to a server straight away
cargo run --release -- --bench              # benchmark, results in target/melon-bench/
```

## Structure

| Path | What |
|---|---|
| `src/main.rs` | Entry point: command line, assets, then the game |
| `src/app/` | Window, frame loop, input, settings, benchmark |
| `src/game/` | A joined world: ticking, entities, culling, meshes, the scene each frame |
| `crates/melon-core` | Shared basics (math, SIMD, instances) that use no other Melon crate |
| `crates/melon-base` | What client and launcher share: platform, accounts, Vulkan setup |
| `crates/melon-protocol` | Packet framing, compression, encryption, text components |
| `crates/melon-net` | Login, play session, server list ping, LAN |
| `crates/melon-world` | Chunks, blocks and models, entities, inventories, terrain meshing |
| `crates/melon-render` | Vulkan renderer: pipelines, terrain draws, post effects |
| `crates/melon-gui` | Widgets, screens, HUD, text |
| `crates/melon-assets` | Assets, resource packs, languages |
| `crates/melon-audio` | Sound |
| `crates/melon-settings` | Options and other files kept between runs |
| `shaders/` | GLSL shaders, compiled to SPIR-V at build time |
| `examples/` | Benchmarks and tools (`cargo run --release --example <name>`) |
| `packaging/` | Icons and the Linux desktop entry (`packaging/install.sh`) |

`TODO.md` lists what is open.
