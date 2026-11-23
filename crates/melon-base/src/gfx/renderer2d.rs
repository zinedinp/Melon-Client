//! A renderer for 2D interfaces alone: one pass that draws a [`DrawList`].

use super::descriptor_pools::DescriptorPools;
use super::gui_uniforms::{self, GuiFrame, GuiUniforms};
use super::pipeline::ui::{self, UiPipelines};
use super::quad::{BlendMode, DrawList, Vertex};
use super::staging::Staging;
use super::texture::{Texture, Uploader};
use super::texture_update::{self, PendingInit};
use super::{
    PassStart, TextureId, choose_surface_format, create_depth_image, create_render_pass,
    create_sampler, device, find_depth_format, instance, pipeline, vk_util,
};
use ash::vk;
use image::RgbaImage;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

const FRAMES_IN_FLIGHT: usize = 2;
/// Descriptor sets in the first pool; a full pool is followed by a bigger one.
const TEXTURES: u32 = 256;
/// Vertices a frame can draw; quads past them are left out.
const VERTEX_CAPACITY: usize = 1 << 16;

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
    vertices: Vertices,
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

/// A mapped vertex buffer per frame in flight, and the quads' indices.
struct Vertices {
    buffers: Vec<(vk::Buffer, vk::DeviceMemory, *mut u8)>,
    indices: (vk::Buffer, vk::DeviceMemory),
}

impl Renderer2d {
    pub fn new(window: &winit::window::Window, enable_validation: bool) -> anyhow::Result<Self> {
        unsafe { Self::new_inner(window, enable_validation) }
    }

    unsafe fn new_inner(
        window: &winit::window::Window,
        enable_validation: bool,
    ) -> anyhow::Result<Self> {
        let entry = ash::Entry::load()?;
        let display = window.display_handle()?.as_raw();
        let instance::Instance {
            instance,
            debug,
            api,
            ..
        } = instance::create(&entry, display, enable_validation)?;
        let (surface_loader, surface) =
            instance::create_surface(&entry, &instance, display, window.window_handle()?.as_raw())?;
        let gpu = device::pick(&instance, &surface_loader, surface)?;
        let features = device::probe(
            &instance,
            gpu.physical,
            api,
            &[
                numerobis::vulkan::Op::RobustBufferAccess,
                numerobis::vulkan::Op::PageableDeviceLocalMemory,
            ],
        );
        let (device, queue) = device::create(&instance, &gpu, &features)?;
        let mem_props = gpu.mem_props;

        let depth_format = find_depth_format(&instance, gpu.physical)?;
        let surface_format = choose_surface_format(&surface_loader, gpu.physical, surface)?.format;
        let render_pass =
            create_render_pass(&device, surface_format, depth_format, PassStart::Clear)?;
        let texture_layout = pipeline::create_descriptor_set_layout(&device)?;
        let gui_layout = gui_uniforms::create_layout(&device)?;
        let gui = GuiUniforms::new(&device, &mem_props, gui_layout, FRAMES_IN_FLIGHT)?;
        let pipelines = UiPipelines {
            built_in: ui::create_all(
                &device,
                render_pass,
                texture_layout,
                gui_layout,
                ui::BUILT_IN,
            )?,
            pack: [None, None, None],
            glint: ui::create(
                &device,
                render_pass,
                texture_layout,
                gui_layout,
                BlendMode::Alpha,
                ui::GLINT,
            )?,
        };
        let pools = DescriptorPools::new(&device, TEXTURES)?;
        let sampler = create_sampler(
            &device,
            vk::Filter::NEAREST,
            false,
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
        )?;
        let frames = create_frames(&device, gpu.queue_family)?;
        let vertices = create_vertices(&device, &mem_props)?;
        let loader_version = entry
            .try_enumerate_instance_version()
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
        renderer.create_swapchain(size.width.max(1), size.height.max(1))?;
        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        renderer.white = renderer.load_texture(&white)?;
        let view = renderer.textures[renderer.white.0].view;
        renderer
            .gui
            .bind_glint(&renderer.device, view, renderer.sampler);
        Ok(renderer)
    }

    /// The GPU it draws with.
    pub fn device_info(&self) -> &super::DeviceInfo {
        &self.info
    }

    /// The Vulkan loader's version, e.g. "1.4.304".
    pub fn loader_version(&self) -> String {
        let v = self.loader_version;
        format!("{}.{}.{}", vk::api_version_major(v), vk::api_version_minor(v), vk::api_version_patch(v))
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
        unsafe { self.render_inner(ui, ui_size, clear_color) }
    }

    unsafe fn create_swapchain(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
        let capabilities = self
            .surface_loader
            .get_physical_device_surface_capabilities(self.physical, self.surface)?;
        let format = choose_surface_format(&self.surface_loader, self.physical, self.surface)?;
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
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
        let old = self.swapchain.handle;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::FIFO)
            .clipped(true)
            .old_swapchain(old);
        let handle = self.swapchain.loader.create_swapchain(&info, None)?;
        self.destroy_swapchain_resources();
        if old != vk::SwapchainKHR::null() {
            self.swapchain.loader.destroy_swapchain(old, None);
        }
        self.swapchain.handle = handle;
        self.swapchain.extent = extent;

        let images = self.swapchain.loader.get_swapchain_images(handle)?;
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
            let view = self.device.create_image_view(&view_info, None)?;
            self.swapchain.views.push(view);
        }
        self.swapchain.depth = create_depth_image(
            &self.device,
            &self.mem_props,
            self.depth_format,
            extent,
            false,
        )?;
        for &view in &self.swapchain.views {
            let attachments = [view, self.swapchain.depth.2];
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(self.render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            let framebuffer = self.device.create_framebuffer(&info, None)?;
            self.swapchain.framebuffers.push(framebuffer);
        }
        for _ in 0..images.len() {
            let semaphore = self
                .device
                .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
            self.frames.render_finished.push(semaphore);
        }
        self.frames.images_in_flight = vec![vk::Fence::null(); images.len()];
        Ok(())
    }

    unsafe fn destroy_swapchain_resources(&mut self) {
        for framebuffer in self.swapchain.framebuffers.drain(..) {
            self.device.destroy_framebuffer(framebuffer, None);
        }
        for view in self.swapchain.views.drain(..) {
            self.device.destroy_image_view(view, None);
        }
        let (image, memory, view) = std::mem::take(&mut self.swapchain.depth);
        if image != vk::Image::null() {
            self.device.destroy_image_view(view, None);
            self.device.destroy_image(image, None);
            self.device.free_memory(memory, None);
        }
        for semaphore in self.frames.render_finished.drain(..) {
            self.device.destroy_semaphore(semaphore, None);
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
        self.device.wait_for_fences(&[fence], true, u64::MAX)?;

        // Every allocation before an image is acquired: a failure after that would leave the
        // image acquired with nothing to present it.
        texture_update::prepare(
            &mut self.staging[current],
            &self.device,
            &self.mem_props,
            &self.pending,
            &[],
        )?;
        let acquired = self.swapchain.loader.acquire_next_image(
            self.swapchain.handle,
            u64::MAX,
            self.frames.image_available[current],
            vk::Fence::null(),
        );
        let (width, height) = (self.swapchain.extent.width, self.swapchain.extent.height);
        let image_index = match acquired {
            Ok((index, _suboptimal)) => index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.device.device_wait_idle()?;
                return self.create_swapchain(width, height);
            }
            Err(e) => return Err(e.into()),
        };
        let image_fence = self.frames.images_in_flight[image_index as usize];
        if image_fence != vk::Fence::null() {
            self.device
                .wait_for_fences(&[image_fence], true, u64::MAX)?;
        }
        self.frames.images_in_flight[image_index as usize] = fence;

        std::ptr::copy_nonoverlapping(
            ui.vertices.as_ptr().cast::<u8>(),
            self.vertices.buffers[current].2,
            vertex_count * std::mem::size_of::<Vertex>(),
        );
        self.gui.write(
            current,
            &GuiFrame {
                gui_size: ui_size,
                screen_size: (width as f32, height as f32),
                game_time: gui_uniforms::game_time(),
            },
        );

        let cmd = self.frames.command_buffers[current];
        self.device
            .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        self.device
            .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        let inits = std::mem::take(&mut self.pending);
        texture_update::record(
            &mut self.staging[current],
            &self.device,
            &self.mem_props,
            cmd,
            &inits,
            &[],
        )?;

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
        self.device
            .cmd_begin_render_pass(cmd, &pass_info, vk::SubpassContents::INLINE);
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        self.device.cmd_set_viewport(cmd, 0, &[viewport]);
        let textures = &self.textures;
        ui::record(
            &ui::Target {
                device: &self.device,
                cmd,
                pipelines: &self.pipelines,
                gui_set: self.gui.set(current),
                vertices: self.vertices.buffers[current].0,
                indices: self.vertices.indices.0,
                extent: self.swapchain.extent,
            },
            ui_size,
            calls,
            full,
            |texture| textures[texture.0].descriptor_set,
        );
        self.device.cmd_end_render_pass(cmd);
        self.device.end_command_buffer(cmd)?;

        self.device.reset_fences(&[fence])?;
        let wait_semaphores = [self.frames.image_available[current]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let signal_semaphores = [self.frames.render_finished[image_index as usize]];
        let command_buffers = [cmd];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores);
        self.device.queue_submit(self.queue, &[submit], fence)?;

        let swapchains = [self.swapchain.handle];
        let image_indices = [image_index];
        let present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        match self.swapchain.loader.queue_present(self.queue, &present) {
            Ok(false) => {}
            Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.device.device_wait_idle()?;
                self.create_swapchain(width, height)?;
            }
            Err(e) => return Err(e.into()),
        }
        self.frames.current = (current + 1) % FRAMES_IN_FLIGHT;
        Ok(())
    }
}

/// The command buffers, semaphores and fences of each frame in flight.
unsafe fn create_frames(device: &ash::Device, queue_family: u32) -> anyhow::Result<Frames> {
    let command_pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let command_buffers = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(FRAMES_IN_FLIGHT as u32),
    )?;
    let mut image_available = Vec::new();
    let mut in_flight = Vec::new();
    for _ in 0..FRAMES_IN_FLIGHT {
        image_available.push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
        in_flight.push(device.create_fence(
            &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
            None,
        )?);
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

unsafe fn create_vertices(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<Vertices> {
    let indices: Vec<u32> = (0..(VERTEX_CAPACITY / 4) as u32)
        .flat_map(|quad| [0, 1, 2, 2, 3, 0].map(|corner| quad * 4 + corner))
        .collect();
    let index_size = std::mem::size_of_val(indices.as_slice()) as vk::DeviceSize;
    let (index_buffer, index_memory) = vk_util::create_buffer_in(
        device,
        mem_props,
        index_size,
        vk::BufferUsageFlags::INDEX_BUFFER,
        &vk_util::MAPPED_FOR_GPU,
    )?;
    let mapped = device.map_memory(index_memory, 0, index_size, vk::MemoryMapFlags::empty())?;
    std::ptr::copy_nonoverlapping(indices.as_ptr(), mapped.cast::<u32>(), indices.len());
    device.unmap_memory(index_memory);

    let size = (VERTEX_CAPACITY * std::mem::size_of::<Vertex>()) as vk::DeviceSize;
    let mut buffers = Vec::new();
    for _ in 0..FRAMES_IN_FLIGHT {
        let (buffer, memory) = vk_util::create_buffer_in(
            device,
            mem_props,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            &vk_util::MAPPED_FOR_GPU,
        )?;
        let mapped = device
            .map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?
            .cast::<u8>();
        buffers.push((buffer, memory, mapped));
    }
    Ok(Vertices {
        buffers,
        indices: (index_buffer, index_memory),
    })
}

impl Drop for Renderer2d {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
            for &(buffer, memory, _) in &self.vertices.buffers {
                self.device.destroy_buffer(buffer, None);
                self.device.unmap_memory(memory);
                self.device.free_memory(memory, None);
            }
            self.device.destroy_buffer(self.vertices.indices.0, None);
            self.device.free_memory(self.vertices.indices.1, None);
            for texture in &self.textures {
                self.device.destroy_image_view(texture.view, None);
                self.device.destroy_image(texture.image, None);
                self.device.free_memory(texture.memory, None);
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
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((debug, messenger)) = self.debug.take() {
                debug.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
