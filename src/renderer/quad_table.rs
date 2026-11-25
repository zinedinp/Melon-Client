//! Every baked block quad's four corners, in one storage buffer the terrain shader reads.

use ash::vk;

use super::{QuadCorner, Renderer, WorldVertex, create_buffer, mesh_arena, vk_util};

/// The table's buffer and the descriptor set binding it (set 1 of the terrain pipelines).
pub(super) struct QuadTable {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    pool: vk::DescriptorPool,
    pub set: vk::DescriptorSet,
}

impl QuadTable {
    pub unsafe fn destroy(self, device: &ash::Device) {
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_buffer(self.buffer, None);
        vk_util::free_memory(device, self.memory);
    }
}

/// The descriptor set layout of the table.
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

impl Renderer {
    /// Uploads the baked quads' corners, four per quad in index order.
    pub fn set_quad_table(&mut self, corners: &[WorldVertex]) -> anyhow::Result<()> {
        unsafe {
            if let Some(old) = self.quad_table.take() {
                self.device.device_wait_idle()?;
                // A table replaced before a frame recorded its upload.
                self.quad_meshes.cancel_copies(old.buffer);
                old.destroy(&self.device);
            }
            let corners: Vec<QuadCorner> = corners.iter().map(|&v| v.into()).collect();
            let bytes = mesh_arena::as_bytes(&corners);
            let size = (bytes.len() as u64).max(std::mem::size_of::<QuadCorner>() as u64);
            let (buffer, memory) = create_buffer(
                &self.device,
                &self.mem_props,
                size,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            let pool_sizes = [vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            }];
            let pool = match self.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&pool_sizes)
                    .max_sets(1),
                None,
            ) {
                Ok(pool) => pool,
                Err(e) => {
                    self.device.destroy_buffer(buffer, None);
                    vk_util::free_memory(&self.device, memory);
                    return Err(e.into());
                }
            };
            let layouts = [self.pipelines.layouts.table];
            let set = match self.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(&layouts),
            ) {
                Ok(sets) => sets[0],
                Err(e) => {
                    QuadTable {
                        buffer,
                        memory,
                        pool,
                        set: vk::DescriptorSet::null(),
                    }
                    .destroy(&self.device);
                    return Err(e.into());
                }
            };
            let buffer_info = [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)];
            self.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&buffer_info)],
                &[],
            );
            self.quad_meshes.queue_copy(buffer, 0, bytes);
            self.quad_table = Some(QuadTable {
                buffer,
                memory,
                pool,
                set,
            });
        }
        Ok(())
    }
}

/// Most terrain mesh buffers there can be sets for.
const MAX_QUAD_BUFFERS: u32 = 256;

/// A descriptor set per terrain mesh buffer, binding it as storage (set 4).
pub(super) struct QuadSets {
    pool: vk::DescriptorPool,
    sets: Vec<(vk::Buffer, vk::DescriptorSet)>,
}

impl QuadSets {
    pub unsafe fn new(device: &ash::Device) -> anyhow::Result<Self> {
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: MAX_QUAD_BUFFERS,
        }];
        let pool = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                .pool_sizes(&sizes)
                .max_sets(MAX_QUAD_BUFFERS),
            None,
        )?;
        Ok(Self {
            pool,
            sets: Vec::new(),
        })
    }

    /// Makes a set for each of `buffers` that has none yet.
    pub unsafe fn sync(
        &mut self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        buffers: impl Iterator<Item = vk::Buffer>,
    ) -> anyhow::Result<()> {
        for buffer in buffers {
            if self.get(buffer).is_some() {
                continue;
            }
            let set = device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&[layout]),
            )?[0];
            let info = [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)];
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&info)],
                &[],
            );
            self.sets.push((buffer, set));
        }
        Ok(())
    }

    /// The set binding `buffer`.
    pub fn get(&self, buffer: vk::Buffer) -> Option<vk::DescriptorSet> {
        self.sets
            .iter()
            .find(|&&(b, _)| b == buffer)
            .map(|&(_, set)| set)
    }

    /// Frees the set of a buffer that was destroyed: a new buffer may get
    /// its handle. No frame in flight may still bind it.
    pub unsafe fn forget(&mut self, device: &ash::Device, buffer: vk::Buffer) {
        if let Some(i) = self.sets.iter().position(|&(b, _)| b == buffer) {
            let (_, set) = self.sets.swap_remove(i);
            if let Err(e) = device.free_descriptor_sets(self.pool, &[set]) {
                crate::warn!("Mesh descriptor not freed: {e}");
            }
        }
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        device.destroy_descriptor_pool(self.pool, None);
    }
}
