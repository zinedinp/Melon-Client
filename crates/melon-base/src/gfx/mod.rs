//! The 2D Vulkan base: device, textures, the UI pipeline and the list of quads it draws.
//! The client's renderer is built from these parts; [`Renderer2d`] is them alone.
//!
//! Vulkan 1.0 is the floor; [`api`] picks the version, [`features`] the
//! faster paths a newer one or an extension adds.
//!
//! Every `unsafe` block leans on three rules: each Vulkan handle is made by the
//! one device and outlives its uses; nothing a frame in flight reads is changed
//! or destroyed before that frame's fence (or a device wait); a command buffer
//! is recorded by one thread at a time.
// TODO(backend): OpenGL and DirectX as render backbones beside Vulkan, for
// systems without a usable Vulkan driver. majestix-vk is the layer they would
// sit behind, but only `renderer/blur.rs`, the swapchain (`Presenter`) and the
// frame ring (`Frames`) use it so far; the rest is raw `ash` (~900 `unsafe`
// blocks) and has to move onto it first.
// TODO(majestix): `suballoc`, `mapped`, `staging`, `residency` and
// `descriptor_pools` do what majestix-vk does too; fold them into it so there
// is one memory manager, not two. Blocked on what it lacks (its TODO.md):
// pipeline caches, timeline semaphores, BAR ranges freed per finished epoch
// (mesh arena), specialization constants, texel buffers, immutable samplers,
// a budget per heap incl. the BAR heap, queue family release/acquire pairs.

pub mod api;
pub mod ascii8;
pub mod descriptor_pools;
pub mod device;
pub mod device_info;
pub mod device_pick;
pub mod features;
pub mod gui_uniforms;
pub mod instance;
pub mod loader;
pub mod mapped;
pub mod pipeline;
pub mod quad;
mod renderer2d;
pub mod residency;
pub mod staging;
pub mod suballoc;
pub mod texture;
pub mod texture_update;
pub mod ui_vertices;
pub mod vk_util;

pub use device_info::read_device_info;
pub use renderer2d::Renderer2d;
pub use vk_util::{
    PassStart, Place, choose_surface_format, create_buffer, create_depth_image, create_render_pass,
    create_sampler, debug_callback, find_depth_format,
};

/// Handle to a 2D texture (UI sprites, font pages).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureId(pub usize);

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
    /// The driver's version, unpacked as its vendor packs it.
    pub driver: String,
    /// The driver's own name and version (Vulkan 1.2 `driverName` and `driverInfo`), e.g.
    /// "NVIDIA 560.94" or "radv Mesa 24.2.4"; empty on 1.0 and on 1.1 drivers without
    /// `VK_KHR_driver_properties`.
    pub driver_detail: String,
    /// Vendor and architecture, for what differs between them.
    pub gpu: numerobis::pci::Gpu,
}
