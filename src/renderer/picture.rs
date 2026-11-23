//! Pictures drawn on the GPU into a texture of their own, then shown by the
//! UI like any other texture: vanilla's `PictureInPictureRenderer` (the
//! entity in an inventory's box). They are drawn before the frame's pass,
//! with the world's pipelines, so they look like the world's entities.

use std::collections::HashMap;

use ash::vk;
use glam::{Mat4, Vec3};

use super::frame::DEPTH_CLEAR;
use super::pipeline::world::{DrawLight, Mode};
use super::{
    Blending, EntityDraw, PassStart, Renderer, TextureId, blur, create_depth_image,
    create_render_pass, texture,
};

/// One picture for this frame: `entities` drawn into `target`.
pub struct PictureDraw {
    /// A texture from [`Renderer::create_picture`].
    pub target: TextureId,
    /// From the draws' space ([`EntityDraw::model`]; their `origin` is not
    /// used) to clip space.
    pub view_proj: Mat4,
    pub entities: Vec<EntityDraw>,
    /// `textures/misc/enchanted_glint_{armor,item}.png`, for the foil.
    pub glint_armor: Option<TextureId>,
    pub glint_item: Option<TextureId>,
}

/// What a picture's colour texture needs besides itself.
struct Target {
    extent: vk::Extent2D,
    depth: (vk::Image, vk::DeviceMemory, vk::ImageView),
    framebuffer: vk::Framebuffer,
}

impl Target {
    unsafe fn destroy(&self, device: &ash::Device) {
        device.destroy_framebuffer(self.framebuffer, None);
        device.destroy_image_view(self.depth.2, None);
        device.destroy_image(self.depth.0, None);
        device.free_memory(self.depth.1, None);
    }
}

pub(super) struct Pictures {
    /// [`PassStart::Picture`].
    pass: vk::RenderPass,
    targets: HashMap<usize, Target>,
    /// Freed targets with the frame they were freed in.
    retired: Vec<(u64, Target)>,
}

impl Pictures {
    pub(super) unsafe fn new(
        device: &ash::Device,
        format: vk::Format,
        depth_format: vk::Format,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            // Made as the frame's pass is, so the world's pipelines fit it:
            // render passes are compatible only with the same dependencies.
            pass: create_render_pass(device, format, depth_format, PassStart::Picture)?,
            targets: HashMap::new(),
            retired: Vec::new(),
        })
    }

    pub(super) unsafe fn destroy(&mut self, device: &ash::Device) {
        for target in self.targets.values() {
            target.destroy(device);
        }
        for (_, target) in &self.retired {
            target.destroy(device);
        }
        self.targets.clear();
        self.retired.clear();
        device.destroy_render_pass(self.pass, None);
    }
}

impl Renderer {
    /// A `width`x`height` texture that [`PictureDraw`]s draw into. It shows
    /// nothing until drawn into: the UI only draws it in a frame that does.
    pub fn create_picture(&mut self, width: u32, height: u32) -> anyhow::Result<TextureId> {
        let extent = vk::Extent2D {
            width: width.max(1),
            height: height.max(1),
        };
        let (tex, target) = unsafe { self.make_picture(extent)? };
        let id = self.put_texture(tex);
        self.pictures.targets.insert(id.0, target);
        Ok(id)
    }

    unsafe fn make_picture(
        &self,
        extent: vk::Extent2D,
    ) -> anyhow::Result<(texture::Texture, Target)> {
        let device = &self.device;
        let color = blur::create_image(
            device,
            &self.mem_props,
            self.swapchain.format,
            extent,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?;
        let destroy_color = || {
            device.destroy_image_view(color.view, None);
            device.destroy_image(color.image, None);
            device.free_memory(color.memory, None);
        };
        let depth = match create_depth_image(
            device,
            &self.mem_props,
            self.swapchain.depth_format,
            extent,
            false,
        ) {
            Ok(depth) => depth,
            Err(e) => {
                destroy_color();
                return Err(e);
            }
        };
        let destroy_depth = || {
            device.destroy_image_view(depth.2, None);
            device.destroy_image(depth.0, None);
            device.free_memory(depth.1, None);
        };
        let attachments = [color.view, depth.2];
        let framebuffer = match device.create_framebuffer(
            &vk::FramebufferCreateInfo::default()
                .render_pass(self.pictures.pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1),
            None,
        ) {
            Ok(framebuffer) => framebuffer,
            Err(e) => {
                destroy_depth();
                destroy_color();
                return Err(e.into());
            }
        };
        let target = Target {
            extent,
            depth,
            framebuffer,
        };
        let set = match self
            .textures
            .descriptor_pools
            .allocate(device, self.pipelines.layout)
        {
            Ok(set) => set,
            Err(e) => {
                target.destroy(device);
                destroy_color();
                return Err(e);
            }
        };
        let image_info = [vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(color.view)
            .sampler(self.textures.nearest_sampler)];
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&image_info)],
            &[],
        );
        let tex = texture::Texture {
            image: color.image,
            memory: color.memory,
            view: color.view,
            descriptor_set: set,
            mip_levels: 1,
            shared: false,
        };
        Ok((tex, target))
    }

    /// The size [`Self::create_picture`] made `id` with.
    pub fn picture_size(&self, id: TextureId) -> Option<(u32, u32)> {
        let extent = self.pictures.targets.get(&id.0)?.extent;
        Some((extent.width, extent.height))
    }

    /// A freed texture's picture target goes with it, once no frame in
    /// flight draws into it.
    pub(super) fn retire_picture(&mut self, id: TextureId) {
        if let Some(target) = self.pictures.targets.remove(&id.0) {
            self.pictures.retired.push((self.frames.number, target));
        }
    }

    pub(super) fn destroy_retired_pictures(&mut self, all: bool) {
        let oldest_in_flight = self.frames.oldest_in_flight();
        let device = &self.device;
        self.pictures.retired.retain(|(frame, target)| {
            if !all && *frame >= oldest_in_flight {
                return true;
            }
            unsafe { target.destroy(device) };
            false
        });
    }

    /// Draws every picture of the frame, before its main pass.
    pub(super) unsafe fn record_pictures(&self, cmd: vk::CommandBuffer, pictures: &[PictureDraw]) {
        for picture in pictures {
            let Some(target) = self.pictures.targets.get(&picture.target.0) else {
                continue;
            };
            let Some(texture) = self.textures.list.get(picture.target.0) else {
                continue;
            };
            self.record_picture(cmd, picture, target, texture.image);
        }
    }

    unsafe fn record_picture(
        &self,
        cmd: vk::CommandBuffer,
        picture: &PictureDraw,
        target: &Target,
        image: vk::Image,
    ) {
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: DEPTH_CLEAR,
                    stencil: 0,
                },
            },
        ];
        let area = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: target.extent,
        };
        self.device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(self.pictures.pass)
                .framebuffer(target.framebuffer)
                .render_area(area)
                .clear_values(&clear_values),
            vk::SubpassContents::INLINE,
        );
        self.device.cmd_set_viewport(
            cmd,
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: target.extent.width as f32,
                height: target.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        self.device.cmd_set_scissor(cmd, 0, &[area]);
        self.recording.forget_all();
        // Opaque first, then the blended ones, as the world's entities.
        let mut entities: Vec<&EntityDraw> = picture.entities.iter().collect();
        entities.sort_by_key(|e| e.blend);
        for entity in entities {
            let (mode, layout) = match entity.blend {
                Blending::Opaque => (Mode::Opaque, self.pipelines.world.layout),
                Blending::Text => (Mode::Text, self.pipelines.text.layout),
                Blending::Translucent | Blending::Additive => (
                    Mode::EntityTranslucent,
                    self.pipelines.entity_translucent.layout,
                ),
            };
            let at = (picture.view_proj * entity.model, Vec3::ZERO);
            let [r, g, b] = entity.light.unwrap_or([1.0; 3]);
            let light = DrawLight::tint([r, g, b, entity.alpha]).overlaid(entity.overlay);
            if self.bind_world_pass(cmd, entity.atlas, mode) {
                self.draw_world_mesh(cmd, layout, entity.mesh, at, &light, None);
            }
            if entity.glint.on() {
                let glint = if entity.glint.is_armor() {
                    picture.glint_armor
                } else {
                    picture.glint_item
                };
                if let Some(glint) = glint
                    && self.bind_glint_pass(cmd, entity.atlas, glint)
                {
                    self.draw_world_mesh(
                        cmd,
                        self.pipelines.glint.layout,
                        entity.mesh,
                        at,
                        &light.glinted(entity.glint.scale()),
                        None,
                    );
                }
            }
        }
        self.device.cmd_end_render_pass(cmd);
        // The UI samples it in the frame's pass.
        let barrier = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(blur::COLOR);
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

/// From a picture's pixels (x right, y down, z toward the viewer, as
/// [`crate::gui::entity_preview::EntityPreview::view`] places an entity)
/// to clip space. `depth` is how far from z = 0 the picture reaches.
pub fn pixel_projection(width: u32, height: u32, depth: f32) -> Mat4 {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    // Reverse Z: nearer is larger, the depth cleared to 0 is the farthest.
    Mat4::from_translation(Vec3::new(-1.0, -1.0, 0.5))
        * Mat4::from_scale(Vec3::new(2.0 / w, 2.0 / h, 0.5 / depth.max(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture's corners land on clip space's, nearer is larger.
    #[test]
    fn pixels_map_onto_clip_space() {
        let projection = pixel_projection(100, 50, 10.0);
        let at = |x, y, z| projection.project_point3(Vec3::new(x, y, z));
        assert!(at(0.0, 0.0, 0.0).abs_diff_eq(Vec3::new(-1.0, -1.0, 0.5), 1e-6));
        assert!(at(100.0, 50.0, 0.0).abs_diff_eq(Vec3::new(1.0, 1.0, 0.5), 1e-6));
        assert!(at(0.0, 0.0, 10.0).z > at(0.0, 0.0, -10.0).z);
        assert!((at(0.0, 0.0, 10.0).z - 1.0).abs() < 1e-6);
        assert!(at(0.0, 0.0, -10.0).z.abs() < 1e-6);
    }
}
