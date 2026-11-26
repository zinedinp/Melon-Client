//! The swapchain and what is sized with it (depth buffer, framebuffers).

use super::{Renderer, choose_surface_format, create_depth_image, vk_util};
use ash::vk;
use numerobis::vulkan::swapchain::{self as present, Pacing, PresentMode};

impl Renderer {
    pub(super) unsafe fn create_swapchain(
        &mut self,
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)
        }?;
        let format =
            choose_surface_format(&self.surface_loader, self.physical_device, self.surface)?;
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, self.surface)
        }
        .unwrap_or_default();
        let offered: Vec<u32> = modes.iter().map(|mode| mode.as_raw() as u32).collect();
        let pacing = if self.swapchain.vsync {
            Pacing::Vsync
        } else {
            Pacing::Uncapped
        };
        let requested = self.swapchain.requested;
        let present_mode =
            present::present_mode(&offered, pacing, requested, self.swapchain.latest_ready);
        if let Some(asked) = requested
            && asked != present_mode
        {
            crate::warn!("vulkan: present mode {asked} unavailable ({modes:?}), choosing one");
        }
        self.swapchain.present_mode = present_mode;

        let window = vk::Extent2D { width, height };
        let has_old = self.swapchain.handle != vk::SwapchainKHR::null();
        let Some(shape) = vk_util::swapchain_shape(&capabilities, window, 3, has_old) else {
            // Minimized on Windows: no extent is valid, keep the old
            // swapchain until the window has a size again.
            return Ok(());
        };
        let extent = shape.extent;

        let can_copy = capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC);
        let usage = if can_copy {
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC
        } else {
            vk::ImageUsageFlags::COLOR_ATTACHMENT
        };

        let old_swapchain = self.swapchain.handle;
        let create_info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(shape.image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(shape.composite_alpha)
            .present_mode(vk::PresentModeKHR::from_raw(present_mode.raw() as i32))
            .clipped(true)
            .old_swapchain(old_swapchain);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let swapchain = unsafe { self.swapchain.loader.create_swapchain(&create_info, None) }?;

        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.destroy_swapchain_resources() };
        if old_swapchain != vk::SwapchainKHR::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.swapchain.loader.destroy_swapchain(old_swapchain, None) };
        }
        // Kept at once: a failure below leaves the new swapchain without
        // framebuffers, which the next frame recreates, not a freed handle.
        self.swapchain.handle = swapchain;
        self.swapchain.extent = extent;
        self.swapchain.format = format.format;
        self.swapchain.can_copy = can_copy;

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let images = unsafe { self.swapchain.loader.get_swapchain_images(swapchain) }?;
        let mut views = Vec::with_capacity(images.len());
        for &image in &images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });
            // SAFETY: `device` is live, and the infos name only its own live handles.
            views.push(unsafe { self.device.create_image_view(&view_info, None) }?);
        }

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let (image, memory, depth_view) = unsafe {
            create_depth_image(
                &self.device,
                &self.mem_props,
                self.swapchain.depth_format,
                extent,
                self.swapchain.depth_sampled,
            )
        }?;
        let depth = super::DepthImage {
            image,
            memory,
            view: depth_view,
        };

        let mut framebuffers = Vec::with_capacity(views.len());
        for &view in &views {
            let attachments = [view, depth_view];
            let fb_info = vk::FramebufferCreateInfo::default()
                .render_pass(self.passes.clear)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            // SAFETY: `device` is live, and the infos name only its own live handles.
            framebuffers.push(unsafe { self.device.create_framebuffer(&fb_info, None) }?);
        }

        let mut render_finished = Vec::with_capacity(images.len());
        for _ in 0..images.len() {
            render_finished.push(
                // SAFETY: `device` is live, and the infos name only its own live handles.
                unsafe {
                    self.device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                }?,
            );
        }

        self.swapchain.views = views;
        self.swapchain.depth = depth;
        self.swapchain.framebuffers = framebuffers;
        self.frames.render_finished = render_finished;
        self.frames.images_in_flight = vec![vk::Fence::null(); images.len()];
        self.swapchain.images = images;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.resize_post() }?;
        if let Some(occlusion) = &mut self.occlusion {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                occlusion.resize(
                    &self.device,
                    &self.mem_props,
                    self.swapchain.depth.image,
                    self.swapchain.depth_format,
                    extent,
                )
            }?;
        }
        Ok(())
    }

    pub(super) unsafe fn destroy_swapchain_resources(&mut self) {
        // SAFETY: live handles of this device, a recording command buffer, nothing in flight freed.
        unsafe {
            self.blur.destroy_targets(&self.device);
            self.post.release_sized(&self.device);
        }
        if let Some(occlusion) = &mut self.occlusion {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { occlusion.release_sized(&self.device) };
        }
        for &fb in &self.swapchain.framebuffers {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_framebuffer(fb, None) };
        }
        self.swapchain.framebuffers.clear();
        for &view in &self.swapchain.views {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_image_view(view, None) };
        }
        self.swapchain.views.clear();
        let depth = std::mem::take(&mut self.swapchain.depth);
        if depth.image != vk::Image::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                self.device.destroy_image_view(depth.view, None);
                self.device.destroy_image(depth.image, None);
                vk_util::free_memory(&self.device, depth.memory);
            }
        }
        for &sem in &self.frames.render_finished {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_semaphore(sem, None) };
        }
        self.frames.render_finished.clear();
    }

    /// Turns VSync on or off, recreating the swapchain if that changes it.
    pub fn set_vsync(&mut self, vsync: bool) -> anyhow::Result<()> {
        self.set_present(vsync, self.swapchain.requested)
    }

    /// VSync and, when `requested` is set, the present mode by name.
    pub fn set_present(
        &mut self,
        vsync: bool,
        requested: Option<PresentMode>,
    ) -> anyhow::Result<()> {
        if (vsync, requested) == (self.swapchain.vsync, self.swapchain.requested) {
            return Ok(());
        }
        self.swapchain.vsync = vsync;
        self.swapchain.requested = requested;
        let (width, height) = (self.swapchain.extent.width, self.swapchain.extent.height);
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            self.device.device_wait_idle()?;
            self.create_swapchain(width.max(1), height.max(1))?;
        }
        Ok(())
    }

    /// The present mode in use, for F3: `fifo`, `immediate` or `mailbox`.
    pub fn present_mode_name(&self) -> &'static str {
        self.swapchain.present_mode.name()
    }

    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            self.device.device_wait_idle()?;
            self.create_swapchain(width, height)?;
        }
        Ok(())
    }
}
