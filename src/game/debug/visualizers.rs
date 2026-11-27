//! The F3 + F6 renderers that draw into the world besides hitboxes and
//! chunk borders (vanilla `client/renderer/debug/*Renderer`): collision
//! boxes, supporting blocks, heightmaps, solid faces, water levels, light
//! levels and light sections, and the occlusion walk's paths, section
//! visibility and culling tree, and the integrated server's chunks.

use std::time::{Duration, Instant};

use glam::{DVec3, IVec3, Vec3};
use pumpkin_data::BlockStateId;
use pumpkin_data::tag::{self, Taggable};

use crate::game::InWorld;
use crate::game::gizmos::{DEFAULT_WIDTH, Fill, Gizmos, Line, TEXT_SCALE, WorldText};
use melon_gui::hud::debug_entries::Entry;
use melon_core::math::argb;
use melon_world::block::direction::Direction;
use melon_world::culling::frustum::Frustum;
use melon_world::fluid::{FluidKind, fluid_of};
use melon_world::physics::{self, Aabb};

/// `CollisionBoxRenderer`'s and `SupportBlockRenderer`'s 100 ms.
const REFRESH: Duration = Duration::from_millis(100);
/// `LightSectionDebugRenderer.REFRESH_INTERVAL`.
const LIGHT_SECTION_REFRESH: Duration = Duration::from_millis(500);
/// `ChunkDebugRenderer`: every 3 s.
const CHUNK_REFRESH: Duration = Duration::from_secs(3);

/// What the slower renderers keep between frames, as vanilla's do.
#[derive(Default)]
pub struct VisualizerCache {
    collisions: Option<(Instant, Vec<Aabb>)>,
    /// The heightmap boxes, for the camera's chunk and the level's revision.
    heightmap: Option<((i32, i32, u64), Vec<Fill>)>,
    light_sections: Option<(Instant, IVec3, Vec<Fill>, Vec<Line>)>,
    chunks: Option<(Instant, Vec<WorldText>)>,
}

/// `ARGB.srgbLerp`, on the packed channels as they are.
fn lerp_color(t: f32, from: u32, to: u32) -> [f32; 4] {
    let (a, b) = (argb(from), argb(to));
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// `Mth.hsvToRgb`, opaque.
fn hsv(hue: f32, saturation: f32, value: f32) -> [f32; 4] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let sector = h.floor() as i32 % 6;
    let f = h - h.floor();
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - f * saturation);
    let t = value * (1.0 - (1.0 - f) * saturation);
    let (r, g, b) = match sector {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    [r, g, b, 1.0]
}

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

/// A block's corner as a point.
fn corner(x: i32, y: i32, z: i32) -> DVec3 {
    DVec3::new(f64::from(x), f64::from(y), f64::from(z))
}

impl InWorld {
    /// Everything the entries below ask for this frame, into `gizmos`.
    pub(in crate::game) fn visualizer_gizmos(
        &mut self,
        gizmos: &mut Gizmos,
        camera: DVec3,
        frustum: &Frustum,
    ) {
        let entries = self.debug.entries();
        let on = |entry: Entry| entries.is_enabled(entry);
        let wanted = [
            on(Entry::VisualizeCollisionBoxes),
            on(Entry::VisualizeEntitySupportingBlocks),
            on(Entry::VisualizeHeightmap),
            on(Entry::VisualizeSolidFaces),
            on(Entry::VisualizeWaterLevels),
            on(Entry::VisualizeBlockLightLevels),
            on(Entry::VisualizeSkyLightLevels),
            on(Entry::VisualizeSkyLightSections),
            on(Entry::ChunkSectionPaths),
            on(Entry::ChunkSectionVisibility),
            on(Entry::ChunkSectionOctree),
            on(Entry::VisualizeChunksOnServer),
        ];
        drop(entries);
        let [
            collisions,
            support,
            heightmap,
            solid,
            water,
            block_light,
            sky_light,
            light_sections,
            paths,
            visibility,
            octree,
            chunks,
        ] = wanted;
        if collisions {
            self.collision_gizmos(gizmos);
        }
        if support {
            self.support_gizmos(gizmos);
        }
        if heightmap {
            self.heightmap_gizmos(gizmos, camera);
        }
        if solid {
            self.solid_face_gizmos(gizmos, camera);
        }
        if water {
            self.water_gizmos(gizmos);
        }
        if block_light || sky_light {
            self.light_gizmos(gizmos, camera, block_light, sky_light);
        }
        if light_sections {
            self.light_section_gizmos(gizmos);
        }
        if paths || visibility {
            self.section_gizmos(gizmos, camera, frustum, paths, visibility);
        }
        if octree {
            self.culling_tree_gizmos(gizmos, camera, frustum);
        }
        if chunks {
            self.chunk_gizmos(gizmos, camera);
        }
    }

    /// The player's box, the base of the two renderers below.
    fn player_box(&self) -> Aabb {
        let (min, max) = self.player.render_bounds();
        Aabb { min, max }
    }

    /// `CollisionBoxRenderer`: every collision box within 6 blocks of the
    /// player's, white, gathered every 100 ms.
    fn collision_gizmos(&mut self, gizmos: &mut Gizmos) {
        let now = Instant::now();
        let stale = self
            .visualizers
            .collisions
            .as_ref()
            .is_none_or(|(at, _)| now.duration_since(*at) > REFRESH);
        if stale {
            let around = self.player_box();
            let around = Aabb {
                min: around.min - DVec3::splat(6.0),
                max: around.max + DVec3::splat(6.0),
            };
            let mut boxes = physics::block_boxes(&self.level, &around);
            boxes.extend(self.border.collision_boxes(&around));
            self.visualizers.collisions = Some((now, boxes));
        }
        if let Some((_, boxes)) = &self.visualizers.collisions {
            for b in boxes {
                gizmos.cuboid(b, argb(0xFFFF_FFFF), DEFAULT_WIDTH);
            }
        }
    }

    /// `SupportBlockRenderer`: the block the player stands on (red), and
    /// the one its effects come from (`getOnPosLegacy`, cyan) when another.
    /// Other entities move only on the server here, so only the player has one.
    fn support_gizmos(&self, gizmos: &mut Gizmos) {
        let Some(supporting) = self.player.supporting_block() else {
            return;
        };
        let feet = self.player.feet();
        let on = melon_world::block_motion::on_pos(&self.level, feet, Some(supporting), 1.0e-5);
        self.highlight(gizmos, on, 0.02, 0xFFFF_0000);
        let legacy = melon_world::block_motion::on_pos(&self.level, feet, Some(supporting), 0.2);
        if legacy != on {
            self.highlight(gizmos, legacy, 0.04, 0xFF00_FFFF);
        }
    }

    /// `SupportBlockRenderer.highlightPosition`.
    fn highlight(&self, gizmos: &mut Gizmos, (x, y, z): (i32, i32, i32), offset: f64, color: u32) {
        let from = corner(x, y, z) - DVec3::splat(2.0 * offset);
        let to = from + DVec3::splat(1.0 + 4.0 * offset);
        gizmos.cuboid(
            &Aabb { min: from, max: to },
            with_alpha(argb(color), 0.4),
            DEFAULT_WIDTH,
        );
        for shape in physics::block_shapes(&self.level, (x, y, z)) {
            gizmos.cuboid(&shape, argb(color), DEFAULT_WIDTH);
        }
    }

    /// `HeightMapRenderer`: a thin box on each column's height of the
    /// client's heightmaps, 2 chunks around. Melon works the heights out
    /// from the blocks, as `client_heightmaps` does for F3; they are kept
    /// until the camera's chunk or the blocks change. Only the tops are
    /// drawn: 19 200 boxes' sides would be millions of vertices.
    fn heightmap_gizmos(&mut self, gizmos: &mut Gizmos, camera: DVec3) {
        let (cx, cz) = (
            (camera.x.floor() as i32) >> 4,
            (camera.z.floor() as i32) >> 4,
        );
        let key = (cx, cz, self.level.revision());
        if self
            .visualizers
            .heightmap
            .as_ref()
            .is_none_or(|(k, _)| *k != key)
        {
            let fills = heightmap_tops(&self.level, cx, cz);
            self.visualizers.heightmap = Some((key, fills));
        }
        if let Some((_, fills)) = &self.visualizers.heightmap {
            gizmos.extend_fills(fills);
        }
    }

    /// `SolidFaceRenderer`: of each block within 6, the faces that are
    /// sturdy, translucent red over its outline.
    fn solid_face_gizmos(&self, gizmos: &mut Gizmos, camera: DVec3) {
        let at = camera.floor().as_ivec3();
        let color = argb(0x80FF_0000);
        for y in at.y - 6..=at.y + 6 {
            for z in at.z - 6..=at.z + 6 {
                for x in at.x - 6..=at.x + 6 {
                    let id = self.level.block_state(x, y, z).unwrap_or(0);
                    let Some(state) = BlockStateId::new(id).filter(|s| s.as_u16() != 0) else {
                        continue;
                    };
                    let state = state.to_state();
                    let sturdy: Vec<Direction> = Direction::ALL
                        .into_iter()
                        .filter(|d| state.is_side_solid(block_direction(*d)))
                        .collect();
                    if sturdy.is_empty() {
                        continue;
                    }
                    for shape in melon_world::pick::outline_shapes(&self.level, (x, y, z)) {
                        let (min, max) = (
                            shape.min - DVec3::splat(0.002),
                            shape.max + DVec3::splat(0.002),
                        );
                        for &face in &sturdy {
                            gizmos.face(min, max, face, color);
                        }
                    }
                }
            }
        }
    }

    /// `WaterDebugRenderer`: each water block within 10 filled green to
    /// its height, with its amount on top.
    fn water_gizmos(&self, gizmos: &mut Gizmos) {
        let at = self.player.feet().floor().as_ivec3();
        for y in at.y - 10..=at.y + 10 {
            for z in at.z - 10..=at.z + 10 {
                for x in at.x - 10..=at.x + 10 {
                    let id = self.level.block_state(x, y, z).unwrap_or(0);
                    let Some(fluid) = fluid_of(id).filter(|f| f.kind == FluidKind::Water) else {
                        continue;
                    };
                    let height =
                        physics::fluid_height(&self.level, IVec3::new(x, y, z), FluidKind::Water)
                            .unwrap_or(0.0);
                    let top = f64::from(y) + f64::from(height);
                    gizmos.filled_cuboid(
                        &Aabb {
                            min: corner(x, y, z) + DVec3::splat(0.01),
                            max: DVec3::new(f64::from(x) + 0.99, top, f64::from(z) + 0.99),
                        },
                        [0.0, 1.0, 0.0, 0.15],
                    );
                    gizmos.text(
                        DVec3::new(f64::from(x) + 0.5, top, f64::from(z) + 0.5),
                        fluid.amount.to_string(),
                        argb(0xFF00_0000),
                        TEXT_SCALE,
                    );
                }
            }
        }
    }

    /// `LightDebugRenderer`: the light within 10 as numbers, sky light
    /// (blue to cyan) where it is not full, block light (red to yellow)
    /// where there is any, and each section's sky light storage
    /// ([`melon_world::level::Level::light_section_type`]) at its middle.
    fn light_gizmos(&self, gizmos: &mut Gizmos, camera: DVec3, block: bool, sky: bool) {
        let at = camera.floor().as_ivec3();
        let mut sections = Vec::new();
        for y in at.y - 10..=at.y + 10 {
            for z in at.z - 10..=at.z + 10 {
                for x in at.x - 10..=at.x + 10 {
                    let section = (x >> 4, y >> 4, z >> 4);
                    if !sections.contains(&section) {
                        sections.push(section);
                        let middle =
                            corner(section.0 * 16 + 8, section.1 * 16 + 8, section.2 * 16 + 8);
                        let kind = self.level.light_section_type(true, section);
                        gizmos.text(middle, kind, argb(0xFFFF_0000), 4.8);
                    }
                    let (block_light, sky_light) = self.level.light_at(x, y, z);
                    if sky && sky_light != 15 {
                        let color =
                            lerp_color(f32::from(sky_light) / 15.0, 0xFF00_00FF, 0xFF00_FFFF);
                        gizmos.text(
                            DVec3::new(f64::from(x) + 0.5, f64::from(y) + 0.25, f64::from(z) + 0.5),
                            sky_light.to_string(),
                            color,
                            TEXT_SCALE,
                        );
                    }
                    if block && block_light != 0 {
                        let color =
                            lerp_color(f32::from(block_light) / 15.0, 0xFFAA_0000, 0xFFFF_FF00);
                        gizmos.text(
                            corner(x, y, z) + DVec3::splat(0.5),
                            block_light.to_string(),
                            color,
                            TEXT_SCALE,
                        );
                    }
                }
            }
        }
    }

    /// `LightSectionDebugRenderer` for sky light: the sections within 10
    /// that keep light and blocks (yellow) or light only (dark green), as
    /// one shape each, outlined, refreshed every 500 ms.
    fn light_section_gizmos(&mut self, gizmos: &mut Gizmos) {
        let now = Instant::now();
        let feet = self.player.feet().floor().as_ivec3();
        let center = IVec3::new(feet.x >> 4, feet.y >> 4, feet.z >> 4);
        let stale = self
            .visualizers
            .light_sections
            .as_ref()
            .is_none_or(|(at, c, _, _)| {
                now.duration_since(*at) > LIGHT_SECTION_REFRESH || *c != center
            });
        if stale {
            const RADIUS: i32 = 10;
            let size = (2 * RADIUS + 1) as usize;
            let mut both = vec![false; size * size * size];
            let mut light = vec![false; size * size * size];
            for z in 0..size {
                for y in 0..size {
                    for x in 0..size {
                        let pos = (
                            center.x + x as i32 - RADIUS,
                            center.y + y as i32 - RADIUS,
                            center.z + z as i32 - RADIUS,
                        );
                        let i = (z * size + y) * size + x;
                        match self.level.light_section_type(true, pos) {
                            "0" => {
                                both[i] = true;
                                light[i] = true;
                            }
                            "1" => light[i] = true,
                            _ => {}
                        }
                    }
                }
            }
            let min = center - IVec3::splat(RADIUS);
            let both_color = [1.0, 1.0, 0.0, 0.25];
            let light_color = [0.25, 0.125, 0.0, 0.125];
            let mut fills = Vec::new();
            let mut lines = Vec::new();
            let shape_both = VoxelShape { cells: &both, size };
            let shape_light = VoxelShape {
                cells: &light,
                size,
            };
            shape_both.edges(min, with_alpha(both_color, 1.0), &mut lines);
            shape_light.edges(min, with_alpha(light_color, 1.0), &mut lines);
            shape_both.faces(min, both_color, &mut fills);
            shape_light.faces(min, light_color, &mut fills);
            self.visualizers.light_sections = Some((now, center, fills, lines));
        }
        if let Some((_, _, fills, lines)) = &self.visualizers.light_sections {
            gizmos.extend_fills(fills);
            gizmos.extend_lines(lines);
        }
    }

    /// `ChunkCullingDebugRenderer` over Melon's occlusion walk: for each
    /// section it reached in view, lines back the way it came in, coloured
    /// by how many steps from the camera (`chunk_section_paths`); and for
    /// each drawn section, which of its faces can't see each other
    /// (`chunk_section_visibility`).
    fn section_gizmos(
        &self,
        gizmos: &mut Gizmos,
        camera: DVec3,
        frustum: &Frustum,
        paths: bool,
        visibility: bool,
    ) {
        let Some(mask) = self.occlusion.mask() else {
            return;
        };
        let (min_y, _) = self.level.height();
        let middle =
            |(x, y, z): (i32, i32, i32)| corner(x * 16 + 8, min_y + y * 16 + 8, z * 16 + 8);
        let in_view = |(x, y, z): (i32, i32, i32)| {
            let min = IVec3::new(x * 16, min_y + y * 16, z * 16).as_dvec3();
            let min = (min - camera).as_vec3();
            frustum.sees_box(min, min + Vec3::splat(16.0))
        };
        if paths {
            for pos in mask.traced_sections().filter(|&p| in_view(p)) {
                let Some((step, entered)) = mask.trace(pos) else {
                    continue;
                };
                let color = if step == 0 {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    hsv(f32::from(step) / 50.0, 0.9, 0.9)
                };
                let from = middle(pos);
                for direction in Direction::ALL {
                    if entered & direction.bit() == 0 {
                        continue;
                    }
                    let [dx, dy, dz] = direction.step();
                    let back = from - corner(dx, dy, dz) * 16.0;
                    gizmos.line(from, back, color, DEFAULT_WIDTH);
                }
            }
        }
        if visibility {
            for ((cx, cz), filled) in self.meshes.filled_columns() {
                for y in 0..32 {
                    let pos = (cx, y, cz);
                    if filled & (1 << y) == 0 || !mask.contains(pos) || !in_view(pos) {
                        continue;
                    }
                    let melon_world::culling::occlusion::SectionState::Loaded(set) =
                        self.level.section_state(pos)
                    else {
                        continue;
                    };
                    let center = middle(pos);
                    let mut blocked = 0;
                    for a in Direction::ALL {
                        for b in Direction::ALL {
                            if set.visible_between(a, b) {
                                continue;
                            }
                            blocked += 1;
                            let [ax, ay, az] = a.step();
                            let [bx, by, bz] = b.step();
                            gizmos.line(
                                center + corner(ax, ay, az) * 8.0,
                                center + corner(bx, by, bz) * 8.0,
                                [1.0, 0.0, 0.0, 1.0],
                                DEFAULT_WIDTH,
                            );
                        }
                    }
                    if blocked > 0 {
                        gizmos.filled_cuboid(
                            &Aabb {
                                min: center - DVec3::splat(7.5),
                                max: center + DVec3::splat(7.5),
                            },
                            [0.9, 0.9, 0.0, 0.2],
                        );
                    }
                }
            }
        }
    }

    /// `OctreeDebugRenderer`'s Melon counterpart. Vanilla culls through an
    /// octree of sections; Melon culls a chunk column's box first, then its
    /// sections. Each column in view is drawn (faint when wholly inside the
    /// frustum, as vanilla draws a fully visible node), then its sections,
    /// numbered in the order culled, green within 32 blocks (`isClose`).
    fn culling_tree_gizmos(&self, gizmos: &mut Gizmos, camera: DVec3, frustum: &Frustum) {
        let (min_y, _) = self.level.height();
        let component = |size: f32, multiplier: f32| (multiplier * size).fract() * 0.9 + 0.1;
        let color = |size: f32, faint: bool| {
            [
                component(size, 0.3),
                component(size, 0.8),
                component(size, 0.5),
                if faint { 0.4 } else { 1.0 },
            ]
        };
        let mut count = 0;
        let mut columns: Vec<_> = self.meshes.filled_columns().collect();
        columns.sort_unstable_by_key(|&(pos, _)| pos);
        for ((cx, cz), filled) in columns {
            let bottom = |y: u32| (min_y + y as i32 * 16) as f32;
            let min = Vec3::new(
                (cx * 16) as f32,
                bottom(filled.trailing_zeros()),
                (cz * 16) as f32,
            );
            let max = Vec3::new(
                min.x + 16.0,
                bottom(32 - filled.leading_zeros()),
                min.z + 16.0,
            );
            // The frustum is the camera's, relative to it.
            let relative = |p: Vec3| (p.as_dvec3() - camera).as_vec3();
            if !frustum.sees_box(relative(min), relative(max)) {
                continue;
            }
            let whole = frustum.contains_box(relative(min), relative(max));
            gizmos.cuboid(
                &Aabb {
                    min: min.as_dvec3(),
                    max: max.as_dvec3(),
                },
                color(7.0, whole),
                DEFAULT_WIDTH,
            );
            for y in 0..32 {
                if filled & (1 << y) == 0 {
                    continue;
                }
                let lo = Vec3::new(min.x, bottom(y), min.z);
                let hi = lo + Vec3::splat(16.0);
                if !whole && !frustum.sees_box(relative(lo), relative(hi)) {
                    continue;
                }
                count += 1;
                let section = Aabb {
                    min: lo.as_dvec3() + DVec3::splat(0.1),
                    max: hi.as_dvec3() - DVec3::splat(0.1),
                };
                gizmos.cuboid(
                    &section,
                    color(6.0, whole || frustum.contains_box(lo, hi)),
                    DEFAULT_WIDTH,
                );
                let centre = (lo + hi).as_dvec3() / 2.0;
                let close = (centre - camera).abs().max_element() <= 32.0;
                gizmos.text(
                    centre,
                    count.to_string(),
                    argb(if close { 0xFF00_FF00 } else { 0xFFFF_FFFF }),
                    4.8,
                );
            }
        }
    }

    /// `ChunkDebugRenderer`: for each chunk within 12, whether the client
    /// has it and the integrated server's state of it, every 3 s.
    fn chunk_gizmos(&mut self, gizmos: &mut Gizmos, camera: DVec3) {
        let now = Instant::now();
        let stale = self
            .visualizers
            .chunks
            .as_ref()
            .is_none_or(|(at, _)| now.duration_since(*at) > CHUNK_REFRESH);
        if stale {
            let server = self.server_world();
            let (cx, cz) = (
                (camera.x.floor() as i32) >> 4,
                (camera.z.floor() as i32) >> 4,
            );
            let y = camera.y * 0.85;
            let mut texts = Vec::new();
            for x in cx - 12..=cx + 12 {
                for z in cz - 12..=cz + 12 {
                    let client = if self.level.is_loaded_at(x * 16, z * 16) {
                        "Client: "
                    } else {
                        "Client: 0n/a"
                    };
                    let mut lines = vec![client.to_owned()];
                    if let Some(world) = &server {
                        lines.push(format!(
                            "Server: {}",
                            super::server::chunk_state(world, x, z)
                        ));
                    }
                    for (i, line) in lines.into_iter().enumerate() {
                        texts.push(WorldText {
                            at: DVec3::new(
                                f64::from(x * 16 + 8),
                                y - 2.0 * i as f64,
                                f64::from(z * 16 + 8),
                            ),
                            text: line,
                            color: argb(0xFFFF_FFFF),
                            scale: 2.4,
                        });
                    }
                }
            }
            self.visualizers.chunks = Some((now, texts));
        }
        if let Some((_, texts)) = &self.visualizers.chunks {
            gizmos.extend_texts(texts);
        }
    }
}

/// [`Direction`] as Pumpkin's.
fn block_direction(direction: Direction) -> pumpkin_data::BlockDirection {
    use pumpkin_data::BlockDirection as B;
    match direction {
        Direction::Down => B::Down,
        Direction::Up => B::Up,
        Direction::North => B::North,
        Direction::South => B::South,
        Direction::West => B::West,
        Direction::East => B::East,
    }
}

/// `HeightMapRenderer`'s boxes for the 5×5 chunks around `(cx, cz)`, the
/// client's three heightmaps: their tops, offset by the type's ordinal.
fn heightmap_tops(level: &melon_world::level::Level, cx: i32, cz: i32) -> Vec<Fill> {
    let (min_y, sections) = level.height();
    let top = min_y + sections * 16;
    // `Heightmap.Types` ordinal and `HeightMapRenderer.getColor`.
    let kinds: [(u32, [f32; 3]); 3] = [
        (1, [0.0, 0.7, 0.0]),
        (4, [0.0, 0.3, 0.3]),
        (5, [0.0, 0.5, 0.5]),
    ];
    let mut out = Vec::new();
    for chunk_x in cx - 2..=cx + 2 {
        for chunk_z in cz - 2..=cz + 2 {
            if !level.is_loaded_at(chunk_x * 16, chunk_z * 16) {
                continue;
            }
            for lx in 0..16 {
                for lz in 0..16 {
                    let (x, z) = (chunk_x * 16 + lx, chunk_z * 16 + lz);
                    let heights = column_heights(level, x, z, min_y, top);
                    for ((ordinal, [r, g, b]), height) in kinds.iter().zip(heights) {
                        let y = f64::from(height) + f64::from(*ordinal) * 0.093_75 + 0.093_75;
                        let (x0, z0) = (f64::from(x) + 0.25, f64::from(z) + 0.25);
                        out.push(Fill {
                            corners: [
                                DVec3::new(x0, y, z0),
                                DVec3::new(x0, y, z0 + 0.5),
                                DVec3::new(x0 + 0.5, y, z0 + 0.5),
                                DVec3::new(x0 + 0.5, y, z0),
                            ],
                            color: [*r, *g, *b, 1.0],
                        });
                    }
                }
            }
        }
    }
    out
}

/// World surface, motion blocking and motion blocking without leaves: one
/// above the highest block that counts, in one walk down.
fn column_heights(
    level: &melon_world::level::Level,
    x: i32,
    z: i32,
    min_y: i32,
    top: i32,
) -> [i32; 3] {
    let mut found: [Option<i32>; 3] = [None; 3];
    for y in (min_y..top).rev() {
        let id = level.block_state(x, y, z).unwrap_or(0);
        if id == 0 {
            continue;
        }
        let block = pumpkin_data::Block::from_state_id(BlockStateId::new_or_air(id));
        let fluid = fluid_of(id).is_some();
        if found[0].is_none() && !block.default_state.is_air() {
            found[0] = Some(y + 1);
        }
        if found[1].is_none()
            && (fluid || block.has_tag(&tag::Block::MINECRAFT_BLOCKS_MOTION_IN_HEIGHTMAP))
        {
            found[1] = Some(y + 1);
        }
        if found[2].is_none()
            && (fluid || block.has_tag(&tag::Block::MINECRAFT_BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES))
        {
            found[2] = Some(y + 1);
        }
        if found.iter().all(Option::is_some) {
            break;
        }
    }
    found.map(|h| h.unwrap_or(min_y))
}

/// `BitSetDiscreteVoxelShape` of sections, a cube of `size` a side.
struct VoxelShape<'a> {
    cells: &'a [bool],
    size: usize,
}

impl VoxelShape<'_> {
    fn filled(&self, x: i32, y: i32, z: i32) -> bool {
        let s = self.size as i32;
        if !(0..s).contains(&x) || !(0..s).contains(&y) || !(0..s).contains(&z) {
            return false;
        }
        self.cells[((z * s + y) * s + x) as usize]
    }

    /// `forAllFaces`: the outside faces, a section (16 blocks) each.
    fn faces(&self, min: IVec3, color: [f32; 4], out: &mut Vec<Fill>) {
        let s = self.size as i32;
        let mut gizmos = Gizmos::default();
        for z in 0..s {
            for y in 0..s {
                for x in 0..s {
                    if !self.filled(x, y, z) {
                        continue;
                    }
                    let lo = (min + IVec3::new(x, y, z)) * 16;
                    let lo = lo.as_dvec3();
                    for face in Direction::ALL {
                        let [dx, dy, dz] = face.step();
                        if !self.filled(x + dx, y + dy, z + dz) {
                            gizmos.face(lo, lo + DVec3::splat(16.0), face, color);
                        }
                    }
                }
            }
        }
        out.extend(gizmos.take_fills());
    }

    /// `forAllEdges`: the shape's edges, where its outside bends; a unit
    /// edge is one when of the four cells around it one or three are
    /// filled, or two across from each other.
    fn edges(&self, min: IVec3, color: [f32; 4], out: &mut Vec<Line>) {
        let s = self.size as i32;
        let base = (min * 16).as_dvec3();
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for a in 0..s {
                for i in 0..=s {
                    for j in 0..=s {
                        let cell = |du: i32, dv: i32| {
                            let mut p = [0; 3];
                            p[axis] = a;
                            p[u] = i - du;
                            p[v] = j - dv;
                            self.filled(p[0], p[1], p[2])
                        };
                        let around = [cell(0, 0), cell(1, 0), cell(1, 1), cell(0, 1)];
                        let n = around.iter().filter(|&&f| f).count();
                        let crease = n == 1 || n == 3 || (n == 2 && around[0] == around[2]);
                        if !crease {
                            continue;
                        }
                        let mut p = [0.0; 3];
                        p[axis] = f64::from(a) * 16.0;
                        p[u] = f64::from(i) * 16.0;
                        p[v] = f64::from(j) * 16.0;
                        let from = base + DVec3::from_array(p);
                        let mut step = [0.0; 3];
                        step[axis] = 16.0;
                        out.push(Line {
                            a: from,
                            b: from + DVec3::from_array(step),
                            color,
                            width: DEFAULT_WIDTH,
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_section_is_six_faces_and_twelve_edges() {
        let cells = [true];
        let shape = VoxelShape {
            cells: &cells,
            size: 1,
        };
        let mut fills = Vec::new();
        shape.faces(IVec3::ZERO, [1.0; 4], &mut fills);
        assert_eq!(fills.len(), 6);
        let mut lines = Vec::new();
        shape.edges(IVec3::ZERO, [1.0; 4], &mut lines);
        assert_eq!(lines.len(), 12);
    }

    #[test]
    fn two_sections_side_by_side_share_no_face_and_no_seam() {
        // 2×1×1: x = 0 and x = 1 filled.
        let cells = [true, true, false, false, false, false, false, false];
        let shape = VoxelShape {
            cells: &cells,
            size: 2,
        };
        let mut fills = Vec::new();
        shape.faces(IVec3::ZERO, [1.0; 4], &mut fills);
        assert_eq!(fills.len(), 10);
        let mut lines = Vec::new();
        shape.edges(IVec3::ZERO, [1.0; 4], &mut lines);
        // A 2×1×1 box: 4 long edges of two units, 8 short ones.
        assert_eq!(lines.len(), 4 * 2 + 8);
    }

    #[test]
    fn hues_go_round() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [1.0, 0.0, 0.0, 1.0]);
        let green = hsv(1.0 / 3.0, 1.0, 1.0);
        assert!((green[1] - 1.0).abs() < 1e-6 && green[0] < 1e-5);
    }
}
