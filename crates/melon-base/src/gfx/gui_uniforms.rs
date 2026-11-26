//! The uniform blocks vanilla's GUI shaders read (`DynamicTransforms`, `Projection`).

use ash::vk;
use numerobis::elem::Pod;

use super::mapped::MappedBuffer;
use super::vk_util::Place;

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

pub struct GuiUniforms {
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    frames: Vec<Frame>,
}

struct Frame {
    buffer: MappedBuffer,
    set: vk::DescriptorSet,
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
    /// One uniform block per frame in flight, each with its set.
    ///
    /// # Safety
    ///
    /// `device` must be live, `layout` from [`create_layout`] on it.
    pub unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        layout: vk::DescriptorSetLayout,
        frames: usize,
    ) -> anyhow::Result<Self> {
        let blocks = BLOCKS.len() as u32;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: frames as u32 * blocks,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: frames as u32,
            },
        ];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&sizes)
                    .max_sets(frames as u32),
                None,
            )
        }?;
        let layouts = vec![layout; frames];
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            )
        }?;
        let size = STRIDE * u64::from(blocks);
        let mut out = Vec::with_capacity(frames);
        for set in sets {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            let buffer = unsafe {
                MappedBuffer::new(
                    device,
                    mem_props,
                    size,
                    vk::BufferUsageFlags::UNIFORM_BUFFER,
                    Place::UPLOAD,
                )
            }?;
            let infos: Vec<[vk::DescriptorBufferInfo; 1]> = (0..u64::from(blocks))
                .map(|i| {
                    [vk::DescriptorBufferInfo::default()
                        .buffer(buffer.buffer)
                        .offset(i * STRIDE)
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
            let frame = Frame { buffer, set };
            // SAFETY: the frame was just mapped, and the GPU hasn't used it yet.
            unsafe {
                frame.write(&GuiFrame {
                    gui_size: (1.0, 1.0),
                    screen_size: (1.0, 1.0),
                    game_time: 0.0,
                })
            };
            out.push(frame);
        }
        Ok(Self {
            layout,
            pool,
            frames: out,
        })
    }

    /// Writes frame `frame`'s blocks; call once its fence was waited on.
    ///
    /// # Safety
    ///
    /// No submitted frame may still read frame `frame`'s blocks.
    pub unsafe fn write(&self, frame: usize, gui: &GuiFrame) {
        // SAFETY: this frame's fence was waited on, so the GPU no longer reads its uniform slot.
        unsafe { self.frames[frame].write(gui) };
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
        for frame in &self.frames {
            let info = [vk::DescriptorImageInfo::default()
                .sampler(sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                device.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(frame.set)
                        .dst_binding(GLINT_BINDING)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&info)],
                    &[],
                )
            };
        }
    }

    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

    /// Frees the buffers and the sets' pool.
    ///
    /// # Safety
    ///
    /// `device` must be live and done with them.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for frame in self.frames.drain(..) {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { frame.buffer.destroy(device) };
        }
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_descriptor_pool(self.pool, None);
            device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

impl Frame {
    unsafe fn write(&self, gui: &GuiFrame) {
        let transforms = DynamicTransforms {
            model_view: IDENTITY,
            texture: IDENTITY,
            color_modulator: [1.0; 4],
            model_offset: [0.0; 4],
        };
        let globals = Globals::gui(glint_millis(), gui.game_time, gui.screen_size);
        let projection = projection(gui.gui_size.0, gui.gui_size.1);
        let stride = STRIDE as usize;
        // SAFETY: the caller keeps the GPU off this frame's blocks (its fence was waited on).
        unsafe {
            self.buffer.write_one(0, &transforms);
            self.buffer.write_one(stride, &projection);
            self.buffer.write_one(2 * stride, &globals);
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
