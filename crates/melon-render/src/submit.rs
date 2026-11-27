//! One frame on the GPU.

use super::globals::Globals;
use super::record::scene::ScenePart;
use super::{Frame, FrameWaits, Renderer, VERTEX_CAPACITY};
use ash::vk;
use majestix_vk::{Frames, Presenter};
use melon_base::gfx::quad;
use numerobis::vulkan::swapchain::Extent;
use std::time::Instant;

impl Renderer {
    pub fn render(&mut self, frame: &Frame<'_>) -> anyhow::Result<()> {
        // SAFETY: the renderer owns its device and frames, and `&mut self` keeps recording on one thread.
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

    /// The newest counted frame: how much each counted pass shaded, and how
    /// many frames were counted so far. Empty without `pipelineStatisticsQuery`
    /// or [`Self::count_shading`].
    pub fn gpu_last_stats(&self) -> (u64, &[(&'static str, super::gpu_timer::PassStats)]) {
        self.gpu_timer.last_stats()
    }

    /// Counts the shading of every `every`th frame (pipeline statistics); 0 stops it.
    pub fn count_shading(&mut self, every: u32) {
        self.gpu_timer.count_every(every);
    }

    /// The staging and draw buffers this frame grows into, and the gathered
    /// draws copied in; once the frame slot is free.
    unsafe fn prepare_frame(&mut self) -> anyhow::Result<()> {
        let current = self.frames.current;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            super::texture_update::prepare(
                &mut self.textures.update_staging[current],
                &self.device,
                &self.mem_props,
                &self.textures.pending_inits,
                &self.textures.pending_updates,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.residency.refresh(&self.instance, self.physical_device) };
        for arena in [
            &mut self.meshes,
            &mut self.quad_meshes,
            &mut self.particle_meshes,
            &mut self.index_lists,
        ] {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { arena.prepare(&self.device, &self.mem_props, current) }?;
        }
        if let Some(indirect) = &mut self.indirect {
            let draws = indirect.gathered();
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { indirect.prepare(&self.device, &self.mem_props, current, draws) }?;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { indirect.upload(current, self.occlusion.is_some()) };
            if let Some(occlusion) = &mut self.occlusion
                && draws > 0
            {
                let buffers = indirect.culling_buffers(current);
                let slots = indirect.slots();
                // SAFETY: `device` is live, and the infos name only its own live handles.
                unsafe {
                    occlusion.prepare(&self.device, &self.mem_props, current, slots, buffers)
                }?;
            }
        }
        Ok(())
    }

    pub(super) unsafe fn render_inner(&mut self, frame: &Frame<'_>) -> anyhow::Result<()> {
        let mut ring = self
            .ring
            .take()
            .expect("the frame ring is back after each frame");
        let mut presenter = self
            .presenter
            .take()
            .expect("the presenter is back after each frame");
        // SAFETY: the caller's promise, passed on.
        let result = unsafe { self.render_frame(&mut ring, &mut presenter, frame) };
        self.ring = Some(ring);
        self.presenter = Some(presenter);
        result
    }

    unsafe fn render_frame(
        &mut self,
        ring: &mut Frames,
        presenter: &mut Presenter,
        frame: &Frame<'_>,
    ) -> anyhow::Result<()> {
        let ui_vertices = frame.ui.vertices.len().min(VERTEX_CAPACITY);
        if ui_vertices < frame.ui.vertices.len() {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                melon_base::warn!(
                    "UI vertex buffer full ({} > {VERTEX_CAPACITY}): the rest is not drawn",
                    frame.ui.vertices.len()
                );
            });
        }
        let calls = fitting(&frame.ui.calls, ui_vertices);
        let mut waits = FrameWaits::default();
        // What only reads the CPU's state, before waiting for the frame's slot.
        let started = Instant::now();
        if let Some(indirect) = &mut self.indirect {
            match frame.scene.and_then(|s| s.world.as_ref()) {
                Some(world) => indirect.gather(world, &self.quad_meshes),
                None => indirect.clear(),
            }
        }
        waits.gather = started.elapsed();
        let started = Instant::now();
        // Waits for the slot's last frame and destroys what finished frames retired.
        let gpu_frame = ring.begin()?;
        let current = gpu_frame.slot().index() as usize;
        self.frames.current = current;
        self.frames.submitted[current] = 0;
        self.frames.number += 1;
        self.destroy_retired_textures();
        waits.gpu = started.elapsed();

        // Every allocation the frame needs, before an image is acquired: an
        // image acquired and then dropped costs the presenter a device wait.
        // Out of memory (another program holding the VRAM) just skips the frame.
        let started = Instant::now();
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.prepare_frame() }?;
        waits.prepare = started.elapsed();

        let started = Instant::now();
        let window = Extent::new(self.swapchain.window.width, self.swapchain.window.height);
        // Minimized: nothing to draw into, the frame is skipped.
        let Some(acquired) = presenter.acquire(&gpu_frame, window)? else {
            return Ok(());
        };
        waits.acquire = started.elapsed();
        let fresh = presenter.images().len() != self.swapchain.images.len()
            || presenter
                .images()
                .iter()
                .zip(&self.swapchain.images)
                .any(|(new, &old)| new.raw() != old);
        if fresh || self.swapchain.framebuffers.is_empty() {
            // A new swapchain (or a rebuild that failed half-way).
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { self.rebuild_targets(presenter) }?;
        }
        let image_index = acquired.index;
        let recording = Instant::now();

        self.ui_vertices.write(&gpu_frame, &frame.ui.vertices);

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
        self.globals.write(&gpu_frame, &globals);
        self.gui_uniforms.write(
            &gpu_frame,
            &super::gui_uniforms::GuiFrame {
                gui_size: frame.ui_size,
                screen_size: (
                    self.swapchain.extent.width as f32,
                    self.swapchain.extent.height as f32,
                ),
                game_time: super::gui_uniforms::game_time(),
            },
        );

        let mut rec = gpu_frame.record()?;
        let cmd = rec.raw();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.gpu_timer.begin(&self.device, cmd, self.frames.current);
            self.gpu_timer.pass("upload");
        }
        let inits = std::mem::take(&mut self.textures.pending_inits);
        self.textures.pending_init_bytes = 0;
        let updates = std::mem::take(&mut self.textures.pending_updates);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            super::texture_update::record(
                &mut self.textures.update_staging[self.frames.current],
                &self.device,
                &self.mem_props,
                cmd,
                &inits,
                &updates,
            )
        }?;
        let oldest_in_flight = self.frames.oldest_in_flight();
        for arena in [
            &mut self.meshes,
            &mut self.quad_meshes,
            &mut self.particle_meshes,
            &mut self.index_lists,
        ] {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                arena.record(
                    &self.device,
                    &self.mem_props,
                    cmd,
                    self.frames.current,
                    oldest_in_flight,
                )
            }?;
        }
        self.meshes.take_destroyed();
        self.index_lists.take_destroyed();
        self.particle_meshes.take_destroyed();
        for buffer in self.quad_meshes.take_destroyed() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.quad_sets.forget(&self.device, buffer) };
        }

        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.gpu_timer.mark(&self.device, "upload");
            self.record_pictures(cmd, frame.pictures);
        }

        // The terrain's occlusion culling: phase 1 picked before the scene's pass.
        let (draws, runs) = self
            .indirect
            .as_ref()
            .map_or((0, 0), |i| (i.gathered(), i.run_count()));
        let occluding = draws > 0
            && frame.scene.is_some_and(|scene| scene.world.is_some())
            && self.occlusion.as_ref().is_some_and(|o| o.ready());
        self.recording.occluding.set(occluding);
        self.recording
            .framebuffer
            .set(self.swapchain.framebuffers[image_index as usize]);
        if occluding && let Some(occlusion) = &mut self.occlusion {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.gpu_timer.pass("occlusion");
                occlusion.record_select(&self.device, cmd, self.frames.current, (draws, runs));
                self.gpu_timer.mark(&self.device, "occlusion");
            }
        }

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
            // Culling reads the depth phase 1 leaves, as a post chain does.
            .render_pass(if split || occluding {
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
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &render_pass_info, vk::SubpassContents::INLINE)
        };

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
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device.cmd_set_viewport(cmd, 0, &[viewport]);
            self.device.cmd_set_scissor(cmd, 0, &[full_scissor]);
        }

        let blur = frame
            .blur
            .filter(|blur| blur.radius > 0.0)
            // SAFETY: `device` is live, and the infos name only its own live handles.
            .filter(|_| unsafe { self.prepare_blur() });
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
            let started = Instant::now();
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_scene(cmd, scene, part) };
            waits.record_scene = started.elapsed();
            if split {
                // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
                unsafe {
                    self.device.cmd_end_render_pass(cmd);
                    self.record_post(cmd, image_index as usize, scene, &chains);
                }
                // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
                unsafe {
                    self.device.cmd_begin_render_pass(
                        cmd,
                        &vk::RenderPassBeginInfo::default()
                            .render_pass(self.passes.resume)
                            .framebuffer(self.swapchain.framebuffers[image_index as usize])
                            .render_area(full_scissor),
                        vk::SubpassContents::INLINE,
                    )
                };
                // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
                unsafe {
                    self.device.cmd_set_viewport(cmd, 0, &[viewport]);
                    self.device.cmd_set_scissor(cmd, 0, &[full_scissor]);
                    self.record_outline_blit(cmd, &chains);
                    self.record_scene(cmd, scene, ScenePart::Hand);
                }
            }
        }
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.counted_pass("ui") };
        let started = Instant::now();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.record_ui(cmd, frame, early, full_scissor) };
        waits.record_ui = started.elapsed();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.mark(&self.device, "ui") };

        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.device.cmd_end_render_pass(cmd) };
        if let Some(blur) = blur {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe {
                self.record_blur(
                    &mut rec,
                    acquired.image,
                    image_index as usize,
                    blur.radius,
                    frame,
                    late,
                );
                self.gpu_timer.mark(&self.device, "menu");
            }
        }
        let readback = if self.capturing() {
            // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
            unsafe { self.record_capture(cmd, image_index as usize) }
        } else {
            None
        };
        self.gpu_timer.end(self.frames.current);
        let recorded = match rec.finish() {
            Ok(recorded) => recorded,
            Err(e) => {
                // SAFETY: made by this device, and no frame in flight uses it any more.
                unsafe { self.drop_capture(readback) };
                return Err(e.into());
            }
        };

        let submitting = Instant::now();
        let waits_on = [(
            acquired.wait,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        )];
        let submitted =
            gpu_frame.submit(&self.layer.queue, [recorded], &waits_on, &[acquired.signal]);
        if let Err(e) = submitted {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.drop_capture(readback) };
            return Err(e.into());
        }
        waits.submit = submitting.elapsed();
        waits.record =
            (submitting - recording).saturating_sub(waits.record_scene + waits.record_ui);
        self.frames.submitted[self.frames.current] = self.frames.number;
        for arena in [
            &mut self.meshes,
            &mut self.quad_meshes,
            &mut self.particle_meshes,
            &mut self.index_lists,
        ] {
            arena.submitted();
        }
        if let Some(readback) = readback {
            // Its frame has to finish first; a capture is rare (a world's icon).
            if let Err(e) = self.layer.device.wait_idle() {
                // SAFETY: the device is waited idle (or lost), so nothing writes it any more.
                unsafe { self.drop_capture(Some(readback)) };
                return Err(e.into());
            }
            // SAFETY: the frame that copied into the readback finished: the device is idle.
            unsafe { self.finish_capture(readback) };
        }

        let started = Instant::now();
        // Out of date or resized: the next acquire makes a new swapchain.
        presenter.present(&self.layer.queue, acquired)?;
        waits.present = started.elapsed();
        self.frames.last_waits = waits;
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
    use melon_base::gfx::TextureId;

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
