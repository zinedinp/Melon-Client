//! The terrain's draws, on the fastest of three paths the device allows.
//! Each draws the same picture; a faster one only saves CPU work, and the
//! counted one also skips sections the depth hides.
//!
//! | Path | Needs | How |
//! |---|---|---|
//! | [`TerrainPath::PerDraw`] | Vulkan 1.0 | one `vkCmdDrawIndexed` per run of a chunk's quads, its offset pushed |
//! | [`TerrainPath::MultiDraw`] | `multiDrawIndirect`, `drawIndirectFirstInstance` (1.0 features) | the quad meshes' commands gathered before the frame (`indirect.rs`), one `vkCmdDrawIndexedIndirect` per buffer run |
//! | [`TerrainPath::Counted`] | multi-draw, `drawIndirectCount` (core 1.2 or `VK_KHR_draw_indirect_count`), `MELON_OCCLUSION=1` | two-phase GPU occlusion culling (`occlusion.rs`) packs the commands, `vkCmdDrawIndexedIndirectCount` draws what it kept |
//!
//! Vertex meshes (lava) and translucent terrain always take the per-draw path;
//! the translucent sections each through their own index list, sorted far
//! to near (vanilla's sorted index buffers), so a sort uploads only that.

use crate::renderer::pipeline::world::{DrawLight, FogInput, Mode};
use crate::renderer::{
    ChunkDraw, IndexMeshId, MeshId, QuadMeshId, RenderLayer, Renderer, TerrainMesh, WorldDraw,
};
use ash::vk;
use glam::{Mat4, Vec3};

use super::super::super::indirect::{Pass, Phase, Run, corner_offset, quad_draws};
use super::super::super::mesh_arena::wide_indices;
use crate::renderer::pipeline;

/// How the terrain's quad meshes are drawn, fixed when the renderer is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::renderer) enum TerrainPath {
    PerDraw,
    MultiDraw,
    Counted,
}

impl TerrainPath {
    pub(in crate::renderer) fn name(self) -> &'static str {
        match self {
            Self::PerDraw => "per draw",
            Self::MultiDraw => "multi-draw",
            Self::Counted => "multi-draw with GPU counts (occlusion culling)",
        }
    }
}

impl Renderer {
    /// The path [`Self::record_terrain`] takes.
    pub(in crate::renderer) fn terrain_path(&self) -> TerrainPath {
        match (&self.indirect, &self.occlusion) {
            (Some(_), Some(_)) => TerrainPath::Counted,
            (Some(_), None) => TerrainPath::MultiDraw,
            (None, _) => TerrainPath::PerDraw,
        }
    }

    /// The opaque terrain (`layer` [`RenderLayer::Terrain`]: solid, then
    /// cutout) or the translucent one, on the device's path.
    pub(super) unsafe fn record_terrain(
        &self,
        cmd: vk::CommandBuffer,
        world: &WorldDraw,
        layer: RenderLayer,
        view_proj: Mat4,
    ) {
        let passes: &[(Mode, &[ChunkDraw])] = if layer == RenderLayer::Translucent {
            &[(Mode::TranslucentTerrain, &world.translucent)]
        } else {
            &[(Mode::Solid, &world.chunks), (Mode::Opaque, &world.cutout)]
        };
        let light = world.light([1.0; 4]);
        let multi = self.indirect.is_some().then_some(Phase::First);
        // Phase 2 draws the rest: what the culling let through and the
        // vertex meshes (lava), which it doesn't cull.
        let occluding = layer == RenderLayer::Terrain && self.recording.occluding.get();
        for &layer_draws in passes {
            let at = (view_proj, &light);
            self.draw_terrain_layer(cmd, world, layer_draws, at, multi, !occluding);
        }
        if occluding {
            self.cull_terrain(cmd, view_proj);
            for &layer_draws in passes {
                let at = (view_proj, &light);
                let phase = Some(Phase::Second);
                self.draw_terrain_layer(cmd, world, layer_draws, at, phase, true);
            }
        }
    }

    /// One terrain layer's draws: `phase`'s multi-draws, when there are
    /// any, and with `rest` the draws they leave out.
    unsafe fn draw_terrain_layer(
        &self,
        cmd: vk::CommandBuffer,
        world: &WorldDraw,
        (mode, chunks): (Mode, &[ChunkDraw]),
        (view_proj, light): (Mat4, &DrawLight),
        phase: Option<Phase>,
        rest: bool,
    ) {
        let by_multi = phase.is_some() && matches!(mode, Mode::Solid | Mode::Opaque);
        let label = match mode {
            Mode::Solid => "solid",
            Mode::Opaque => "cutout",
            Mode::TranslucentTerrain => "translucent",
            _ => "terrain",
        };
        self.gpu_timer.counted_pass(label);
        if let Some(phase) = phase
            && by_multi
        {
            self.draw_terrain_multi(cmd, world, mode, view_proj, light, phase);
        }
        let mut bound: Option<bool> = None;
        let mut own_indices = false;
        for chunk in chunks.iter().filter(|_| rest) {
            let quads = matches!(chunk.mesh, TerrainMesh::Quads(_));
            if quads && by_multi {
                continue;
            }
            if bound != Some(quads) {
                let ok = if quads {
                    self.bind_terrain_pass(cmd, world.atlas, mode)
                } else {
                    self.bind_world_pass(cmd, world.atlas, mode)
                };
                if !ok {
                    bound = None;
                    continue;
                }
                bound = Some(quads);
            }
            let at = (view_proj, world.offset(chunk.origin));
            match chunk.mesh {
                TerrainMesh::Quads(id) => {
                    self.draw_terrain_quads(cmd, id, at, light, &chunk.range.into());
                }
                TerrainMesh::Vertices(id) => self.draw_world_mesh(
                    cmd,
                    self.pipelines.world.layout,
                    id,
                    at,
                    &light.fogged(FogInput::Chunk),
                    Some(&chunk.range.into()),
                ),
                TerrainMesh::Sorted { vertices, indices } => {
                    let light = light.fogged(FogInput::Chunk);
                    own_indices |= self.draw_sorted_mesh(cmd, (vertices, indices), at, &light);
                }
            }
        }
        if own_indices {
            // What follows draws through the shared quad indices again.
            self.device.cmd_bind_index_buffer(
                cmd,
                self.meshes.index_buffer(),
                0,
                vk::IndexType::UINT16,
            );
        }
        self.gpu_timer.mark(&self.device, label);
    }

    /// Between the culling's two phases: ends the render pass, picks phase
    /// 2's draws against what phase 1 drew (see [`super::super::super::occlusion`]),
    /// and starts the pass again on top of it.
    unsafe fn cull_terrain(&self, cmd: vk::CommandBuffer, view_proj: Mat4) {
        self.gpu_timer.close_pass();
        self.device.cmd_end_render_pass(cmd);
        if let (Some(occlusion), Some(indirect)) = (&self.occlusion, &self.indirect) {
            self.gpu_timer.pass("occlusion");
            let draws = (indirect.gathered(), indirect.run_count());
            occlusion.record_cull(&self.device, cmd, self.frames.current, draws, &view_proj);
            self.gpu_timer.mark(&self.device, "occlusion");
        }
        let extent = self.swapchain.extent;
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        self.device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(self.passes.resume_keep)
                .framebuffer(self.recording.framebuffer.get())
                .render_area(area),
            vk::SubpassContents::INLINE,
        );
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        self.device.cmd_set_viewport(cmd, 0, &[viewport]);
        self.device.cmd_set_scissor(cmd, 0, &[area]);
        self.recording.forget_all();
    }

    /// The quad meshes of `mode`'s pass as multi-draws, gathered before the
    /// frame (see [`super::super::super::indirect`]), from `phase`'s commands.
    unsafe fn draw_terrain_multi(
        &self,
        cmd: vk::CommandBuffer,
        world: &WorldDraw,
        mode: Mode,
        view_proj: Mat4,
        light: &DrawLight,
        phase: Phase,
    ) {
        let Some(indirect) = &self.indirect else {
            return;
        };
        let pass = match mode {
            Mode::Solid => Pass::Solid,
            Mode::Opaque => Pass::Cutout,
            _ => return,
        };
        if !self.bind_terrain_pass(cmd, world.atlas, mode) {
            return;
        }
        let layout = self.pipelines.terrain[0].layout;
        let frame = self.frames.current;
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            2,
            &[indirect.set(frame)],
            &[],
        );
        // Culled, each phase draws what the GPU packed for it (see `occlusion.rs`).
        let packed = self
            .recording
            .occluding
            .get()
            .then(|| (indirect.run_start(pass), phase));
        for (i, &run) in indirect.runs(pass).iter().enumerate() {
            let packed = packed.map(|(start, phase)| (start + i as u32, phase));
            self.flush_run(cmd, layout, (view_proj, light), run, packed);
        }
    }

    /// Draws a run of [`Self::draw_terrain_multi`]: as gathered, or what
    /// the culling packed of it, given the run's place among all and the phase.
    unsafe fn flush_run(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        (view_proj, light): (Mat4, &DrawLight),
        Run {
            buffer,
            first,
            count,
        }: Run,
        packed: Option<(u32, Phase)>,
    ) {
        let Some(indirect) = &self.indirect else {
            return;
        };
        self.bind_quads(cmd, layout, buffer);
        self.push_world(cmd, layout, view_proj, Vec3::ZERO, light);
        let at = [0.0; 4];
        self.device.cmd_push_constants(
            cmd,
            layout,
            pipeline::world::PUSH_STAGES,
            pipeline::world::OFFSET_AT,
            pipeline::push_constant_bytes(&at, pipeline::world::OFFSET_SIZE),
        );
        self.recording.offset.set(None);
        let commands = indirect.commands(self.frames.current);
        let stride = super::super::super::indirect::COMMAND_SIZE;
        let draw_count = self.occlusion.as_ref().and_then(|o| o.draw_count);
        match (packed, draw_count) {
            (Some((run, phase)), Some(draw_count)) => {
                let start = indirect.phase_start(phase) + first;
                let counts = indirect.counts(self.frames.current);
                let at = (2 * run + phase as u32) * 4;
                draw_count(
                    cmd,
                    commands,
                    u64::from(start) * u64::from(stride),
                    counts,
                    u64::from(at),
                    count,
                    stride,
                );
            }
            _ => self.device.cmd_draw_indexed_indirect(
                cmd,
                commands,
                u64::from(first) * u64::from(stride),
                count,
                stride,
            ),
        }
    }

    /// Draws `quads` of a terrain quad mesh with the bound terrain pass.
    pub(in crate::renderer) unsafe fn draw_terrain_quads(
        &self,
        cmd: vk::CommandBuffer,
        mesh: QuadMeshId,
        (mvp, offset): (Mat4, Vec3),
        light: &DrawLight,
        quads: &std::ops::Range<u32>,
    ) {
        let Some(slot) = self.quad_meshes.slot(mesh.0) else {
            return;
        };
        let (buffer, base) = self.quad_meshes.location(slot);
        let layout = self.pipelines.terrain[0].layout;
        self.bind_quads(cmd, layout, buffer);
        self.push_world(cmd, layout, mvp, offset, light);
        for (first, n) in quad_draws(quads, slot.count) {
            self.device
                .cmd_draw_indexed(cmd, n * 6, 1, 0, corner_offset(base + first), 0);
        }
    }

    /// Draws a vertex mesh through its index list with the bound world
    /// pass; whether it bound that list.
    unsafe fn draw_sorted_mesh(
        &self,
        cmd: vk::CommandBuffer,
        (vertices, indices): (MeshId, IndexMeshId),
        (mvp, offset): (Mat4, Vec3),
        light: &DrawLight,
    ) -> bool {
        let (Some(mesh), Some(list)) = (
            self.meshes.slot(vertices.0),
            self.index_lists.slot(indices.0),
        ) else {
            return false;
        };
        let (vertex_buffer, base) = self.meshes.location(mesh);
        let (index_buffer, words) = self.index_lists.location(list);
        let (kind, first) = if wide_indices(mesh.count) {
            (vk::IndexType::UINT32, words)
        } else {
            (vk::IndexType::UINT16, words * 2)
        };
        self.bind_vertices(cmd, vertex_buffer);
        self.device
            .cmd_bind_index_buffer(cmd, index_buffer, 0, kind);
        self.push_world(cmd, self.pipelines.world.layout, mvp, offset, light);
        self.device
            .cmd_draw_indexed(cmd, mesh.count / 4 * 6, 1, first, base as i32, 0);
        true
    }

    /// Binds mesh buffer `buffer` as the terrain shader's quads (set 4).
    unsafe fn bind_quads(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        buffer: vk::Buffer,
    ) {
        if self.recording.quads.replace(buffer) == buffer {
            return;
        }
        let Some(set) = self.quad_sets.get(buffer) else {
            return;
        };
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            4,
            &[set],
            &[],
        );
    }
}
