//! Bringing Vulkan up, one step each: instance, device, render passes and
//! pipelines, then the long-lived pools and buffers.

mod pipelines;

use melon_base::gfx::{device, features, instance, residency};
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
        // SAFETY: the renderer makes every handle itself, so the module's rules start out true.
        unsafe { Self::new_inner(window, enable_validation) }
    }

    pub(super) unsafe fn new_inner(
        window: &winit::window::Window,
        enable_validation: bool,
    ) -> anyhow::Result<Self> {
        residency::warm_up();
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let entry = unsafe { melon_base::gfx::loader::load() }?;
        let display = window.display_handle()?.as_raw();
        let instance::Instance {
            instance,
            debug,
            labels,
            api,
            // SAFETY: `device` is live, and the infos name only its own live handles.
        } = unsafe { instance::create(&entry, display, enable_validation) }?;
        let (surface_loader, surface) =
            // SAFETY: `instance` is live, and the physical device and surface are its own.
            unsafe { instance::create_surface(&entry, &instance, display, window.window_handle()?.as_raw()) }?;

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let gpu = unsafe { device::pick(&instance, api, &surface_loader, surface) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let features = unsafe { features::probe(&instance, gpu.physical, api, features::GAME) };
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
        let pipeline_cache = super::pipeline_cache::PipelineCache::open(&layer);
        let device = layer.raw().clone();
        let queue = layer.queue(gpu.queue_family, 0)?;
        let ring = majestix_vk::Frames::new(&layer, &queue, MAX_FRAMES_IN_FLIGHT as u32)?;
        let mem_props = gpu.mem_props;
        let residency = residency::Residency::new(&features, gpu.info.gpu);

        // SAFETY: `device` is live, and the infos name only its own live handles.
        let depth_format = unsafe { find_depth_format(&instance, gpu.physical) }?;
        let surface_format = choose_surface_format(&surface_loader, gpu.physical, surface)?.format;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let passes =
            unsafe { pipelines::create_render_passes(&device, surface_format, depth_format) }?;
        let post = super::post::gpu::Post::new(&layer, &queue, surface_format)?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pictures =
            unsafe { super::picture::Pictures::new(&device, surface_format, depth_format) }?;

        let globals = super::globals::GlobalBuffers::new(&layer, &ring)?;
        // The glint is white until a world's resources bind theirs.
        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let gui_uniforms = super::gui_uniforms::GuiUniforms::new(&layer, &queue, &ring, &white)?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let layouts = unsafe {
            pipelines::create_set_layouts(&device, globals.layout(), gui_uniforms.layout())
        }?;
        let indirect = if features.multi_draw() {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            Some(unsafe {
                super::indirect::DrawBuffers::new(
                    &device,
                    MAX_FRAMES_IN_FLIGHT,
                    layouts.world.draws,
                )
            }?)
        } else {
            None
        };
        // Level 0 of its pyramid is read from the depth buffer.
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let depth_readable =
            unsafe { instance.get_physical_device_format_properties(gpu.physical, depth_format) }
                .optimal_tiling_features
                .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE);
        let draw_count = features.draw_count();
        let occlusion = if let Some(draw_count) = draw_count
            && indirect.is_some()
            && depth_readable
            && super::occlusion::wanted()
        {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            Some(unsafe {
                super::occlusion::Occlusion::new(
                    &device,
                    &mem_props,
                    MAX_FRAMES_IN_FLIGHT,
                    Some(draw_count.load(&instance, &device)),
                )
            }?)
        } else {
            None
        };
        let expand = if indirect.is_some() && super::expand::wanted() {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            Some(unsafe { super::expand::Expand::new(&device, MAX_FRAMES_IN_FLIGHT) }?)
        } else {
            None
        };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pipelines = unsafe {
            pipelines::create(
                &device,
                passes.clear,
                post.color_pass,
                &layouts,
                features.multi_draw(),
            )
        }?;

        let descriptor_pools =
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { super::descriptor_pools::DescriptorPools::new(&device, MAX_TEXTURES) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let samplers = unsafe { resources::create_samplers(&device) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let blur = unsafe {
            blur::Blur::new(
                &layer,
                surface_format,
                passes.clear,
                layouts.texture,
                &descriptor_pools,
                samplers.linear,
                pipeline_cache
                    .as_ref()
                    .map(super::pipeline_cache::PipelineCache::handle),
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let frames = unsafe { resources::create_frames(&device, gpu.queue_family) }?;
        let ui_vertices = super::UiVertices::new(&layer, &queue, &ring)?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sky_cube = unsafe { resources::create_sky_cube(&device, &mem_props) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let meshes = unsafe {
            mesh_arena::MeshArena::new::<melon_world::mesh::WorldVertex>(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                mesh_arena::Kind::Vertices,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let quad_meshes = unsafe {
            mesh_arena::MeshArena::new::<melon_world::mesh::TerrainQuad>(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                mesh_arena::Kind::Quads,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let particle_meshes = unsafe {
            mesh_arena::MeshArena::new::<melon_world::particle::mesh::ParticleQuad>(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                mesh_arena::Kind::Instances,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let line_meshes = unsafe {
            mesh_arena::MeshArena::new::<melon_world::mesh::LineSegment>(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                mesh_arena::Kind::Instances,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let index_lists = unsafe {
            mesh_arena::MeshArena::new::<u32>(
                &device,
                &mem_props,
                MAX_FRAMES_IN_FLIGHT,
                mesh_arena::Kind::Indices,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let quad_sets = unsafe { super::quad_table::QuadSets::new(&device) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let gpu_timer = unsafe {
            super::gpu_timer::GpuTimer::new(
                &device,
                &gpu.limits,
                gpu.timestamp_bits,
                MAX_FRAMES_IN_FLIGHT,
                labels.then(|| ash::ext::debug_utils::Device::new(&instance, &device)),
                features.statistics(),
            )
        };
        let size = window.inner_size();
        let window_extent = vk::Extent2D {
            width: size.width.max(1),
            height: size.height.max(1),
        };
        // What the presenter will grant, known before its first swapchain:
        // the post chains are loaded before the first frame.
        // SAFETY: `instance` is live, and the physical device and surface are its own.
        let can_copy = unsafe {
            surface_loader.get_physical_device_surface_capabilities(gpu.physical, surface)
        }?
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_SRC);
        let swapchain = resources::swapchain(window_extent, surface_format, depth_format, can_copy);
        // SAFETY: `surface` is the instance's, outlives the presenter (dropped
        // first in `Drop`), and `device_pick` chose a family that presents to it.
        let presenter = unsafe {
            majestix_vk::Presenter::new(&layer, &entry, surface, &ring, swapchain.present_config())
        }?;

        let mut renderer = Self {
            _entry: entry,
            instance,
            debug,
            surface_loader,
            surface,
            physical_device: gpu.physical,
            mem_props,
            residency,
            device,
            layer: std::mem::ManuallyDrop::new(super::Layer {
                queue,
                device: layer,
            }),
            ring: Some(ring),
            presenter: Some(presenter),
            passes,
            post,
            blur,
            pipeline_cache,
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
            particle_meshes,
            line_meshes,
            index_lists,
            quad_table: None,
            gpu_timer,
            indirect,
            occlusion,
            expand,
            quad_sets,
            globals,
            recording: crate::record::Recording::new(),
            max_texture_side: gpu.limits.max_image_dimension2_d,
            tracked_meshes: None,
            textures: resources::textures(descriptor_pools, samplers),
        };

        renderer.swapchain.depth_sampled = renderer.occlusion.is_some();

        renderer.textures.white = renderer.load_texture(&white)?;
        melon_base::info!(
            "renderer: terrain draws {}{}",
            renderer.terrain_path().name(),
            if renderer.expand.is_some() {
                ", expanded on the GPU"
            } else {
                ""
            }
        );

        Ok(renderer)
    }
}
