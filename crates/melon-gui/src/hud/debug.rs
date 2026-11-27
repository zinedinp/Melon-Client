//! The F3 debug overlay (vanilla `client/gui/components/DebugScreenOverlay`
//! and the `DebugEntry*` classes): each entry that is on (see
//! [`super::debug_entries`]) adds its lines, in `Identifier` order, and the
//! overlay lays them out in two columns as vanilla does.

use super::debug_entries::{DebugEntries, Entry};
use melon_core::math::{section_coord, section_relative, wrap_degrees};
use melon_render::quad::DrawList;
use melon_render::{DeviceInfo, TextureId};
use crate::text::font::Font;

/// `DebugScreenOverlay.extractLines`: `int height = 9`.
const LINE_HEIGHT: f32 = 9.0;
/// `graphics.fill(..., -1873784752)` = `0x90505050`.
const BACKGROUND: [f32; 4] = [
    0x50 as f32 / 255.0,
    0x50 as f32 / 255.0,
    0x50 as f32 / 255.0,
    0x90 as f32 / 255.0,
];
/// `graphics.text(..., -2039584, false)` = `0xFFE0E0E0`, no shadow.
const TEXT: [f32; 4] = [
    0xE0 as f32 / 255.0,
    0xE0 as f32 / 255.0,
    0xE0 as f32 / 255.0,
    1.0,
];

/// Everything the entries read off the running game, gathered only for
/// the entries that are on.
pub struct DebugContext<'a> {
    pub entries: &'a DebugEntries,
    /// `Minecraft.getFps()`: whole frames in the last second, not `1 / dt`.
    pub fps: u32,
    /// The framerate limit in force (`None`: unlimited).
    pub framerate_limit: Option<u32>,
    /// The swapchain's present mode, e.g. `fifo`.
    pub present_mode: &'static str,
    /// `VideoMode.refreshRateLabel` of the window's monitor.
    pub refresh_rate: Option<String>,
    /// Feet position (`Entity.getX/getY/getZ`), not the eye.
    pub position: glam::DVec3,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    /// `Window.getWidth()`/`getHeight()`, i.e. physical pixels.
    pub framebuffer: (u32, u32),
    /// `Window.getScreenWidth/Height` (logical) and `getPixelDensity`.
    pub window: (u32, u32, f64),
    pub device: &'a DeviceInfo,
    /// `Level.dimension().identifier()`, e.g. `minecraft:overworld`.
    pub dimension: &'a str,
    pub render: RenderStats,
    pub mesh: melon_world::level::MeshStats,
    /// Bytes of GPU mesh memory allocated and in use.
    pub mesh_memory: (u64, u64),
    /// the game's `FrameProfiler::lines`.
    pub profile: Vec<String>,
    /// The GPU's time per pass ([`melon_render::Renderer::gpu_profile`]).
    pub gpu_profile: Vec<String>,
    /// `Minecraft.getGpuUtilization`: the GPU's busy share of a frame, in
    /// percent; `None` without GPU timings.
    pub gpu_utilization: Option<f64>,
    /// The connection, sound, server and CPU per thread.
    pub subsystems: Vec<String>,
    /// Overworld day count and ticks into the day (`DebugEntryDayCount`).
    pub day: i64,
    pub day_time: i64,
    pub extras: DebugExtras,
}

/// The lines the game works out itself, one field per entry; each is left
/// empty when its entry is off.
#[derive(Default)]
pub struct DebugExtras {
    /// `DebugEntryTps`.
    pub tps: Option<String>,
    /// `DebugEntrySimplePerformanceImpactors`.
    pub impactors: Vec<String>,
    /// `DebugEntryPlayerSpeed`.
    pub speed: Option<String>,
    /// `DebugEntryBiome`.
    pub biome: Option<String>,
    /// `DebugEntryLight`.
    pub light: Option<String>,
    /// `DebugEntryChunkSourceStats`.
    pub chunk_source: Option<String>,
    /// `DebugEntryHeightmap`: the client's and the server's.
    pub heightmaps: Vec<String>,
    /// `DebugEntryLookingAt`: the block's state and tags, the fluid's.
    pub block: Vec<String>,
    pub block_tags: Vec<String>,
    pub fluid: Vec<String>,
    pub fluid_tags: Vec<String>,
    /// `DebugEntryLookingAtEntity` and its tags.
    pub entity: Vec<String>,
    pub entity_tags: Vec<String>,
    /// `DebugEntrySoundMood`, `DebugEntrySoundCache`.
    pub sound_mood: Option<String>,
    pub sound_cache: Option<String>,
    /// The integrated server's: `DebugEntryLocalDifficulty`,
    /// `DebugEntrySpawnCounts`, `DebugEntryChunkGeneration`.
    pub local_difficulty: Option<String>,
    pub spawn_counts: Option<String>,
    pub chunk_generation: Vec<String>,
    /// `DebugEntryPostEffects`: the chains the last frame ran.
    pub post_effects: Vec<String>,
}

/// What the culling let through this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    /// Non-empty 16³ sections drawn and meshed, like vanilla's `C:`.
    pub sections_drawn: usize,
    pub sections_total: usize,
    /// Terrain vertices and draw calls (Melon's own line, `V:`).
    pub vertices_drawn: usize,
    pub terrain_draws: usize,
    pub entities_drawn: usize,
    pub entities_total: usize,
    /// Live particles drawn (`SQ`) and drawing nothing (`NR`, emitters).
    pub particles: (usize, usize),
}

/// `DebugScreenDisplayer`: the sink entries push their lines into.
#[derive(Default)]
struct DebugLines {
    priority: Vec<String>,
    regular: Vec<String>,
    /// One per group id, in the order first added (vanilla's `LinkedHashMap`).
    groups: Vec<(&'static str, Vec<String>)>,
}

impl DebugLines {
    fn line(&mut self, line: impl Into<String>) {
        self.regular.push(line.into());
    }

    fn lines(&mut self, lines: &[String]) {
        self.regular.extend(lines.iter().cloned());
    }

    /// `addToGroup`: lines of an entry that share a block with others'.
    fn group(&mut self, group: &'static str, lines: impl IntoIterator<Item = String>) {
        let at = match self.groups.iter().position(|(id, _)| *id == group) {
            Some(at) => at,
            None => {
                self.groups.push((group, Vec::new()));
                self.groups.len() - 1
            }
        };
        self.groups[at].1.extend(lines);
    }

    /// Each entry that is on, in `Identifier` order.
    fn collect(context: &DebugContext<'_>) -> Self {
        let mut lines = Self::default();
        for entry in context.entries.enabled() {
            lines.entry(entry, context);
        }
        lines
    }

    fn entry(&mut self, entry: Entry, context: &DebugContext<'_>) {
        let extras = &context.extras;
        match entry {
            Entry::GameVersion => self.priority.push(game_version()),
            Entry::Fps => self.priority.push(fps_line(context)),
            Entry::Tps => self.regular.extend(extras.tps.clone()),
            Entry::Memory => self.group("memory", memory(context)),
            Entry::SystemSpecs => self.group("system", system_specs(context)),
            Entry::LookingAtBlockState => self.group("looking_at_block", extras.block.clone()),
            Entry::LookingAtBlockTags => self.group("looking_at_block", extras.block_tags.clone()),
            Entry::LookingAtFluidState => self.group("looking_at_fluid", extras.fluid.clone()),
            Entry::LookingAtFluidTags => self.group("looking_at_fluid", extras.fluid_tags.clone()),
            Entry::LookingAtEntity => self.group("looking_at_entity", extras.entity.clone()),
            Entry::LookingAtEntityTags => {
                self.group("looking_at_entity", extras.entity_tags.clone());
            }
            Entry::ChunkRenderStats => self.line(format!(
                "C: {}/{} pC: {:03} pM: {}",
                context.render.sections_drawn,
                context.render.sections_total,
                context.mesh.waiting,
                context.mesh.in_flight
            )),
            Entry::EntityRenderStats => self.line(format!(
                "E: {}/{}",
                context.render.entities_drawn, context.render.entities_total
            )),
            Entry::ParticleRenderStats => {
                let (quads, hidden) = context.render.particles;
                self.line(format!("P: SQ {quads} NR {hidden} T {}", quads + hidden));
            }
            Entry::ChunkSourceStats => self.regular.extend(extras.chunk_source.clone()),
            Entry::PlayerPosition => self.group("position", position(context)),
            Entry::PlayerSectionPosition => {
                let pos = context.position.floor().as_ivec3();
                self.group(
                    "position",
                    [format!(
                        "Section-relative: {:02} {:02} {:02}",
                        section_relative(pos.x),
                        section_relative(pos.y),
                        section_relative(pos.z)
                    )],
                );
            }
            Entry::PlayerSpeed => self.group("position", extras.speed.clone()),
            Entry::LightLevels => self.group("light", extras.light.clone()),
            Entry::Heightmap => self.group("heightmaps", extras.heightmaps.clone()),
            Entry::Biome => self.regular.extend(extras.biome.clone()),
            Entry::DayCount => self.line(format!("Day #{}", context.day)),
            Entry::SoundMood => self.regular.extend(extras.sound_mood.clone()),
            Entry::SoundCache => self.regular.extend(extras.sound_cache.clone()),
            Entry::GpuUtilization => {
                if let Some(percent) = context.gpu_utilization {
                    self.line(if percent > 100.0 {
                        "GPU: §c100%".to_owned()
                    } else {
                        format!("GPU: {}%", percent.round())
                    });
                }
            }
            Entry::SimplePerformanceImpactors => self.lines(&extras.impactors),
            Entry::FrameProfile => self.group("melon:frame_profile", context.profile.clone()),
            Entry::GpuPasses => self.group("melon:gpu_passes", context.gpu_profile.clone()),
            Entry::Subsystems => self.group("melon:subsystems", context.subsystems.clone()),
            Entry::TerrainDraws => self.line(format!(
                "V: {}k draws: {}",
                context.render.vertices_drawn / 1000,
                context.render.terrain_draws
            )),
            Entry::LocalTime => self.line(format!("Local time: {}", context.day_time)),
            Entry::DetailedMemory => self.group("memory", detailed_memory()),
            Entry::ChunkGenerationStats => {
                self.group("chunk_generation", extras.chunk_generation.clone());
            }
            Entry::LocalDifficulty => self.regular.extend(extras.local_difficulty.clone()),
            Entry::EntitySpawnCounts => self.regular.extend(extras.spawn_counts.clone()),
            Entry::PostEffects => {
                if !extras.post_effects.is_empty() {
                    self.line(format!("Post: {}", extras.post_effects.join(", ")));
                }
            }
            // Drawn into the world, or nothing Melon has.
            Entry::EntityHitboxes
            | Entry::ChunkBorders
            | Entry::ThreeDimensionalCrosshair
            | Entry::ChunkSectionPaths
            | Entry::ChunkSectionOctree
            | Entry::VisualizeWaterLevels
            | Entry::VisualizeHeightmap
            | Entry::VisualizeCollisionBoxes
            | Entry::VisualizeEntitySupportingBlocks
            | Entry::VisualizeBlockLightLevels
            | Entry::VisualizeSkyLightLevels
            | Entry::VisualizeSolidFaces
            | Entry::VisualizeChunksOnServer
            | Entry::VisualizeSkyLightSections
            | Entry::ChunkSectionVisibility => {}
        }
    }
}

/// `DebugEntryVersion`: `Minecraft <version> (<launched>/<brand>)`.
fn game_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let minecraft = version.split_once("+mc").map_or("?", |(_, mc)| mc);
    format!("Minecraft {minecraft} (melon {version}/melon)")
}

/// `DebugEntryFps`: `%d fps T: %s%s @%sHz` - the framerate limit (`inf`
/// without one), the present mode and the monitor's refresh rate.
fn fps_line(context: &DebugContext<'_>) -> String {
    let limit = context
        .framerate_limit
        .map_or_else(|| "inf".to_owned(), |l| l.to_string());
    let mode = if context.present_mode.is_empty() {
        String::new()
    } else {
        format!(" ({})", context.present_mode)
    };
    let rate = context.refresh_rate.as_deref().unwrap_or("0");
    format!("{} fps T: {limit}{mode} @{rate}Hz", context.fps)
}

/// `VideoMode.refreshRateLabel`: whole hertz, else two decimals.
pub fn refresh_rate_label(millihertz: u32) -> String {
    if millihertz.is_multiple_of(1000) {
        (millihertz / 1000).to_string()
    } else {
        format!("{:.2}", f64::from(millihertz) / 1000.0)
    }
}

/// `DebugEntryPosition`.
fn position(context: &DebugContext<'_>) -> Vec<String> {
    let pos = context.position;
    let (bx, by, bz) = (
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let (cx, cz) = (section_coord(bx), section_coord(bz));
    let direction = facing(context.yaw_degrees);
    vec![
        format!("XYZ: {:.3} / {:.5} / {:.3}", pos.x, pos.y, pos.z),
        format!("Block: {bx} {by} {bz}"),
        format!(
            "Chunk: {cx} {} {cz} [{} {} in r.{}.{}.mca]",
            section_coord(by),
            cx & 31,
            cz & 31,
            cx >> 5,
            cz >> 5
        ),
        format!(
            "Facing: {} ({}) ({:.1} / {:.1})",
            direction.0,
            direction.1,
            wrap_degrees(context.yaw_degrees),
            wrap_degrees(context.pitch_degrees)
        ),
        format!("{} FC: 0", context.dimension),
    ]
}

/// `DebugEntryMemory`, reduced to what a Rust process can answer: what it
/// holds, and the GPU's mesh memory.
fn memory(context: &DebugContext<'_>) -> Vec<String> {
    let (allocated, used) = context.mesh_memory;
    vec![
        match resident_set_bytes() {
            Some(bytes) => format!("Mem: {:03}MiB (RSS)", bytes / 1024 / 1024),
            None => "Mem: unknown".to_string(),
        },
        format!(
            "Meshes: {}/{}MiB (GPU)",
            used / 1024 / 1024,
            allocated / 1024 / 1024
        ),
    ]
}

/// `DebugEntryDetailedMemory`'s JVM heap and non-heap pools, in a Rust
/// process's terms: what it holds in RAM now and at most, the private
/// memory it committed (the allocator's heap), and of what it holds, how
/// much is its own (anonymous) and how much mapped files (code, libraries).
fn detailed_memory() -> Vec<String> {
    let Some(m) = melon_base::platform::memory_details() else {
        return vec!["Memory (process): unknown".to_owned()];
    };
    let mib = |bytes: u64| bytes / 1024 / 1024;
    let mut lines = vec![format!(
        "Memory (process): u={:03}MiB p={:03}MiB c={:03}MiB{}",
        mib(m.resident),
        mib(m.peak_resident),
        mib(m.committed),
        m.reserved
            .map_or_else(String::new, |r| format!(" r={:03}MiB", mib(r))),
    )];
    if let (Some(anonymous), Some(file)) = (m.anonymous, m.file) {
        lines.push(format!(
            "Memory (resident): anon={:03}MiB file={:03}MiB swap={:03}MiB",
            mib(anonymous),
            mib(file),
            mib(m.swapped.unwrap_or(0)),
        ));
    }
    lines
}

/// `DebugEntrySystemSpecs`.
fn system_specs(context: &DebugContext<'_>) -> Vec<String> {
    let device = context.device;
    let (width, height, density) = context.window;
    vec![
        format!("Rust: {}", env!("MELON_RUSTC")),
        format!("CPU: {}", cpu_name()),
        format!(
            "Display: {}x{} ({})",
            context.framebuffer.0, context.framebuffer.1, device.vendor
        ),
        format!("Window: {width}x{height} ({density:.2}x pixel density)"),
        format!("{}{}", device.name, device.type_suffix),
        format!("{} {}", device.backend, device.driver),
    ]
}

/// Toggled by F3 (`DebugScreenEntryList.toggleDebugOverlay`), holding which
/// entries are on: shared with the F3 + F6 screen, as vanilla's
/// `Minecraft.debugEntries` is one for the game.
#[derive(Default)]
pub struct DebugOverlay {
    entries: SharedEntries,
}

/// The entries the overlay and its options screen both see.
pub type SharedEntries = std::rc::Rc<std::cell::RefCell<DebugEntries>>;

impl DebugOverlay {
    pub fn new(entries: DebugEntries) -> Self {
        Self {
            entries: std::rc::Rc::new(std::cell::RefCell::new(entries)),
        }
    }

    pub fn entries(&self) -> std::cell::Ref<'_, DebugEntries> {
        self.entries.borrow()
    }

    pub fn entries_mut(&self) -> std::cell::RefMut<'_, DebugEntries> {
        self.entries.borrow_mut()
    }

    /// For the F3 + F6 screen.
    pub fn shared(&self) -> SharedEntries {
        std::rc::Rc::clone(&self.entries)
    }

    pub fn toggle(&mut self) {
        self.entries.borrow_mut().toggle_overlay();
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.entries.borrow_mut().set_overlay_visible(visible);
    }

    /// F3 is open.
    pub fn visible(&self) -> bool {
        self.entries.borrow().overlay_visible()
    }

    /// `DebugScreenOverlay.showDebugScreen`: F3 is open or an entry is
    /// always on, and the HUD is not hidden (unless a screen is open).
    pub fn shown(&self, hud_hidden: bool, screen_open: bool) -> bool {
        self.entries.borrow().shows_anything() && (!hud_hidden || screen_open)
    }

    /// `DebugScreenOverlay.render`'s column split, verbatim: priority lines
    /// alternate, the regular lines and the groups split in halves.
    fn columns(lines: DebugLines) -> (Vec<String>, Vec<String>) {
        let mut left: Vec<String> = Vec::new();
        let mut right: Vec<String> = Vec::new();

        for line in lines.priority {
            if left.len() > right.len() {
                right.push(line);
            } else {
                left.push(line);
            }
        }
        if !left.is_empty() {
            left.push(String::new());
        }
        if !right.is_empty() {
            right.push(String::new());
        }

        if !lines.regular.is_empty() {
            let mid = lines.regular.len().div_ceil(2);
            let mut regular = lines.regular;
            let tail = regular.split_off(mid);
            let had_tail = !tail.is_empty();
            left.extend(regular);
            right.extend(tail);
            left.push(String::new());
            if had_tail {
                right.push(String::new());
            }
        }

        let mid = lines.groups.len().div_ceil(2);
        for (i, (_, group)) in lines.groups.into_iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let column = if i < mid { &mut left } else { &mut right };
            column.extend(group);
            column.push(String::new());
        }

        (left, right)
    }

    /// `width`, `height`: the GUI screen the overlay is laid out on.
    /// TODO(perf): ~5% of the render thread while F3 is open (the bench
    /// shows it): every line laid out anew each frame. Lay out only the lines
    /// whose text changed.
    pub fn draw(
        &self,
        draw_list: &mut DrawList,
        font: &Font,
        font_textures: &[TextureId],
        white: TextureId,
        context: &DebugContext<'_>,
        width: f32,
    ) {
        let (mut left, right) = Self::columns(DebugLines::collect(context));
        if context.entries.overlay_visible() {
            // Vanilla's chart toggles come first here; Melon has no charts.
            left.push(String::new());
            left.push("To edit: press F3 + F6".to_owned());
        }
        push_column(draw_list, font, font_textures, white, &left, width, true);
        push_column(draw_list, font, font_textures, white, &right, width, false);
    }
}

/// `DebugScreenOverlay.extractLines`: all backgrounds first, then all text,
/// a line from the top each, at the left or right edge.
fn push_column(
    draw_list: &mut DrawList,
    font: &Font,
    font_textures: &[TextureId],
    white: TextureId,
    lines: &[String],
    screen_width: f32,
    align_left: bool,
) {
    let placement = |line: &str, i: usize| {
        let text_width = font.text_width(line);
        let left = if align_left {
            2.0
        } else {
            screen_width - 2.0 - text_width
        };
        (left, 2.0 + LINE_HEIGHT * i as f32, text_width)
    };

    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let (left, top, text_width) = placement(line, i);
        draw_list.push_rect(
            white,
            [left - 1.0, top - 1.0, text_width + 2.0, LINE_HEIGHT],
            [0.0, 0.0, 1.0, 1.0],
            BACKGROUND,
        );
    }
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let (left, top, _) = placement(line, i);
        font.push_text_flat(draw_list, font_textures, line, left.round(), top, TEXT);
    }
}

/// `Entity.getDirection()` (the horizontal facing yaw rounds to) plus
/// `DebugEntryPosition`'s words for it.
fn facing(yaw_degrees: f32) -> (&'static str, &'static str) {
    match ((yaw_degrees / 90.0 + 0.5).floor() as i32) & 3 {
        0 => ("south", "Towards positive Z"),
        1 => ("west", "Towards negative X"),
        2 => ("north", "Towards negative Z"),
        _ => ("east", "Towards positive X"),
    }
}

use melon_base::platform::resident_set_bytes;

/// `GLX._getCpuInfo()`'s equivalent. Read once, like vanilla: the overlay
/// asks every frame.
fn cpu_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| melon_base::platform::cpu_name().unwrap_or_else(|| "unknown".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rates_read_as_vanilla_writes_them() {
        assert_eq!(refresh_rate_label(144_000), "144");
        assert_eq!(refresh_rate_label(59_940), "59.94");
    }

    #[test]
    fn groups_keep_the_order_they_were_first_added_in() {
        let mut lines = DebugLines::default();
        lines.group("position", ["XYZ".to_owned()]);
        lines.group("light", ["Light".to_owned()]);
        lines.group("position", ["Speed".to_owned()]);
        let ids: Vec<_> = lines.groups.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, ["position", "light"]);
        assert_eq!(lines.groups[0].1, ["XYZ", "Speed"]);
        let (left, right) = DebugOverlay::columns(lines);
        assert_eq!(left, ["XYZ", "Speed", ""]);
        assert_eq!(right, ["Light", ""]);
    }
}
