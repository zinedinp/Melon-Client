//! What is sized with the presenter's swapchain (views, depth buffer,
//! framebuffers), and the settings the presenter makes it with.

use super::{Renderer, create_depth_image, vk_util};
use ash::vk;
use majestix_vk::{PresentConfig, Presenter};
use numerobis::vulkan::swapchain::{Encoding, Output, Pacing, PresentMode};

impl super::Swapchain {
    /// What the presenter makes swapchains with: UNORM, as vanilla's GL
    /// framebuffer stores colour (an `_SRGB` format would encode it a second
    /// time), copyable for the blur and screenshots where the surface allows.
    pub(super) fn present_config(&self) -> PresentConfig {
        PresentConfig {
            pacing: if self.vsync {
                Pacing::Vsync
            } else {
                Pacing::Uncapped
            },
            requested: self.requested,
            outputs: vec![Output::Sdr(Encoding::Unorm)],
            usage: vk::ImageUsageFlags::TRANSFER_SRC,
        }
    }
}

impl Renderer {
    /// The views, depth buffer, framebuffers and what is sized with them,
    /// for `presenter`'s swapchain.
    pub(super) unsafe fn rebuild_targets(&mut self, presenter: &Presenter) -> anyhow::Result<()> {
        let Some((extent, format, _, mode)) = presenter.current() else {
            return Ok(());
        };
        anyhow::ensure!(
            format == self.swapchain.format,
            "the swapchain came in {format:?}, the render passes were made for {:?}",
            self.swapchain.format
        );
        if let Some(asked) = self.swapchain.requested
            && asked != mode
        {
            melon_base::warn!("vulkan: present mode {asked} unavailable, presenting with {mode}");
        }
        // The old targets may still be drawn into by a frame in flight.
        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        unsafe { self.device.device_wait_idle() }?;
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.destroy_swapchain_resources() };
        self.swapchain.extent = extent;
        self.swapchain.present_mode = mode;
        self.swapchain.can_copy = presenter
            .usage()
            .contains(vk::ImageUsageFlags::TRANSFER_SRC);
        let images: Vec<vk::Image> = presenter.images().iter().map(|i| i.raw()).collect();

        let mut views = Vec::with_capacity(images.len());
        for &image in &images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
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
        // Kept at once, so a failure below leaves them to the next rebuild.
        self.swapchain.views = views;
        self.swapchain.images = images;

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
        self.swapchain.depth = super::DepthImage {
            image,
            memory,
            view: depth_view,
        };

        let mut framebuffers = Vec::with_capacity(self.swapchain.views.len());
        for &view in &self.swapchain.views {
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
        self.swapchain.framebuffers = framebuffers;
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

    /// [`Self::rebuild_targets`] outside a frame (a post chain now reads depth).
    pub(super) unsafe fn rebuild_targets_now(&mut self) -> anyhow::Result<()> {
        let Some(presenter) = self.presenter.take() else {
            return Ok(());
        };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let result = unsafe { self.rebuild_targets(&presenter) };
        self.presenter = Some(presenter);
        result
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
        self.swapchain.images.clear();
        let depth = std::mem::take(&mut self.swapchain.depth);
        if depth.image != vk::Image::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                self.device.destroy_image_view(depth.view, None);
                self.device.destroy_image(depth.image, None);
                vk_util::free_memory(&self.device, depth.memory);
            }
        }
    }

    /// Turns VSync on or off, recreating the swapchain if that changes it.
    pub fn set_vsync(&mut self, vsync: bool) -> anyhow::Result<()> {
        self.set_present(vsync, self.swapchain.requested)
    }

    /// VSync and, when `requested` is set, the present mode by name; the
    /// next frame makes the swapchain with them.
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
        let config = self.swapchain.present_config();
        if let Some(presenter) = &mut self.presenter {
            presenter.set_config(config);
        }
        Ok(())
    }

    /// The present mode in use, for F3: `fifo`, `immediate` or `mailbox`.
    pub fn present_mode_name(&self) -> &'static str {
        self.swapchain.present_mode.name()
    }

    /// The window's new size; the next frame makes the swapchain for it.
    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.swapchain.window = vk::Extent2D { width, height };
        if let Some(presenter) = &mut self.presenter {
            presenter.invalidate();
        }
        Ok(())
    }
}
