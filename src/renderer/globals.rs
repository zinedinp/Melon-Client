//! A world scene's constants for the whole frame, in a uniform buffer the world shaders read.

use ash::vk;
use glam::Mat4;

use super::frame::WorldDraw;
use super::vk_util::Place;
use crate::world::fog::FogData;
use melon_base::gfx::mapped::MappedBuffer;
use numerobis::elem::Pod;

/// `Globals` in `globals.glsl`, std140: ten `vec4`s.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Globals {
    /// The fog's colour; alpha 0 without fog.
    pub fog_color: [f32; 4],
    /// Environmental start and end, render distance start and end.
    pub fog_env: [f32; 4],
    /// The sky's end, the clouds' end, the aspect ratio.
    pub fog_sky: [f32; 4],
    pub ambient_color: [f32; 4],
    pub block_tint: [f32; 4],
    pub cardinal_a: [f32; 4],
    pub cardinal_b: [f32; 4],
    /// The world's up in view space.
    pub view_up: [f32; 4],
    /// x: `Util.getMillis() * glintSpeed * 8`, y: `GlintAlpha`.
    pub glint: [f32; 4],
    /// xy: the size of the frame in pixels (vanilla `ScreenSize`).
    pub screen: [f32; 4],
}

const _: () = assert!(std::mem::size_of::<Globals>() == 160);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl Pod for Globals {}

impl Globals {
    /// No fog and the overworld's light: the title screen, menus.
    pub fn plain() -> Self {
        Self::of(
            &FogData::NONE,
            [0.0; 3],
            [1.0, 0.847, 0.549],
            [1.0; 6],
            1.0,
            Mat4::IDENTITY,
        )
    }

    /// The globals of a world frame.
    pub fn for_world(world: &WorldDraw, aspect: f32) -> Self {
        Self::of(
            &world.fog,
            world.lightmap.ambient,
            world.lightmap.block_tint,
            world.cardinal,
            aspect,
            world.view,
        )
    }

    fn of(
        fog: &FogData,
        ambient: [f32; 3],
        block_tint: [f32; 3],
        cardinal: [f32; 6],
        aspect: f32,
        view: Mat4,
    ) -> Self {
        let on = fog.color[3];
        let up = view.y_axis.truncate();
        let [down, top, north, south, west, east] = cardinal;
        Self {
            fog_color: [fog.color[0], fog.color[1], fog.color[2], on],
            fog_env: [
                fog.environmental_start,
                fog.environmental_end,
                fog.render_distance_start,
                fog.render_distance_end,
            ],
            fog_sky: [fog.sky_end, fog.cloud_end, aspect, 0.0],
            ambient_color: [ambient[0], ambient[1], ambient[2], 0.0],
            block_tint: [block_tint[0], block_tint[1], block_tint[2], 0.0],
            cardinal_a: [down, top, north, south],
            cardinal_b: [west, east, 1.0, 1.0],
            view_up: [up.x, up.y, up.z, 0.0],
            glint: [crate::world::glint::millis(), 1.0, 0.0, 0.0],
            screen: [1.0, 1.0, 0.0, 0.0],
        }
    }
}

/// The projection's `1 / m00` and `1 / m11`, which `world.vert` uses for its view rays.
pub fn inverse_scale(projection: &Mat4) -> [f32; 2] {
    [1.0 / projection.x_axis.x, 1.0 / projection.y_axis.y]
}

/// World up in view space, for tests.
#[cfg(test)]
pub fn up_in_view(view: &Mat4) -> glam::Vec3 {
    view.y_axis.truncate()
}

/// One buffer per frame in flight and the set binding it.
pub(super) struct GlobalBuffers {
    pub layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    frames: Vec<Frame>,
}

struct Frame {
    buffer: MappedBuffer,
    set: vk::DescriptorSet,
}

/// The set layout: one uniform buffer, for both shader stages.
pub(super) unsafe fn create_layout(
    device: &ash::Device,
) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

impl GlobalBuffers {
    pub unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        layout: vk::DescriptorSetLayout,
        frames: usize,
    ) -> anyhow::Result<Self> {
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: frames as u32,
        }];
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
        let size = std::mem::size_of::<Globals>() as u64;
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
            // SAFETY: the buffer is new, so no submit names it yet.
            unsafe { buffer.write_one(0, &Globals::plain()) };
            let info = [vk::DescriptorBufferInfo::default()
                .buffer(buffer.buffer)
                .offset(0)
                .range(size)];
            // SAFETY: `device` is live, and the infos name only its own live handles.
            unsafe {
                device.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(&info)],
                    &[],
                )
            };
            out.push(Frame { buffer, set });
        }
        Ok(Self {
            layout,
            pool,
            frames: out,
        })
    }

    /// Writes frame `frame`'s globals; call once its fence was waited on.
    pub unsafe fn write(&self, frame: usize, globals: &Globals) {
        // SAFETY: the caller waited on this frame's fence (this fn's contract), so the GPU doesn't read it.
        unsafe { self.frames[frame].buffer.write_one(0, globals) };
    }

    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::angle::Rotation;

    #[test]
    fn the_blocks_size_is_ten_vec4s() {
        assert_eq!(std::mem::size_of::<Globals>(), 10 * 16);
    }

    #[test]
    fn no_fog_clears_the_fog_alpha() {
        assert_eq!(Globals::plain().fog_color[3], 0.0);
        let mut fog = FogData::NONE;
        fog.color = [0.2, 0.3, 0.4, 1.0];
        let globals = Globals::of(&fog, [0.0; 3], [1.0; 3], [1.0; 6], 1.0, Mat4::IDENTITY);
        assert_eq!(globals.fog_color, [0.2, 0.3, 0.4, 1.0]);
    }

    #[test]
    fn up_is_a_unit_vector_and_the_scales_undo_the_projection() {
        let view = Mat4::rot_x(-0.7) * Mat4::rot_y(2.0);
        assert!((up_in_view(&view).length() - 1.0).abs() < 1e-6);
        let projection = crate::renderer::world_projection(90.0, 2.0, 100.0);
        let [x, y] = inverse_scale(&projection);
        assert!((x - 2.0).abs() < 1e-5, "{x}");
        assert!((y.abs() - 1.0).abs() < 1e-5, "{y}");
    }
}
