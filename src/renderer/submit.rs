//! One frame on the GPU.

use super::globals::Globals;
use super::record::scene::ScenePart;
use super::{Frame, FrameWaits, MAX_FRAMES_IN_FLIGHT, Renderer, VERTEX_CAPACITY, mesh_arena, quad};
use ash::vk;
use std::time::Instant;

impl Renderer {
    pub fn render(&mut self, frame: &Frame<'_>) -> anyhow::Result<()> {
        unsafe { self.render_inner(frame) }
    }

    /// Whether the device does the terrain's multi-draws.
    pub fn multi_draws(&self) -> bool {
        self.indirect.is_some()
    }

    /// Where the last frame waited.
    pub fn last_waits(&self) -> FrameWaits {
        self.frames.last_waits
    }

    /// `GameRenderer.getAppliedPostEffects`: the post chains of the last
    /// frame that showed the world, the entity outline's not among them
    /// (vanilla runs that one apart from the requested effects).
    pub fn applied_post_effects(&self) -> &[String] {
        &self.frames.applied_post_effects
    }

    /// F3's GPU lines.
    pub fn gpu_profile(&self) -> Vec<String> {
        self.gpu_timer.lines()
    }

    /// The newest frame the GPU finished, unsmoothed.
    pub fn gpu_last_frame(&self) -> (f32, &[(&'static str, f32)]) {
        self.gpu_timer.last_frame()
    }

    /// The newest frame the GPU finished: how much each counted pass shaded.
    /// Empty without `pipelineStatisticsQuery`.
    pub fn gpu_last_stats(&self) -> &[(&'static str, super::gpu_timer::PassStats)] {
        self.gpu_timer.last_stats()
    }

    /// The staging and draw buffers this frame grows into.
    unsafe fn prepare_frame(&mut self, frame: &Frame<'_>) -> anyhow::Result<()> {
        let current = self.frames.current;
        super::texture_update::prepare(
            &mut self.textures.update_staging[current],
            &self.device,
            &self.mem_props,
            &self.textures.pending_inits,
            &self.textures.pending_updates,
        )?;
        for arena in [&mut self.meshes, &mut self.quad_meshes] {
            arena.prepare(&self.device, &self.mem_props, current)?;
        }
        if let (Some(indirect), Some(world)) = (
            &mut self.indirect,
            frame.scene.and_then(|s| s.world.as_ref()),
        ) {
            let draws: u32 = world
                .chunks
                .iter()
                .chain(&world.cutout)
                .map(|chunk| {
                    chunk
                        .range
                        .len()
                        .div_ceil(mesh_arena::QUADS_PER_DRAW as usize)
                        .max(1) as u32
                })
                .sum();
            indirect.prepare(&self.device, &self.mem_props, current, draws)?;
        }
        Ok(())
    }

    pub(super) unsafe fn render_inner(&mut self, frame: &Frame<'_>) -> anyhow::Result<()> {
        let ui_vertices = frame.ui.vertices.len().min(VERTEX_CAPACITY);
        if ui_vertices < frame.ui.vertices.len() {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                crate::warn!(
                    "UI vertex buffer full ({} > {VERTEX_CAPACITY}): the rest is not drawn",
                    frame.ui.vertices.len()
                );
            });
        }
        let calls = fitting(&frame.ui.calls, ui_vertices);
        let mut waits = FrameWaits::default();
        let started = Instant::now();
        let fence = self.frames.in_flight[self.frames.current];
        self.device.wait_for_fences(&[fence], true, u64::MAX)?;
        self.frames.submitted[self.frames.current] = 0;
        self.frames.number += 1;
        self.destroy_retired_textures();
        waits.gpu = started.elapsed();

        // Every allocation the frame needs, before an image is acquired:
        // past that point a failure would leave the image acquired and its
        // semaphore signalled with nothing to wait on it. Out of memory
        // (another program holding the VRAM) now just skips the frame.
        self.prepare_frame(frame)?;

        let started = Instant::now();
        let acquire = self.swapchain.loader.acquire_next_image(
            self.swapchain.handle,
            u64::MAX,
            self.frames.image_available[self.frames.current],
            vk::Fence::null(),
        );
        let image_index = match acquire {
            Ok((idx, _suboptimal)) => idx,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                let (w, h) = (self.swapchain.extent.width, self.swapchain.extent.height);
                // The other frame in flight may still draw into the
                // framebuffers and depth buffer this destroys.
                self.device.device_wait_idle()?;
                self.create_swapchain(w, h)?;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        waits.acquire = started.elapsed();

        let started = Instant::now();
        let img_fence = self.frames.images_in_flight[image_index as usize];
        if img_fence != vk::Fence::null() {
            self.device.wait_for_fences(&[img_fence], true, u64::MAX)?;
        }
        waits.gpu += started.elapsed();
        self.frames.images_in_flight[image_index as usize] = fence;

        std::ptr::copy_nonoverlapping(
            frame.ui.vertices.as_ptr() as *const u8,
            self.ui_vertices.mapped[self.frames.current],
            ui_vertices * std::mem::size_of::<quad::Vertex>(),
        );

        self.recording.draw_cursor.set(0);

        let mut globals = frame
            .scene
            .and_then(|scene| scene.world.as_ref())
            .map_or_else(Globals::plain, |world| {
                Globals::for_world(world, self.aspect_ratio())
            });
        globals.screen = [
            self.swapchain.extent.width.max(1) as f32,
            self.swapchain.extent.height.max(1) as f32,
            0.0,
            0.0,
        ];
        self.globals.write(self.frames.current, &globals);
        self.gui_uniforms.write(
            self.frames.current,
            &super::gui_uniforms::GuiFrame {
                gui_size: frame.ui_size,
                screen_size: (
                    self.swapchain.extent.width as f32,
                    self.swapchain.extent.height as f32,
                ),
                game_time: super::gui_uniforms::game_time(),
            },
        );

        let cmd = self.frames.command_buffers[self.frames.current];
        self.device
            .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        self.device
            .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        self.gpu_timer.begin(&self.device, cmd, self.frames.current);
        self.gpu_timer.pass("upload");
        let inits = std::mem::take(&mut self.textures.pending_inits);
        self.textures.pending_init_bytes = 0;
        let updates = std::mem::take(&mut self.textures.pending_updates);
        super::texture_update::record(
            &mut self.textures.update_staging[self.frames.current],
            &self.device,
            &self.mem_props,
            cmd,
            &inits,
            &updates,
        )?;
        let oldest_in_flight = self.frames.oldest_in_flight();
        for arena in [&mut self.meshes, &mut self.quad_meshes] {
            arena.record(
                &self.device,
                &self.mem_props,
                cmd,
                self.frames.current,
                oldest_in_flight,
            )?;
        }
        self.meshes.take_destroyed();
        for buffer in self.quad_meshes.take_destroyed() {
            self.quad_sets.forget(&self.device, buffer);
        }

        self.gpu_timer.mark(&self.device, "upload");
        self.record_pictures(cmd, frame.pictures);

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: frame.clear_color,
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: super::frame::DEPTH_CLEAR,
                    stencil: 0,
                },
            },
        ];
        let chains = frame
            .scene
            .map(|scene| self.post_chains(scene))
            .unwrap_or_default();
        if frame.scene.is_some_and(|scene| scene.world.is_some()) {
            self.frames.applied_post_effects.clear();
            self.frames.applied_post_effects.extend(
                chains
                    .iter()
                    .filter(|c| *c != super::post::record::OUTLINE_CHAIN)
                    .cloned(),
            );
        }
        let split = !chains.is_empty() && self.swapchain.depth_sampled;
        let render_pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(if split {
                self.passes.split
            } else {
                self.passes.clear
            })
            .framebuffer(self.swapchain.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: self.swapchain.extent,
            })
            .clear_values(&clear_values);
        self.device
            .cmd_begin_render_pass(cmd, &render_pass_info, vk::SubpassContents::INLINE);

        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: self.swapchain.extent.width as f32,
            height: self.swapchain.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.swapchain.extent,
        };
        self.device.cmd_set_viewport(cmd, 0, &[viewport]);
        self.device.cmd_set_scissor(cmd, 0, &[full_scissor]);

        let blur = frame
            .blur
            .filter(|blur| blur.radius > 0.0)
            .filter(|_| self.prepare_blur());
        let (early, late) = match blur {
            Some(blur) => calls.split_at(blur.ui_split.min(calls.len())),
            None => (calls, &[][..]),
        };

        if let Some(scene) = frame.scene {
            let part = if split {
                ScenePart::World
            } else {
                ScenePart::All
            };
            self.record_scene(cmd, scene, part);
            if split {
                self.device.cmd_end_render_pass(cmd);
                self.record_post(cmd, image_index as usize, scene, &chains);
                self.device.cmd_begin_render_pass(
                    cmd,
                    &vk::RenderPassBeginInfo::default()
                        .render_pass(self.passes.resume)
                        .framebuffer(self.swapchain.framebuffers[image_index as usize])
                        .render_area(full_scissor),
                    vk::SubpassContents::INLINE,
                );
                self.device.cmd_set_viewport(cmd, 0, &[viewport]);
                self.device.cmd_set_scissor(cmd, 0, &[full_scissor]);
                self.record_outline_blit(cmd, &chains);
                self.record_scene(cmd, scene, ScenePart::Hand);
            }
        }
        self.gpu_timer.counted_pass("ui");
        self.record_ui(cmd, frame, early, full_scissor);
        self.gpu_timer.mark(&self.device, "ui");

        self.device.cmd_end_render_pass(cmd);
        if let Some(blur) = blur {
            self.record_blur(cmd, image_index as usize, blur.radius, frame, late);
            self.gpu_timer.mark(&self.device, "menu");
        }
        let readback = if self.capturing() {
            self.record_capture(cmd, image_index as usize)
        } else {
            None
        };
        self.gpu_timer.end(self.frames.current);
        self.device.end_command_buffer(cmd)?;

        self.device.reset_fences(&[fence])?;

        let wait_semaphores = [self.frames.image_available[self.frames.current]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.frames.render_finished[image_index as usize]];
        let command_buffers = [cmd];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        self.device.queue_submit(self.queue, &[submit], fence)?;
        self.frames.submitted[self.frames.current] = self.frames.number;
        for arena in [&mut self.meshes, &mut self.quad_meshes] {
            arena.submitted();
        }
        if let Some(readback) = readback {
            self.device.wait_for_fences(&[fence], true, u64::MAX)?;
            self.finish_capture(readback);
        }

        let swapchains = [self.swapchain.handle];
        let image_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        let started = Instant::now();
        let present_result = self
            .swapchain
            .loader
            .queue_present(self.queue, &present_info);
        waits.present = started.elapsed();
        self.frames.last_waits = waits;
        match present_result {
            Ok(_) => {}
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) | Err(vk::Result::SUBOPTIMAL_KHR) => {
                let (w, h) = (self.swapchain.extent.width, self.swapchain.extent.height);
                // The frame just submitted still uses the framebuffer, the
                // depth buffer and the semaphore this destroys.
                self.device.device_wait_idle()?;
                self.create_swapchain(w, h)?;
            }
            Err(e) => return Err(e.into()),
        }

        self.frames.current = (self.frames.current + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(())
    }
}

/// The leading calls whose vertices all fit in the first `vertices`.
fn fitting(calls: &[quad::DrawCall], vertices: usize) -> &[quad::DrawCall] {
    &calls[..calls.partition_point(|c| (c.first + c.count) as usize / 6 * 4 <= vertices)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::TextureId;

    fn call(first: u32, count: u32) -> quad::DrawCall {
        quad::DrawCall {
            texture: TextureId(0),
            first,
            count,
            scissor: None,
            blend: quad::BlendMode::Alpha,
            program: quad::Program::Sprite,
        }
    }

    #[test]
    fn calls_past_the_buffer_are_dropped_whole() {
        let calls = [call(0, 6), call(6, 6), call(12, 6)];
        assert_eq!(fitting(&calls, 12).len(), 3);
        assert_eq!(fitting(&calls, 11).len(), 2);
        assert_eq!(fitting(&calls, 3).len(), 0);
    }
}
