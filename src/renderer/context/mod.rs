//! Bringing Vulkan up, one step each: instance, device, render passes and
//! pipelines, then the long-lived pools and buffers.

mod pipelines;

use melon_base::gfx::{device, instance};
pub(super) use pipelines::RenderPasses;
mod resources;

use super::{
    MAX_FRAMES_IN_FLIGHT, MAX_TEXTURES, Renderer, blur, choose_surface_format, find_depth_format,
    mesh_arena,
};
use ash::vk;
use image::RgbaImage;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

impl Renderer {
    pub fn new(window: &winit::window::Window, enable_validation: bool) -> anyhow::Result<Self> {
        unsafe { Self::new_inner(window, enable_validation) }
    }

    pub(super) unsafe fn new_inner(
        window: &winit::window::Window,
        enable_validation: bool,
    ) -> anyhow::Result<Self> {
        let entry = ash::Entry::load()?;
        let display = window.display_handle()?.as_raw();
        let instance::Instance {
            instance,
            debug,
            labels,
        } = instance::create(&entry, display, enable_validation)?;
        let (surface_loader, surface) =
            instance::create_surface(&entry, &instance, display, window.window_handle()?.as_raw())?;

        let gpu = device::pick(&instance, &surface_loader, surface)?;
        let features = device::probe(&instance, gpu.physical);
        let (device, queue) = device::create(&instance, &gpu, &features)?;
        let mem_props = gpu.mem_props;

        let depth_format = find_depth_format(&instance, gpu.physical)?;
        let surface_format = choose_surface_format(&surface_loader, gpu.physical, surface)?.format;
        let passes = pipelines::create_render_passes(&device, surface_format, depth_format)?;
        let post = super::post::gpu::Post::new(&device, surface_format)?;
        let pictures = super::picture::Pictures::new(&device, surface_format, depth_format)?;

        let layouts = pipelines::create_set_layouts(&device)?;
        let gui_uniforms = super::gui_uniforms::GuiUniforms::new(
            &device,
            &mem_props,
            layouts.gui,
            MAX_FRAMES_IN_FLIGHT,
        )?;
        let indirect = if features.multi_draw {
            Some(super::indirect::DrawBuffers::new(
                &device,
                MAX_FRAMES_IN_FLIGHT,
                layouts.world.draws,
            )?)
        } else {
            None
        };
        // Level 0 of its pyramid is read from the depth buffer.
        let depth_readable = instance
            .get_physical_device_format_properties(gpu.physical, depth_format)
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE);
        let occlusion = if indirect.is_some()
            && features.draw_count
            && depth_readable
            && super::occlusion::wanted()
        {
            Some(super::occlusion::Occlusion::new(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                Some(ash::khr::draw_indirect_count::Device::new(
                    &instance, &device,
                )),
            )?)
        } else {
            None
        };
        let globals = super::globals::GlobalBuffers::new(
            &device,
            &mem_props,
            layouts.world.globals,
            MAX_FRAMES_IN_FLIGHT,
        )?;
        let pipelines = pipelines::create(
            &device,
            passes.clear,
            post.color_pass,
            &layouts,
            features.multi_draw,
        )?;

        let descriptor_pools =
            super::descriptor_pools::DescriptorPools::new(&device, MAX_TEXTURES)?;
        let samplers = resources::create_samplers(&device)?;
        let blur = blur::Blur::new(
            &instance,
            gpu.physical,
            &device,
            surface_format,
            passes.clear,
            layouts.texture,
            &descriptor_pools,
            samplers.linear,
        )?;
        let frames = resources::create_frames(&device, gpu.queue_family)?;
        let ui_vertices = resources::create_ui_vertices(&device, &mem_props)?;
        let sky_cube = resources::create_sky_cube(&device, &mem_props)?;
        let meshes = mesh_arena::MeshArena::new::<super::WorldVertex>(
            &device,
            &mem_props,
            MAX_FRAMES_IN_FLIGHT,
            mesh_arena::Kind::Vertices,
        )?;
        let quad_meshes = mesh_arena::MeshArena::new::<super::TerrainQuad>(
            &device,
            &mem_props,
            MAX_FRAMES_IN_FLIGHT,
            mesh_arena::Kind::Quads,
        )?;
        let quad_sets = super::quad_table::QuadSets::new(&device)?;
        let gpu_timer = super::gpu_timer::GpuTimer::new(
            &device,
            &gpu.limits,
            gpu.timestamp_bits,
            MAX_FRAMES_IN_FLIGHT,
            labels.then(|| ash::ext::debug_utils::Device::new(&instance, &device)),
            features.statistics,
        );
        let swapchain = resources::swapchain(
            ash::khr::swapchain::Device::new(&instance, &device),
            features.latest_ready.is_some(),
            surface_format,
            depth_format,
        );

        let mut renderer = Self {
            _entry: entry,
            instance,
            debug,
            surface_loader,
            surface,
            physical_device: gpu.physical,
            mem_props,
            device,
            queue,
            passes,
            post,
            blur,
            device_info: gpu.info,
            swapchain,
            pipelines,
            frames,
            ui_vertices,
            gui_uniforms,
            pack_outline: None,
            capture: Default::default(),
            pictures,
            sky_cube,
            meshes,
            quad_meshes,
            quad_table: None,
            gpu_timer,
            indirect,
            occlusion,
            quad_sets,
            globals,
            recording: crate::renderer::record::Recording::new(),
            max_texture_side: gpu.limits.max_image_dimension2_d,
            tracked_meshes: None,
            textures: resources::textures(descriptor_pools, samplers),
        };

        renderer.swapchain.depth_sampled = renderer.occlusion.is_some();
        let size = window.inner_size();
        renderer.create_swapchain(size.width.max(1), size.height.max(1))?;

        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        renderer.textures.white = renderer.load_texture(&white)?;
        renderer.bind_gui_glint(renderer.textures.white);

        Ok(renderer)
    }
}
