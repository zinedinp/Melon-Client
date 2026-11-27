//! The uniform blocks vanilla's GUI shaders read (`DynamicTransforms`, `Projection`).

use ash::vk;
use majestix_vk::{Frames, Mapped, MemoryUse};
use numerobis::elem::Pod;

/// Each block's binding in set 1, by the name a shader declares it with.
pub const BLOCKS: [&str; 3] = ["DynamicTransforms", "Projection", "Globals"];

/// Room for each block; `minUniformBufferOffsetAlignment` is at most 256.
pub const STRIDE: u64 = 256;

/// `dynamictransforms.glsl`, std140.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DynamicTransforms {
    pub model_view: [f32; 16],
    pub texture: [f32; 16],
    pub color_modulator: [f32; 4],
    /// `vec3 ModelOffset` and its padding.
    pub model_offset: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<DynamicTransforms>() == 160);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for DynamicTransforms {}

/// `globals.glsl`, std140.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Globals {
    pub camera_block_pos: [i32; 3],
    pub glint_alpha: f32,
    pub camera_offset: [f32; 3],
    pub game_time: f32,
    pub screen_size: [f32; 2],
    pub menu_blur_radius: i32,
    pub use_rgss: i32,
}

const _: () = assert!(std::mem::size_of::<Globals>() == 48);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for Globals {}

impl Globals {
    /// The GUI's globals: no camera, the glint scrolling by `glint_millis`.
    pub fn gui(glint_millis: f32, game_time: f32, screen_size: (f32, f32)) -> Self {
        Self {
            camera_block_pos: [0; 3],
            glint_alpha: 1.0,
            camera_offset: [glint_millis, 0.0, 0.0],
            game_time,
            screen_size: [screen_size.0, screen_size.1],
            menu_blur_radius: 5,
            use_rgss: 0,
        }
    }
}

pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// What the blocks say this frame.
#[derive(Clone, Copy, Debug)]
pub struct GuiFrame {
    /// The GUI's size in its own coordinates.
    pub gui_size: (f32, f32),
    /// The window's size in pixels.
    pub screen_size: (f32, f32),
    /// `Globals.GameTime`: the day's fraction, 0..1.
    pub game_time: f32,
}

/// `Util.getMillis() * glintSpeed * 8` from process start, speed 1: what scrolls the glint.
pub fn glint_millis() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    (START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 1000.0
        * 8.0) as f32
}

/// `Globals.GameTime` without a world's clock.
pub fn game_time() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let ticks = START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        * 20.0;
    (ticks % 24000.0 / 24000.0) as f32
}

/// `ProjMat` for GUI coordinates.
fn projection(w: f32, h: f32) -> [f32; 16] {
    [
        2.0 / w,
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / h,
        0.0,
        0.0,
        0.0,
        0.0,
        -1.0 / 20000.0,
        0.0,
        -1.0,
        -1.0,
        0.5,
        1.0,
    ]
}

/// Each frame slot's blocks in one ring, and a set per slot naming them.
pub struct GuiUniforms {
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    /// By slot index.
    sets: Vec<vk::DescriptorSet>,
    /// majestix-vk's part; given up in [`Self::destroy`], before the device goes.
    ring: Option<Mapped<u8>>,
}

/// Binding of the item-glint sampler in set 1.
pub const GLINT_BINDING: u32 = BLOCKS.len() as u32;

/// The set layout.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn create_layout(device: &ash::Device) -> anyhow::Result<vk::DescriptorSetLayout> {
    let mut bindings: Vec<_> = (0..BLOCKS.len() as u32)
        .map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
        })
        .collect();
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(GLINT_BINDING)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

impl GuiUniforms {
    /// The blocks of every slot of `frames` in one ring, each slot with its set.
    ///
    /// # Safety
    ///
    /// `layout` must be from [`create_layout`] on `layer`'s device.
    pub unsafe fn new(
        layer: &majestix_vk::Device,
        frames: &Frames,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Self> {
        let device = layer.raw();
        let blocks = BLOCKS.len() as u32;
        let count = frames.count();
        let ring = Mapped::<u8>::new(
            layer,
            frames,
            (STRIDE * u64::from(blocks)) as usize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryUse::Upload,
        )?;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: count * blocks,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: count,
            },
        ];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&sizes)
                    .max_sets(count),
                None,
            )
        }?;
        let layouts = vec![layout; count as usize];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sets = match unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        } {
            Ok(sets) => sets,
            Err(e) => {
                // SAFETY: made just now, and nothing uses it.
                unsafe { device.destroy_descriptor_pool(pool, None) };
                return Err(e.into());
            }
        };
        for (slot, &set) in frames.slots().zip(&sets) {
            let start = ring.offset(slot);
            let infos: Vec<[vk::DescriptorBufferInfo; 1]> = (0..u64::from(blocks))
                .map(|i| {
                    [vk::DescriptorBufferInfo::default()
                        .buffer(ring.buffer().raw())
                        .offset(start + i * STRIDE)
                        .range(STRIDE)]
                })
                .collect();
            let writes: Vec<_> = infos
                .iter()
                .enumerate()
                .map(|(binding, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }
        Ok(Self {
            layout,
            pool,
            sets,
            ring: Some(ring),
        })
    }

    /// Writes the frame's blocks.
    pub fn write(&mut self, frame: &majestix_vk::Frame<'_>, gui: &GuiFrame) {
        let ring = self
            .ring
            .as_mut()
            .expect("gui uniforms written after destroy");
        let transforms = DynamicTransforms {
            model_view: IDENTITY,
            texture: IDENTITY,
            color_modulator: [1.0; 4],
            model_offset: [0.0; 4],
        };
        let globals = Globals::gui(glint_millis(), gui.game_time, gui.screen_size);
        let stride = STRIDE as usize;
        ring.write(frame, 0, &transforms);
        ring.write(frame, stride, &projection(gui.gui_size.0, gui.gui_size.1));
        ring.write(frame, 2 * stride, &globals);
    }

    /// Binds `view` sampled by `sampler` as the item glint, every frame.
    ///
    /// # Safety
    ///
    /// `device` must be live, `view` and `sampler` live; no frame in flight may use the sets.
    pub unsafe fn bind_glint(
        &self,
        device: &ash::Device,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) {
        for &set in &self.sets {
            let info = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                device.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(GLINT_BINDING)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&info)],
                    &[],
                )
            };
        }
    }

    /// Frame slot `slot`'s set.
    pub fn set(&self, slot: usize) -> vk::DescriptorSet {
        self.sets[slot]
    }

    /// Retires the ring and frees the sets' pool and the layout.
    ///
    /// # Safety
    ///
    /// `device` must be live and done with them.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        if self.ring.take().is_none() {
            return;
        }
        self.sets.clear();
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_have_their_std140_sizes() {
        assert_eq!(std::mem::size_of::<DynamicTransforms>(), 160);
        assert_eq!(std::mem::size_of::<Globals>(), 48);
    }

    #[test]
    fn the_projection_maps_gui_corners_to_clip_corners() {
        let m = projection(320.0, 240.0);
        let apply = |x: f32, y: f32| [m[0] * x + m[12], m[5] * y + m[13], m[14]];
        assert_eq!(apply(0.0, 0.0), [-1.0, -1.0, 0.5]);
        assert_eq!(apply(320.0, 240.0), [1.0, 1.0, 0.5]);
    }
}
