//! Running the chains in a frame, and handing them to the renderer.

use ash::vk;

use super::gpu::{ChainSource, Dest, Source};
use crate::renderer::blur::COLOR;
use crate::renderer::{Renderer, SceneFrame, free_effect_texture};

/// The chain glowing entities feed (`entity_outline.json`).
pub const OUTLINE_CHAIN: &str = "minecraft:entity_outline";

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
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                crate::warn!("Waiting for the GPU failed: {e}");
            }
            let mut prepared = Vec::new();
            for source in sources {
                match self.upload_effect_textures(&source) {
                    Ok(textures) => prepared.push((source, textures)),
                    Err(e) => failed.push((source.id, e)),
                }
            }
            let (device, pools) = (self.device.clone(), &self.textures.descriptor_pools);
            failed.extend(self.post.set_chains(
                &self.device,
                &self.mem_props,
                self.gui_uniforms.layout,
                prepared,
                &|t| free_effect_texture(&device, pools, t),
            ));
            if let Err(e) = self
                .post
                .set_blit(&self.device, self.passes.clear, blit.as_ref())
            {
                crate::warn!("Outline blit: {e:#}");
            }
            // Occlusion culling reads the depth buffer too.
            let wanted = self.post.is_active() || self.occlusion.is_some();
            let (w, h) = (self.swapchain.extent.width, self.swapchain.extent.height);
            let result = if wanted != self.swapchain.depth_sampled && w > 0 {
                self.swapchain.depth_sampled = wanted;
                self.create_swapchain(w, h)
            } else {
                self.resize_post()
            };
            if let Err(e) = result {
                crate::warn!("Post effects: {e:#}");
            }
        }
        failed
    }

    /// Makes what the chains size with the screen, again.
    pub(in crate::renderer) unsafe fn resize_post(&mut self) -> anyhow::Result<()> {
        let depth = self
            .swapchain
            .depth_sampled
            .then_some((self.swapchain.depth.image, self.swapchain.depth_format));
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            self.post.resize(
                &self.device,
                &self.mem_props,
                self.swapchain.extent,
                &self.swapchain.views,
                depth,
            )
        }
    }

    /// Each texture input of `source`'s passes, in order.
    unsafe fn upload_effect_textures(
        &self,
        source: &ChainSource,
    ) -> anyhow::Result<Vec<(crate::renderer::texture::Texture, u32, u32)>> {
        use crate::renderer::post::config::Input;
        let mut out: Vec<(crate::renderer::texture::Texture, u32, u32)> = Vec::new();
        let mut inits = Vec::new();
        let fail = |out: &mut Vec<(crate::renderer::texture::Texture, u32, u32)>, e| {
            let (device, pools) = (self.device.clone(), &self.textures.descriptor_pools);
            for (texture, _, _) in out.drain(..) {
                // SAFETY: made by this device, and no frame in flight uses it any more.
                unsafe { free_effect_texture(&device, pools, texture) };
            }
            e
        };
        for pass in &source.config.passes {
            for input in &pass.inputs {
                let Input::Texture {
                    location,
                    width,
                    height,
                    ..
                } = input
                else {
                    continue;
                };
                let image = source
                    .textures
                    .get(location)
                    .cloned()
                    .unwrap_or_else(crate::assets::missing_texture);
                // SAFETY: `device` is live, and the infos name only its own live handles.
                match unsafe {
                    self.uploader(self.textures.nearest_sampler)
                        .image_2d(&image)
                } {
                    Ok((texture, init)) => {
                        out.push((texture, *width, *height));
                        inits.push(init);
                    }
                    Err(e) => return Err(fail(&mut out, e)),
                }
            }
        }
        // Loaded while the device is idle anyway, and freed without
        // waiting for a frame: up now, not with the next frame.
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        if let Err(e) = unsafe {
            crate::renderer::texture_update::submit_now(
                &self.device,
                &self.mem_props,
                self.frames.command_pool,
                self.queue,
                &inits,
            )
        } {
            return Err(fail(&mut out, e));
        }
        Ok(out)
    }

    /// The chains this frame runs, in order.
    pub(in crate::renderer) fn post_chains(&self, scene: &SceneFrame) -> Vec<String> {
        let mut out = Vec::new();
        let outline_every_frame = scene.world.is_some()
            && self
                .post
                .chains
                .get(OUTLINE_CHAIN)
                .is_some_and(|chain| chain.config.targets.len() > 1);
        if (outline_every_frame || !scene.glowing.is_empty()) && self.has_post_chain(OUTLINE_CHAIN)
        {
            out.push(OUTLINE_CHAIN.to_owned());
        }
        if scene.world.is_some() && self.has_post_chain(super::load::END_OF_FRAME) {
            out.push(super::load::END_OF_FRAME.to_owned());
        }
        for id in &scene.post_effects {
            if self.has_post_chain(id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// Everything between the scene's pass and the one after it.
    pub(in crate::renderer) unsafe fn record_post(
        &self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        scene: &SceneFrame,
        chains: &[String],
    ) {
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.begin_post(cmd) };
        if chains.iter().any(|c| c == OUTLINE_CHAIN) {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_outline_pass(cmd, scene) };
        }
        for id in chains {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_post_chain(cmd, image_index, id) };
        }
    }

    /// `blitEntityOutline`.
    pub(in crate::renderer) unsafe fn record_outline_blit(
        &self,
        cmd: vk::CommandBuffer,
        chains: &[String],
    ) {
        if !chains.iter().any(|c| c == OUTLINE_CHAIN) {
            return;
        }
        let Some((pass, set)) = &self.post.blit else {
            return;
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline)
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pass.layout,
                0,
                &[*set],
                &[],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.device.cmd_draw(cmd, 3, 1, 0, 0) };
    }

    /// Whether the chain `id` is loaded.
    pub(in crate::renderer) fn has_post_chain(&self, id: &str) -> bool {
        self.post.chains.contains_key(id)
    }

    /// Before the chains of a frame.
    pub(in crate::renderer) unsafe fn begin_post(&self, cmd: vk::CommandBuffer) {
        let device = &self.device;
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(
                        vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                            | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    )
                    .dst_access_mask(
                        vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_READ,
                    )],
                &[],
                &[],
            )
        };
        self.post.main_dirty.set(true);
        if self.post.needs_init.replace(false) {
            let images = self.post.internal_images();
            let barrier = |image, old, new, src, dst| {
                vk::ImageMemoryBarrier::default()
                    .image(image)
                    .old_layout(old)
                    .new_layout(new)
                    .src_access_mask(src)
                    .dst_access_mask(dst)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .subresource_range(COLOR)
            };
            let to_clear: Vec<_> = images
                .iter()
                .map(|&image| {
                    barrier(
                        image,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::AccessFlags::empty(),
                        vk::AccessFlags::TRANSFER_WRITE,
                    )
                })
                .collect();
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_clear,
                )
            };
            for &image in &images {
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe {
                    device.cmd_clear_color_image(
                        cmd,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue { float32: [0.0; 4] },
                        &[COLOR],
                    )
                };
            }
            let to_read: Vec<_> = images
                .iter()
                .map(|&image| {
                    barrier(
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::SHADER_READ,
                    )
                })
                .collect();
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_read,
                )
            };
        }
    }

    /// Runs the chain `id`.
    pub(in crate::renderer) unsafe fn record_post_chain(
        &self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        id: &str,
    ) {
        let Some(chain) = self.post.chains.get(id) else {
            return;
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.pass("post") };
        let device = &self.device;
        for pass in &chain.passes {
            if pass.inputs.contains(&Source::MainColor) && self.post.main_dirty.get() {
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe { self.snapshot_main(cmd, image_index) };
            }
            let (render_pass, framebuffer, extent, clear) = match pass.output {
                Dest::Main => (
                    self.post.main_pass,
                    self.post.main_framebuffers[image_index],
                    self.swapchain.extent,
                    [0.0; 4],
                ),
                dest => {
                    let Some((framebuffer, extent, clear)) = self.post.target(chain, dest) else {
                        continue;
                    };
                    (self.post.color_pass, framebuffer, extent, clear)
                }
            };
            let area = vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent,
            };
            let clears = [vk::ClearValue {
                color: vk::ClearColorValue { float32: clear },
            }];
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_begin_render_pass(
                    cmd,
                    &vk::RenderPassBeginInfo::default()
                        .render_pass(render_pass)
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
            unsafe {
                device.cmd_set_scissor(cmd, 0, &[area]);
                device.cmd_bind_pipeline(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pass.pipeline.pipeline,
                );
            }
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pass.pipeline.layout,
                    0,
                    &[
                        pass.sampler_set,
                        self.gui_uniforms.set(self.frames.current),
                        pass.block_set,
                    ],
                    &[],
                )
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                device.cmd_draw(cmd, 3, 1, 0, 0);
                device.cmd_end_render_pass(cmd);
            }
            if pass.output == Dest::Main {
                self.post.main_dirty.set(true);
            }
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.mark(device, "post") };
    }

    /// Copies the swapchain image into the image the passes read `main`
    unsafe fn snapshot_main(&self, cmd: vk::CommandBuffer, image_index: usize) {
        let Some(snapshot) = &self.post.snapshot else {
            return;
        };
        let device = &self.device;
        let source = self.swapchain.images[image_index];
        let barrier = |image, old, new, src, dst| {
            vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(old)
                .new_layout(new)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(COLOR)
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    barrier(
                        source,
                        vk::ImageLayout::PRESENT_SRC_KHR,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                        vk::AccessFlags::TRANSFER_READ,
                    ),
                    barrier(
                        snapshot.image,
                        vk::ImageLayout::UNDEFINED,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::AccessFlags::empty(),
                        vk::AccessFlags::TRANSFER_WRITE,
                    ),
                ],
            )
        };
        let layer = vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        };
        let extent = self.swapchain.extent;
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_copy_image(
                cmd,
                source,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                snapshot.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy {
                    src_subresource: layer,
                    src_offset: vk::Offset3D::default(),
                    dst_subresource: layer,
                    dst_offset: vk::Offset3D::default(),
                    extent: vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    },
                }],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[
                    barrier(
                        snapshot.image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::SHADER_READ,
                    ),
                    barrier(
                        source,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::ImageLayout::PRESENT_SRC_KHR,
                        vk::AccessFlags::TRANSFER_READ,
                        vk::AccessFlags::COLOR_ATTACHMENT_READ
                            | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    ),
                ],
            )
        };
        self.post.main_dirty.set(false);
    }
}

impl Renderer {
    /// Draws the glowing entities flat into the outline target.
    pub(in crate::renderer) unsafe fn record_outline_pass(
        &self,
        cmd: vk::CommandBuffer,
        scene: &SceneFrame,
    ) {
        use crate::renderer::RenderLayer;
        use crate::renderer::pipeline::world::{DrawLight, Mode};
        let Some(world) = &scene.world else {
            return;
        };
        if self.post.outline.is_none() {
            return;
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.pass("outline") };
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
                    .framebuffer(self.post.outline_framebuffer)
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
            unsafe { self.record_pack_outline(cmd, scene, world) };
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
    unsafe fn record_pack_outline(
        &self,
        cmd: vk::CommandBuffer,
        scene: &SceneFrame,
        world: &crate::renderer::WorldDraw,
    ) {
        use crate::renderer::RenderLayer;
        use crate::renderer::{mesh_arena, world_projection};
        use glam::{Mat4, Vec4};
        let Some(pack) = self.pack_outline.as_ref() else {
            return;
        };
        let extent = self.swapchain.extent;
        let mut rotation = world.view;
        rotation.w_axis = Vec4::W;
        let view =
            world.bob * world.distortion * rotation * Mat4::from_translation(-world.eye_fraction());
        let proj = world_projection(
            world.fov_for(RenderLayer::Entities),
            self.aspect_ratio(),
            world.far,
        );
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { pack.write_frame(proj, (extent.width as f32, extent.height as f32)) };
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
        for (slot, &(index, [r, g, b])) in scene
            .glowing
            .iter()
            .take(crate::renderer::pack_outline::PackOutline::slots())
            .enumerate()
        {
            let Some(entity) = scene.entities.get(index) else {
                continue;
            };
            let Some(texture) = self.textures.list.get(entity.atlas.0) else {
                continue;
            };
            let colour = [r, g, b].map(|c| f32::from(c) / 255.0);
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                pack.write_entity(
                    slot,
                    view * world.relative_to(entity.model, entity.origin),
                    [colour[0], colour[1], colour[2], 1.0],
                )
            };
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pack.pass.layout,
                    0,
                    &[texture.descriptor_set, pack.set(slot)],
                    &[],
                )
            };
            let Some(mesh) = self.meshes.slot(entity.mesh.0) else {
                continue;
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
