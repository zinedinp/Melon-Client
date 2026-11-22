//! The 2D Vulkan base: device, textures, the UI pipeline and the list of quads it draws.
//! The client's renderer is built from these parts; [`Renderer2d`] is them alone.
#![allow(unsafe_op_in_unsafe_fn)]

pub mod ascii8;
pub mod descriptor_pools;
pub mod device;
pub mod device_info;
pub mod device_pick;
pub mod gui_uniforms;
pub mod instance;
pub mod pipeline;
pub mod quad;
mod renderer2d;
pub mod staging;
pub mod texture;
pub mod texture_update;
pub mod vk_util;

pub use device_info::read_device_info;
pub use renderer2d::Renderer2d;
pub use vk_util::{
    PassStart, choose_surface_format, create_buffer, create_depth_image, create_render_pass,
    create_sampler, debug_callback, find_depth_format, find_memory_type,
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
    pub driver: String,
}

