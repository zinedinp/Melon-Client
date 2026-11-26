//! A one-off copy of a finished frame to the CPU: the world's `icon.png`

use super::Renderer;
use super::vk_util::Place;
use ash::vk;
use melon_base::gfx::mapped::MappedBuffer;

#[derive(Default)]
pub(super) struct Capture {
    requested: bool,
    image: Option<image::RgbaImage>,
}

/// The host buffer one frame was copied to.
pub(super) struct Readback {
    buffer: MappedBuffer,
    /// The swapchain's size and byte order when it was copied: a resize may come before the read.
    extent: vk::Extent2D,
    bgra: bool,
}

const COLOR: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};

impl Renderer {
    /// Copies the next frame out.
    pub fn request_capture(&mut self) {
        self.capture.requested = true;
    }

    /// The frame [`Self::request_capture`] asked for, once it was drawn.
    pub fn take_capture(&mut self) -> Option<image::RgbaImage> {
        self.capture.image.take()
    }

    /// Whether this frame is to be copied.
    pub(super) fn capturing(&mut self) -> bool {
        if !std::mem::take(&mut self.capture.requested) {
            return false;
        }
        self.swapchain.can_copy
            && matches!(
                self.swapchain.format,
                vk::Format::B8G8R8A8_UNORM
                    | vk::Format::B8G8R8A8_SRGB
                    | vk::Format::R8G8B8A8_UNORM
                    | vk::Format::R8G8B8A8_SRGB
            )
    }

    /// Records the copy of the swapchain image (`PRESENT_SRC` on entry and exit).
    pub(super) unsafe fn record_capture(
        &self,
        cmd: vk::CommandBuffer,
        image_index: usize,
    ) -> Option<Readback> {
        let device = &self.device;
        let extent = self.swapchain.extent;
        let size = u64::from(extent.width) * u64::from(extent.height) * 4;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let buffer = unsafe {
            MappedBuffer::new(
                device,
                &self.mem_props,
                size,
                vk::BufferUsageFlags::TRANSFER_DST,
                Place::READBACK,
            )
        }
        .ok()?;
        let image = self.swapchain.images[image_index];
        let barrier = |old, new, src, dst| {
            vk::ImageMemoryBarrier::default()
                .image(image)
                .old_layout(old)
                .new_layout(new)
                .src_access_mask(src)
                .dst_access_mask(dst)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .subresource_range(COLOR)
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::MEMORY_WRITE,
                    vk::AccessFlags::TRANSFER_READ,
                )],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer.buffer,
                &[vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D::default(),
                    image_extent: vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    },
                }],
            )
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier(
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::PRESENT_SRC_KHR,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::AccessFlags::empty(),
                )],
            )
        };
        let bgra = matches!(
            self.swapchain.format,
            vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
        );
        Some(Readback {
            buffer,
            extent,
            bgra,
        })
    }

    /// Reads the copy once the GPU finished it.
    pub(super) unsafe fn finish_capture(&mut self, readback: Readback) {
        let Readback {
            buffer,
            extent,
            bgra,
        } = readback;
        // SAFETY: the copy's frame fence was waited on, so the GPU is done writing.
        let mut pixels = unsafe { buffer.read(0, buffer.size()) };
        for pixel in pixels.as_chunks_mut::<4>().0 {
            if bgra {
                pixel.swap(0, 2);
            }
            pixel[3] = 255;
        }
        self.capture.image = image::RgbaImage::from_raw(extent.width, extent.height, pixels);
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { buffer.destroy(&self.device) };
    }

    /// Frees the copy's buffer, read or not: no frame in flight may still write it.
    pub(super) unsafe fn drop_capture(&self, readback: Option<Readback>) {
        if let Some(readback) = readback {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { readback.buffer.destroy(&self.device) };
        }
    }
}
