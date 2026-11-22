//! The descriptor pools every texture's set comes from: one pool, plus
//! overflow pools of twice the size while it is full, merged back into one
//! by [`DescriptorPools::compact`] where the frame may stall anyway (a
//! resource reload, leaving a world).
//!
//! A server pack with many custom item models can need more sets than the
//! first pool holds, growing keeps those uploads from failing, merging keeps
//! the sets in one pool instead of spread over a chain of them.

use std::cell::RefCell;
use std::collections::HashMap;

use ash::vk;

struct Pool {
    handle: vk::DescriptorPool,
    capacity: u32,
    live: u32,
}

#[derive(Default)]
struct Inner {
    pools: Vec<Pool>,
    /// Every live set and the index of its pool in `pools`.
    owner: HashMap<vk::DescriptorSet, usize>,
}

/// Sets of one layout with one combined image sampler at binding 0.
pub struct DescriptorPools {
    /// The first pool's size; every size is this times a power of two.
    base: u32,
    /// Behind a `RefCell` so the `&self` texture uploader can allocate.
    inner: RefCell<Inner>,
}

impl DescriptorPools {
    pub unsafe fn new(device: &ash::Device, base: u32) -> anyhow::Result<Self> {
        let handle = create_pool(device, base)?;
        Ok(Self {
            base,
            inner: RefCell::new(Inner {
                pools: vec![Pool {
                    handle,
                    capacity: base,
                    live: 0,
                }],
                owner: HashMap::new(),
            }),
        })
    }

    /// A new set from the newest pool with room, else from a new pool twice
    /// the size of the last one.
    pub unsafe fn allocate(
        &self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<vk::DescriptorSet> {
        let mut inner = self.inner.borrow_mut();
        for index in (0..inner.pools.len()).rev() {
            let pool = &inner.pools[index];
            if pool.live >= pool.capacity {
                continue;
            }
            match allocate_one(device, pool.handle, layout) {
                Ok(set) => return Ok(inner.record(set, index)),
                Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY | vk::Result::ERROR_FRAGMENTED_POOL) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let capacity = inner.pools.last().map_or(self.base, |p| p.capacity * 2);
        let handle = create_pool(device, capacity)?;
        inner.pools.push(Pool {
            handle,
            capacity,
            live: 0,
        });
        let total: u32 = inner.pools.iter().map(|p| p.capacity).sum();
        crate::info!("Texture descriptors: pool grown by {capacity}, {total} in all");
        let set = allocate_one(device, handle, layout)?;
        let index = inner.pools.len() - 1;
        Ok(inner.record(set, index))
    }

    /// Frees `set`, once no frame in flight uses it. Null and unknown sets
    /// (freed already) are ignored.
    pub unsafe fn free(&self, device: &ash::Device, set: vk::DescriptorSet) {
        let mut inner = self.inner.borrow_mut();
        let Some(index) = inner.owner.remove(&set) else {
            return;
        };
        let pool = &mut inner.pools[index];
        let _ = device.free_descriptor_sets(pool.handle, &[set]);
        pool.live -= 1;
    }

    /// Whether [`Self::compact`] would change anything: overflow pools
    /// exist, or the one pool is four times what its sets need.
    pub fn needs_compaction(&self) -> bool {
        let inner = self.inner.borrow();
        let target = self.target_capacity(inner.owner.len());
        inner.pools.len() > 1
            || inner
                .pools
                .first()
                .is_some_and(|p| p.capacity >= 4 * target)
    }

    /// The smallest `base · 2^k` holding `live` sets with a quarter to spare.
    fn target_capacity(&self, live: usize) -> u32 {
        let need = live + live / 4;
        let mut capacity = self.base;
        while (capacity as usize) < need {
            capacity *= 2;
        }
        capacity
    }

    /// Moves every live set into one new pool of [`Self::target_capacity`]
    /// and destroys the old pools. `holders` is handed a function that
    /// swaps an old handle for its copy, and must pass it every handle
    /// that is still drawn; handles it does not know (freed ones) it
    /// leaves alone.
    ///
    /// No frame may be in flight (wait for the device first).
    pub unsafe fn compact(
        &self,
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        holders: impl FnOnce(&mut dyn FnMut(&mut vk::DescriptorSet)),
    ) -> anyhow::Result<()> {
        let mut inner = self.inner.borrow_mut();
        let olds: Vec<vk::DescriptorSet> = inner.owner.keys().copied().collect();
        let capacity = self.target_capacity(olds.len());
        let handle = create_pool(device, capacity)?;
        let news = if olds.is_empty() {
            Vec::new()
        } else {
            let layouts = vec![layout; olds.len()];
            match device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(handle)
                    .set_layouts(&layouts),
            ) {
                Ok(news) => news,
                Err(e) => {
                    device.destroy_descriptor_pool(handle, None);
                    return Err(e.into());
                }
            }
        };
        let copies: Vec<vk::CopyDescriptorSet> = olds
            .iter()
            .zip(&news)
            .map(|(&src, &dst)| {
                vk::CopyDescriptorSet::default()
                    .src_set(src)
                    .src_binding(0)
                    .dst_set(dst)
                    .dst_binding(0)
                    .descriptor_count(1)
            })
            .collect();
        device.update_descriptor_sets(&[], &copies);

        let moved: HashMap<vk::DescriptorSet, vk::DescriptorSet> =
            olds.iter().copied().zip(news.iter().copied()).collect();
        holders(&mut |set: &mut vk::DescriptorSet| {
            if let Some(&new) = moved.get(set) {
                *set = new;
            }
        });

        for pool in inner.pools.drain(..) {
            device.destroy_descriptor_pool(pool.handle, None);
        }
        inner.pools.push(Pool {
            handle,
            capacity,
            live: news.len() as u32,
        });
        inner.owner = news.into_iter().map(|set| (set, 0)).collect();
        Ok(())
    }

    /// The pools' sizes, oldest first.
    pub fn capacities(&self) -> Vec<u32> {
        self.inner
            .borrow()
            .pools
            .iter()
            .map(|p| p.capacity)
            .collect()
    }

    pub unsafe fn destroy(&self, device: &ash::Device) {
        let mut inner = self.inner.borrow_mut();
        for pool in inner.pools.drain(..) {
            device.destroy_descriptor_pool(pool.handle, None);
        }
        inner.owner.clear();
    }
}

impl Inner {
    fn record(&mut self, set: vk::DescriptorSet, index: usize) -> vk::DescriptorSet {
        self.pools[index].live += 1;
        self.owner.insert(set, index);
        set
    }
}

unsafe fn create_pool(device: &ash::Device, capacity: u32) -> anyhow::Result<vk::DescriptorPool> {
    let sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        descriptor_count: capacity,
    }];
    Ok(device.create_descriptor_pool(
        &vk::DescriptorPoolCreateInfo::default()
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
            .pool_sizes(&sizes)
            .max_sets(capacity),
        None,
    )?)
}

unsafe fn allocate_one(
    device: &ash::Device,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> Result<vk::DescriptorSet, vk::Result> {
    let layouts = [layout];
    device
        .allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(pool)
                .set_layouts(&layouts),
        )
        .map(|sets| sets[0])
}
