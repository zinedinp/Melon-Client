//! Running the chains in a frame, and handing them to the renderer.

use ash::vk;
use majestix_vk::{Barriers, Image, ImageBarrier, MemoryBarrier, Recording, Scope};

use super::gpu::{COLOR_LAYER, ChainSource, Dest, READABLE, Source};
use crate::blur::COLOR;
use crate::{Renderer, SceneFrame};

/// The chain glowing entities feed (`entity_outline.json`).
pub const OUTLINE_CHAIN: &str = "minecraft:entity_outline";

/// Stages and accesses of the barriers around the chains.
const STAGES_AROUND: vk::PipelineStageFlags2 = vk::PipelineStageFlags2::from_raw(
    vk::PipelineStageFlags2::FRAGMENT_SHADER.as_raw()
        | vk::PipelineStageFlags2::ALL_TRANSFER.as_raw()
        | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT.as_raw(),
);
/// What a pass's output saw last: sampled, cleared, copied or drawn into.
const BEFORE_PASS: Scope = Scope::new(
    STAGES_AROUND,
    vk::AccessFlags2::from_raw(
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw()
            | vk::AccessFlags2::TRANSFER_WRITE.as_raw(),
    ),
);
/// A pass's output as its attachment.
const ATTACHMENT: Scope = Scope::new(
    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
    vk::AccessFlags2::from_raw(
        vk::AccessFlags2::COLOR_ATTACHMENT_READ.as_raw()
            | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw(),
    ),
);
/// What a pass's output may see next: sampled, copied or drawn into.
const AFTER_PASS: Scope = Scope::new(
    STAGES_AROUND,
    vk::AccessFlags2::from_raw(
        vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw()
            | vk::AccessFlags2::TRANSFER_READ.as_raw()
            | vk::AccessFlags2::COLOR_ATTACHMENT_READ.as_raw(),
    ),
);

impl Renderer {
    /// Loads these chains in place of the ones there are; those that can't be built are left
    /// out.
    pub fn set_post_chains(
        &mut self,
        sources: Vec<ChainSource>,
        blit: Option<super::shader::Compiled>,
    ) -> Vec<(String, anyhow::Error)> {
        let mut failed = Vec::new();
        let sources = if self.swapchain.can_copy {
            sources
        } else {
            for source in sources {
                failed.push((
                    source.id,
                    anyhow::anyhow!("the swapchain image can't be copied from"),
                ));
            }
            Vec::new()
        };
        failed.extend(
            self.post
                .set_chains(self.gui_uniforms.set_layout(), sources),
        );
        // SAFETY: the scene's render pass, which lives as long as the renderer.
        if let Err(e) = unsafe { self.post.set_blit(self.passes.clear, blit.as_ref()) } {
            melon_base::warn!("Outline blit: {e:#}");
        }
        // Occlusion culling reads the depth buffer too.
        let wanted = self.post.is_active() || self.occlusion.is_some();
        let result = if wanted != self.swapchain.depth_sampled {
            self.swapchain.depth_sampled = wanted;
            // SAFETY: between frames, on the renderer's own device.
            unsafe { self.rebuild_targets_now() }
        } else {
            self.resize_post()
        };
        if let Err(e) = result {
            melon_base::warn!("Post effects: {e:#}");
        }
        failed
    }

    /// Makes what the chains size with the screen, again.
    pub(crate) fn resize_post(&mut self) -> anyhow::Result<()> {
        let depth = self
            .swapchain
            .depth
            .0
            .as_ref()
            .filter(|_| self.swapchain.depth_sampled)
            .map(|&(image, _)| image);
        self.post
            .resize(self.swapchain.extent, &self.swapchain.views, depth)
    }

    /// The chains this frame runs, in order: the outline's when an outlined entity is drawn
    /// (`LevelRenderer.currentFrameRendersEntityOutline`), then the ones the scene asks for.
    pub(crate) fn post_chains(&self, scene: &SceneFrame) -> Vec<String> {
        let mut out = Vec::new();
        if !scene.glowing.is_empty() && self.has_post_chain(OUTLINE_CHAIN) {
            out.push(OUTLINE_CHAIN.to_owned());
        }
        for id in &scene.post_effects {
            if self.has_post_chain(id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// The outline's pass and chain, between the level and the hand (`LevelRenderer.render`);
    /// `main` is the swapchain's image `image_index`.
    pub(crate) unsafe fn record_outline_post(
        &self,
        rec: &mut Recording<'_>,
        main: (Image, usize),
        scene: &SceneFrame,
    ) {
        self.begin_post(rec);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.record_outline_pass(rec, scene);
            self.record_post_chain(rec, main, OUTLINE_CHAIN);
        }
    }

    /// The requested chains, after the hand and the outline's blit
    /// (`GameRenderer.applyPostEffects`).
    pub(crate) unsafe fn record_effect_chains(
        &self,
        rec: &mut Recording<'_>,
        main: (Image, usize),
        chains: &[String],
    ) {
        self.begin_post(rec);
        for id in chains.iter().filter(|id| *id != OUTLINE_CHAIN) {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_post_chain(rec, main, id) };
        }
    }

    /// `blitEntityOutline`.
    pub(crate) unsafe fn record_outline_blit(&self, cmd: vk::CommandBuffer, chains: &[String]) {
        if !chains.iter().any(|c| c == OUTLINE_CHAIN) {
            return;
        }
        let Some((pass, set)) = self.post.blit() else {
            return;
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pass.layout,
                0,
                &[set],
                &[],
            );
            self.device.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }

    /// Whether the chain `id` is loaded.
    pub(crate) fn has_post_chain(&self, id: &str) -> bool {
        self.post.chains.contains_key(id)
    }

    /// Before the chains of a frame.
    pub(crate) fn begin_post(&self, rec: &mut Recording<'_>) {
        rec.barrier(&Barriers {
            memory: &[MemoryBarrier {
                src: Scope::new(
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                ),
                dst: Scope::new(
                    vk::PipelineStageFlags2::FRAGMENT_SHADER
                        | vk::PipelineStageFlags2::ALL_TRANSFER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ | vk::AccessFlags2::TRANSFER_READ,
                ),
            }],
            ..Barriers::default()
        });
        self.post.main_dirty.set(true);
        if self.post.needs_init.replace(false) {
            let mut images = self.post.internal_images();
            images.extend(self.post.outline().map(|(o, _)| (o, [0.0; 4])));
            self.clear_targets(rec, &images, vk::ImageLayout::UNDEFINED);
        }
    }

    /// Clears `images` to their colours; they come from `from` and are left readable.
    pub(crate) fn clear_targets(
        &self,
        rec: &mut Recording<'_>,
        images: &[(Image, [f32; 4])],
        from: vk::ImageLayout,
    ) {
        if images.is_empty() {
            return;
        }
        let src = if from == vk::ImageLayout::UNDEFINED {
            Scope::NONE
        } else {
            Scope::new(
                vk::PipelineStageFlags2::FRAGMENT_SHADER
                    | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            )
        };
        let cleared = Scope::new(
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let readable = Scope::new(
            vk::PipelineStageFlags2::FRAGMENT_SHADER
                | vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::SHADER_SAMPLED_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_READ
                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        let dst = vk::ImageLayout::TRANSFER_DST_OPTIMAL;
        let to_clear: Vec<_> = images
            .iter()
            .map(|&(image, _)| ImageBarrier::whole(image, src, cleared, from, dst))
            .collect();
        rec.barrier(&Barriers {
            images: &to_clear,
            ..Barriers::default()
        });
        for &(image, color) in images {
            rec.clear_color_image(image, dst, vk::ClearColorValue { float32: color }, &[COLOR]);
        }
        let to_read: Vec<_> = images
            .iter()
            .map(|&(image, _)| ImageBarrier::whole(image, cleared, readable, dst, READABLE))
            .collect();
        rec.barrier(&Barriers {
            images: &to_read,
            ..Barriers::default()
        });
    }

    /// Runs the chain `id`; `main` is the swapchain's image `image_index`.
    pub(crate) unsafe fn record_post_chain(
        &self,
        rec: &mut Recording<'_>,
        main: (Image, usize),
        id: &str,
    ) {
        let Some(chain) = self.post.chains.get(id) else {
            return;
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.pass("post") };
        let clears = self.post.run_clears(chain);
        self.clear_targets(rec, &clears, READABLE);
        let gui = self.gui_uniforms.slot_set(self.frames.current);
        for pass in &chain.passes {
            if pass.inputs.contains(&Source::MainColor) && self.post.main_dirty.get() {
                self.snapshot_main(rec, main.0);
            }
            let Some(sets) = pass.sets else {
                continue;
            };
            let Some((target, image, between)) = self.post.target(chain, pass.output, main) else {
                continue;
            };
            let attachment = vk::ImageLayout::ATTACHMENT_OPTIMAL;
            rec.barrier(&Barriers {
                images: &[ImageBarrier::whole(
                    image,
                    BEFORE_PASS,
                    ATTACHMENT,
                    between,
                    attachment,
                )],
                ..Barriers::default()
            });
            let extent = target.extent();
            {
                let mut drawing = rec.begin_pass(target, &[]);
                drawing.set_viewport(
                    0,
                    &[vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    }],
                );
                drawing.set_scissor(
                    0,
                    &[vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent,
                    }],
                );
                drawing.bind_pipeline(pass.pipeline);
                drawing.bind_descriptor_sets(
                    vk::PipelineBindPoint::GRAPHICS,
                    pass.layout,
                    0,
                    &[sets[0], gui, sets[1]],
                    &[],
                );
                drawing.draw(3, 1, 0, 0);
            }
            rec.barrier(&Barriers {
                images: &[ImageBarrier::whole(
                    image, ATTACHMENT, AFTER_PASS, attachment, between,
                )],
                ..Barriers::default()
            });
            if pass.output == Dest::Main {
                self.post.main_dirty.set(true);
            }
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.mark(&self.device, "post") };
    }

    /// Copies the swapchain's image `main` into the image the passes read `main` from.
    fn snapshot_main(&self, rec: &mut Recording<'_>, main: Image) {
        let Some(snapshot) = self.post.snapshot() else {
            return;
        };
        // Drawn into, or sampled by the passes before.
        let used = Scope::new(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        let read = Scope::new(
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_READ,
        );
        let written = Scope::new(
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let (present, src, dst) = (
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        rec.barrier(&Barriers {
            images: &[
                ImageBarrier::whole(main, used, read, present, src),
                ImageBarrier::whole(
                    snapshot,
                    Scope::new(used.stages, vk::AccessFlags2::NONE),
                    written,
                    vk::ImageLayout::UNDEFINED,
                    dst,
                ),
            ],
            ..Barriers::default()
        });
        rec.copy_image(
            main,
            src,
            snapshot,
            dst,
            &[vk::ImageCopy {
                src_subresource: COLOR_LAYER,
                src_offset: vk::Offset3D::default(),
                dst_subresource: COLOR_LAYER,
                dst_offset: vk::Offset3D::default(),
                extent: snapshot.extent(),
            }],
        );
        rec.barrier(&Barriers {
            images: &[
                ImageBarrier::whole(
                    snapshot,
                    written,
                    Scope::new(
                        vk::PipelineStageFlags2::FRAGMENT_SHADER,
                        vk::AccessFlags2::SHADER_SAMPLED_READ,
                    ),
                    dst,
                    READABLE,
                ),
                ImageBarrier::whole(main, read, ATTACHMENT, src, present),
            ],
            ..Barriers::default()
        });
        self.post.main_dirty.set(false);
    }
}

impl Renderer {
    /// Draws the glowing entities flat into the outline target.
    pub(crate) unsafe fn record_outline_pass(&self, rec: &mut Recording<'_>, scene: &SceneFrame) {
        use crate::RenderLayer;
        use crate::pipeline::world::{DrawLight, Mode};
        let Some(world) = &scene.world else {
            return;
        };
        let Some((outline, framebuffer)) = self.post.outline() else {
            return;
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.pass("outline") };
        // `LevelRenderer`'s outline pass starts from `ZERO_CLEAR_COLOR`.
        self.clear_targets(rec, &[(outline, [0.0; 4])], READABLE);
        let cmd = rec.raw();
        let device = &self.device;
        let extent = self.swapchain.extent;
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let clears = [vk::ClearValue {
            color: vk::ClearColorValue { float32: [0.0; 4] },
        }];
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.post.color_pass)
                    .framebuffer(framebuffer)
                    .render_area(area)
                    .clear_values(&clears),
                vk::SubpassContents::INLINE,
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { device.cmd_set_scissor(cmd, 0, &[area]) };
        self.recording.forget_all();
        if self.pack_outline.is_some() {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_pack_outline(cmd, scene) };
        } else {
            let view_proj = self.world_view_projection(world, RenderLayer::Entities);
            for &(index, [r, g, b]) in &scene.glowing {
                let Some(entity) = scene.entities.get(index) else {
                    continue;
                };
                // SAFETY: `device` is live, and the infos name only its own live handles.
                if !unsafe { self.bind_world_pass(cmd, entity.atlas, Mode::Outline) } {
                    continue;
                }
                let colour = [r, g, b].map(|c| f32::from(c) / 255.0);
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe {
                    self.draw_world_mesh(
                        cmd,
                        self.pipelines.outline.layout,
                        entity.mesh,
                        (
                            view_proj * world.relative_to(entity.model, entity.origin),
                            glam::Vec3::ZERO,
                        ),
                        &DrawLight::tint([colour[0], colour[1], colour[2], 1.0]),
                        None,
                    )
                };
            }
        }
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            device.cmd_end_render_pass(cmd);
            self.gpu_timer.mark(device, "outline");
        }
    }

    /// Pack `rendertype_outline`.
    /// Writes the pack outline's blocks for this frame's glowing entities, before recording.
    pub(crate) fn prepare_pack_outline(
        &mut self,
        frame: &majestix_vk::Frame<'_>,
        scene: &SceneFrame,
    ) {
        use crate::RenderLayer;
        use glam::{Mat4, Vec4};
        use melon_world::projection::world_projection;
        let Some(world) = &scene.world else {
            return;
        };
        if scene.glowing.is_empty() || self.pack_outline.is_none() {
            return;
        }
        let mut rotation = world.view;
        rotation.w_axis = Vec4::W;
        let view =
            world.bob * world.distortion * rotation * Mat4::from_translation(-world.eye_fraction());
        let proj = world_projection(
            world.fov_for(RenderLayer::Entities),
            self.aspect_ratio(),
            world.far,
        );
        let globals =
            super::super::gui_uniforms::Globals::of(&self.gui_frame((1.0, 1.0), Some(world)));
        let (textures, meshes) = (&self.textures, &self.meshes);
        let entities = scene.glowing.iter().filter_map(|&(index, [r, g, b])| {
            let entity = scene.entities.get(index)?;
            textures.list.get(entity.atlas.0)?;
            meshes.slot(entity.mesh.0)?;
            let colour = [r, g, b].map(|c| f32::from(c) / 255.0);
            Some((
                index,
                view * world.relative_to(entity.model, entity.origin),
                [colour[0], colour[1], colour[2], 1.0],
            ))
        });
        if let Some(pack) = self.pack_outline.as_mut() {
            pack.prepare(frame, proj, &globals, entities);
        }
    }

    unsafe fn record_pack_outline(&self, cmd: vk::CommandBuffer, scene: &SceneFrame) {
        use crate::mesh_arena;
        let Some(pack) = self.pack_outline.as_ref() else {
            return;
        };
        if pack.drawn().next().is_none() {
            return;
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pack.pass.pipeline)
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_index_buffer(
                cmd,
                self.meshes.index_buffer(),
                0,
                vk::IndexType::UINT16,
            )
        };
        let blocks = pack.set(self.frames.current);
        for (index, offset) in pack.drawn() {
            let entity = &scene.entities[index];
            let (Some(texture), Some(mesh)) = (
                self.textures.list.get(entity.atlas.0),
                self.meshes.slot(entity.mesh.0),
            ) else {
                continue;
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pack.pass.layout,
                    0,
                    &[texture.descriptor_set, blocks],
                    &[offset],
                )
            };
            let (buffer, base) = self.meshes.location(mesh);
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { self.bind_vertices(cmd, buffer) };
            let mut quad = 0;
            let end = mesh.count / 4;
            while quad < end {
                let n = (end - quad).min(mesh_arena::QUADS_PER_DRAW);
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe {
                    self.device
                        .cmd_draw_indexed(cmd, n * 6, 1, 0, (base + quad * 4) as i32, 0)
                };
                quad += n;
            }
        }
    }
}
