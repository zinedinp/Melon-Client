//! The menu background blur (`Screen.extractBlurredBackground`). The
//! frame is copied (or shrunk) into an image of its own and blurred along
//! x into a second through majestix-vk; the y axis is drawn into the
//! swapchain in the frame's own render pass.

use super::{Frame, Place, Renderer, pipeline, vk_util};
use crate::quad::DrawCall;
use ash::vk;
use majestix_vk::{
    Attachment, Barriers, Binding, Descriptor, DescriptorSet, DescriptorSetLayout, Graphics,
    ImageBarrier, ImageDesc, ImageView, Pass, Pipeline, PipelineLayout, Recording, Sampler, Scope,
    Shader, Target, ViewDesc, Write,
};

/// What blurring needs besides the swapchain.
pub(super) struct Blur {
    /// majestix-vk's part; given up in [`Self::destroy`], before the device goes.
    own: Option<Own>,
    /// Built for the frame's own render pass, which draws the y axis.
    vertical: pipeline::Pass,
    /// Samples [`Targets::scratch`] for `vertical`, from the renderer's pools.
    vertical_set: [vk::DescriptorSet; 1],
    /// The renderer's sampler `vertical_set` names.
    vertical_sampler: vk::Sampler,
    /// Whether the frame is blurred at half size.
    halve: bool,
    targets: Option<Targets>,
}

/// The x axis's pass and what it draws with.
struct Own {
    device: majestix_vk::Device,
    /// One colour attachment: draws into [`Targets::scratch`].
    pass: Pass,
    horizontal: Pipeline,
    layout: PipelineLayout,
    set_layout: DescriptorSetLayout,
    sampler: Sampler,
}

struct Targets {
    /// The swapchain's size they were made for.
    extent: vk::Extent2D,
    /// Their own: half of it, or all of it without [`Blur::halve`].
    size: vk::Extent2D,
    /// The frame, shrunk or copied out of the swapchain image.
    copy: majestix_vk::Image,
    /// The frame blurred along x.
    scratch: majestix_vk::Image,
    /// Draws into `scratch` with [`Blur::pass`].
    target: Target,
    /// Samples `copy` for [`Blur::horizontal`].
    copy_set: DescriptorSet,
    /// What `copy_set` and `target` name, retired with them.
    views: [ImageView; 2],
}

/// A colour image the renderer makes itself, for [`create_image`].
pub(super) struct Image {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
}

impl Image {
    pub(super) unsafe fn destroy(&self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_image_view(self.view, None);
            device.destroy_image(self.image, None);
            vk_util::free_memory(device, self.memory);
        }
    }
}

impl Blur {
    /// The physical device tells whether `format` can be blitted down to half size.
    pub(super) unsafe fn new(
        layer: &majestix_vk::Device,
        format: vk::Format,
        frame_pass: vk::RenderPass,
        set_layout: vk::DescriptorSetLayout,
        pools: &super::descriptor_pools::DescriptorPools,
        sampler: vk::Sampler,
        cache: Option<majestix_vk::PipelineCache>,
    ) -> anyhow::Result<Self> {
        let device = layer.raw();
        let pass = Pass::new(layer, &[Attachment::cleared(format)], None)?;
        let own_layout = layer.create_descriptor_set_layout(&[Binding {
            binding: 0,
            ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            count: 1,
            stages: vk::ShaderStageFlags::FRAGMENT,
        }])?;
        let layout = layer.create_pipeline_layout(
            &[own_layout],
            &[vk::PushConstantRange {
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                offset: 0,
                size: pipeline::blur::PUSH_CONSTANT_SIZE,
            }],
        )?;
        let spirv = |bytes: &[u8]| ash::util::read_spv(&mut std::io::Cursor::new(bytes));
        // SAFETY: the build script's SPIR-V of `blur.vert` and `blur.frag`.
        let (vert, frag) = unsafe {
            (
                layer.create_shader_module(&spirv(pipeline::blur::VERT_SPV)?)?,
                layer.create_shader_module(&spirv(pipeline::blur::FRAG_SPV)?)?,
            )
        };
        let mut desc = Graphics::new(layout, Shader::main(vert), Some(Shader::main(frag)));
        desc.blend = &[majestix_vk::render::pipeline::OPAQUE];
        desc.dynamic = &[vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        desc.cache = cache;
        let horizontal = layer.create_graphics_pipeline(&pass, &desc);
        layer.retire(vert);
        layer.retire(frag);
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let vertical = unsafe { pipeline::blur::create(device, frame_pass, set_layout) }?;
        // Never freed one by one: it goes with the pools.
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let vertical_set = [unsafe { pools.allocate(device, set_layout) }?];
        // SAFETY: the layer's own physical device and instance.
        let features = unsafe {
            layer
                .instance()
                .get_physical_device_format_properties(layer.physical(), format)
        }
        .optimal_tiling_features;
        let halve = features.contains(
            vk::FormatFeatureFlags::BLIT_SRC
                | vk::FormatFeatureFlags::BLIT_DST
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
        );
        Ok(Self {
            own: Some(Own {
                device: layer.clone(),
                pass,
                horizontal: horizontal?,
                layout,
                set_layout: own_layout,
                sampler: layer.create_sampler(&sampler_info)?,
            }),
            vertical,
            vertical_set,
            vertical_sampler: sampler,
            halve,
            targets: None,
        })
    }

    /// Its set from the renderer's pools, for
    /// [`super::descriptor_pools::DescriptorPools::compact`].
    pub(super) fn sets_mut(&mut self) -> &mut [vk::DescriptorSet; 1] {
        &mut self.vertical_set
    }

    pub(super) unsafe fn destroy(&mut self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            self.destroy_targets(device);
            self.vertical.destroy(device);
        }
        if let Some(own) = self.own.take() {
            own.device.retire(own.horizontal);
            own.device.retire(own.layout);
            own.device.retire(own.set_layout);
            own.device.retire(own.sampler);
        }
    }

    pub(super) unsafe fn destroy_targets(&mut self, device: &ash::Device) {
        if let Some(targets) = self.targets.take() {
            // `vertical_set` names the scratch view, and the renderer
            // rewrites it in place: no frame may still sample it.
            // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
            let _ = unsafe { device.device_wait_idle() };
            let Targets {
                copy,
                scratch,
                target,
                copy_set,
                views,
                ..
            } = targets;
            drop(target);
            if let Some(own) = &self.own {
                own.device.retire(copy_set);
                for view in views {
                    own.device.retire(view);
                }
                own.device.retire(copy);
                own.device.retire(scratch);
            }
        }
    }

    /// The images for `extent`, made if the last frame had none or another size.
    unsafe fn ensure_targets(
        &mut self,
        format: vk::Format,
        extent: vk::Extent2D,
    ) -> anyhow::Result<()> {
        if self.targets.as_ref().is_some_and(|t| t.extent == extent) {
            return Ok(());
        }
        let own = self
            .own
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the blur was destroyed"))?;
        let device = own.device.clone();
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.destroy_targets(device.raw()) };
        let own = self
            .own
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("the blur was destroyed"))?;
        let size = if self.halve {
            vk::Extent2D {
                width: extent.width.div_ceil(2),
                height: extent.height.div_ceil(2),
            }
        } else {
            extent
        };
        let image =
            |usage| device.create_image(&ImageDesc::d2(format, size.width, size.height, usage));
        let copy = image(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)?;
        let scratch = image(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)?;
        let copy_view = device.create_image_view(&ViewDesc::d2(copy))?;
        let scratch_view = device.create_image_view(&ViewDesc::d2(scratch))?;
        let target = Target::new(&own.pass, &[scratch_view], None)?;
        let copy_set = device.create_descriptor_set(
            own.set_layout,
            &[Write::new(0, Descriptor::sampled(copy_view, own.sampler))],
        )?;
        let info = [vk::DescriptorImageInfo {
            sampler: self.vertical_sampler,
            image_view: scratch_view.raw(),
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        // SAFETY: the renderer's own set, which no frame samples (the device
        // was waited idle above, or it never was bound).
        unsafe {
            device.raw().update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(self.vertical_set[0])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            )
        };
        self.targets = Some(Targets {
            extent,
            size,
            copy,
            scratch,
            target,
            copy_set,
            views: [copy_view, scratch_view],
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
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let image = unsafe {
        device.create_image(
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
        )
    }?;
    let mut made = Image {
        image,
        memory: vk::DeviceMemory::null(),
        view: vk::ImageView::null(),
    };
    // SAFETY: `device` is live, and the infos name only its own live handles.
    match unsafe { bind_and_view(device, mem_props, format, &mut made) } {
        Ok(()) => Ok(made),
        Err(e) => {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { made.destroy(device) };
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
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let requirements = unsafe { device.get_image_memory_requirements(made.image) };
    // SAFETY: `device` is live, and the infos name only its own live handles.
    made.memory =
        unsafe { vk_util::allocate_for(device, mem_props, requirements, Place::DEVICE) }?.0;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    unsafe { device.bind_image_memory(made.image, made.memory, 0) }?;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    made.view = unsafe {
        device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(made.image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(COLOR),
            None,
        )
    }?;
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
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let made = unsafe {
            self.blur
                .ensure_targets(self.swapchain.format, self.swapchain.extent)
        };
        if let Err(e) = &made {
            melon_base::warn!("Blur unavailable: {e:#}");
            self.swapchain.can_copy = false;
        }
        made.is_ok()
    }

    /// Everything after the scene's pass: the swapchain's `image` blurred
    /// into itself, then the UI drawn over it.
    pub(super) unsafe fn record_blur(
        &self,
        rec: &mut Recording<'_>,
        image: majestix_vk::Image,
        image_index: usize,
        radius: f32,
        frame: &Frame<'_>,
        late_calls: &[DrawCall],
    ) {
        let (Some(targets), Some(own)) = (&self.blur.targets, &self.blur.own) else {
            return;
        };
        let cmd = rec.raw();
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.gpu_timer.pass("blur") };
        let extent = self.swapchain.extent;
        let size = targets.size;
        let halved = size != extent;
        let whole = |extent: vk::Extent2D| vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent,
        };
        let drawn = Scope::new(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        );
        let sampled = Scope::new(
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
        let read = Scope::new(
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_READ,
        );
        let written = Scope::new(
            vk::PipelineStageFlags2::COPY,
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let undefined = vk::ImageLayout::UNDEFINED;
        let (src, dst, shader) = (
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );
        rec.barrier(&Barriers {
            images: &[
                ImageBarrier::whole(image, drawn, read, vk::ImageLayout::PRESENT_SRC_KHR, src),
                ImageBarrier::whole(targets.copy, Scope::NONE, written, undefined, dst),
            ],
            ..Barriers::default()
        });
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
            rec.blit_image(
                image,
                src,
                targets.copy,
                dst,
                &[vk::ImageBlit {
                    src_subresource: layer,
                    src_offsets: [vk::Offset3D::default(), corner(extent)],
                    dst_subresource: layer,
                    dst_offsets: [vk::Offset3D::default(), corner(size)],
                }],
                vk::Filter::LINEAR,
            );
        } else {
            rec.copy_image(
                image,
                src,
                targets.copy,
                dst,
                &[vk::ImageCopy {
                    src_subresource: layer,
                    src_offset: vk::Offset3D::default(),
                    dst_subresource: layer,
                    dst_offset: vk::Offset3D::default(),
                    extent: targets.copy.extent(),
                }],
            );
        }
        rec.barrier(&Barriers {
            images: &[
                ImageBarrier::whole(targets.copy, written, sampled, dst, shader),
                ImageBarrier::whole(
                    targets.scratch,
                    Scope::NONE,
                    drawn,
                    undefined,
                    vk::ImageLayout::ATTACHMENT_OPTIMAL,
                ),
            ],
            ..Barriers::default()
        });
        let radius = if halved {
            (radius / 2.0).floor()
        } else {
            radius
        };
        let viewport = |extent: vk::Extent2D| vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        {
            let clear = vk::ClearValue::default();
            let mut pass = rec.begin_pass(&targets.target, &[clear]);
            pass.set_viewport(0, &[viewport(size)]);
            pass.set_scissor(0, &[whole(size)]);
            pass.bind_pipeline(own.horizontal);
            pass.bind_descriptor_sets(
                vk::PipelineBindPoint::GRAPHICS,
                own.layout,
                0,
                &[targets.copy_set],
                &[],
            );
            let push = [1.0 / size.width as f32, 0.0, radius, 0.0];
            pass.push_constants(own.layout, vk::ShaderStageFlags::FRAGMENT, 0, &push);
            pass.draw(3, 1, 0, 0);
        }
        rec.barrier(&Barriers {
            images: &[ImageBarrier::whole(
                targets.scratch,
                drawn,
                sampled,
                vk::ImageLayout::ATTACHMENT_OPTIMAL,
                shader,
            )],
            ..Barriers::default()
        });

        // The y axis, straight into the swapchain in the frame's own pass.
        let device = &self.device;
        let vertical = &self.blur.vertical;
        let push = [0.0, 1.0 / size.height as f32, radius, 0.0];
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.passes.overwrite)
                    .framebuffer(self.swapchain.framebuffers[image_index])
                    .render_area(whole(extent)),
                vk::SubpassContents::INLINE,
            );
            device.cmd_set_viewport(cmd, 0, &[viewport(extent)]);
            device.cmd_set_scissor(cmd, 0, &[whole(extent)]);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, vertical.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                vertical.layout,
                0,
                &self.blur.vertical_set,
                &[],
            );
            device.cmd_push_constants(
                cmd,
                vertical.layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                pipeline::push_constant_bytes(&push, pipeline::blur::PUSH_CONSTANT_SIZE),
            );
            device.cmd_draw(cmd, 3, 1, 0, 0);
        }
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.gpu_timer.mark(device, "blur");
            self.gpu_timer.pass("menu");
            self.record_ui(cmd, frame, late_calls, whole(extent));
            device.cmd_end_render_pass(cmd);
        }
    }
}
