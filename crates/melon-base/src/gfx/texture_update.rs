//! Texture uploads recorded into the frame's command buffer: the first
//! contents of new textures and rewrites of parts of existing ones
//! (animated atlas sprites, icon pages). Nothing waits for the GPU; the
//! frame's fence guards the staging buffer.

use ash::vk;
use image::RgbaImage;

use super::staging::Staging;

/// A new texture's contents, copied in before anything samples it.
pub struct PendingInit {
    pub image: vk::Image,
    pub mip_levels: u32,
    pub layer_count: u32,
    /// `(layer, level, pixels)`, each covering its whole level.
    pub parts: Vec<(u32, u32, RgbaImage)>,
}

impl PendingInit {
    pub fn bytes(&self) -> u64 {
        self.parts
            .iter()
            .map(|(_, _, image)| image.as_raw().len() as u64)
            .sum()
    }
}

/// One queued region.
pub struct PendingUpdate {
    pub image: vk::Image,
    /// The mip level, with `x`/`y` in its pixels.
    pub level: u32,
    pub x: u32,
    pub y: u32,
    pub pixels: RgbaImage,
}

/// Staging bytes `inits` and `updates` need.
fn total(inits: &[PendingInit], updates: &[PendingUpdate]) -> u64 {
    inits.iter().map(PendingInit::bytes).sum::<u64>()
        + updates
            .iter()
            .map(|u| u.pixels.as_raw().len() as u64)
            .sum::<u64>()
}

/// Room in `staging` for `inits` and `updates`, before the frame acquires
/// an image.
///
/// # Safety
///
/// `device` must be live; no frame in flight may read `staging`.
pub unsafe fn prepare(
    staging: &mut Staging,
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    inits: &[PendingInit],
    updates: &[PendingUpdate],
) -> anyhow::Result<()> {
    let total = total(inits, updates);
    staging.trim(device, total, 1 << 20);
    if total == 0 {
        return Ok(());
    }
    staging.ensure(device, mem_props, total, 64 * 1024)
}

/// Copies `inits`, then `updates`, into `staging` and records their
/// uploads into `cmd`, ahead of the frame's draws.
///
/// # Safety
///
/// `device` must be live, `cmd` recording; no frame in flight may read `staging`.
pub unsafe fn record(
    staging: &mut Staging,
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    cmd: vk::CommandBuffer,
    inits: &[PendingInit],
    updates: &[PendingUpdate],
) -> anyhow::Result<()> {
    let total = total(inits, updates);
    if total == 0 {
        return Ok(());
    }
    staging.ensure(device, mem_props, total, 64 * 1024)?;
    let Some(mapped) = staging.mapped else {
        return Ok(());
    };
    let offset = record_inits(device, cmd, staging.buffer, mapped, 0, inits);
    record_updates(device, cmd, staging.buffer, mapped, offset, updates);
    Ok(())
}

/// Uploads `inits` at once in a command buffer of their own, waiting for
/// just that submit: a burst too big for the frame's staging buffer (the
/// block atlas while loading), and the post chains' inputs.
///
/// # Safety
///
/// `device` must be live, `command_pool` and `queue` on it, `queue` unused by other threads meanwhile.
pub unsafe fn submit_now(
    device: &ash::Device,
    mem_props: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    inits: &[PendingInit],
) -> anyhow::Result<()> {
    let total = total(inits, &[]);
    if total == 0 {
        return Ok(());
    }
    let mut staging = Staging::default();
    let mut cmd = vk::CommandBuffer::null();
    let mut fence = vk::Fence::null();
    let result = (|| -> anyhow::Result<()> {
        staging.ensure(device, mem_props, total, 0)?;
        let mapped = staging
            .mapped
            .ok_or_else(|| anyhow::anyhow!("staging memory not mapped"))?;
        cmd = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        record_inits(device, cmd, staging.buffer, mapped, 0, inits);
        device.end_command_buffer(cmd)?;
        fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        let cmds = [cmd];
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(&cmds)],
            fence,
        )?;
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        Ok(())
    })();
    if fence != vk::Fence::null() {
        device.destroy_fence(fence, None);
    }
    if cmd != vk::CommandBuffer::null() {
        device.free_command_buffers(command_pool, &[cmd]);
    }
    staging.destroy(device);
    result
}

/// Records `inits` with their pixels from `offset` in the staging buffer
/// on; returns where the next bytes go.
unsafe fn record_inits(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    buffer: vk::Buffer,
    mapped: std::ptr::NonNull<u8>,
    mut offset: u64,
    inits: &[PendingInit],
) -> u64 {
    if inits.is_empty() {
        return offset;
    }
    let barriers = |old, new, src, dst| -> Vec<vk::ImageMemoryBarrier> {
        inits
            .iter()
            .map(|init| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(old)
                    .new_layout(new)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(init.image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(init.mip_levels)
                            .layer_count(init.layer_count),
                    )
                    .src_access_mask(src)
                    .dst_access_mask(dst)
            })
            .collect()
    };
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers(
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
        ),
    );
    for init in inits {
        let regions: Vec<vk::BufferImageCopy> = init
            .parts
            .iter()
            .map(|(layer, level, image)| {
                let bytes = image.as_raw();
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    mapped.as_ptr().add(offset as usize),
                    bytes.len(),
                );
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: *level,
                        base_array_layer: *layer,
                        layer_count: 1,
                    })
                    .image_extent(vk::Extent3D {
                        width: image.width(),
                        height: image.height(),
                        depth: 1,
                    });
                offset += bytes.len() as u64;
                region
            })
            .collect();
        device.cmd_copy_buffer_to_image(
            cmd,
            buffer,
            init.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &regions,
        );
    }
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
        ),
    );
    offset
}

/// Records `updates` with their pixels from `offset` in the staging buffer on.
unsafe fn record_updates(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    buffer: vk::Buffer,
    mapped: std::ptr::NonNull<u8>,
    mut offset: u64,
    updates: &[PendingUpdate],
) {
    if updates.is_empty() {
        return;
    }
    let mut images: Vec<vk::Image> = Vec::new();
    for update in updates {
        if !images.contains(&update.image) {
            images.push(update.image);
        }
    }
    let barriers = |old, new, src, dst| -> Vec<vk::ImageMemoryBarrier> {
        images
            .iter()
            .map(|&image| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(old)
                    .new_layout(new)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .level_count(vk::REMAINING_MIP_LEVELS)
                            .layer_count(1),
                    )
                    .src_access_mask(src)
                    .dst_access_mask(dst)
            })
            .collect()
    };
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers(
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::SHADER_READ,
            vk::AccessFlags::TRANSFER_WRITE,
        ),
    );
    let mut regions: Vec<Vec<vk::BufferImageCopy>> = vec![Vec::new(); images.len()];
    for update in updates {
        let bytes = update.pixels.as_raw();
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            mapped.as_ptr().add(offset as usize),
            bytes.len(),
        );
        let (width, height) = update.pixels.dimensions();
        let image = images
            .iter()
            .position(|&i| i == update.image)
            .expect("collected above");
        regions[image].push(
            vk::BufferImageCopy::default()
                .buffer_offset(offset)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: update.level,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D {
                    x: update.x as i32,
                    y: update.y as i32,
                    z: 0,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                }),
        );
        offset += bytes.len() as u64;
    }
    for (&image, regions) in images.iter().zip(&regions) {
        device.cmd_copy_buffer_to_image(
            cmd,
            buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            regions,
        );
    }
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::FRAGMENT_SHADER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &barriers(
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
        ),
    );
}
