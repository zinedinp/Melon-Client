//! What is sized with the presenter's swapchain (views, depth buffer,
//! framebuffers), and the settings the presenter makes it with.

use super::Renderer;
use ash::vk;
use majestix_vk::{ImageDesc, MemoryUse, PresentConfig, Presenter, ViewDesc};
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
        let layer = &self.layer.device;
        let images: Vec<vk::Image> = presenter.images().iter().map(|i| i.raw()).collect();

        let mut views = Vec::with_capacity(images.len());
        for &image in presenter.images() {
            views.push(layer.create_image_view(&ViewDesc::d2(image))?);
        }
        // Kept at once, so a failure below leaves them to the next rebuild.
        self.swapchain.views = views;
        self.swapchain.images = images;

        // Transient unless a post chain or culling reads it.
        let sampled = self.swapchain.depth_sampled;
        let mut desc = ImageDesc::d2(
            self.swapchain.depth_format,
            extent.width,
            extent.height,
            if sampled {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED
            } else {
                vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT
                    | vk::ImageUsageFlags::TRANSIENT_ATTACHMENT
            },
        );
        if !sampled {
            desc.memory = MemoryUse::Transient;
        }
        let depth = layer.create_image(&desc)?;
        let depth_view = match layer.create_image_view(&ViewDesc::d2(depth)) {
            Ok(view) => view,
            Err(e) => {
                layer.retire(depth);
                return Err(e.into());
            }
        };
        self.swapchain.depth = super::DepthImage(Some((depth, depth_view)));
        let depth_view = depth_view.raw();

        let mut framebuffers = Vec::with_capacity(self.swapchain.views.len());
        for view in &self.swapchain.views {
            let attachments = [view.raw(), depth_view];
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
        self.resize_post()?;
        if let Some(occlusion) = &mut self.occlusion {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                occlusion.resize(
                    &self.device,
                    &self.mem_props,
                    self.swapchain.depth.image(),
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
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.blur.destroy_targets(&self.device) };
        self.post.release_sized();
        if let Some(occlusion) = &mut self.occlusion {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { occlusion.release_sized(&self.device) };
        }
        for &fb in &self.swapchain.framebuffers {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_framebuffer(fb, None) };
        }
        self.swapchain.framebuffers.clear();
        let layer = &self.layer.device;
        for view in self.swapchain.views.drain(..) {
            layer.retire(view);
        }
        self.swapchain.images.clear();
        if let Some((image, view)) = std::mem::take(&mut self.swapchain.depth).0 {
            layer.retire(view);
            layer.retire(image);
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
