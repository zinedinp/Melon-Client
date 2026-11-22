//! Headless Vulkan check.

use super::{
    globals, indirect, pipeline, quad_table,
    vk_util::{PassStart, create_render_pass, find_depth_format},
};
use ash::vk;
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::sync::Mutex;

static MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Held by each test on the device for its whole run: [`MESSAGES`] is one
/// log for all of them.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn messages() -> std::sync::MutexGuard<'static, Vec<String>> {
    MESSAGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn local_layer_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".vulkan/validation")
}

fn ensure_layer_search_path() {
    if std::env::var_os("VK_LAYER_PATH").is_some() {
        return;
    }
    let dir = local_layer_dir();
    if dir.join("VkLayer_khronos_validation.json").is_file() {
        unsafe {
            std::env::set_var("VK_LAYER_PATH", dir);
        }
    }
}

unsafe extern "system" fn collect_messages(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _types: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let wanted = vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR;
    if !severity.intersects(wanted) {
        return vk::FALSE;
    }
    let message = CStr::from_ptr((*data).p_message).to_string_lossy();
    messages().push(message.into_owned());
    vk::FALSE
}

struct Gpu {
    _entry: ash::Entry,
    instance: ash::Instance,
    debug: ash::ext::debug_utils::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
    physical: vk::PhysicalDevice,
    device: ash::Device,
}

impl Gpu {
    unsafe fn new() -> anyhow::Result<Self> {
        ensure_layer_search_path();
        let entry = ash::Entry::load()?;
        let layer = c"VK_LAYER_KHRONOS_validation";
        let found = entry
            .enumerate_instance_layer_properties()?
            .iter()
            .any(|l| CStr::from_ptr(l.layer_name.as_ptr()) == layer);
        anyhow::ensure!(
            found,
            "VK_LAYER_KHRONOS_validation missing. Install vulkan-validation-layers, \
             or put the Khronos layer in .vulkan/validation (VkLayer_khronos_validation.json), \
             or set VK_LAYER_PATH. See https://github.com/KhronosGroup/Vulkan-ValidationLayers"
        );

        let app_name = c"Melon";
        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_1);
        let layers = [layer.as_ptr()];
        let extensions = [
            ash::ext::debug_utils::NAME.as_ptr(),
            ash::khr::surface::NAME.as_ptr(),
        ];
        let mut messenger_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(collect_messages));
        let instance_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .push_next(&mut messenger_info);
        let instance = entry.create_instance(&instance_info, None)?;
        let debug = ash::ext::debug_utils::Instance::new(&entry, &instance);
        let messenger = debug.create_debug_utils_messenger(&messenger_info, None)?;

        let physical = instance
            .enumerate_physical_devices()?
            .into_iter()
            .find(|&pd| {
                instance
                    .get_physical_device_queue_family_properties(pd)
                    .iter()
                    .any(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            })
            .ok_or_else(|| anyhow::anyhow!("no graphics GPU found"))?;
        let family = instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .expect("graphics queue") as u32;

        let priorities = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities);
        let queues = [queue_info];
        let supported = instance.get_physical_device_features(physical);
        let mut supported_11 = vk::PhysicalDeviceVulkan11Features::default();
        let mut supported_2 = vk::PhysicalDeviceFeatures2::default().push_next(&mut supported_11);
        instance.get_physical_device_features2(physical, &mut supported_2);
        let multi_draw = supported.multi_draw_indirect == vk::TRUE
            && supported.draw_indirect_first_instance == vk::TRUE
            && supported_11.shader_draw_parameters == vk::TRUE;
        let features = vk::PhysicalDeviceFeatures::default()
            .multi_draw_indirect(multi_draw)
            .draw_indirect_first_instance(multi_draw);
        let mut features_11 =
            vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(multi_draw);
        let device_extensions = [ash::khr::swapchain::NAME.as_ptr()];
        let device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_features(&features)
            .enabled_extension_names(&device_extensions)
            .push_next(&mut features_11);
        let device = instance.create_device(physical, &device_info, None)?;

        Ok(Self {
            _entry: entry,
            instance,
            debug,
            messenger,
            physical,
            device,
        })
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
            self.device.destroy_device(None);
            self.debug
                .destroy_debug_utils_messenger(self.messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

unsafe fn color_only_pass(
    device: &ash::Device,
    format: vk::Format,
) -> anyhow::Result<vk::RenderPass> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
    let color = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color)];
    Ok(device.create_render_pass(
        &vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses),
        None,
    )?)
}

#[test]
fn validation_layers_accept_every_pipeline() {
    let _serial = serial();
    messages().clear();
    unsafe {
        let gpu = Gpu::new().expect("Vulkan instance with validation");
        let depth = find_depth_format(&gpu.instance, gpu.physical).expect("depth format");
        let color = vk::Format::B8G8R8A8_UNORM;
        let render_pass = create_render_pass(&gpu.device, color, depth, PassStart::Clear)
            .expect("scene render pass");
        let blur_pass = color_only_pass(&gpu.device, color).expect("blur render pass");
        let tex_layout =
            pipeline::create_descriptor_set_layout(&gpu.device).expect("texture set layout");
        let layouts = pipeline::world::Layouts {
            table: quad_table::create_layout(&gpu.device).expect("quad table layout"),
            draws: indirect::create_layout(&gpu.device).expect("draw offset layout"),
            globals: globals::create_layout(&gpu.device).expect("globals layout"),
        };

        let mut passes = Vec::new();
        let mut keep = |pass: pipeline::Pass| passes.push(pass);

        keep(pipeline::sky::create(&gpu.device, render_pass, tex_layout).expect("sky"));
        let gui_layout = super::gui_uniforms::create_layout(&gpu.device).expect("gui layout");
        for mode in pipeline::ui::BLENDS {
            keep(
                pipeline::ui::create(
                    &gpu.device,
                    render_pass,
                    tex_layout,
                    gui_layout,
                    mode,
                    pipeline::ui::BUILT_IN,
                )
                .expect("ui"),
            );
        }
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        if assets.join("minecraft/shaders/core/text.vsh").is_file() {
            let shaders = crate::gui::pack_shaders::compile_all(&assets).expect("GUI shaders");
            for shader in &shaders {
                let bytes = |words: &[u32]| -> Vec<u8> {
                    words.iter().flat_map(|w| w.to_le_bytes()).collect()
                };
                let (vert, frag) = (bytes(&shader.vert), bytes(&shader.frag));
                let shaders = pipeline::ui::Shaders {
                    vert: &vert,
                    frag: &frag,
                    inputs: Some(&shader.inputs),
                };
                let built = pipeline::ui::create_all(
                    &gpu.device,
                    render_pass,
                    tex_layout,
                    gui_layout,
                    shaders,
                )
                .expect("pack GUI pipeline");
                built.into_iter().for_each(&mut keep);
            }
        }
        keep(pipeline::blur::create(&gpu.device, blur_pass, tex_layout).expect("blur x"));
        keep(pipeline::blur::create(&gpu.device, render_pass, tex_layout).expect("blur y"));

        use pipeline::world::{Geometry, Mode};
        let world = |mode, geometry| {
            pipeline::world::create(
                &gpu.device,
                render_pass,
                tex_layout,
                &layouts,
                mode,
                geometry,
            )
        };
        for mode in [
            Mode::Solid,
            Mode::Opaque,
            Mode::Translucent,
            Mode::Lightning,
            Mode::EnergySwirl,
            Mode::Scrolled,
            Mode::Text,
            Mode::TranslucentTerrain,
            Mode::Clouds,
            Mode::Weather,
            Mode::Celestial,
            Mode::SkyDisc,
            Mode::SkyGlow,
            Mode::Crumbling,
            Mode::WaterMask,
            Mode::Lines,
            Mode::LinesDepthBias,
            Mode::LinesTranslucent,
            Mode::LinesNoDepthWrite,
        ] {
            keep(world(mode, Geometry::Vertices).expect("world vertices"));
        }
        keep(
            pipeline::world::create(
                &gpu.device,
                blur_pass,
                tex_layout,
                &layouts,
                Mode::Outline,
                Geometry::Vertices,
            )
            .expect("outline"),
        );
        for mode in [Mode::Solid, Mode::Opaque] {
            keep(world(mode, Geometry::Quads { indirect: false }).expect("terrain quads"));
            keep(world(mode, Geometry::Quads { indirect: true }).expect("terrain indirect"));
        }

        gpu.device.device_wait_idle().expect("idle after pipelines");
        for pass in &passes {
            pass.destroy(&gpu.device);
        }
        gpu.device.destroy_descriptor_set_layout(tex_layout, None);
        gpu.device.destroy_descriptor_set_layout(gui_layout, None);
        gpu.device
            .destroy_descriptor_set_layout(layouts.table, None);
        gpu.device
            .destroy_descriptor_set_layout(layouts.draws, None);
        gpu.device
            .destroy_descriptor_set_layout(layouts.globals, None);
        gpu.device.destroy_render_pass(render_pass, None);
        gpu.device.destroy_render_pass(blur_pass, None);
    }

    let messages = messages();
    assert!(
        messages.is_empty(),
        "Vulkan validation:\n{}",
        messages.join("\n")
    );
}

/// The post chains of the packs (vanilla's, and `MELON_PACK`'s on top)
#[test]
fn post_chains_pass_validation() {
    use super::post::gpu::Post;
    use super::post::load;
    use super::texture::Texture;
    use crate::assets::packs::{self, Pack, ResourceStack};
    use std::path::Path;

    let Some(root) = crate::assets::test_root() else {
        eprintln!("no assets (MELON_ASSETS), test skipped");
        return;
    };
    let _serial = serial();
    messages().clear();
    if let Some(pack) = std::env::var_os("MELON_PACK") {
        let pack = Pack::open("test".into(), "test".into(), Path::new(&pack)).expect("pack");
        packs::activate(ResourceStack {
            packs: vec![std::sync::Arc::new(pack)],
        });
    }
    unsafe {
        let gpu = Gpu::new().expect("Vulkan instance with validation");
        let mem_props = gpu
            .instance
            .get_physical_device_memory_properties(gpu.physical);
        let depth = find_depth_format(&gpu.instance, gpu.physical).expect("depth format");
        let color = vk::Format::B8G8R8A8_UNORM;
        let scene = create_render_pass(&gpu.device, color, depth, PassStart::Clear).expect("clear");
        let split =
            create_render_pass(&gpu.device, color, depth, PassStart::SceneSplit).expect("split");
        let resume =
            create_render_pass(&gpu.device, color, depth, PassStart::Resume).expect("resume");
        let gui_layout = super::gui_uniforms::create_layout(&gpu.device).expect("gui layout");
        let mut post = Post::new(&gpu.device, color).expect("post");

        let (chains, blit) = load::load(&root);
        assert!(!chains.is_empty(), "no chains in {}", root.display());
        let sizes = |chain: &super::post::ChainSource| -> Vec<(u32, u32)> {
            chain
                .config
                .passes
                .iter()
                .flat_map(|p| p.inputs.iter())
                .filter_map(|i| match i {
                    super::post::config::Input::Texture { width, height, .. } => {
                        Some((*width, *height))
                    }
                    _ => None,
                })
                .collect()
        };
        let extent = vk::Extent2D {
            width: 640,
            height: 360,
        };
        let mut prepared = Vec::new();
        for chain in chains {
            let textures = sizes(&chain)
                .into_iter()
                .map(|(width, height)| {
                    let image = super::blur::create_image(
                        &gpu.device,
                        &mem_props,
                        vk::Format::R8G8B8A8_UNORM,
                        vk::Extent2D {
                            width: 8,
                            height: 8,
                        },
                        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    )
                    .expect("effect texture");
                    let texture = Texture {
                        image: image.image,
                        memory: image.memory,
                        view: image.view,
                        descriptor_set: vk::DescriptorSet::null(),
                        mip_levels: 1,
                        shared: false,
                    };
                    (texture, width, height)
                })
                .collect();
            prepared.push((chain, textures));
        }
        let device = &gpu.device;
        let free = |t: Texture| {
            device.destroy_image_view(t.view, None);
            device.destroy_image(t.image, None);
            device.free_memory(t.memory, None);
        };
        let failed = post.set_chains(device, &mem_props, gui_layout, prepared, &free);
        assert!(failed.is_empty(), "chains failed: {failed:?}");
        post.set_blit(device, scene, blit.as_ref()).expect("blit");
        let (image, memory, view) =
            super::vk_util::create_depth_image(device, &mem_props, depth, extent, true)
                .expect("depth");
        post.resize(device, &mem_props, extent, &[], Some((image, depth)))
            .expect("resize");
        post.resize(device, &mem_props, extent, &[], Some((image, depth)))
            .expect("resize again");

        device.device_wait_idle().expect("idle");
        post.destroy(device, &free);
        device.destroy_image_view(view, None);
        device.destroy_image(image, None);
        device.free_memory(memory, None);
        device.destroy_descriptor_set_layout(gui_layout, None);
        for pass in [scene, split, resume] {
            device.destroy_render_pass(pass, None);
        }
    }
    packs::activate(ResourceStack::default());
    let messages = messages();
    assert!(
        messages.is_empty(),
        "Vulkan validation:\n{}",
        messages.join("\n")
    );
}

#[test]
fn khronos_validation_layer_is_on_the_loader_path() {
    ensure_layer_search_path();
    let entry = unsafe { ash::Entry::load() }.expect("libvulkan");
    let layer = CString::new("VK_LAYER_KHRONOS_validation").unwrap();
    let found = unsafe {
        entry
            .enumerate_instance_layer_properties()
            .expect("instance layers")
            .iter()
            .any(|l| CStr::from_ptr(l.layer_name.as_ptr()) == layer.as_c_str())
    };
    assert!(
        found,
        "VK_LAYER_KHRONOS_validation not visible to the loader (VK_LAYER_PATH={:?})",
        std::env::var_os("VK_LAYER_PATH")
    );
}

/// Texture sets overflow into pools twice the size, and
/// [`DescriptorPools::compact`] moves them back into one, with validation
/// on: real uploads, plus one set never written (the blur's before its
/// first use).
#[test]
fn texture_pools_grow_and_compact() {
    use super::descriptor_pools::DescriptorPools;
    use super::texture::{Texture, Uploader};

    let _serial = serial();
    messages().clear();
    unsafe {
        let gpu = Gpu::new().expect("Vulkan instance with validation");
        let device = &gpu.device;
        let family = gpu
            .instance
            .get_physical_device_queue_family_properties(gpu.physical)
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .expect("graphics queue") as u32;
        let queue = device.get_device_queue(family, 0);
        let command_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(family),
                None,
            )
            .expect("command pool");
        let mem_props = gpu
            .instance
            .get_physical_device_memory_properties(gpu.physical);
        let layout = pipeline::create_descriptor_set_layout(device).expect("texture set layout");
        let sampler = super::vk_util::create_sampler(
            device,
            vk::Filter::NEAREST,
            false,
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
        )
        .expect("sampler");

        let pools = DescriptorPools::new(device, 4).expect("pools");
        let unwritten = pools.allocate(device, layout).expect("unwritten set");
        let uploader = Uploader {
            device,
            mem_props: &mem_props,
            descriptor_pools: &pools,
            descriptor_set_layout: layout,
            sampler,
        };
        let pixel = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let (mut textures, inits): (Vec<Texture>, Vec<_>) = (0..19)
            .map(|_| uploader.image_2d(&pixel).expect("upload"))
            .unzip();
        super::texture_update::submit_now(device, &mem_props, command_pool, queue, &inits)
            .expect("pixels up");
        assert_eq!(pools.capacities(), [4, 8, 16]);
        assert!(pools.needs_compaction());

        let before: Vec<vk::DescriptorSet> = textures.iter().map(|t| t.descriptor_set).collect();
        let mut sets = [unwritten];
        pools
            .compact(device, layout, |swap| {
                for tex in &mut textures {
                    swap(&mut tex.descriptor_set);
                }
                swap(&mut sets[0]);
            })
            .expect("compact");
        // 20 sets and a quarter to spare: 25, so 4 · 2^3.
        assert_eq!(pools.capacities(), [32]);
        assert!(!pools.needs_compaction());
        let after: std::collections::HashSet<vk::DescriptorSet> =
            textures.iter().map(|t| t.descriptor_set).collect();
        assert_eq!(after.len(), textures.len(), "every texture its own set");
        assert!(
            before.iter().all(|old| !after.contains(old)),
            "every set moved"
        );

        // Most textures gone: the pool shrinks to fit the rest.
        for tex in textures.drain(3..) {
            pools.free(device, tex.descriptor_set);
            device.destroy_image_view(tex.view, None);
            device.destroy_image(tex.image, None);
            device.free_memory(tex.memory, None);
        }
        assert!(pools.needs_compaction());
        pools
            .compact(device, layout, |swap| {
                for tex in &mut textures {
                    swap(&mut tex.descriptor_set);
                }
                swap(&mut sets[0]);
            })
            .expect("compact smaller");
        assert_eq!(pools.capacities(), [8]);

        for tex in textures.drain(..) {
            pools.free(device, tex.descriptor_set);
            device.destroy_image_view(tex.view, None);
            device.destroy_image(tex.image, None);
            device.free_memory(tex.memory, None);
        }
        pools.free(device, sets[0]);
        pools.destroy(device);
        device.destroy_sampler(sampler, None);
        device.destroy_descriptor_set_layout(layout, None);
        device.destroy_command_pool(command_pool, None);
    }

    let messages = messages();
    assert!(
        messages.is_empty(),
        "Vulkan validation:\n{}",
        messages.join("\n")
    );
}

/// New textures' pixels recorded into a frame's command buffer (with an
/// atlas-style rewrite after them), and the batched upload past the
/// budget, read back texel for texel, with validation on.
#[test]
fn queued_texture_uploads_land_without_waiting() {
    use super::staging::Staging;
    use super::texture::{Texture, Uploader};
    use super::texture_update::{self, PendingUpdate};
    use image::{Rgba, RgbaImage};

    let _serial = serial();
    messages().clear();
    // Reading back needs `TRANSFER_SRC`, which textures don't have: the log
    // is checked before each read and cleared after it.
    let clean = |stage: &str| {
        let mut messages = messages();
        let found = std::mem::take(&mut *messages);
        drop(messages);
        assert!(
            found.is_empty(),
            "Vulkan validation ({stage}):\n{}",
            found.join("\n")
        );
    };
    let forget = || messages().clear();
    unsafe {
        let gpu = Gpu::new().expect("Vulkan instance with validation");
        let device = &gpu.device;
        let family = gpu
            .instance
            .get_physical_device_queue_family_properties(gpu.physical)
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .expect("graphics queue") as u32;
        let queue = device.get_device_queue(family, 0);
        let command_pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(family),
                None,
            )
            .expect("command pool");
        let mem_props = gpu
            .instance
            .get_physical_device_memory_properties(gpu.physical);
        let layout = pipeline::create_descriptor_set_layout(device).expect("texture set layout");
        let sampler = super::vk_util::create_sampler(
            device,
            vk::Filter::NEAREST,
            false,
            vk::SamplerAddressMode::CLAMP_TO_EDGE,
        )
        .expect("sampler");
        let pools = super::descriptor_pools::DescriptorPools::new(device, 8).expect("pools");
        let uploader = Uploader {
            device,
            mem_props: &mem_props,
            descriptor_pools: &pools,
            descriptor_set_layout: layout,
            sampler,
        };

        let gradient = RgbaImage::from_fn(4, 4, |x, y| Rgba([x as u8 * 60, y as u8 * 60, 7, 255]));
        let (flat, flat_init) = uploader.image_2d(&gradient).expect("2d");
        let (mipped, mipped_init) = uploader
            .image_2d_mipmapped(
                &RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 4])),
                &[RgbaImage::from_pixel(1, 1, Rgba([9, 8, 7, 6]))],
            )
            .expect("mipmapped");
        let faces: [RgbaImage; 6] =
            std::array::from_fn(|i| RgbaImage::from_pixel(1, 1, Rgba([i as u8 * 40, 0, 0, 255])));
        let (cube, cube_init) = uploader.cubemap(&faces).expect("cubemap");
        let patch = RgbaImage::from_pixel(2, 1, Rgba([200, 100, 50, 255]));
        let updates = [PendingUpdate {
            image: flat.image,
            level: 0,
            x: 1,
            y: 2,
            pixels: patch,
        }];
        let inits = [flat_init, mipped_init, cube_init];

        // The frame: `prepare`, then `record` into its command buffer.
        let mut staging = Staging::default();
        texture_update::prepare(&mut staging, device, &mem_props, &inits, &updates)
            .expect("prepare");
        let cmd = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .expect("cmd")[0];
        device
            .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
            .expect("begin");
        texture_update::record(&mut staging, device, &mem_props, cmd, &inits, &updates)
            .expect("record");
        device.end_command_buffer(cmd).expect("end");
        let cmds = [cmd];
        device
            .queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&cmds)],
                vk::Fence::null(),
            )
            .expect("submit");
        device.queue_wait_idle(queue).expect("idle");
        clean("recorded into the frame");

        let read = |image: vk::Image, level: u32, layer: u32, (w, h): (u32, u32)| -> RgbaImage {
            read_back(
                device,
                &mem_props,
                command_pool,
                queue,
                image,
                level,
                layer,
                (w, h),
            )
        };
        let mut expected = gradient.clone();
        for x in 1..3 {
            expected.put_pixel(x, 2, Rgba([200, 100, 50, 255]));
        }
        assert_eq!(
            read(flat.image, 0, 0, (4, 4)),
            expected,
            "pixels plus the rewrite"
        );
        assert_eq!(
            read(mipped.image, 0, 0, (2, 2)).get_pixel(1, 1),
            &Rgba([1, 2, 3, 4])
        );
        assert_eq!(
            read(mipped.image, 1, 0, (1, 1)).get_pixel(0, 0),
            &Rgba([9, 8, 7, 6])
        );
        for (layer, face) in faces.iter().enumerate() {
            assert_eq!(
                &read(cube.image, 0, layer as u32, (1, 1)),
                face,
                "face {layer}"
            );
        }
        forget();

        // Past the budget: all at once, in one submit.
        let (late, late_init) = uploader.image_2d(&gradient).expect("2d");
        texture_update::submit_now(device, &mem_props, command_pool, queue, &[late_init])
            .expect("submit now");
        clean("submitted at once");
        assert_eq!(read(late.image, 0, 0, (4, 4)), gradient);
        forget();

        staging.destroy(device);
        for tex in [flat, mipped, cube, late] {
            let tex: Texture = tex;
            pools.free(device, tex.descriptor_set);
            device.destroy_image_view(tex.view, None);
            device.destroy_image(tex.image, None);
            device.free_memory(tex.memory, None);
        }
        pools.destroy(device);
        device.destroy_sampler(sampler, None);
        device.destroy_descriptor_set_layout(layout, None);
        device.destroy_command_pool(command_pool, None);
    }

    let messages = messages();
    assert!(
        messages.is_empty(),
        "Vulkan validation:\n{}",
        messages.join("\n")
    );
}

/// Mesh blocks a world left behind are destroyed once no frame in flight
/// can draw from them, all but one spare, and their descriptor sets go with
/// them; later meshes fill the spare first, then get a new block.
#[test]
fn empty_mesh_blocks_are_released() {
    use super::TerrainQuad;
    use super::mesh_arena::{Kind, MeshArena};
    use super::quad_table::{QuadSets, create_layout};

    let _serial = serial();
    messages().clear();
    unsafe {
        let gpu = Gpu::new().expect("Vulkan instance with validation");
        let device = &gpu.device;
        let mem_props = gpu
            .instance
            .get_physical_device_memory_properties(gpu.physical);
        let mut arena =
            MeshArena::new::<TerrainQuad>(device, &mem_props, 2, Kind::Quads).expect("arena");
        let layout = create_layout(device).expect("quad set layout");
        let mut sets = QuadSets::new(device).expect("quad sets");
        let block_bytes = arena.block_bytes();
        let per_block = (block_bytes / std::mem::size_of::<TerrainQuad>() as u64) as usize;
        let quads = vec![TerrainQuad([0; 5]); per_block + 4];
        let upload = |arena: &mut MeshArena, n: usize| {
            arena
                .upload(device, &mem_props, &quads[..n])
                .expect("mesh upload")
        };
        let buffer = |arena: &MeshArena, id: usize| arena.location(arena.slot(id).unwrap()).0;

        let full = upload(&mut arena, per_block);
        let halves = [
            upload(&mut arena, per_block / 2),
            upload(&mut arena, per_block / 2),
        ];
        let oversized = upload(&mut arena, per_block + 4);
        let empty = upload(&mut arena, 0);
        assert_eq!(arena.slot(empty).map(|s| s.count), None, "nothing to draw");
        assert_eq!(arena.buffers().count(), 3);
        assert_eq!(buffer(&arena, halves[0]), buffer(&arena, halves[1]));
        sets.sync(device, layout, arena.buffers()).expect("sets");
        let spare = buffer(&arena, full);

        for id in [full, halves[0], halves[1], oversized, empty] {
            arena.free(id, 7);
        }
        arena.release_retired(device, 7);
        assert_eq!(arena.buffers().count(), 3, "frame 7 may still draw them");
        arena.release_retired(device, 8);
        assert_eq!(arena.buffers().collect::<Vec<_>>(), vec![spare]);
        assert_eq!(arena.usage(), (block_bytes, 0));
        let gone = arena.take_destroyed();
        assert_eq!(gone.len(), 2);
        for &buffer in &gone {
            assert!(sets.get(buffer).is_some());
            sets.forget(device, buffer);
            assert!(sets.get(buffer).is_none());
        }
        assert!(sets.get(spare).is_some());

        let again = upload(&mut arena, per_block);
        assert_eq!(buffer(&arena, again), spare, "the spare is filled first");
        let more = upload(&mut arena, 4);
        assert_ne!(buffer(&arena, more), spare);
        sets.sync(device, layout, arena.buffers()).expect("sets");
        assert!(sets.get(buffer(&arena, more)).is_some());
        assert_eq!(arena.buffers().count(), 2);

        sets.destroy(device);
        device.destroy_descriptor_set_layout(layout, None);
        arena.destroy(device);
    }
    let messages = messages();
    assert!(
        messages.is_empty(),
        "Vulkan validation:\n{}",
        messages.join("\n")
    );
}

/// One level and layer of a sampled `R8G8B8A8` image, copied back to the
/// host; leaves it in `SHADER_READ_ONLY_OPTIMAL` again.
#[allow(clippy::too_many_arguments)]
unsafe fn read_back(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    level: u32,
    layer: u32,
    (width, height): (u32, u32),
) -> image::RgbaImage {
    let size = (width * height * 4) as u64;
    let (buffer, memory) = super::create_buffer(
        device,
        mem_props,
        size,
        vk::BufferUsageFlags::TRANSFER_DST,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )
    .expect("readback buffer");
    let cmd = device
        .allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )
        .expect("cmd")[0];
    device
        .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
        .expect("begin");
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(level)
        .level_count(1)
        .base_array_layer(layer)
        .layer_count(1);
    let barrier = |old, new, src, dst| {
        vk::ImageMemoryBarrier::default()
            .old_layout(old)
            .new_layout(new)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(range)
            .src_access_mask(src)
            .dst_access_mask(dst)
    };
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::SHADER_READ,
            vk::AccessFlags::TRANSFER_READ,
        )],
    );
    device.cmd_copy_image_to_buffer(
        cmd,
        image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        buffer,
        &[vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: level,
                base_array_layer: layer,
                layer_count: 1,
            })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })],
    );
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::empty(),
        )],
    );
    device.end_command_buffer(cmd).expect("end");
    let cmds = [cmd];
    device
        .queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(&cmds)],
            vk::Fence::null(),
        )
        .expect("submit");
    device.queue_wait_idle(queue).expect("idle");
    let mapped = device
        .map_memory(memory, 0, size, vk::MemoryMapFlags::empty())
        .expect("map") as *const u8;
    let bytes = std::slice::from_raw_parts(mapped, size as usize).to_vec();
    device.unmap_memory(memory);
    device.free_command_buffers(command_pool, &[cmd]);
    device.destroy_buffer(buffer, None);
    device.free_memory(memory, None);
    image::RgbaImage::from_raw(width, height, bytes).expect("pixels")
}
