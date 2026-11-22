//! Bringing Vulkan up: instance, device, queues, the long-lived pools,

use super::vk_util;
use super::{
    MAX_FRAMES_IN_FLIGHT, MAX_TEXTURES, PassStart, Renderer, TextureId, VERTEX_CAPACITY, blur,
    choose_surface_format, create_render_pass, create_sampler, debug_callback, find_depth_format,
    mesh_arena, pipeline, quad, read_device_info,
};
use ash::vk;
use image::RgbaImage;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::CStr;

impl Renderer {
    pub fn new(window: &winit::window::Window, enable_validation: bool) -> anyhow::Result<Self> {
        unsafe { Self::new_inner(window, enable_validation) }
    }

    pub(super) unsafe fn new_inner(
        window: &winit::window::Window,
        enable_validation: bool,
    ) -> anyhow::Result<Self> {
        let entry = ash::Entry::load()?;

        let display_handle = window.display_handle()?.as_raw();
        let window_handle = window.window_handle()?.as_raw();

        let mut extension_names =
            ash_window::enumerate_required_extensions(display_handle)?.to_vec();
        let mut layer_names: Vec<*const i8> = Vec::new();
        let validation_layer = c"VK_LAYER_KHRONOS_validation";
        let validation_available = entry
            .enumerate_instance_layer_properties()?
            .iter()
            .any(|l| CStr::from_ptr(l.layer_name.as_ptr()) == validation_layer);
        let use_validation = enable_validation && validation_available;
        if enable_validation && !validation_available {
            eprintln!(
                "vulkan: VK_LAYER_KHRONOS_validation nicht gefunden, fahre ohne Validation fort"
            );
        }
        // Debug utils also name the passes for GPU profilers (Nsight Graphics,
        // RenderDoc), so they are on whenever the driver offers them.
        let debug_utils = entry
            .enumerate_instance_extension_properties(None)?
            .iter()
            .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == ash::ext::debug_utils::NAME);
        if use_validation {
            layer_names.push(validation_layer.as_ptr());
        }
        if debug_utils || use_validation {
            extension_names.push(ash::ext::debug_utils::NAME.as_ptr());
        }

        let app_name = c"Melon";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_1);
        let instance_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layer_names)
            .enabled_extension_names(&extension_names);
        let instance = entry.create_instance(&instance_info, None)?;

        let debug = if use_validation {
            let debug_instance = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_callback));
            let messenger = debug_instance.create_debug_utils_messenger(&messenger_info, None)?;
            Some((debug_instance, messenger))
        } else {
            None
        };

        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let surface =
            ash_window::create_surface(&entry, &instance, display_handle, window_handle, None)?;

        let (physical_device, queue_family) =
            super::device_pick::pick(&instance, &surface_loader, surface)?;

        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let limits = instance
            .get_physical_device_properties(physical_device)
            .limits;
        let timestamp_bits = instance
            .get_physical_device_queue_family_properties(physical_device)
            .get(queue_family as usize)
            .map_or(0, |family| family.timestamp_valid_bits);
        let gpu_info = read_device_info(&instance, physical_device);

        let queue_priorities = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&queue_priorities);
        let queue_infos = [queue_info];
        let mut device_extensions = vec![ash::khr::swapchain::NAME.as_ptr()];
        let offered = instance
            .enumerate_device_extension_properties(physical_device)
            .unwrap_or_default();
        let latest_ready_extension = [
            c"VK_KHR_present_mode_fifo_latest_ready",
            c"VK_EXT_present_mode_fifo_latest_ready",
        ]
        .into_iter()
        .find(|name| {
            offered
                .iter()
                .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == *name)
        });
        let supported = instance.get_physical_device_features(physical_device);
        let mut supported_11 = vk::PhysicalDeviceVulkan11Features::default();
        let mut supported_latest = super::swapchain::FifoLatestReadyFeatures::new(false);
        if latest_ready_extension.is_some() {
            supported_11.p_next = std::ptr::from_mut(&mut supported_latest).cast();
        }
        let mut supported_2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut supported_11);
        instance.get_physical_device_features2(physical_device, &mut supported_2);
        let latest_ready = latest_ready_extension.is_some()
            && supported_latest.present_mode_fifo_latest_ready == vk::TRUE;
        if let (true, Some(name)) = (latest_ready, latest_ready_extension) {
            device_extensions.push(name.as_ptr());
        }
        // `VK_EXT_pageable_device_local_memory`: when another program fills
        // the VRAM, the driver moves allocations to system memory instead of
        // failing them (as DXVK and vkd3d ask too), so the game keeps running
        // slower rather than losing its swapchain and textures.
        let offers = |name: &CStr| {
            offered
                .iter()
                .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name)
        };
        let pageable = offers(ash::ext::pageable_device_local_memory::NAME)
            && offers(ash::ext::memory_priority::NAME)
            && {
                let mut query = vk::PhysicalDevicePageableDeviceLocalMemoryFeaturesEXT::default();
                let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut query);
                instance.get_physical_device_features2(physical_device, &mut features);
                query.pageable_device_local_memory == vk::TRUE
            };
        if pageable {
            device_extensions.push(ash::ext::memory_priority::NAME.as_ptr());
            device_extensions.push(ash::ext::pageable_device_local_memory::NAME.as_ptr());
        }
        let multi_draw = std::env::var_os("MELON_NO_MULTIDRAW").is_none()
            && supported.multi_draw_indirect == vk::TRUE
            && supported.draw_indirect_first_instance == vk::TRUE
            && supported_11.shader_draw_parameters == vk::TRUE;
        // Pipeline statistics: how much each pass shades, for the benchmark.
        let statistics = supported.pipeline_statistics_query == vk::TRUE;
        // `robustBufferAccess`: a read past a buffer's end gives zeros instead
        // of a GPU page fault that loses the device (Xid 13 on NVIDIA). A
        // safety net for broken data; `MELON_NO_ROBUST_BUFFERS` drops it to
        // measure what it costs.
        let robust = std::env::var_os("MELON_NO_ROBUST_BUFFERS").is_none()
            && supported.robust_buffer_access == vk::TRUE;
        let features = vk::PhysicalDeviceFeatures::default()
            .robust_buffer_access(robust)
            .multi_draw_indirect(multi_draw)
            .draw_indirect_first_instance(multi_draw)
            .pipeline_statistics_query(statistics);
        let mut features_11 =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(multi_draw);
        let mut enabled_latest = super::swapchain::FifoLatestReadyFeatures::new(true);
        if latest_ready {
            features_11.p_next = std::ptr::from_mut(&mut enabled_latest).cast();
        }
        let mut enabled_pageable =
            vk::PhysicalDevicePageableDeviceLocalMemoryFeaturesEXT::default()
                .pageable_device_local_memory(true);
        let mut device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_features(&features)
            .push_next(&mut features_11)
            .enabled_extension_names(&device_extensions);
        if pageable {
            device_info = device_info.push_next(&mut enabled_pageable);
        }
        let device = instance.create_device(physical_device, &device_info, None)?;
        let queue = device.get_device_queue(queue_family, 0);

        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);

        let depth_format = find_depth_format(&instance, physical_device)?;
        let surface_format =
            choose_surface_format(&surface_loader, physical_device, surface)?.format;
        let render_pass =
            create_render_pass(&device, surface_format, depth_format, PassStart::Clear)?;
        let overwrite_pass =
            create_render_pass(&device, surface_format, depth_format, PassStart::Overwrite)?;
        let split_pass =
            create_render_pass(&device, surface_format, depth_format, PassStart::SceneSplit)?;
        let resume_pass =
            create_render_pass(&device, surface_format, depth_format, PassStart::Resume)?;
        let post = super::post::gpu::Post::new(&device, surface_format)?;
        let pictures = super::picture::Pictures::new(&device, surface_format, depth_format)?;

        let descriptor_set_layout = pipeline::create_descriptor_set_layout(&device)?;
        let sky_pass = pipeline::sky::create(&device, render_pass, descriptor_set_layout)?;
        let gui_layout = super::gui_uniforms::create_layout(&device)?;
        let ui_passes = pipeline::ui::create_all(
            &device,
            render_pass,
            descriptor_set_layout,
            gui_layout,
            pipeline::ui::BUILT_IN,
        )?;
        let ui_glint_pass = pipeline::ui::create(
            &device,
            render_pass,
            descriptor_set_layout,
            gui_layout,
            crate::renderer::quad::BlendMode::Alpha,
            pipeline::ui::GLINT,
        )?;
        let gui_uniforms = super::gui_uniforms::GuiUniforms::new(
            &device,
            &mem_props,
            gui_layout,
            MAX_FRAMES_IN_FLIGHT,
        )?;
        let layouts = pipeline::world::Layouts {
            table: super::quad_table::create_layout(&device)?,
            draws: super::indirect::create_layout(&device)?,
            globals: super::globals::create_layout(&device)?,
        };
        let indirect = if multi_draw {
            Some(super::indirect::DrawBuffers::new(
                &device,
                MAX_FRAMES_IN_FLIGHT,
                layouts.draws,
            )?)
        } else {
            None
        };
        let globals = super::globals::GlobalBuffers::new(
            &device,
            &mem_props,
            layouts.globals,
            MAX_FRAMES_IN_FLIGHT,
        )?;
        let world = |mode, geometry| {
            pipeline::world::create(
                &device,
                render_pass,
                descriptor_set_layout,
                &layouts,
                mode,
                geometry,
            )
        };
        use pipeline::world::{Geometry, Mode};
        let solid_pass = world(Mode::Solid, Geometry::Vertices)?;
        let world_pass = world(Mode::Opaque, Geometry::Vertices)?;
        let translucent_pass = world(Mode::Translucent, Geometry::Vertices)?;
        let translucent_terrain_pass = world(Mode::TranslucentTerrain, Geometry::Vertices)?;
        let lightning_pass = world(Mode::Lightning, Geometry::Vertices)?;
        let energy_swirl_pass = world(Mode::EnergySwirl, Geometry::Vertices)?;
        let scrolled_pass = world(Mode::Scrolled, Geometry::Vertices)?;
        let text_pass = world(Mode::Text, Geometry::Vertices)?;
        let clouds_pass = world(Mode::Clouds, Geometry::Vertices)?;
        let weather_pass = world(Mode::Weather, Geometry::Vertices)?;
        let celestial_pass = world(Mode::Celestial, Geometry::Vertices)?;
        let sky_disc_pass = world(Mode::SkyDisc, Geometry::Vertices)?;
        let sky_glow_pass = world(Mode::SkyGlow, Geometry::Vertices)?;
        let crumbling_pass = world(Mode::Crumbling, Geometry::Vertices)?;
        let water_mask_pass = world(Mode::WaterMask, Geometry::Vertices)?;
        let outline_pass = pipeline::world::create(
            &device,
            post.color_pass,
            descriptor_set_layout,
            &layouts,
            Mode::Outline,
            Geometry::Vertices,
        )?;
        let glint_pass = world(Mode::Glint, Geometry::Vertices)?;
        let lines = [
            world(Mode::Lines, Geometry::Vertices)?,
            world(Mode::LinesDepthBias, Geometry::Vertices)?,
            world(Mode::LinesTranslucent, Geometry::Vertices)?,
            world(Mode::LinesNoDepthWrite, Geometry::Vertices)?,
        ];
        let terrain = [
            world(
                Mode::Solid,
                Geometry::Quads {
                    indirect: multi_draw,
                },
            )?,
            world(
                Mode::Opaque,
                Geometry::Quads {
                    indirect: multi_draw,
                },
            )?,
        ];

        let descriptor_pools =
            super::descriptor_pools::DescriptorPools::new(&device, MAX_TEXTURES)?;

        let clamp = vk::SamplerAddressMode::CLAMP_TO_EDGE;
        let nearest_sampler = create_sampler(&device, vk::Filter::NEAREST, false, clamp)?;
        let mipmap_sampler = create_sampler(&device, vk::Filter::NEAREST, true, clamp)?;
        let linear_sampler = create_sampler(&device, vk::Filter::LINEAR, false, clamp)?;
        let repeat_sampler = create_sampler(
            &device,
            vk::Filter::NEAREST,
            false,
            vk::SamplerAddressMode::REPEAT,
        )?;
        let linear_repeat_sampler = create_sampler(
            &device,
            vk::Filter::LINEAR,
            false,
            vk::SamplerAddressMode::REPEAT,
        )?;

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
                .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32),
        )?;

        let mut image_available = Vec::new();
        let mut in_flight_fences = Vec::new();
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            image_available
                .push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
            in_flight_fences.push(device.create_fence(
                &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                None,
            )?);
        }

        let mut ui_vertex_buffers = Vec::new();
        let mut ui_vertex_memories = Vec::new();
        let mut ui_vertex_mapped = Vec::new();
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let size = (VERTEX_CAPACITY * std::mem::size_of::<quad::Vertex>()) as vk::DeviceSize;
            let (buffer, memory) = vk_util::create_buffer_in(
                &device,
                &mem_props,
                size,
                vk::BufferUsageFlags::VERTEX_BUFFER,
                &vk_util::MAPPED_FOR_GPU,
            )?;
            let mapped =
                device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
            ui_vertex_buffers.push(buffer);
            ui_vertex_memories.push(memory);
            ui_vertex_mapped.push(mapped);
        }
        let ui_indices = create_quad_indices(&device, &mem_props)?;

        let cube = &pipeline::sky::CUBE_VERTICES;
        let cube_size = std::mem::size_of_val(cube) as vk::DeviceSize;
        let (sky_vertex_buffer, sky_vertex_memory) = vk_util::create_buffer_in(
            &device,
            &mem_props,
            cube_size,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            &vk_util::MAPPED_FOR_GPU,
        )?;
        let cube_mapped =
            device.map_memory(sky_vertex_memory, 0, cube_size, vk::MemoryMapFlags::empty())?
                as *mut u8;
        std::ptr::copy_nonoverlapping(cube.as_ptr() as *const u8, cube_mapped, cube_size as usize);
        device.unmap_memory(sky_vertex_memory);

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

        let pass_labels = (debug_utils || use_validation)
            .then(|| ash::ext::debug_utils::Device::new(&instance, &device));
        let gpu_timer = super::gpu_timer::GpuTimer::new(
            &device,
            &limits,
            timestamp_bits,
            MAX_FRAMES_IN_FLIGHT,
            pass_labels,
            statistics,
        );

        let blur = blur::Blur::new(
            &instance,
            physical_device,
            &device,
            surface_format,
            render_pass,
            descriptor_set_layout,
            &descriptor_pools,
            linear_sampler,
        )?;

        let mut renderer = Self {
            entry,
            instance,
            debug,
            surface_loader,
            surface,
            physical_device,
            mem_props,
            device,
            queue,
            render_pass,
            overwrite_pass,
            split_pass,
            resume_pass,
            post,
            blur,
            device_info: gpu_info,
            swapchain: super::Swapchain {
                loader: swapchain_loader,
                handle: vk::SwapchainKHR::null(),
                extent: vk::Extent2D::default(),
                vsync: true,
                requested: None,
                latest_ready,
                present_mode: vk::PresentModeKHR::FIFO,
                images: Vec::new(),
                views: Vec::new(),
                format: surface_format,
                can_copy: false,
                depth_format,
                depth_sampled: false,
                depth: super::DepthImage::default(),
                framebuffers: Vec::new(),
            },
            pipelines: super::Pipelines {
                layout: descriptor_set_layout,
                ui: pipeline::ui::UiPipelines {
                    built_in: ui_passes,
                    pack: [None, None, None],
                    glint: ui_glint_pass,
                },
                sky: sky_pass,
                solid: solid_pass,
                world: world_pass,
                translucent: translucent_pass,
                translucent_terrain: translucent_terrain_pass,
                lightning: lightning_pass,
                energy_swirl: energy_swirl_pass,
                scrolled: scrolled_pass,
                text: text_pass,
                clouds: clouds_pass,
                weather: weather_pass,
                celestial: celestial_pass,
                sky_disc: sky_disc_pass,
                sky_glow: sky_glow_pass,
                crumbling: crumbling_pass,
                water_mask: water_mask_pass,
                outline: outline_pass,
                glint: glint_pass,
                lines,
                layouts,
                terrain,
            },
            frames: super::Frames {
                command_pool,
                command_buffers,
                image_available,
                render_finished: Vec::new(),
                in_flight: in_flight_fences,
                images_in_flight: Vec::new(),
                current: 0,
                number: 0,
                submitted: [0; super::MAX_FRAMES_IN_FLIGHT],
                last_waits: super::FrameWaits::default(),
                applied_post_effects: Vec::new(),
            },
            ui_vertices: super::UiVertices {
                buffers: ui_vertex_buffers,
                memories: ui_vertex_memories,
                mapped: ui_vertex_mapped,
                indices: ui_indices,
            },
            gui_uniforms,
            pack_outline: None,
            capture: Default::default(),
            pictures,
            sky_cube: super::SkyCube {
                buffer: sky_vertex_buffer,
                memory: sky_vertex_memory,
            },
            meshes,
            quad_meshes,
            quad_table: None,
            bound_vertex_buffer: std::cell::Cell::new(vk::Buffer::null()),
            pushed: std::cell::Cell::new(None),
            pushed_offset: std::cell::Cell::new(None),
            gpu_timer,
            indirect,
            draw_cursor: std::cell::Cell::new(0),
            quad_sets,
            bound_quads: std::cell::Cell::new(vk::Buffer::null()),
            globals,
            pushed_view: std::cell::Cell::new(None),
            max_texture_side: limits.max_image_dimension2_d,
            tracked_meshes: None,
            textures: super::Textures {
                list: Vec::new(),
                cubemaps: Vec::new(),
                pending_inits: Vec::new(),
                pending_init_bytes: 0,
                pending_updates: Vec::new(),
                update_staging: (0..MAX_FRAMES_IN_FLIGHT)
                    .map(|_| super::staging::Staging::default())
                    .collect(),
                white: TextureId(0),
                descriptor_pools,
                nearest_sampler,
                mipmap_sampler,
                linear_sampler,
                repeat_sampler,
                linear_repeat_sampler,
                free: Vec::new(),
                reuse: Default::default(),
                tracked: None,
                retired: Vec::new(),
            },
        };

        let size = window.inner_size();
        renderer.create_swapchain(size.width.max(1), size.height.max(1))?;

        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        renderer.textures.white = renderer.load_texture(&white)?;
        renderer.bind_gui_glint(renderer.textures.white);

        Ok(renderer)
    }
}

/// The UI's index buffer: vanilla's `QUADS` pattern (0, 1, 2, 2, 3, 0) for
unsafe fn create_quad_indices(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let indices: Vec<u32> = (0..(VERTEX_CAPACITY / 4) as u32)
        .flat_map(|quad| [0, 1, 2, 2, 3, 0].map(|corner| quad * 4 + corner))
        .collect();
    let size = std::mem::size_of_val(indices.as_slice()) as vk::DeviceSize;
    let (buffer, memory) = vk_util::create_buffer_in(
        device,
        mem_props,
        size,
        vk::BufferUsageFlags::INDEX_BUFFER,
        &vk_util::MAPPED_FOR_GPU,
    )?;
    let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
    std::ptr::copy_nonoverlapping(indices.as_ptr(), mapped.cast::<u32>(), indices.len());
    device.unmap_memory(memory);
    Ok((buffer, memory))
}
