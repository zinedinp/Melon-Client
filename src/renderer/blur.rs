//! The menu background blur (`Screen.extractBlurredBackground`).

use super::{Frame, Renderer, find_memory_type, pipeline, vk_util};
use crate::renderer::quad::DrawCall;
use ash::vk;

/// What blurring needs besides the swapchain.
pub(super) struct Blur {
    /// One colour attachment, left readable: draws the x axis into `scratch`.
    pass: vk::RenderPass,
    /// Built for `pass`.
    horizontal: pipeline::Pass,
    /// Built for the frame's own render pass.
    vertical: pipeline::Pass,
    /// Sample [`Targets::copy`] and [`Targets::scratch`].
    sets: [vk::DescriptorSet; 2],
    sampler: vk::Sampler,
    /// Whether the frame is blurred at half size.
    halve: bool,
    targets: Option<Targets>,
}

struct Targets {
    /// The swapchain's size they were made for.
    extent: vk::Extent2D,
    /// Their own: half of it, or all of it without [`Blur::halve`].
    size: vk::Extent2D,
    /// The frame, shrunk or copied out of the swapchain image.
    copy: Image,
    /// The frame blurred along x.
    scratch: Image,
    /// Draws into `scratch` with [`Blur::pass`].
    framebuffer: vk::Framebuffer,
}

pub(super) struct Image {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
}

impl Image {
    pub(super) unsafe fn destroy(&self, device: &ash::Device) {
        device.destroy_image_view(self.view, None);
        device.destroy_image(self.image, None);
        vk_util::free_memory(device, self.memory);
    }
}

impl Blur {
    /// The physical device tells whether `format` can be blitted down to half size.
    pub(super) unsafe fn new(
        (instance, physical_device): (&ash::Instance, vk::PhysicalDevice),
        device: &ash::Device,
        format: vk::Format,
        frame_pass: vk::RenderPass,
        set_layout: vk::DescriptorSetLayout,
        pools: &super::descriptor_pools::DescriptorPools,
        sampler: vk::Sampler,
    ) -> anyhow::Result<Self> {
        let pass = Self::create_pass(device, format)?;
        let horizontal = pipeline::blur::create(device, pass, set_layout)?;
        let vertical = pipeline::blur::create(device, frame_pass, set_layout)?;
        // Never freed one by one: they go with the pools.
        let sets = [
            pools.allocate(device, set_layout)?,
            pools.allocate(device, set_layout)?,
        ];
        let features = instance
            .get_physical_device_format_properties(physical_device, format)
            .optimal_tiling_features;
        let halve = features.contains(
            vk::FormatFeatureFlags::BLIT_SRC
                | vk::FormatFeatureFlags::BLIT_DST
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        );
        Ok(Self {
            pass,
            horizontal,
            vertical,
            sets,
            sampler,
            halve,
            targets: None,
        })
    }

    /// Its sets, for [`super::descriptor_pools::DescriptorPools::compact`].
    pub(super) fn sets_mut(&mut self) -> &mut [vk::DescriptorSet; 2] {
        &mut self.sets
    }

    /// Colour only; ready to be sampled when it ends.
    unsafe fn create_pass(
        device: &ash::Device,
        format: vk::Format,
    ) -> anyhow::Result<vk::RenderPass> {
        let attachments = [vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let color = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color)];
        let dependencies = [
            vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE),
            vk::SubpassDependency::default()
                .src_subpass(0)
                .dst_subpass(vk::SUBPASS_EXTERNAL)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ),
        ];
        Ok(device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies),
            None,
        )?)
    }

    pub(super) unsafe fn destroy(&mut self, device: &ash::Device) {
        self.destroy_targets(device);
        self.horizontal.destroy(device);
        self.vertical.destroy(device);
        device.destroy_render_pass(self.pass, None);
    }

    pub(super) unsafe fn destroy_targets(&mut self, device: &ash::Device) {
        if let Some(targets) = self.targets.take() {
            let _ = device.device_wait_idle();
            device.destroy_framebuffer(targets.framebuffer, None);
            targets.copy.destroy(device);
            targets.scratch.destroy(device);
        }
    }

    /// The images for `extent`, made if the last frame had none or another size.
    unsafe fn ensure_targets(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> anyhow::Result<()> {
        if self.targets.as_ref().is_some_and(|t| t.extent == extent) {
            return Ok(());
        }
        self.destroy_targets(device);
        let size = if self.halve {
            vk::Extent2D {
                width: extent.width.div_ceil(2),
                height: extent.height.div_ceil(2),
            }
        } else {
            extent
        };
        let copy = create_image(
            device,
            mem_props,
            format,
            size,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        )?;
        let scratch = match create_image(
            device,
            mem_props,
            format,
            size,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        ) {
            Ok(scratch) => scratch,
            Err(e) => {
                copy.destroy(device);
                return Err(e);
            }
        };
        let attachments = [scratch.view];
        let framebuffer = match device.create_framebuffer(
            &vk::FramebufferCreateInfo::default()
                .render_pass(self.pass)
                .attachments(&attachments)
                .width(size.width)
                .height(size.height)
                .layers(1),
            None,
        ) {
            Ok(framebuffer) => framebuffer,
            Err(e) => {
                copy.destroy(device);
                scratch.destroy(device);
                return Err(e.into());
            }
        };
        for (set, image) in self.sets.iter().zip([&copy, &scratch]) {
            let info = [vk::DescriptorImageInfo {
                sampler: self.sampler,
                image_view: image.view,
                image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            }];
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(*set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            );
        }
        self.targets = Some(Targets {
            extent,
            size,
            copy,
            scratch,
            framebuffer,
        });
        Ok(())
    }
}

pub(super) unsafe fn create_image(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    format: vk::Format,
    extent: vk::Extent2D,
    usage: vk::ImageUsageFlags,
) -> anyhow::Result<Image> {
    let image = device.create_image(
        &vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED),
        None,
    )?;
    let mut made = Image {
        image,
        memory: vk::DeviceMemory::null(),
        view: vk::ImageView::null(),
    };
    match bind_and_view(device, mem_props, format, &mut made) {
        Ok(()) => Ok(made),
        Err(e) => {
            made.destroy(device);
            Err(e)
        }
    }
}

/// Gives `made.image` its memory and view, each kept in `made` as it is made.
unsafe fn bind_and_view(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    format: vk::Format,
    made: &mut Image,
) -> anyhow::Result<()> {
    let requirements = device.get_image_memory_requirements(made.image);
    let type_index = find_memory_type(
        mem_props,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;
    made.memory = vk_util::allocate_memory(
        device,
        &vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(type_index),
    )?;
    device.bind_image_memory(made.image, made.memory, 0)?;
    made.view = device.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(made.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(COLOR),
        None,
    )?;
    Ok(())
}

pub(super) const COLOR: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

impl Renderer {
    /// Whether this frame can be blurred.
    pub(super) unsafe fn prepare_blur(&mut self) -> bool {
        if !self.swapchain.can_copy {
            return false;
        }
        let made = self.blur.ensure_targets(
            &self.device,
            &self.mem_props,
            self.swapchain.format,
            self.swapchain.extent,
        );
        if let Err(e) = &made {
            crate::warn!("Blur unavailable: {e:#}");
            self.swapchain.can_copy = false;
        }
        made.is_ok()
    }

    /// Everything after the scene's pass.
    pub(super) unsafe fn record_blur(
        &self,
        cmd: vk::CommandBuffer,
        image_index: usize,
        radius: f32,
        frame: &Frame<'_>,
        late_calls: &[DrawCall],
    ) {
        let Some(targets) = &self.blur.targets else {
            return;
        };
        self.gpu_timer.pass("blur");
        let device = &self.device;
        let swapchain_image = self.swapchain.images[image_index];
        let extent = self.swapchain.extent;
        let size = targets.size;
        let halved = size != extent;
        let whole = |extent: vk::Extent2D| vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let scissor = whole(extent);
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
                    swapchain_image,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                    vk::AccessFlags::TRANSFER_READ,
                ),
                barrier(
                    targets.copy.image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                ),
            ],
        );
        let layer = vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        };
        let corner = |e: vk::Extent2D| vk::Offset3D {
            x: e.width as i32,
            y: e.height as i32,
            z: 1,
        };
        if halved {
            device.cmd_blit_image(
                cmd,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                targets.copy.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageBlit {
                    src_subresource: layer,
                    src_offsets: [vk::Offset3D::default(), corner(extent)],
                    dst_subresource: layer,
                    dst_offsets: [vk::Offset3D::default(), corner(size)],
                }],
                vk::Filter::LINEAR,
            );
        } else {
            device.cmd_copy_image(
                cmd,
                swapchain_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                targets.copy.image,
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
            );
        }
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier(
                targets.copy.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
            )],
        );

        let begin = |render_pass, framebuffer, extent: vk::Extent2D| {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(render_pass)
                    .framebuffer(framebuffer)
                    .render_area(whole(extent)),
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
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            device.cmd_set_scissor(cmd, 0, &[whole(extent)]);
        };
        let radius = if halved {
            (radius / 2.0).floor()
        } else {
            radius
        };
        let draw_axis = |pass: &pipeline::Pass, set, step: [f32; 2]| {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                pass.layout,
                0,
                &[set],
                &[],
            );
            let push = [step[0], step[1], radius, 0.0];
            device.cmd_push_constants(
                cmd,
                pass.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                pipeline::push_constant_bytes(&push, pipeline::blur::PUSH_CONSTANT_SIZE),
            );
            device.cmd_draw(cmd, 3, 1, 0, 0);
        };
        begin(self.blur.pass, targets.framebuffer, size);
        draw_axis(
            &self.blur.horizontal,
            self.blur.sets[0],
            [1.0 / size.width as f32, 0.0],
        );
        device.cmd_end_render_pass(cmd);
        begin(
            self.passes.overwrite,
            self.swapchain.framebuffers[image_index],
            extent,
        );
        draw_axis(
            &self.blur.vertical,
            self.blur.sets[1],
            [0.0, 1.0 / size.height as f32],
        );
        self.gpu_timer.mark(device, "blur");
        self.gpu_timer.pass("menu");
        self.record_ui(cmd, frame, late_calls, scissor);
        device.cmd_end_render_pass(cmd);
    }
}
