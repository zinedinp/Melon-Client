//! The terrain's quad draws as indirect multi-draws.

use ash::vk;

use super::vk_util::{MAPPED_FOR_GPU, create_buffer_in};

/// Bytes of one `VkDrawIndexedIndirectCommand`.
pub const COMMAND_SIZE: u32 = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32;
/// Bytes of one section offset: a `vec4`.
const OFFSET_SIZE: u32 = 16;
/// Draws a frame's buffers hold at first.
const MIN_DRAWS: u32 = 4096;

/// A host-visible, mapped buffer.
#[derive(Default)]
struct Mapped {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    ptr: Option<std::ptr::NonNull<u8>>,
}

impl Mapped {
    unsafe fn new(
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> anyhow::Result<Self> {
        let (buffer, memory) = create_buffer_in(device, mem_props, size, usage, &MAPPED_FOR_GPU)?;
        let ptr = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
        Ok(Self {
            buffer,
            memory,
            ptr: std::ptr::NonNull::new(ptr),
        })
    }

    unsafe fn destroy(&mut self, device: &ash::Device) {
        if self.buffer != vk::Buffer::null() {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
        *self = Self::default();
    }
}

/// One frame in flight's buffers and the descriptor set binding them.
#[derive(Default)]
struct FrameDraws {
    commands: Mapped,
    offsets: Mapped,
    capacity: u32,
    set: vk::DescriptorSet,
}

pub(super) struct DrawBuffers {
    pool: vk::DescriptorPool,
    frames: Vec<FrameDraws>,
}

/// The set layout of the offsets: one storage buffer for the vertex stage.
pub(super) unsafe fn create_layout(
    device: &ash::Device,
) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX)];
    Ok(device.create_descriptor_set_layout(
        &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
        None,
    )?)
}

impl DrawBuffers {
    pub unsafe fn new(
        device: &ash::Device,
        frames: usize,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Self> {
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: frames as u32,
        }];
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
        Ok(Self {
            pool,
            frames: sets
                .into_iter()
                .map(|set| FrameDraws {
                    set,
                    ..FrameDraws::default()
                })
                .collect(),
        })
    }

    /// Room for `draws` draws in frame slot `frame`; call once its fence signalled.
    pub unsafe fn prepare(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        frame: usize,
        draws: u32,
    ) -> anyhow::Result<()> {
        let slot = &mut self.frames[frame];
        if draws <= slot.capacity {
            return Ok(());
        }
        let capacity = draws.next_power_of_two().max(MIN_DRAWS);
        slot.commands.destroy(device);
        slot.offsets.destroy(device);
        slot.capacity = 0;
        slot.commands = Mapped::new(
            device,
            mem_props,
            u64::from(capacity) * u64::from(COMMAND_SIZE),
            vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?;
        slot.offsets = Mapped::new(
            device,
            mem_props,
            u64::from(capacity) * u64::from(OFFSET_SIZE),
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?;
        slot.capacity = capacity;
        let info = [vk::DescriptorBufferInfo::default()
            .buffer(slot.offsets.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)];
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(slot.set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&info)],
            &[],
        );
        Ok(())
    }

    /// The set binding frame `frame`'s offsets.
    pub fn set(&self, frame: usize) -> vk::DescriptorSet {
        self.frames[frame].set
    }

    /// How many draws frame `frame`'s buffers hold.
    pub fn capacity(&self, frame: usize) -> u32 {
        self.frames[frame].capacity
    }

    /// The commands buffer of frame `frame`, for `cmd_draw_indexed_indirect`.
    pub fn commands(&self, frame: usize) -> vk::Buffer {
        self.frames[frame].commands.buffer
    }

    /// Writes draw number `index` of frame `frame`. `index` stays below the
    /// TODO(perf): 20-byte commands written one by one into mapped memory
    /// (~12% of `record_scene`). Collect them in a CPU `Vec` and copy once at
    /// the end, in whole lines: measure against this.
    pub unsafe fn write(
        &self,
        frame: usize,
        index: u32,
        command: vk::DrawIndexedIndirectCommand,
        offset: [f32; 3],
    ) {
        let slot = &self.frames[frame];
        debug_assert!(index < slot.capacity, "draw {index} over {}", slot.capacity);
        let (Some(commands), Some(offsets)) = (slot.commands.ptr, slot.offsets.ptr) else {
            return;
        };
        commands
            .as_ptr()
            .add(index as usize * COMMAND_SIZE as usize)
            .cast::<vk::DrawIndexedIndirectCommand>()
            .write_unaligned(command);
        offsets
            .as_ptr()
            .cast::<[f32; 4]>()
            .add(index as usize)
            .write([offset[0], offset[1], offset[2], 0.0]);
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        for slot in &mut self.frames {
            slot.commands.destroy(device);
            slot.offsets.destroy(device);
        }
        device.destroy_descriptor_pool(self.pool, None);
    }
}
