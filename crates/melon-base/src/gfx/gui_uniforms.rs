//! The uniform blocks vanilla's GUI shaders read (`DynamicTransforms`, `Projection`).

use ash::vk;
use image::RgbaImage;
use majestix_vk::{
    Binding, Descriptor, DescriptorSet, DescriptorSetLayout, Frames, Image, ImageDesc, ImageView,
    Mapped, MemoryUse, Queue, Sampler, Simple as _, Slot, ViewDesc, Write,
};
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

/// Binding of the item-glint sampler in set 1.
pub const GLINT_BINDING: u32 = BLOCKS.len() as u32;

/// The blocks' stages.
const BLOCK_STAGES: vk::ShaderStageFlags = vk::ShaderStageFlags::from_raw(
    vk::ShaderStageFlags::VERTEX.as_raw() | vk::ShaderStageFlags::FRAGMENT.as_raw(),
);

/// The set's bindings: the blocks, then the glint.
const BINDINGS: [Binding; 4] = [
    block_binding(0),
    block_binding(1),
    block_binding(2),
    Binding {
        binding: GLINT_BINDING,
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        count: 1,
        stages: vk::ShaderStageFlags::FRAGMENT,
    },
];
const _: () = assert!(BLOCKS.len() == 3);

const fn block_binding(binding: u32) -> Binding {
    Binding {
        binding,
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        count: 1,
        stages: BLOCK_STAGES,
    }
}

/// The set layout, made apart from a layer: the headless checks build their
/// pipelines against it.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn create_layout(device: &ash::Device) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings = BINDINGS.map(|b| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(b.binding)
            .descriptor_type(b.ty)
            .descriptor_count(b.count)
            .stage_flags(b.stages)
    });
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

/// Each frame slot's blocks in one ring, and a set per slot naming them and
/// the glint.
pub struct GuiUniforms {
    /// majestix-vk's part; given up in [`Self::destroy`], before the device goes.
    own: Option<Own>,
}

struct Own {
    device: majestix_vk::Device,
    layout: DescriptorSetLayout,
    ring: Mapped<u8>,
    /// The ring's slots, by index.
    slots: Vec<Slot>,
    /// Linear and repeating, as the glint scrolls.
    sampler: Sampler,
    /// The glint's own copy and its view.
    glint: (Image, ImageView),
    /// By slot index.
    sets: Vec<DescriptorSet>,
}

impl GuiUniforms {
    /// The blocks of every slot of `frames` in one ring, each slot with its
    /// set, `glint` uploaded on `queue` as the item glint.
    pub fn new(
        layer: &majestix_vk::Device,
        queue: &Queue,
        frames: &Frames,
        glint: &RgbaImage,
    ) -> anyhow::Result<Self> {
        let layout = layer.create_descriptor_set_layout(&BINDINGS)?;
        let made = Mapped::<u8>::new(
            layer,
            frames,
            (STRIDE * BLOCKS.len() as u64) as usize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryUse::Upload,
        )
        .and_then(|ring| Ok((ring, layer.create_sampler(&sampler_info())?)));
        let (ring, sampler) = match made {
            Ok(made) => made,
            Err(e) => {
                layer.retire(layout);
                return Err(e.into());
            }
        };
        let slots: Vec<Slot> = frames.slots().collect();
        let made = upload(layer, queue, glint).and_then(|glint| {
            match sets(layer, layout, &ring, &slots, glint.1, sampler) {
                Ok(sets) => Ok((glint, sets)),
                Err(e) => {
                    retire_glint(layer, glint);
                    Err(e)
                }
            }
        });
        let (glint, sets) = match made {
            Ok(made) => made,
            Err(e) => {
                layer.retire(sampler);
                layer.retire(layout);
                return Err(e);
            }
        };
        Ok(Self {
            own: Some(Own {
                device: layer.clone(),
                layout,
                ring,
                slots,
                sampler,
                glint,
                sets,
            }),
        })
    }

    /// Set 1 of the GUI pipelines.
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.own().layout.raw()
    }

    /// Writes the frame's blocks.
    pub fn write(&mut self, frame: &majestix_vk::Frame<'_>, gui: &GuiFrame) {
        let own = self
            .own
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
        own.ring.write(frame, 0, &transforms);
        own.ring
            .write(frame, stride, &projection(gui.gui_size.0, gui.gui_size.1));
        own.ring.write(frame, 2 * stride, &globals);
    }

    /// Uploads `glint` on `queue` as the item glint, in new sets; the old
    /// ones and the old glint are retired, so frames in flight keep theirs.
    pub fn bind_glint(&mut self, queue: &Queue, glint: &RgbaImage) -> anyhow::Result<()> {
        let own = self.own.as_mut().expect("gui uniforms bound after destroy");
        let device = &own.device;
        let new = upload(device, queue, glint)?;
        let sets = match sets(
            device,
            own.layout,
            &own.ring,
            &own.slots,
            new.1,
            own.sampler,
        ) {
            Ok(sets) => sets,
            Err(e) => {
                retire_glint(device, new);
                return Err(e);
            }
        };
        for set in std::mem::replace(&mut own.sets, sets) {
            device.retire(set);
        }
        retire_glint(device, std::mem::replace(&mut own.glint, new));
        Ok(())
    }

    /// Frame slot `slot`'s set.
    pub fn set(&self, slot: usize) -> vk::DescriptorSet {
        self.own().sets[slot].raw()
    }

    /// Retires the ring, the sets, the glint, the sampler and the layout.
    pub fn destroy(&mut self) {
        if let Some(own) = self.own.take() {
            for set in own.sets {
                own.device.retire(set);
            }
            retire_glint(&own.device, own.glint);
            own.device.retire(own.sampler);
            own.device.retire(own.layout);
        }
    }

    fn own(&self) -> &Own {
        self.own.as_ref().expect("gui uniforms used after destroy")
    }
}

/// Linear and repeating, one level: what the glint is drawn with.
fn sampler_info() -> vk::SamplerCreateInfo<'static> {
    vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .max_lod(0.0)
}

/// `pixels` in an image of its own, ready to sample, and its view.
fn upload(
    device: &majestix_vk::Device,
    queue: &Queue,
    pixels: &RgbaImage,
) -> anyhow::Result<(Image, ImageView)> {
    let (width, height) = pixels.dimensions();
    let image = device.create_image(&ImageDesc::d2(
        vk::Format::R8G8B8A8_UNORM,
        width,
        height,
        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
    ))?;
    let region = vk::BufferImageCopy::default()
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    let view = device
        .upload_image(
            queue,
            image,
            pixels.as_raw(),
            &[region],
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )
        .and_then(|()| device.create_image_view(&ViewDesc::d2(image)));
    match view {
        Ok(view) => Ok((image, view)),
        Err(e) => {
            device.retire(image);
            Err(e.into())
        }
    }
}

/// A set per slot: its range of `ring` and the glint.
fn sets(
    device: &majestix_vk::Device,
    layout: DescriptorSetLayout,
    ring: &Mapped<u8>,
    slots: &[Slot],
    glint: ImageView,
    sampler: Sampler,
) -> anyhow::Result<Vec<DescriptorSet>> {
    let mut sets = Vec::with_capacity(slots.len());
    for &slot in slots {
        let start = ring.offset(slot);
        let block = |i: u32| {
            Write::new(
                i,
                Descriptor::Buffer {
                    buffer: ring.buffer(),
                    offset: start + u64::from(i) * STRIDE,
                    range: STRIDE,
                },
            )
        };
        let writes = [
            block(0),
            block(1),
            block(2),
            Write::new(
                GLINT_BINDING,
                Descriptor::Sampled {
                    view: glint,
                    sampler,
                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                },
            ),
        ];
        match device.create_descriptor_set(layout, &writes) {
            Ok(set) => sets.push(set),
            Err(e) => {
                for set in sets {
                    device.retire(set);
                }
                return Err(e.into());
            }
        }
    }
    Ok(sets)
}

/// Retires a glint's view, then its image.
fn retire_glint(device: &majestix_vk::Device, (image, view): (Image, ImageView)) {
    device.retire(view);
    device.retire(image);
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
