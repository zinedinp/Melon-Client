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
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

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
    physical: vk::PhysicalDevice,
    info: super::DeviceInfo,
    /// The Vulkan loader's instance version, packed.
    loader_version: u32,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    device: ash::Device,
    queue: vk::Queue,
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
    frames: Frames,
    vertices: UiVertices,
}

struct Swapchain {
    loader: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    extent: vk::Extent2D,
    views: Vec<vk::ImageView>,
    depth: (vk::Image, vk::DeviceMemory, vk::ImageView),
    framebuffers: Vec<vk::Framebuffer>,
}

struct Frames {
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    /// One per swapchain image.
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    images_in_flight: Vec<vk::Fence>,
    current: usize,
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
        let (device, queue) = unsafe { device::create(&instance, &gpu, &features) }?;
        let mem_props = gpu.mem_props;

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let depth_format = unsafe { find_depth_format(&instance, gpu.physical) }?;
        let surface_format = choose_surface_format(&surface_loader, gpu.physical, surface)?.format;
        let render_pass =
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { create_render_pass(&device, surface_format, depth_format, PassStart::Clear) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let texture_layout = unsafe { pipeline::create_descriptor_set_layout(&device) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let gui_layout = unsafe { gui_uniforms::create_layout(&device) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let gui = unsafe { GuiUniforms::new(&device, &mem_props, gui_layout, FRAMES_IN_FLIGHT) }?;
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
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let frames = unsafe { create_frames(&device, gpu.queue_family) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let vertices = unsafe { UiVertices::new(&device, &mem_props, FRAMES_IN_FLIGHT) }?;
        // SAFETY: `entry` holds the loaded Vulkan loader.
        let loader_version = unsafe { entry.try_enumerate_instance_version() }
            .ok()
            .flatten()
            .unwrap_or(vk::API_VERSION_1_0);

        let mut renderer = Self {
            _entry: entry,
            swapchain: Swapchain {
                loader: ash::khr::swapchain::Device::new(&instance, &device),
                handle: vk::SwapchainKHR::null(),
                extent: vk::Extent2D::default(),
                views: Vec::new(),
                depth: Default::default(),
                framebuffers: Vec::new(),
            },
            instance,
            debug,
            surface_loader,
            surface,
            physical: gpu.physical,
            info: gpu.info.clone(),
            loader_version,
            mem_props,
            device,
            queue,
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
            frames,
            vertices,
        };
        let size = window.inner_size();
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { renderer.create_swapchain(size.width.max(1), size.height.max(1)) }?;
        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        renderer.white = renderer.load_texture(&white)?;
        let view = renderer.textures[renderer.white.0].view;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            renderer
                .gui
                .bind_glint(&renderer.device, view, renderer.sampler)
        };
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

    /// The window was resized to `width` x `height` pixels.
    pub fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            self.device.device_wait_idle()?;
            self.create_swapchain(width, height)
        }
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

    unsafe fn create_swapchain(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical, self.surface)
        }?;
        let format = choose_surface_format(&self.surface_loader, self.physical, self.surface)?;
        let old = self.swapchain.handle;
        let window = vk::Extent2D { width, height };
        let Some(shape) =
            vk_util::swapchain_shape(&capabilities, window, 3, old != vk::SwapchainKHR::null())
        else {
            // Minimized: keep the old swapchain until the window has a size again.
            return Ok(());
        };
        let extent = shape.extent;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(shape.image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(shape.composite_alpha)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true)
            .old_swapchain(old);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let handle = unsafe { self.swapchain.loader.create_swapchain(&info, None) }?;
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.destroy_swapchain_resources() };
        if old != vk::SwapchainKHR::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.swapchain.loader.destroy_swapchain(old, None) };
        }
        self.swapchain.handle = handle;
        self.swapchain.extent = extent;

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let images = unsafe { self.swapchain.loader.get_swapchain_images(handle) }?;
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
        for _ in 0..images.len() {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let semaphore = unsafe {
                self.device
                    .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            }?;
            self.frames.render_finished.push(semaphore);
        }
        self.frames.images_in_flight = vec![vk::Fence::null(); images.len()];
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
        let (image, memory, view) = std::mem::take(&mut self.swapchain.depth);
        if image != vk::Image::null() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe {
                self.device.destroy_image_view(view, None);
                self.device.destroy_image(image, None);
                vk_util::free_memory(&self.device, memory);
            }
        }
        for semaphore in self.frames.render_finished.drain(..) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
    }

    unsafe fn render_inner(
        &mut self,
        ui: &DrawList,
        ui_size: (f32, f32),
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let current = self.frames.current;
        let vertex_count = ui.vertices.len().min(VERTEX_CAPACITY);
        let fitting = ui
            .calls
            .iter()
            .take_while(|call| (call.first + call.count) as usize / 6 * 4 <= vertex_count)
            .count();
        let calls = &ui.calls[..fitting];
        let fence = self.frames.in_flight[current];
        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX) }?;

        // Every allocation before an image is acquired: a failure after that would leave the
        // image acquired with nothing to present it.
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
        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        let acquired = unsafe {
            self.swapchain.loader.acquire_next_image(
                self.swapchain.handle,
                u64::MAX,
                self.frames.image_available[current],
                vk::Fence::null(),
            )
        };
        let (width, height) = (self.swapchain.extent.width, self.swapchain.extent.height);
        let image_index = match acquired {
            Ok((index, _suboptimal)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
                unsafe { self.device.device_wait_idle() }?;
                // SAFETY: `device` is live, and the infos name only its own live handles.
                return unsafe { self.create_swapchain(width, height) };
            }
            Err(e) => return Err(e.into()),
        };
        let image_fence = self.frames.images_in_flight[image_index as usize];
        if image_fence != vk::Fence::null() {
            // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
            unsafe { self.device.wait_for_fences(&[image_fence], true, u64::MAX) }?;
        }
        self.frames.images_in_flight[image_index as usize] = fence;

        // SAFETY: this frame's fence was waited on, so the GPU no longer reads its vertices.
        unsafe { self.vertices.write(current, &ui.vertices) };
        // SAFETY: this frame's fence was waited on, so the GPU no longer reads its uniform slot.
        unsafe {
            self.gui.write(
                current,
                &GuiFrame {
                    gui_size: ui_size,
                    screen_size: (width as f32, height as f32),
                    game_time: gui_uniforms::game_time(),
                },
            )
        };

        let cmd = self.frames.command_buffers[current];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            self.device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe {
            self.device
                .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
        }?;
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
            .framebuffer(self.swapchain.framebuffers[image_index as usize])
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
                    vertices: self.vertices.buffer(current),
                    indices: self.vertices.indices,
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
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { self.device.end_command_buffer(cmd) }?;

        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        unsafe { self.device.reset_fences(&[fence]) }?;
        let wait_semaphores = [self.frames.image_available[current]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.frames.render_finished[image_index as usize]];
        let command_buffers = [cmd];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        unsafe { self.device.queue_submit(self.queue, &[submit], fence) }?;

        let swapchains = [self.swapchain.handle];
        let image_indices = [image_index];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
        let presented = unsafe { self.swapchain.loader.queue_present(self.queue, &present) };
        let recreate = vk_util::recreate_after_present(presented, self.swapchain.extent, || {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { vk_util::surface_extent(&self.surface_loader, self.physical, self.surface) }
        })?;
        if recreate {
            // SAFETY: the queue, swapchain and sync objects are this device's, used from one thread.
            unsafe { self.device.device_wait_idle() }?;
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { self.create_swapchain(width, height) }?;
        }
        self.frames.current = (current + 1) % FRAMES_IN_FLIGHT;
        Ok(())
    }
}

/// The command buffers, semaphores and fences of each frame in flight.
unsafe fn create_frames(device: &ash::Device, queue_family: u32) -> anyhow::Result<Frames> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let command_pool = unsafe {
        device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )
    }?;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let command_buffers = unsafe {
        device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(FRAMES_IN_FLIGHT as u32),
        )
    }?;
    let mut image_available = Vec::new();
    let mut in_flight = Vec::new();
    for _ in 0..FRAMES_IN_FLIGHT {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        image_available
            .push(unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None) }?);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        in_flight.push(unsafe {
            device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )
        }?);
    }
    Ok(Frames {
        command_pool,
        command_buffers,
        image_available,
        render_finished: Vec::new(),
        in_flight,
        images_in_flight: Vec::new(),
        current: 0,
    })
}

impl Drop for Renderer2d {
    fn drop(&mut self) {
        // SAFETY: the device is waited idle first, and every handle freed is this renderer's own.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.vertices.destroy(&self.device);
            for texture in &self.textures {
                self.device.destroy_image_view(texture.view, None);
                self.device.destroy_image(texture.image, None);
                suballoc::release(&self.device, texture.memory);
            }
            for staging in &mut self.staging {
                staging.destroy(&self.device);
            }
            for &semaphore in &self.frames.image_available {
                self.device.destroy_semaphore(semaphore, None);
            }
            for &fence in &self.frames.in_flight {
                self.device.destroy_fence(fence, None);
            }
            self.destroy_swapchain_resources();
            if self.swapchain.handle != vk::SwapchainKHR::null() {
                self.swapchain
                    .loader
                    .destroy_swapchain(self.swapchain.handle, None);
            }
            self.device
                .destroy_command_pool(self.frames.command_pool, None);
            self.device.destroy_sampler(self.sampler, None);
            self.pipelines.destroy(&self.device);
            self.gui.destroy(&self.device);
            self.device
                .destroy_descriptor_set_layout(self.texture_layout, None);
            self.pools.destroy(&self.device);
            self.device.destroy_render_pass(self.render_pass, None);
            suballoc::forget(&self.device);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((debug, messenger)) = self.debug.take() {
                debug.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
