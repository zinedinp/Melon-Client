//! Vanilla's F3 entries (`DebugScreenEntries`) and which of them show
//! (`DebugScreenEntryList`): each is always on, on in the F3 overlay, or off,
//! from a profile or set one by one in F3 + F6, kept in `debug-profile.json`.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// `DebugEntryCategory`: what the F3 + F6 list groups them under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// `SCREEN_TEXT`: lines of the overlay.
    Text,
    /// `RENDERER`: things drawn into the world.
    Renderer,
}

impl Category {
    pub fn label_key(self) -> &'static str {
        match self {
            Category::Text => "debug.options.category.text",
            Category::Renderer => "debug.options.category.renderer",
        }
    }
}

macro_rules! entries {
    ($($variant:ident = $namespace:literal : $path:literal, $category:ident, $works:literal;)*) => {
        /// One entry: vanilla's registered ones, then Melon's own.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Entry {
            $($variant,)*
        }

        impl Entry {
            /// In the order vanilla registers them.
            pub const ALL: &'static [Entry] = &[$(Entry::$variant,)*];

            pub fn namespace(self) -> &'static str {
                match self {
                    $(Entry::$variant => $namespace,)*
                }
            }

            /// `Identifier.getPath`, the name the F3 + F6 list shows.
            pub fn path(self) -> &'static str {
                match self {
                    $(Entry::$variant => $path,)*
                }
            }

            pub fn category(self) -> Category {
                match self {
                    $(Entry::$variant => Category::$category,)*
                }
            }

            /// Whether Melon has it. Vanilla's that need its integrated
            /// server or the JVM, and renderers Melon lacks, are listed but
            /// show nothing (grey in F3 + F6, as vanilla greys the ones
            /// reduced debug info hides).
            pub fn works(self) -> bool {
                match self {
                    $(Entry::$variant => $works,)*
                }
            }
        }
    };
}

entries! {
    GameVersion = "minecraft":"game_version", Text, true;
    Fps = "minecraft":"fps", Text, true;
    Tps = "minecraft":"tps", Text, true;
    Memory = "minecraft":"memory", Text, true;
    DetailedMemory = "minecraft":"detailed_memory", Text, true;
    SystemSpecs = "minecraft":"system_specs", Text, true;
    LookingAtBlockState = "minecraft":"looking_at_block_state", Text, true;
    LookingAtBlockTags = "minecraft":"looking_at_block_tags", Text, true;
    LookingAtFluidState = "minecraft":"looking_at_fluid_state", Text, true;
    LookingAtFluidTags = "minecraft":"looking_at_fluid_tags", Text, true;
    LookingAtEntity = "minecraft":"looking_at_entity", Text, true;
    LookingAtEntityTags = "minecraft":"looking_at_entity_tags", Text, true;
    ChunkRenderStats = "minecraft":"chunk_render_stats", Text, true;
    ChunkGenerationStats = "minecraft":"chunk_generation_stats", Text, true;
    EntityRenderStats = "minecraft":"entity_render_stats", Text, true;
    ParticleRenderStats = "minecraft":"particle_render_stats", Text, true;
    ChunkSourceStats = "minecraft":"chunk_source_stats", Text, true;
    PlayerPosition = "minecraft":"player_position", Text, true;
    PlayerSectionPosition = "minecraft":"player_section_position", Text, true;
    PlayerSpeed = "minecraft":"player_speed", Text, true;
    LightLevels = "minecraft":"light_levels", Text, true;
    Heightmap = "minecraft":"heightmap", Text, true;
    Biome = "minecraft":"biome", Text, true;
    LocalDifficulty = "minecraft":"local_difficulty", Text, true;
    DayCount = "minecraft":"day_count", Text, true;
    EntitySpawnCounts = "minecraft":"entity_spawn_counts", Text, true;
    SoundMood = "minecraft":"sound_mood", Text, true;
    SoundCache = "minecraft":"sound_cache", Text, true;
    PostEffects = "minecraft":"post_effects", Text, true;
    EntityHitboxes = "minecraft":"entity_hitboxes", Renderer, true;
    ChunkBorders = "minecraft":"chunk_borders", Renderer, true;
    ThreeDimensionalCrosshair = "minecraft":"3d_crosshair", Renderer, true;
    ChunkSectionPaths = "minecraft":"chunk_section_paths", Renderer, true;
    GpuUtilization = "minecraft":"gpu_utilization", Text, true;
    SimplePerformanceImpactors = "minecraft":"simple_performance_impactors", Text, true;
    ChunkSectionOctree = "minecraft":"chunk_section_octree", Renderer, true;
    VisualizeWaterLevels = "minecraft":"visualize_water_levels", Renderer, true;
    VisualizeHeightmap = "minecraft":"visualize_heightmap", Renderer, true;
    VisualizeCollisionBoxes = "minecraft":"visualize_collision_boxes", Renderer, true;
    VisualizeEntitySupportingBlocks = "minecraft":"visualize_entity_supporting_blocks", Renderer, true;
    VisualizeBlockLightLevels = "minecraft":"visualize_block_light_levels", Renderer, true;
    VisualizeSkyLightLevels = "minecraft":"visualize_sky_light_levels", Renderer, true;
    VisualizeSolidFaces = "minecraft":"visualize_solid_faces", Renderer, true;
    VisualizeChunksOnServer = "minecraft":"visualize_chunks_on_server", Renderer, true;
    VisualizeSkyLightSections = "minecraft":"visualize_sky_light_sections", Renderer, true;
    ChunkSectionVisibility = "minecraft":"chunk_section_visibility", Renderer, true;
    // Melon's own: the frame's time per phase, the GPU's per pass, the
    // connection, sound and threads, terrain draws, and the time of day.
    FrameProfile = "melon":"frame_profile", Text, true;
    GpuPasses = "melon":"gpu_passes", Text, true;
    Subsystems = "melon":"subsystems", Text, true;
    TerrainDraws = "melon":"terrain_draws", Text, true;
    LocalTime = "melon":"local_time", Text, true;
}

impl Entry {
    /// `Identifier.toString`.
    pub fn id(self) -> String {
        format!("{}:{}", self.namespace(), self.path())
    }

    /// `Identifier.parse`, the namespace defaulting to `minecraft`.
    pub fn from_id(id: &str) -> Option<Self> {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        Entry::ALL
            .iter()
            .copied()
            .find(|e| e.namespace() == namespace && e.path() == path)
    }

    /// `Identifier.compareTo`: by path, then by namespace.
    pub fn cmp_id(self, other: Self) -> std::cmp::Ordering {
        self.path()
            .cmp(other.path())
            .then(self.namespace().cmp(other.namespace()))
    }

    fn index(self) -> usize {
        Entry::ALL
            .iter()
            .position(|&e| e == self)
            .expect("ALL lists every entry")
    }
}

/// `DebugScreenEntryStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    AlwaysOn,
    InOverlay,
    #[default]
    Never,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::AlwaysOn => "alwaysOn",
            Status::InOverlay => "inOverlay",
            Status::Never => "never",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        [Status::AlwaysOn, Status::InOverlay, Status::Never]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// `DebugScreenProfile`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Default,
    Performance,
}

impl Profile {
    pub const ALL: [Profile; 2] = [Profile::Default, Profile::Performance];

    pub fn name(self) -> &'static str {
        match self {
            Profile::Default => "default",
            Profile::Performance => "performance",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Profile::Default => "debug.options.profile.default",
            Profile::Performance => "debug.options.profile.performance",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Profile::ALL.into_iter().find(|p| p.name() == name)
    }

    /// `DebugScreenEntries.PROFILES`, with Melon's own entries in the
    /// overlay as well.
    fn statuses(self) -> Vec<(Entry, Status)> {
        use Entry::*;
        let overlay = |entries: &[Entry]| {
            entries
                .iter()
                .map(|&e| (e, Status::InOverlay))
                .collect::<Vec<_>>()
        };
        match self {
            Profile::Default => overlay(&[
                ThreeDimensionalCrosshair,
                GameVersion,
                Tps,
                Fps,
                Memory,
                SystemSpecs,
                PlayerPosition,
                PlayerSectionPosition,
                SimplePerformanceImpactors,
                FrameProfile,
                GpuPasses,
                Subsystems,
            ]),
            Profile::Performance => {
                let mut statuses = overlay(&[
                    Tps,
                    GpuUtilization,
                    Memory,
                    SimplePerformanceImpactors,
                    FrameProfile,
                    GpuPasses,
                    Subsystems,
                ]);
                statuses.push((Fps, Status::AlwaysOn));
                statuses
            }
        }
    }
}

/// `DebugScreenEntryList`.
#[derive(Debug)]
pub struct DebugEntries {
    statuses: Vec<Status>,
    profile: Option<Profile>,
    overlay: bool,
    path: Option<PathBuf>,
}

impl Default for DebugEntries {
    fn default() -> Self {
        let mut entries = Self {
            statuses: vec![Status::Never; Entry::ALL.len()],
            profile: None,
            overlay: false,
            path: None,
        };
        entries.reset_to_profile(Profile::Default);
        entries
    }
}

/// `DebugScreenEntryList`'s file in the game folder.
pub const FILE: &str = "debug-profile.json";

impl DebugEntries {
    /// `load`: the file's profile or entries; the default profile without
    /// a file, and when it cannot be read (then written anew).
    pub fn load(game_dir: &Path) -> Self {
        let path = game_dir.join(FILE);
        let mut entries = Self {
            path: Some(path.clone()),
            ..Self::default()
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return entries;
        };
        match serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|json| entries.read(&json))
        {
            Some(()) => {}
            None => {
                melon_base::warn!("{FILE} unreadable, F3 back on the default profile");
                entries.reset_to_profile(Profile::Default);
                entries.save();
            }
        }
        entries
    }

    fn read(&mut self, json: &Value) -> Option<()> {
        let json = json.as_object()?;
        if let Some(profile) = json.get("profile") {
            self.reset_to_profile(Profile::from_name(profile.as_str()?)?);
            return Some(());
        }
        self.statuses.fill(Status::Never);
        self.profile = None;
        if let Some(custom) = json.get("custom") {
            for (id, status) in custom.as_object()? {
                let status = Status::from_name(status.as_str()?)?;
                // An entry this client does not know is left out.
                if let Some(entry) = Entry::from_id(id) {
                    self.statuses[entry.index()] = status;
                }
            }
        }
        Some(())
    }

    /// `save`: the profile in use, or each entry that is not off.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let mut json = Map::new();
        match self.profile {
            Some(profile) => {
                json.insert("profile".into(), profile.name().into());
            }
            None => {
                let custom: Map<String, Value> = Entry::ALL
                    .iter()
                    .filter(|&&e| self.status(e) != Status::Never)
                    .map(|&e| (e.id(), self.status(e).name().into()))
                    .collect();
                json.insert("custom".into(), custom.into());
            }
        }
        if let Err(e) = std::fs::write(path, Value::Object(json).to_string()) {
            melon_base::warn!("{FILE} not saved: {e}");
        }
    }

    fn reset_to_profile(&mut self, profile: Profile) {
        self.profile = Some(profile);
        self.statuses.fill(Status::Never);
        for (entry, status) in profile.statuses() {
            self.statuses[entry.index()] = status;
        }
    }

    /// `loadProfile`, then saved.
    pub fn load_profile(&mut self, profile: Profile) {
        self.reset_to_profile(profile);
        self.save();
    }

    pub fn is_using_profile(&self, profile: Profile) -> bool {
        self.profile == Some(profile)
    }

    pub fn status(&self, entry: Entry) -> Status {
        self.statuses[entry.index()]
    }

    /// `setStatus`: the profile is left, and the file written.
    pub fn set_status(&mut self, entry: Entry, status: Status) {
        self.profile = None;
        self.statuses[entry.index()] = status;
        self.save();
    }

    /// `toggleStatus` (F3 + B, F3 + G): whether it is shown now. In the
    /// overlay an entry goes between off and the overlay, outside between
    /// off and always.
    pub fn toggle_status(&mut self, entry: Entry) -> bool {
        let next = match (self.status(entry), self.overlay) {
            (Status::AlwaysOn, _) | (Status::InOverlay, true) => Status::Never,
            (Status::InOverlay, false) => Status::AlwaysOn,
            (Status::Never, true) => Status::InOverlay,
            (Status::Never, false) => Status::AlwaysOn,
        };
        self.set_status(entry, next);
        next != Status::Never
    }

    /// `isOverlayVisible`: F3 is open.
    pub fn overlay_visible(&self) -> bool {
        self.overlay
    }

    pub fn toggle_overlay(&mut self) {
        self.overlay = !self.overlay;
    }

    pub fn set_overlay_visible(&mut self, visible: bool) {
        self.overlay = visible;
    }

    /// `isCurrentlyEnabled`: on now, and something Melon has.
    pub fn is_enabled(&self, entry: Entry) -> bool {
        entry.works()
            && match self.status(entry) {
                Status::AlwaysOn => true,
                Status::InOverlay => self.overlay,
                Status::Never => false,
            }
    }

    /// `getCurrentlyEnabled`: in `Identifier` order, as the overlay asks them.
    pub fn enabled(&self) -> Vec<Entry> {
        let mut enabled: Vec<Entry> = Entry::ALL
            .iter()
            .copied()
            .filter(|&e| self.is_enabled(e))
            .collect();
        enabled.sort_by(|a, b| a.cmp_id(*b));
        enabled
    }

    /// `DebugScreenOverlay.showDebugScreen`, before the hidden HUD: F3 is
    /// open, or an entry is always on.
    pub fn shows_anything(&self) -> bool {
        self.overlay || Entry::ALL.iter().any(|&e| self.is_enabled(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_sort_by_path() {
        for &entry in Entry::ALL {
            assert_eq!(Entry::from_id(&entry.id()), Some(entry));
        }
        assert_eq!(Entry::from_id("fps"), Some(Entry::Fps));
        assert_eq!(Entry::from_id("minecraft:nope"), None);
        let mut sorted = vec![
            Entry::Tps,
            Entry::FrameProfile,
            Entry::Fps,
            Entry::ThreeDimensionalCrosshair,
        ];
        sorted.sort_by(|a, b| a.cmp_id(*b));
        assert_eq!(
            sorted,
            [
                Entry::ThreeDimensionalCrosshair,
                Entry::Fps,
                Entry::FrameProfile,
                Entry::Tps
            ]
        );
    }

    #[test]
    fn the_default_profile_shows_its_entries_only_in_the_overlay() {
        let mut entries = DebugEntries::default();
        assert!(entries.is_using_profile(Profile::Default));
        assert!(!entries.shows_anything());
        entries.toggle_overlay();
        assert!(entries.is_enabled(Entry::Fps));
        assert!(!entries.is_enabled(Entry::Biome));
        entries.set_status(Entry::LocalDifficulty, Status::AlwaysOn);
        assert!(entries.is_enabled(Entry::LocalDifficulty));
        assert!(Entry::ALL.iter().all(|e| e.works()));
        assert!(entries.profile.is_none());
    }

    #[test]
    fn toggling_follows_whether_the_overlay_is_open() {
        let mut entries = DebugEntries::default();
        assert!(entries.toggle_status(Entry::EntityHitboxes));
        assert_eq!(entries.status(Entry::EntityHitboxes), Status::AlwaysOn);
        assert!(!entries.toggle_status(Entry::EntityHitboxes));
        entries.toggle_overlay();
        assert!(entries.toggle_status(Entry::ChunkBorders));
        assert_eq!(entries.status(Entry::ChunkBorders), Status::InOverlay);
        assert!(!entries.toggle_status(Entry::ChunkBorders));
        let mut performance = DebugEntries::default();
        performance.load_profile(Profile::Performance);
        assert!(performance.shows_anything(), "fps is always on");
    }

    #[test]
    fn the_file_keeps_a_profile_or_the_entries() {
        let dir = std::env::temp_dir().join(format!("melon-debug-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut entries = DebugEntries::load(&dir);
        entries.load_profile(Profile::Performance);
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert_eq!(text, r#"{"profile":"performance"}"#);
        entries.set_status(Entry::Biome, Status::AlwaysOn);
        let back = DebugEntries::load(&dir);
        assert_eq!(back.status(Entry::Biome), Status::AlwaysOn);
        assert_eq!(back.status(Entry::Fps), Status::AlwaysOn);
        assert_eq!(back.status(Entry::Heightmap), Status::Never);
        assert!(back.profile.is_none());
        std::fs::write(dir.join(FILE), "nope").unwrap();
        assert!(DebugEntries::load(&dir).is_using_profile(Profile::Default));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
