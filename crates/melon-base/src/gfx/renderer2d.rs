//! A renderer for 2D interfaces alone: one pass that draws a [`DrawList`].

use super::descriptor_pools::DescriptorPools;
use super::gui_uniforms::{self, GuiFrame, GuiUniforms};
use super::pipeline::ui::{self, UiPipelines};
use super::quad::{BlendMode, DrawList};
use super::staging::Staging;
use super::suballoc;
use super::texture::{Texture, Uploader};
use super::texture_update::{self, PendingInit};
use super::ui_vertices::{UiVertices, VERTEX_CAPACITY};
use super::{
    PassStart, TextureId, choose_surface_format, create_depth_image, create_render_pass,
    create_sampler, device, features, find_depth_format, instance, pipeline, vk_util,
};
use ash::vk;
use image::RgbaImage;
use majestix_vk::{Frames, PresentConfig, Presenter};
use numerobis::vulkan::swapchain::{Encoding, Extent, Output, Pacing};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::mem::ManuallyDrop;

const FRAMES_IN_FLIGHT: usize = 2;
/// Descriptor sets in the first pool; a full pool is followed by a bigger one.
const TEXTURES: u32 = 256;

pub struct Renderer2d {
    /// Keeps the Vulkan loader loaded.
    _entry: ash::Entry,
    instance: ash::Instance,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    info: super::DeviceInfo,
    /// The Vulkan loader's instance version, packed.
    loader_version: u32,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    /// `layer`'s device, for the calls the renderer still makes itself.
    device: ash::Device,
    /// majestix-vk's device and queue.
    layer: ManuallyDrop<Layer>,
    /// The frames in flight; lent out while a frame is rendered.
    ring: Option<Frames>,
    /// The swapchain; lent out while a frame is rendered.
    presenter: Option<Presenter>,
    /// The window's size, which the next swapchain is made for.
    window: vk::Extent2D,
    render_pass: vk::RenderPass,
    depth_format: vk::Format,
    swapchain: Swapchain,
    texture_layout: vk::DescriptorSetLayout,
    gui: GuiUniforms,
    pipelines: UiPipelines,
    pools: DescriptorPools,
    sampler: vk::Sampler,
    textures: Vec<Texture>,
    /// New textures' pixels, copied in at the start of the next frame.
    pending: Vec<PendingInit>,
    staging: Vec<Staging>,
    white: TextureId,
    vertices: UiVertices,
}

/// majestix-vk's device and queue, dropped by hand in [`Drop`]: after the
/// renderer's own handles, before the surface and the instance.
struct Layer {
    queue: majestix_vk::Queue,
    device: majestix_vk::Device,
}

/// What is sized with the presenter's swapchain.
struct Swapchain {
    /// What the render pass was made for.
    format: vk::Format,
    extent: vk::Extent2D,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    depth: (vk::Image, vk::DeviceMemory, vk::ImageView),
    framebuffers: Vec<vk::Framebuffer>,
}

/// FIFO, in UNORM as the client's (an `_SRGB` format would encode the
/// colour a second time).
fn present_config() -> PresentConfig {
    PresentConfig {
        pacing: Pacing::Vsync,
        requested: None,
        outputs: vec![Output::Sdr(Encoding::Unorm)],
        usage: vk::ImageUsageFlags::empty(),
    }
}

impl Renderer2d {
    pub fn new(window: &winit::window::Window, enable_validation: bool) -> anyhow::Result<Self> {
        // SAFETY: the renderer makes every handle itself, so the module's rules start out true.
        unsafe { Self::new_inner(window, enable_validation) }
    }

    unsafe fn new_inner(
        window: &winit::window::Window,
        enable_validation: bool,
    ) -> anyhow::Result<Self> {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let entry = unsafe { super::loader::load() }?;
        let display = window.display_handle()?.as_raw();
        let instance::Instance {
            instance,
            debug,
            api,
            ..
        // SAFETY: `device` is live, and the infos name only its own live handles.
        } = unsafe { instance::create(&entry, display, enable_validation) }?;
        let (surface_loader, surface) =
            // SAFETY: `instance` is live, and the physical device and surface are its own.
            unsafe { instance::create_surface(&entry, &instance, display, window.window_handle()?.as_raw()) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let gpu = unsafe { device::pick(&instance, api, &surface_loader, surface) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let features = unsafe {
            features::probe(
                &instance,
                gpu.physical,
                api,
                &[
                    numerobis::vulkan::Capability::RobustBufferAccess,
                    numerobis::vulkan::Capability::PageableDeviceLocalMemory,
                ],
            )
        };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let (raw_device, _) = unsafe { device::create(&instance, &gpu, &features) }?;
        // SAFETY: made just now from `features.plan` on `gpu.physical`, with one
        // queue of `gpu.queue_family`; the layer is dropped before the instance.
        let layer = unsafe {
            majestix_vk::Device::adopt(
                &instance,
                gpu.physical,
                raw_device,
                features.plan.clone(),
                &[(gpu.queue_family, 1)],
            )
        }?;
        let device = layer.raw().clone();
        let queue = layer.queue(gpu.queue_family, 0)?;
        let ring = Frames::new(&layer, &queue, FRAMES_IN_FLIGHT as u32)?;
        let mem_props = gpu.mem_props;

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let depth_format = unsafe { find_depth_format(&instance, gpu.physical) }?;
        let surface_format = choose_surface_format(&surface_loader, gpu.physical, surface)?.format;
        let render_pass =
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { create_render_pass(&device, surface_format, depth_format, PassStart::Clear) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let texture_layout = unsafe { pipeline::create_descriptor_set_layout(&device) }?;
        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let gui = GuiUniforms::new(&layer, &queue, &ring, &white)?;
        let gui_layout = gui.layout();
        let pipelines = UiPipelines {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            built_in: unsafe {
                ui::create_all(
                    &device,
                    render_pass,
                    texture_layout,
                    gui_layout,
                    ui::BUILT_IN,
                )
            }?,
            pack: [None, None, None],
            // SAFETY: `device` is live, and the infos name only its own live handles.
            glint: unsafe {
                ui::create(
                    &device,
                    render_pass,
                    texture_layout,
                    gui_layout,
                    BlendMode::Alpha,
                    ui::GLINT,
                )
            }?,
        };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pools = unsafe { DescriptorPools::new(&device, TEXTURES) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sampler = unsafe {
            create_sampler(
                &device,
                vk::Filter::NEAREST,
                false,
                vk::SamplerAddressMode::CLAMP_TO_EDGE,
            )
        }?;
        let vertices = UiVertices::new(&layer, &queue, &ring)?;
        // SAFETY: `surface` is the instance's, outlives the presenter (dropped
        // first in `Drop`), and `device::pick` chose a family that presents to it.
        let presenter =
            unsafe { Presenter::new(&layer, &entry, surface, &ring, present_config()) }?;
        // SAFETY: `entry` holds the loaded Vulkan loader.
        let loader_version = unsafe { entry.try_enumerate_instance_version() }
            .ok()
            .flatten()
            .unwrap_or(vk::API_VERSION_1_0);

        let size = window.inner_size();
        let mut renderer = Self {
            _entry: entry,
            swapchain: Swapchain {
                format: surface_format,
                extent: vk::Extent2D::default(),
                images: Vec::new(),
                views: Vec::new(),
                depth: Default::default(),
                framebuffers: Vec::new(),
            },
            instance,
            debug,
            surface_loader,
            surface,
            info: gpu.info.clone(),
            loader_version,
            mem_props,
            device,
            layer: ManuallyDrop::new(Layer {
                queue,
                device: layer,
            }),
            ring: Some(ring),
            presenter: Some(presenter),
            window: vk::Extent2D {
                width: size.width.max(1),
                height: size.height.max(1),
            },
            render_pass,
            depth_format,
            texture_layout,
            gui,
            pipelines,
            pools,
            sampler,
            textures: Vec::new(),
            pending: Vec::new(),
            staging: (0..FRAMES_IN_FLIGHT).map(|_| Staging::default()).collect(),
            white: TextureId(0),
            vertices,
        };
        renderer.white = renderer.load_texture(&white)?;
        Ok(renderer)
    }

    /// The GPU it draws with.
    pub fn device_info(&self) -> &super::DeviceInfo {
        &self.info
    }

    /// The Vulkan loader's version, e.g. "1.4.304".
    pub fn loader_version(&self) -> String {
        let v = self.loader_version;
        format!(
            "{}.{}.{}",
            vk::api_version_major(v),
            vk::api_version_minor(v),
            vk::api_version_patch(v)
        )
    }

    /// A 1x1 white texture, for untextured quads.
    pub fn white_texture(&self) -> TextureId {
        self.white
    }

    /// A texture sampled without smoothing; it can be drawn from the next frame on.
    pub fn load_texture(&mut self, rgba: &RgbaImage) -> anyhow::Result<TextureId> {
        let uploader = Uploader {
            device: &self.device,
            mem_props: &self.mem_props,
            descriptor_pools: &self.pools,
            descriptor_set_layout: self.texture_layout,
            sampler: self.sampler,
        };
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let (texture, init) = unsafe { uploader.image_2d(rgba)? };
        self.pending.push(init);
        self.textures.push(texture);
        Ok(TextureId(self.textures.len() - 1))
    }

    /// The window's new size; the next frame makes the swapchain for it.
    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.window = vk::Extent2D { width, height };
        if let Some(presenter) = &mut self.presenter {
            presenter.invalidate();
        }
        Ok(())
    }

    /// Draws `ui`, whose coordinates span `ui_size`, over `clear_color`.
    pub fn render(
        &mut self,
        ui: &DrawList,
        ui_size: (f32, f32),
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        // SAFETY: the renderer owns its device and frames, and `&mut self` keeps recording on one thread.
        unsafe { self.render_inner(ui, ui_size, clear_color) }
    }

    /// The views, depth buffer and framebuffers for `presenter`'s swapchain.
    unsafe fn rebuild_targets(&mut self, presenter: &Presenter) -> anyhow::Result<()> {
        let Some((extent, format, _, _)) = presenter.current() else {
            return Ok(());
        };
        anyhow::ensure!(
            format == self.swapchain.format,
            "the swapchain came in {format:?}, the render pass was made for {:?}",
            self.swapchain.format
        );
        // The old targets may still be drawn into by a frame in flight.
        self.layer.device.wait_idle()?;
        // SAFETY: made by this device, and the wait above finished every frame using it.
        unsafe { self.destroy_swapchain_resources() };
        self.swapchain.extent = extent;
        self.swapchain.images = presenter.images().iter().map(|i| i.raw()).collect();
        for &image in &self.swapchain.images {
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
            let view = unsafe { self.device.create_image_view(&view_info, None) }?;
            self.swapchain.views.push(view);
        }
        // SAFETY: `device` is live, and the infos name only its own live handles.
        self.swapchain.depth = unsafe {
            create_depth_image(
                &self.device,
                &self.mem_props,
                self.depth_format,
                extent,
                false,
            )
        }?;
        for &view in &self.swapchain.views {
            let attachments = [view, self.swapchain.depth.2];
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(self.render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let framebuffer = unsafe { self.device.create_framebuffer(&info, None) }?;
            self.swapchain.framebuffers.push(framebuffer);
        }
        Ok(())
    }

    unsafe fn destroy_swapchain_resources(&mut self) {
        for framebuffer in self.swapchain.framebuffers.drain(..) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_framebuffer(framebuffer, None) };
        }
        for view in self.swapchain.views.drain(..) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_image_view(view, None) };
        }
        self.swapchain.images.clear();
        let (image, memory, view) = std::mem::take(&mut self.swapchain.depth);
        if image != vk::Image::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                self.device.destroy_image_view(view, None);
                self.device.destroy_image(image, None);
                vk_util::free_memory(&self.device, memory);
            }
        }
    }

    unsafe fn render_inner(
        &mut self,
        ui: &DrawList,
        ui_size: (f32, f32),
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let mut ring = self
            .ring
            .take()
            .expect("the frame ring is back after each frame");
        let mut presenter = self
            .presenter
            .take()
            .expect("the presenter is back after each frame");
        // SAFETY: the caller's promise, passed on.
        let result =
            unsafe { self.render_frame(&mut ring, &mut presenter, ui, ui_size, clear_color) };
        self.ring = Some(ring);
        self.presenter = Some(presenter);
        result
    }

    unsafe fn render_frame(
        &mut self,
        ring: &mut Frames,
        presenter: &mut Presenter,
        ui: &DrawList,
        ui_size: (f32, f32),
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let vertex_count = ui.vertices.len().min(VERTEX_CAPACITY);
        let fitting = ui
            .calls
            .iter()
            .take_while(|call| (call.first + call.count) as usize / 6 * 4 <= vertex_count)
            .count();
        let calls = &ui.calls[..fitting];
        // Waits for the slot's last frame and destroys what finished frames retired.
        let frame = ring.begin()?;
        let current = frame.slot().index() as usize;

        // Every allocation before an image is acquired: an image acquired and
        // then dropped costs the presenter a device wait.
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            texture_update::prepare(
                &mut self.staging[current],
                &self.device,
                &self.mem_props,
                &self.pending,
                &[],
            )
        }?;
        let window = Extent::new(self.window.width, self.window.height);
        // Minimized: nothing to draw into, the frame is skipped.
        let Some(acquired) = presenter.acquire(&frame, window)? else {
            return Ok(());
        };
        let fresh = presenter.images().len() != self.swapchain.images.len()
            || presenter
                .images()
                .iter()
                .zip(&self.swapchain.images)
                .any(|(new, &old)| new.raw() != old);
        if fresh || self.swapchain.framebuffers.is_empty() {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { self.rebuild_targets(presenter) }?;
        }
        let image_index = acquired.index as usize;
        let (width, height) = (self.swapchain.extent.width, self.swapchain.extent.height);

        self.vertices.write(&frame, &ui.vertices);
        self.gui.write(
            &frame,
            &GuiFrame {
                gui_size: ui_size,
                screen_size: (width as f32, height as f32),
                game_time: gui_uniforms::game_time(),
            },
        );

        let rec = frame.record()?;
        let cmd = rec.raw();
        let inits = std::mem::take(&mut self.pending);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            texture_update::record(
                &mut self.staging[current],
                &self.device,
                &self.mem_props,
                cmd,
                &inits,
                &[],
            )
        }?;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color,
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let full = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: self.swapchain.extent,
        };
        let pass_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.swapchain.framebuffers[image_index])
            .render_area(full)
            .clear_values(&clear_values);
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            self.device
                .cmd_begin_render_pass(cmd, &pass_info, vk::SubpassContents::INLINE)
        };
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.device.cmd_set_viewport(cmd, 0, &[viewport]) };
        let textures = &self.textures;
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            ui::record(
                &ui::Target {
                    device: &self.device,
                    cmd,
                    pipelines: &self.pipelines,
                    gui_set: self.gui.set(current),
                    vertices: self.vertices.bound(current),
                    extent: self.swapchain.extent,
                },
                ui_size,
                calls,
                full,
                |texture| textures[texture.0].descriptor_set,
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe { self.device.cmd_end_render_pass(cmd) };
        let recorded = rec.finish()?;

        let waits_on = [(
            acquired.wait,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        )];
        frame.submit(&self.layer.queue, [recorded], &waits_on, &[acquired.signal])?;
        // Out of date or resized: the next acquire makes a new swapchain.
        presenter.present(&self.layer.queue, acquired)?;
        Ok(())
    }
}

impl Drop for Renderer2d {
    fn drop(&mut self) {
        // SAFETY: the device is waited idle first, and every handle freed is this renderer's own.
        unsafe {
            let _ = self.layer.device.wait_idle();
            self.vertices.destroy();
            for texture in &self.textures {
                self.device.destroy_image_view(texture.view, None);
                self.device.destroy_image(texture.image, None);
                suballoc::release(&self.device, texture.memory);
            }
            for staging in &mut self.staging {
                staging.destroy(&self.device);
            }
            self.destroy_swapchain_resources();
            self.device.destroy_sampler(self.sampler, None);
            self.pipelines.destroy(&self.device);
            self.gui.destroy();
            self.device
                .destroy_descriptor_set_layout(self.texture_layout, None);
            self.pools.destroy(&self.device);
            self.device.destroy_render_pass(self.render_pass, None);
            suballoc::forget(&self.device);
            // The swapchain, then the frames (which destroy what was
            // retired), then the device, all before the surface.
            drop(self.presenter.take());
            drop(self.ring.take());
            ManuallyDrop::drop(&mut self.layer);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((debug, messenger)) = self.debug.take() {
                debug.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
