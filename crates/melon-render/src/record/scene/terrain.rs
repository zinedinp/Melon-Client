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
//! With multi-draws, the solid and cutout quads are expanded per section on
//! the GPU (`expand.rs`) unless `MELON_NO_EXPAND=1`, and the translucent
//! sections are multi-draws too, one per stretch that shares its vertex and
//! index buffers. Each translucent section draws through its own index
//! list, sorted far to near (vanilla's sorted index buffers), so a sort
//! uploads only that. Vertex meshes (lava) always take the per-draw path.

use crate::pipeline::world::{DrawLight, FogInput, Mode};
use crate::{
    ChunkDraw, IndexMeshId, MeshId, QuadMeshId, RenderLayer, Renderer, TerrainMesh, WorldDraw,
};
use ash::vk;
use glam::{Mat4, Vec3};

use super::super::super::indirect::{Pass, Phase, Run, corner_offset, quad_draws};
use crate::pipeline;
use melon_world::mesh::wide_indices;

/// How the terrain's quad meshes are drawn, fixed when the renderer is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerrainPath {
    PerDraw,
    MultiDraw,
    Counted,
}

impl TerrainPath {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PerDraw => "per draw",
            Self::MultiDraw => "multi-draw",
            Self::Counted => "multi-draw with GPU counts (occlusion culling)",
        }
    }
}

impl Renderer {
    /// The path [`Self::record_terrain`] takes.
    pub(crate) fn terrain_path(&self) -> TerrainPath {
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
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.draw_terrain_layer(cmd, world, layer_draws, at, multi, !occluding) };
        }
        if occluding {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.cull_terrain(cmd, view_proj) };
            for &layer_draws in passes {
                let at = (view_proj, &light);
                let phase = Some(Phase::Second);
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe { self.draw_terrain_layer(cmd, world, layer_draws, at, phase, true) };
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
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.counted_pass(label) };
        if let Some(phase) = phase
            && by_multi
        {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.draw_terrain_multi(cmd, world, mode, view_proj, light, phase) };
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        let rest = rest
            && !(mode == Mode::TranslucentTerrain
                && unsafe { self.draw_sorted_multi(cmd, world, view_proj, light) });
        let mut bound: Option<bool> = None;
        let mut own_indices = false;
        // The sorted meshes' index buffer last bound: most share an arena block.
        let mut index = None;
        for chunk in chunks.iter().filter(|_| rest) {
            let quads = matches!(chunk.mesh, TerrainMesh::Quads(_));
            if quads && by_multi {
                continue;
            }
            if bound != Some(quads) {
                index = None;
                let ok = if quads {
                    // SAFETY: `device` is live, and the infos name only its own live handles.
                    unsafe { self.bind_terrain_pass(cmd, world.atlas, mode) }
                } else {
                    // SAFETY: `device` is live, and the infos name only its own live handles.
                    unsafe { self.bind_world_pass(cmd, world.atlas, mode) }
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
                    // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                    unsafe { self.draw_terrain_quads(cmd, id, at, light, &chunk.range.into()) };
                }
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                TerrainMesh::Vertices(id) => unsafe {
                    self.draw_world_mesh(
                        cmd,
                        self.pipelines.world.layout,
                        id,
                        at,
                        &light.fogged(FogInput::Chunk),
                        Some(&chunk.range.into()),
                    )
                },
                TerrainMesh::Sorted { vertices, indices } => {
                    let light = light.fogged(FogInput::Chunk);
                    // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                    own_indices |= unsafe {
                        self.draw_sorted_mesh(cmd, (vertices, indices), at, &light, &mut index)
                    };
                }
            }
        }
        if own_indices {
            // What follows draws through the shared quad indices again.
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.device.cmd_bind_index_buffer(
                    cmd,
                    self.meshes.index_buffer(),
                    0,
                    vk::IndexType::UINT16,
                )
            };
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.mark(&self.device, label) };
    }

    /// The translucent sections as gathered multi-draws (see
    /// [`super::super::super::indirect`]), far to near; whether it drew them.
    unsafe fn draw_sorted_multi(
        &self,
        cmd: vk::CommandBuffer,
        world: &WorldDraw,
        view_proj: Mat4,
        light: &DrawLight,
    ) -> bool {
        let (Some(indirect), Some(pass)) = (
            &self.indirect,
            &self.pipelines.translucent_terrain_indirect,
        ) else {
            return false;
        };
        if !indirect.sorted_ready() {
            return false;
        }
        let runs = indirect.sorted_runs();
        if runs.is_empty() {
            return true;
        }
        // SAFETY: `device` is live, and the infos name only its own live handles.
        if !unsafe { self.bind_world_pipeline(cmd, world.atlas, pass) } {
            return false;
        }
        let layout = self.pipelines.world.layout;
        let (commands, set) = indirect.sorted(self.frames.current);
        let stride = super::super::super::indirect::COMMAND_SIZE;
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                2,
                &[set],
                &[],
            );
            self.push_world(cmd, layout, view_proj, Vec3::ZERO, &light.fogged(FogInput::Chunk));
        }
        for run in runs {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.bind_vertices(cmd, run.vertices);
                self.device
                    .cmd_bind_index_buffer(cmd, run.indices, 0, run.kind);
                self.device.cmd_draw_indexed_indirect(
                    cmd,
                    commands,
                    u64::from(run.first) * u64::from(stride),
                    run.count,
                    stride,
                );
            }
        }
        // What follows draws through the shared quad indices again.
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_index_buffer(
                cmd,
                self.meshes.index_buffer(),
                0,
                vk::IndexType::UINT16,
            )
        };
        true
    }

    /// Between the culling's two phases: ends the render pass, picks phase
    /// 2's draws against what phase 1 drew (see [`super::super::super::occlusion`]),
    /// and starts the pass again on top of it.
    unsafe fn cull_terrain(&self, cmd: vk::CommandBuffer, view_proj: Mat4) {
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.gpu_timer.close_pass();
            self.device.cmd_end_render_pass(cmd);
        }
        if let (Some(occlusion), Some(indirect)) = (&self.occlusion, &self.indirect) {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.gpu_timer.pass("occlusion") };
            let draws = (indirect.gathered(), indirect.run_count());
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                occlusion.record_cull(&self.device, cmd, self.frames.current, draws, &view_proj);
                self.gpu_timer.mark(&self.device, "occlusion");
            }
        }
        let extent = self.swapchain.extent;
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.passes.resume_keep)
                    .framebuffer(self.recording.framebuffer.get())
                    .render_area(area),
                vk::SubpassContents::INLINE,
            )
        };
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_set_viewport(cmd, 0, &[viewport]);
            self.device.cmd_set_scissor(cmd, 0, &[area]);
        }
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
        // SAFETY: `device` is live, and the infos name only its own live handles.
        if !unsafe { self.bind_terrain_pass(cmd, world.atlas, mode) } {
            return;
        }
        let layout = self.pipelines.terrain[0].layout;
        let frame = self.frames.current;
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                2,
                &[indirect.set(frame)],
                &[],
            )
        };
        // Culled, each phase draws what the GPU packed for it (see `occlusion.rs`).
        let packed = self
            .recording
            .occluding
            .get()
            .then(|| (indirect.run_start(pass), phase));
        for (i, &run) in indirect.runs(pass).iter().enumerate() {
            let packed = packed.map(|(start, phase)| (start + i as u32, phase));
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.flush_run(cmd, layout, (view_proj, light), run, packed) };
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
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.bind_quads(cmd, layout, buffer);
            self.push_world(cmd, layout, view_proj, Vec3::ZERO, light);
        }
        let at = [0.0; 4];
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_push_constants(
                cmd,
                layout,
                pipeline::world::PUSH_STAGES,
                pipeline::world::OFFSET_AT,
                pipeline::push_constant_bytes(&at, pipeline::world::OFFSET_SIZE),
            )
        };
        self.recording.offset.set(None);
        let commands = indirect.commands(self.frames.current);
        let stride = super::super::super::indirect::COMMAND_SIZE;
        let draw_count = self.occlusion.as_ref().and_then(|o| o.draw_count);
        match (packed, draw_count) {
            (Some((run, phase)), Some(draw_count)) => {
                let start = indirect.phase_start(phase) + first;
                let counts = indirect.counts(self.frames.current);
                let at = (2 * run + phase as u32) * 4;
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe {
                    draw_count(
                        cmd,
                        commands,
                        u64::from(start) * u64::from(stride),
                        counts,
                        u64::from(at),
                        count,
                        stride,
                    )
                };
            }
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            _ => unsafe {
                self.device.cmd_draw_indexed_indirect(
                    cmd,
                    commands,
                    u64::from(first) * u64::from(stride),
                    count,
                    stride,
                )
            },
        }
    }

    /// Draws `quads` of a terrain quad mesh with the bound terrain pass.
    pub(crate) unsafe fn draw_terrain_quads(
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
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.bind_quads(cmd, layout, buffer);
            self.push_world(cmd, layout, mvp, offset, light);
        }
        for (first, n) in quad_draws(quads, slot.count) {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.device
                    .cmd_draw_indexed(cmd, n * 6, 1, 0, corner_offset(base + first), 0)
            };
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
        bound: &mut Option<(vk::Buffer, vk::IndexType)>,
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
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.bind_vertices(cmd, vertex_buffer) };
        if bound.replace((index_buffer, kind)) != Some((index_buffer, kind)) {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.device
                    .cmd_bind_index_buffer(cmd, index_buffer, 0, kind)
            };
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.push_world(cmd, self.pipelines.world.layout, mvp, offset, light) };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_draw_indexed(cmd, mesh.count / 4 * 6, 1, first, base as i32, 0)
        };
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
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                4,
                &[set],
                &[],
            )
        };
    }
}
