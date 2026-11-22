//! The uniform blocks vanilla's GUI shaders read (`DynamicTransforms`, `Projection`).

use ash::vk;

use super::vk_util::{MAPPED_FOR_GPU, create_buffer_in};

/// Each block's binding in set 1, by the name a shader declares it with.
pub const BLOCKS: [&str; 3] = ["DynamicTransforms", "Projection", "Globals"];

/// Room for each block; `minUniformBufferOffsetAlignment` is at most 256.
const STRIDE: u64 = 256;

/// `dynamictransforms.glsl`, std140.
#[repr(C)]
#[derive(Clone, Copy)]
struct DynamicTransforms {
    model_view: [f32; 16],
    texture: [f32; 16],
    color_modulator: [f32; 4],
    /// `vec3 ModelOffset` and its padding.
    model_offset: [f32; 4],
}

/// `globals.glsl`, std140.
#[repr(C)]
#[derive(Clone, Copy)]
struct Globals {
    camera_block_pos: [i32; 3],
    glint_alpha: f32,
    camera_offset: [f32; 3],
    game_time: f32,
    screen_size: [f32; 2],
    menu_blur_radius: i32,
    use_rgss: i32,
}

const IDENTITY: [f32; 16] = [
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

pub(super) struct GuiUniforms {
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    frames: Vec<Frame>,
}

struct Frame {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: *mut u8,
    set: vk::DescriptorSet,
}

/// Binding of the item-glint sampler in set 1.
pub const GLINT_BINDING: u32 = BLOCKS.len() as u32;

/// The set layout.
pub(super) unsafe fn create_layout(
    device: &ash::Device,
) -> anyhow::Result<vk::DescriptorSetLayout> {
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
    Ok(device.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
        None,
    )?)
}

impl GuiUniforms {
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
        let pool = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .pool_sizes(&sizes)
                .max_sets(frames as u32),
            None,
        )?;
        let layouts = vec![layout; frames];
        let sets = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )?;
        let size = STRIDE * u64::from(blocks);
        let mut out = Vec::with_capacity(frames);
        for set in sets {
            let (buffer, memory) = create_buffer_in(
                device,
                mem_props,
                size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                &MAPPED_FOR_GPU,
            )?;
            let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
            let infos: Vec<[vk::DescriptorBufferInfo; 1]> = (0..u64::from(blocks))
                .map(|i| {
                    [vk::DescriptorBufferInfo::default()
                        .buffer(buffer)
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
            device.update_descriptor_sets(&writes, &[]);
            let frame = Frame {
                buffer,
                memory,
                ptr,
                set,
            };
            frame.write(&GuiFrame {
                gui_size: (1.0, 1.0),
                screen_size: (1.0, 1.0),
                game_time: 0.0,
            });
            out.push(frame);
        }
        Ok(Self {
            layout,
            pool,
            frames: out,
        })
    }

    /// Writes frame `frame`'s blocks; call once its fence was waited on.
    pub unsafe fn write(&self, frame: usize, gui: &GuiFrame) {
        self.frames[frame].write(gui);
    }

    /// Binds `view` sampled by `sampler` as the item glint, every frame.
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
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(frame.set)
                    .dst_binding(GLINT_BINDING)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&info)],
                &[],
            );
        }
    }

    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for frame in self.frames.drain(..) {
            device.unmap_memory(frame.memory);
            device.destroy_buffer(frame.buffer, None);
            device.free_memory(frame.memory, None);
        }
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_descriptor_set_layout(self.layout, None);
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
        let globals = Globals {
            camera_block_pos: [0; 3],
            glint_alpha: 1.0,
            camera_offset: [crate::world::glint::millis(), 0.0, 0.0],
            game_time: gui.game_time,
            screen_size: [gui.screen_size.0, gui.screen_size.1],
            menu_blur_radius: 5,
            use_rgss: 0,
        };
        let projection = projection(gui.gui_size.0, gui.gui_size.1);
        self.ptr.cast::<DynamicTransforms>().write(transforms);
        self.ptr
            .add(STRIDE as usize)
            .cast::<[f32; 16]>()
            .write(projection);
        self.ptr
            .add(2 * STRIDE as usize)
            .cast::<Globals>()
            .write(globals);
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
