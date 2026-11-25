//! The swapchain and what is sized with it (depth buffer, framebuffers).

use super::{Renderer, choose_surface_format, create_depth_image, vk_util};
use ash::vk;

/// `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR` (same value as the `EXT` one);
pub const FIFO_LATEST_READY: vk::PresentModeKHR = vk::PresentModeKHR::from_raw(1000361000);

impl Renderer {
    pub(super) unsafe fn create_swapchain(
        &mut self,
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        let capabilities = self
            .surface_loader
            .get_physical_device_surface_capabilities(self.physical_device, self.surface)?;
        let format =
            choose_surface_format(&self.surface_loader, self.physical_device, self.surface)?;
        let modes = self
            .surface_loader
            .get_physical_device_surface_present_modes(self.physical_device, self.surface)
            .unwrap_or_default();
        let requested = self.swapchain.requested.filter(|mode| {
            let offered = *mode == vk::PresentModeKHR::FIFO
                || (modes.contains(mode)
                    && (*mode != FIFO_LATEST_READY || self.swapchain.latest_ready));
            if !offered {
                crate::warn!(
                    "vulkan: present mode {} unavailable ({:?}), choosing one",
                    mode_name(*mode),
                    modes
                );
            }
            offered
        });
        let present_mode = requested.unwrap_or(if self.swapchain.vsync {
            vk::PresentModeKHR::FIFO
        } else {
            [vk::PresentModeKHR::IMMEDIATE, vk::PresentModeKHR::MAILBOX]
                .into_iter()
                .find(|mode| modes.contains(mode))
                .unwrap_or(vk::PresentModeKHR::FIFO)
        });
        self.swapchain.present_mode = present_mode;

        let current = capabilities.current_extent;
        if (current.width == 0 || current.height == 0)
            && self.swapchain.handle != vk::SwapchainKHR::null()
        {
            // Minimized on Windows: no extent is valid, keep the old
            // swapchain until the window has a size again.
            return Ok(());
        }
        let extent = if current.width != u32::MAX {
            current
        } else {
            vk::Extent2D {
                width: width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        };
        let extent = vk::Extent2D {
            width: extent.width.max(1),
            height: extent.height.max(1),
        };

        let mut image_count = (capabilities.min_image_count + 1).max(3);
        if capabilities.max_image_count > 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }

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
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(super::vk_util::composite_alpha(
                capabilities.supported_composite_alpha,
            ))
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old_swapchain);
        let swapchain = self.swapchain.loader.create_swapchain(&create_info, None)?;

        self.destroy_swapchain_resources();
        if old_swapchain != vk::SwapchainKHR::null() {
            self.swapchain.loader.destroy_swapchain(old_swapchain, None);
        }
        // Kept at once: a failure below leaves the new swapchain without
        // framebuffers, which the next frame recreates, not a freed handle.
        self.swapchain.handle = swapchain;
        self.swapchain.extent = extent;
        self.swapchain.format = format.format;
        self.swapchain.can_copy = can_copy;

        let images = self.swapchain.loader.get_swapchain_images(swapchain)?;
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
            views.push(self.device.create_image_view(&view_info, None)?);
        }

        let (image, memory, depth_view) = create_depth_image(
            &self.device,
            &self.mem_props,
            self.swapchain.depth_format,
            extent,
            self.swapchain.depth_sampled,
        )?;
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
            framebuffers.push(self.device.create_framebuffer(&fb_info, None)?);
        }

        let mut render_finished = Vec::with_capacity(images.len());
        for _ in 0..images.len() {
            render_finished.push(
                self.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
            );
        }

        self.swapchain.views = views;
        self.swapchain.depth = depth;
        self.swapchain.framebuffers = framebuffers;
        self.frames.render_finished = render_finished;
        self.frames.images_in_flight = vec![vk::Fence::null(); images.len()];
        self.swapchain.images = images;
        self.resize_post()?;
        if let Some(occlusion) = &mut self.occlusion {
            occlusion.resize(
                &self.device,
                &self.mem_props,
                self.swapchain.depth.image,
                self.swapchain.depth_format,
                extent,
            )?;
        }
        Ok(())
    }

    pub(super) unsafe fn destroy_swapchain_resources(&mut self) {
        self.blur.destroy_targets(&self.device);
        self.post.release_sized(&self.device);
        if let Some(occlusion) = &mut self.occlusion {
            occlusion.release_sized(&self.device);
        }
        for &fb in &self.swapchain.framebuffers {
            self.device.destroy_framebuffer(fb, None);
        }
        self.swapchain.framebuffers.clear();
        for &view in &self.swapchain.views {
            self.device.destroy_image_view(view, None);
        }
        self.swapchain.views.clear();
        let depth = std::mem::take(&mut self.swapchain.depth);
        if depth.image != vk::Image::null() {
            self.device.destroy_image_view(depth.view, None);
            self.device.destroy_image(depth.image, None);
            vk_util::free_memory(&self.device, depth.memory);
        }
        for &sem in &self.frames.render_finished {
            self.device.destroy_semaphore(sem, None);
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
        requested: Option<vk::PresentModeKHR>,
    ) -> anyhow::Result<()> {
        if (vsync, requested) == (self.swapchain.vsync, self.swapchain.requested) {
            return Ok(());
        }
        self.swapchain.vsync = vsync;
        self.swapchain.requested = requested;
        let (width, height) = (self.swapchain.extent.width, self.swapchain.extent.height);
        unsafe {
            self.device.device_wait_idle()?;
            self.create_swapchain(width.max(1), height.max(1))?;
        }
        Ok(())
    }

    /// The present mode in use, for F3: `fifo`, `immediate` or `mailbox`.
    pub fn present_mode_name(&self) -> &'static str {
        mode_name(self.swapchain.present_mode)
    }

    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        unsafe {
            self.device.device_wait_idle()?;
            self.create_swapchain(width, height)?;
        }
        Ok(())
    }
}

/// A present mode's short name, for F3 and the log.
fn mode_name(mode: vk::PresentModeKHR) -> &'static str {
    match mode {
        vk::PresentModeKHR::IMMEDIATE => "immediate",
        vk::PresentModeKHR::MAILBOX => "mailbox",
        vk::PresentModeKHR::FIFO_RELAXED => "fifo_relaxed",
        FIFO_LATEST_READY => "fifo_latest_ready",
        _ => "fifo",
    }
}
