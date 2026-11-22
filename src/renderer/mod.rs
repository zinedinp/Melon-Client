//! Vulkan renderer: device, swapchain, textures, meshes and six pipelines.
#![allow(unsafe_op_in_unsafe_fn)]

mod blur;
mod capture;
mod context;
mod descriptor_pools;
mod device_info;
mod device_pick;
mod frame;
mod globals;
mod gpu_timer;
mod gui_uniforms;
mod indirect;
mod mesh_arena;
pub(crate) mod pack_outline;
mod picture;
mod pipeline;
pub mod post;
pub mod quad;
mod quad_table;
mod record;
mod resources;
mod staging;
mod submit;
mod swapchain;
mod texture;
mod texture_update;
#[cfg(test)]
mod vk_check;
mod vk_util;

use device_info::read_device_info;
pub use gpu_timer::PassStats;
pub use picture::{PictureDraw, pixel_projection};
pub use swapchain::FIFO_LATEST_READY;
use vk_util::{
    PassStart, choose_surface_format, create_buffer, create_depth_image, create_render_pass,
    create_sampler, debug_callback, find_depth_format, find_memory_type,
};

pub use frame::{
    Blending, Blur, CelestialDraw, ChunkDraw, CloudDraw, CornerLight, EntityDraw, Frame, Glint,
    LineDraw, LineStyle, QuadCorner, RenderLayer, SceneFrame, Sky, SkyKind, TerrainMesh,
    TerrainQuad, WORLD_Z_NEAR, WeatherDraw, WorldDraw, WorldVertex, world_projection,
};

use ash::vk;

const MAX_FRAMES_IN_FLIGHT: usize = 2;
/// Descriptor sets for 2D textures and cube maps in the first pool. Every
/// baked item icon is one; a full pool is followed by one twice its size
/// (see [`descriptor_pools`]).
const MAX_TEXTURES: u32 = 4096;
const VERTEX_CAPACITY: usize = 1 << 16;

/// A resource pack's replacement for one of vanilla's GUI shaders, compiled to SPIR-V.
pub struct GuiShader {
    pub program: quad::Program,
    pub vert: Vec<u32>,
    pub frag: Vec<u32>,
    /// The vertex attribute locations `vert` reads.
    pub inputs: Vec<u32>,
}

/// Handle to a 2D texture (UI sprites, font pages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureId(pub usize);

/// Handle to a cubemap (sky / panorama).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CubeMapId(pub usize);

/// Handle to an uploaded mesh ([`WorldVertex`] list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId(pub usize);

/// Handle to an uploaded list of [`TerrainQuad`]s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuadMeshId(pub usize);

/// What the F3 overlay's system-specs group needs about the GPU.
#[derive(Clone)]
pub struct DeviceInfo {
    /// `deviceName`, e.g. "AMD Radeon RX 7900 XTX (RADV NAVI31)".
    pub name: String,
    /// `DeviceInfo.type` as vanilla's suffix already applied.
    pub type_suffix: &'static str,
    /// `vendorName`.
    pub vendor: String,
    /// `backendName` plus `driverInfo`.
    pub backend: String,
    pub driver: String,
}

pub struct Renderer {
    #[allow(dead_code)]
    entry: ash::Entry,
    instance: ash::Instance,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    mem_props: vk::PhysicalDeviceMemoryProperties,
    device: ash::Device,
    queue: vk::Queue,
    passes: context::RenderPasses,
    /// Post effect chains.
    post: post::gpu::Post,
    /// The menu background blur.
    blur: blur::Blur,
    device_info: DeviceInfo,

    swapchain: Swapchain,
    pipelines: Pipelines,
    frames: Frames,
    ui_vertices: UiVertices,
    /// Static cube for the sky pass, uploaded once at startup.
    sky_cube: SkyCube,
    /// Entity, fluid and other vertex meshes, slices of a few device-local buffers.
    meshes: mesh_arena::MeshArena,
    /// Terrain [`TerrainQuad`] lists, the same way.
    quad_meshes: mesh_arena::MeshArena,
    /// Every baked block quad's corners, which terrain quads index.
    quad_table: Option<quad_table::QuadTable>,
    /// What the command buffer being recorded has bound and pushed.
    recording: record::Recording,
    textures: Textures,
    /// `maxImageDimension2D`.
    max_texture_side: u32,
    /// The ids of meshes uploaded since [`Renderer::track_textures`].
    tracked_meshes: Option<Vec<MeshId>>,
    /// GPU time per pass, for F3.
    gpu_timer: gpu_timer::GpuTimer,
    /// The buffers of the terrain's multi-draws; `None` when the device can't draw indirect.
    indirect: Option<indirect::DrawBuffers>,
    /// A set per terrain mesh buffer, binding it for the shader (set 4).
    quad_sets: quad_table::QuadSets,
    /// The fog and the dimension's light, per frame in flight.
    globals: globals::GlobalBuffers,
    /// Vanilla's GUI uniform blocks, for pack shaders, per frame in flight.
    gui_uniforms: gui_uniforms::GuiUniforms,
    /// Pack `rendertype_outline`, when a pack ships it (Lumiere portals).
    pack_outline: Option<pack_outline::PackOutline>,
    /// A frame asked for by [`Renderer::request_capture`].
    capture: capture::Capture,
    /// Targets of [`PictureDraw`]s.
    pictures: picture::Pictures,
}

/// The swapchain and what is sized with it, rebuilt together.
struct Swapchain {
    loader: ash::khr::swapchain::Device,
    handle: vk::SwapchainKHR,
    extent: vk::Extent2D,
    /// `Options.enableVsync`.
    vsync: bool,
    /// A present mode asked for by name, overriding `vsync` when offered.
    requested: Option<vk::PresentModeKHR>,
    /// Whether `VK_KHR/EXT_present_mode_fifo_latest_ready` is enabled on the device.
    latest_ready: bool,
    /// What the current swapchain presents with.
    present_mode: vk::PresentModeKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    format: vk::Format,
    /// Whether the images can be copied from, which the blur needs.
    can_copy: bool,
    depth_format: vk::Format,
    /// Whether the depth buffer can be read by shaders (a post chain is on).
    depth_sampled: bool,
    /// One depth buffer shared by every framebuffer.
    depth: DepthImage,
    framebuffers: Vec<vk::Framebuffer>,
}

/// The pipelines, all sharing one descriptor set layout.
struct Pipelines {
    layout: vk::DescriptorSetLayout,
    /// The UI's, one per blend mode ([`quad::BlendMode`]), and a resource pack's replacements.
    ui: pipeline::ui::UiPipelines,
    sky: pipeline::Pass,
    /// [`pipeline::world`] without `discard`, for fully opaque terrain.
    solid: pipeline::Pass,
    /// Terrain with holes, and entities.
    world: pipeline::Pass,
    /// [`pipeline::world`]'s blended variant, for blob shadows and other overlays.
    translucent: pipeline::Pass,
    /// The blended variant that writes depth, for translucent terrain.
    translucent_terrain: pipeline::Pass,
    /// Added on: lightning bolts.
    lightning: pipeline::Pass,
    /// Slid and added on: charged creepers, the wither's armor.
    energy_swirl: pipeline::Pass,
    /// Slid and blended: the breeze's wind.
    scrolled: pipeline::Pass,
    /// Text displays: blended, depth written.
    text: pipeline::Pass,
    /// Fancy clouds: blended, no back-face cull.
    clouds: pipeline::Pass,
    /// Rain and snow: blended, no cull.
    weather: pipeline::Pass,
    /// Sun, moon and stars (`RenderPipelines.CELESTIAL`).
    celestial: pipeline::Pass,
    /// The sky's colour, faded into the fog (`RenderPipelines.SKY`).
    sky_disc: pipeline::Pass,
    /// Dusk's glow and the end's sky: blended, no fog.
    sky_glow: pipeline::Pass,
    /// Block cracks, multiplied onto the block (`RenderPipelines.CRUMBLING`).
    crumbling: pipeline::Pass,
    /// The boat's water patch: depth only.
    water_mask: pipeline::Pass,
    /// Glowing entities' flat colour, into the entity outline target.
    outline: pipeline::Pass,
    /// Enchantment glint overlay.
    glint: pipeline::Pass,
    /// The block outline and the gizmos: [`LineStyle`]'s pipelines, `LINES`,
    /// `LINES_DEPTH_BIAS`, `LINES_TRANSLUCENT` and
    /// `LINES_TRANSLUCENT_NO_DEPTH_WRITE`.
    lines: [pipeline::Pass; 4],
    /// The sets after the texture's that the world pipelines share.
    layouts: pipeline::world::Layouts,
    /// Terrain from [`TerrainQuad`]s.
    terrain: [pipeline::Pass; 2],
}

/// The frames in flight.
struct Frames {
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    images_in_flight: Vec<vk::Fence>,
    /// Which frame in flight is being recorded.
    current: usize,
    /// Frames rendered so far.
    number: u64,
    /// By frame in flight: the [`Self::number`] of the frame it last
    /// submitted, 0 once its fence was waited on.
    submitted: [u64; MAX_FRAMES_IN_FLIGHT],
    /// Where the last [`Renderer::render`] waited.
    last_waits: FrameWaits,
    /// The post chains the last world frame ran, the entity outline's left
    /// out (`GameRenderer.getAppliedPostEffects`).
    applied_post_effects: Vec<String>,
}

impl Frames {
    /// The oldest frame the GPU may still run: whatever only earlier frames
    /// used can be reused or destroyed. Frames that end before they are
    /// submitted (an allocation failing) count up [`Self::number`] without
    /// taking a fence, so a count of frames since is not enough.
    fn oldest_in_flight(&self) -> u64 {
        self.submitted
            .iter()
            .copied()
            .filter(|&n| n != 0)
            .min()
            .unwrap_or(u64::MAX)
    }
}

/// What one [`Renderer::render`] spent blocked, for the F3 profiler.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameWaits {
    /// On fences.
    pub gpu: std::time::Duration,
    /// In `acquire_next_image`.
    pub acquire: std::time::Duration,
    /// In `queue_present`, which some drivers block in.
    pub present: std::time::Duration,
}

/// One host-visible UI vertex buffer per frame in flight, mapped for good.
struct UiVertices {
    buffers: Vec<vk::Buffer>,
    memories: Vec<vk::DeviceMemory>,
    mapped: Vec<*mut u8>,
    /// Six indices per quad, shared by every frame.
    indices: (vk::Buffer, vk::DeviceMemory),
}

struct SkyCube {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

/// Textures and cube maps with their descriptor sets and samplers.
struct Textures {
    list: Vec<texture::Texture>,
    cubemaps: Vec<texture::Texture>,
    /// New textures' contents, copied in at the start of the next frame,
    /// before the rewrites.
    pending_inits: Vec<texture_update::PendingInit>,
    /// Their bytes, for the budget past which they go up at once.
    pending_init_bytes: u64,
    /// Texture regions to rewrite at the start of the next frame.
    pending_updates: Vec<texture_update::PendingUpdate>,
    /// One staging buffer per frame in flight for those rewrites.
    update_staging: Vec<staging::Staging>,
    /// A 1x1 white texture, for untextured quads.
    white: TextureId,
    descriptor_pools: descriptor_pools::DescriptorPools,
    /// Nearest: UI sprites and font (vanilla look).
    nearest_sampler: vk::Sampler,
    /// Nearest within a level, blended between levels: the block atlas.
    mipmap_sampler: vk::Sampler,
    /// Linear: cubemap (vanilla `TextureMetadataSection(blur=true)`).
    linear_sampler: vk::Sampler,
    /// Nearest and wrapping: GUI textures drawn tiled in one quad.
    repeat_sampler: vk::Sampler,
    /// Linear and wrapping: the enchanted glint (`blur: true`).
    linear_repeat_sampler: vk::Sampler,
    /// Freed slots, handed out again by the next uploads.
    free: Vec<TextureId>,
    /// Slots the next uploads replace, in order.
    reuse: std::collections::VecDeque<TextureId>,
    /// The ids of uploads since [`Renderer::track_textures`].
    tracked: Option<Vec<TextureId>>,
    /// Replaced textures and the frame they were last drawn in, destroyed once no frame in
    /// flight uses them.
    retired: Vec<(u64, texture::Texture)>,
}

impl Renderer {
    /// Drawable aspect ratio.
    pub fn aspect_ratio(&self) -> f32 {
        self.swapchain.extent.width as f32 / self.swapchain.extent.height.max(1) as f32
    }

    /// Swapchain size in physical pixels - `Window.getWidth()`/`getHeight()`
    pub fn framebuffer_size(&self) -> (u32, u32) {
        (self.swapchain.extent.width, self.swapchain.extent.height)
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    /// The largest 2D texture side the GPU takes.
    pub fn max_texture_side(&self) -> u32 {
        self.max_texture_side
    }
}

/// Whether a renderer error is `VK_ERROR_DEVICE_LOST`: the driver gave the
/// device up (a GPU fault or reset), and nothing on it works again.
pub fn is_device_lost(e: &anyhow::Error) -> bool {
    e.chain()
        .any(|cause| cause.downcast_ref::<vk::Result>() == Some(&vk::Result::ERROR_DEVICE_LOST))
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();

            for &buf in &self.ui_vertices.buffers {
                self.device.destroy_buffer(buf, None);
            }
            for &mem in &self.ui_vertices.memories {
                self.device.unmap_memory(mem);
                self.device.free_memory(mem, None);
            }
            self.device.destroy_buffer(self.sky_cube.buffer, None);
            self.device.free_memory(self.sky_cube.memory, None);
            self.meshes.destroy(&self.device);
            self.quad_meshes.destroy(&self.device);
            self.gpu_timer.destroy(&self.device);
            if let Some(indirect) = &mut self.indirect {
                indirect.destroy(&self.device);
            }
            self.globals.destroy(&self.device);
            self.gui_uniforms.destroy(&self.device);
            self.device.destroy_buffer(self.ui_vertices.indices.0, None);
            self.device.free_memory(self.ui_vertices.indices.1, None);
            if let Some(table) = self.quad_table.take() {
                table.destroy(&self.device);
            }

            let retired = self.textures.retired.iter().map(|(_, tex)| tex);
            for tex in self
                .textures
                .list
                .iter()
                .chain(&self.textures.cubemaps)
                .chain(retired)
            {
                if tex.shared {
                    continue;
                }
                self.device.destroy_image_view(tex.view, None);
                self.device.destroy_image(tex.image, None);
                self.device.free_memory(tex.memory, None);
            }

            for &sem in &self.frames.image_available {
                self.device.destroy_semaphore(sem, None);
            }
            for &fence in &self.frames.in_flight {
                self.device.destroy_fence(fence, None);
            }
            self.blur.destroy(&self.device);
            self.destroy_swapchain_resources();
            if self.swapchain.handle != vk::SwapchainKHR::null() {
                self.swapchain
                    .loader
                    .destroy_swapchain(self.swapchain.handle, None);
            }

            self.device
                .destroy_command_pool(self.frames.command_pool, None);
            self.device
                .destroy_sampler(self.textures.nearest_sampler, None);
            self.device
                .destroy_sampler(self.textures.mipmap_sampler, None);
            self.device
                .destroy_sampler(self.textures.linear_sampler, None);
            self.device
                .destroy_sampler(self.textures.repeat_sampler, None);
            self.device
                .destroy_sampler(self.textures.linear_repeat_sampler, None);
            self.pipelines.ui.destroy(&self.device);
            self.pipelines.sky.destroy(&self.device);
            self.pipelines.solid.destroy(&self.device);
            self.pipelines.world.destroy(&self.device);
            self.pipelines.translucent.destroy(&self.device);
            self.pipelines.translucent_terrain.destroy(&self.device);
            self.pipelines.lightning.destroy(&self.device);
            self.pipelines.energy_swirl.destroy(&self.device);
            self.pipelines.scrolled.destroy(&self.device);
            self.pipelines.text.destroy(&self.device);
            self.pipelines.clouds.destroy(&self.device);
            self.pipelines.weather.destroy(&self.device);
            self.pipelines.celestial.destroy(&self.device);
            self.pipelines.sky_disc.destroy(&self.device);
            self.pipelines.sky_glow.destroy(&self.device);
            self.pipelines.crumbling.destroy(&self.device);
            self.pipelines.water_mask.destroy(&self.device);
            if let Some(mut pack) = self.pack_outline.take() {
                pack.destroy(&self.device);
            }
            self.pipelines.outline.destroy(&self.device);
            self.pipelines.glint.destroy(&self.device);
            for pass in &self.pipelines.lines {
                pass.destroy(&self.device);
            }
            for pass in &self.pipelines.terrain {
                pass.destroy(&self.device);
            }
            self.quad_sets.destroy(&self.device);
            self.device
                .destroy_descriptor_set_layout(self.pipelines.layouts.table, None);
            self.device
                .destroy_descriptor_set_layout(self.pipelines.layouts.draws, None);
            self.textures.pending_inits.clear();
            self.textures.pending_updates.clear();
            for staging in &mut self.textures.update_staging {
                staging.destroy(&self.device);
            }
            self.device
                .destroy_descriptor_set_layout(self.pipelines.layout, None);
            let (device, pools) = (self.device.clone(), &self.textures.descriptor_pools);
            self.post
                .destroy(&self.device, &|t| free_effect_texture(&device, pools, t));
            self.textures.descriptor_pools.destroy(&self.device);
            self.pictures.destroy(&self.device);
            self.passes.destroy(&self.device);
            self.device.destroy_device(None);

            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((debug_instance, messenger)) = self.debug.take() {
                debug_instance.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// The shared depth buffer; null handles while there is no swapchain.
#[derive(Default)]
struct DepthImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

/// Frees a texture a post chain owns outside the texture list.
unsafe fn free_effect_texture(
    device: &ash::Device,
    pools: &descriptor_pools::DescriptorPools,
    texture: texture::Texture,
) {
    device.destroy_image_view(texture.view, None);
    device.destroy_image(texture.image, None);
    device.free_memory(texture.memory, None);
    pools.free(device, texture.descriptor_set);
}
