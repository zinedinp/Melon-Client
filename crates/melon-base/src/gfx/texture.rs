//! GPU textures: 2D (UI sprites, font pages) and cubemaps (panorama).

use ash::vk;
use image::RgbaImage;

use super::descriptor_pools::DescriptorPools;
use super::find_memory_type;
use super::texture_update::PendingInit;

#[derive(Clone, Copy)]
pub struct Texture {
    pub image: vk::Image,
    pub memory: vk::DeviceMemory,
    pub view: vk::ImageView,
    pub descriptor_set: vk::DescriptorSet,
    /// 1 plus the smaller levels below the full-size image.
    pub mip_levels: u32,
    /// The handles belong to another slot (a freed slot showing the white texture).
    pub shared: bool,
}

/// Vulkan handles every texture upload needs, bundled so callers pass one value.
#[derive(Clone, Copy)]
pub struct Uploader<'a> {
    pub device: &'a ash::Device,
    pub mem_props: &'a vk::PhysicalDeviceMemoryProperties,
    pub descriptor_pools: &'a DescriptorPools,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub sampler: vk::Sampler,
}

impl Uploader<'_> {
    /// A 2D texture of `rgba`, uploaded by the next frame.
    ///
    /// # Safety
    ///
    /// The uploader's handles must be live and from one device.
    pub unsafe fn image_2d(&self, rgba: &RgbaImage) -> anyhow::Result<(Texture, PendingInit)> {
        self.upload(std::slice::from_ref(rgba), &[], vk::ImageViewType::TYPE_2D)
    }

    /// A 2D texture with its smaller levels, each half the size of the one before.
    ///
    /// # Safety
    ///
    /// The uploader's handles must be live and from one device.
    pub unsafe fn image_2d_mipmapped(
        &self,
        rgba: &RgbaImage,
        mips: &[RgbaImage],
    ) -> anyhow::Result<(Texture, PendingInit)> {
        self.upload(std::slice::from_ref(rgba), mips, vk::ImageViewType::TYPE_2D)
    }

    /// Six equal-size faces in Vulkan cubemap order (+X, -X, +Y, -Y, +Z, -Z).
    ///
    /// # Safety
    ///
    /// The uploader's handles must be live and from one device.
    pub unsafe fn cubemap(&self, faces: &[RgbaImage; 6]) -> anyhow::Result<(Texture, PendingInit)> {
        self.upload(faces, &[], vk::ImageViewType::CUBE)
    }

    /// A device-local `R8G8B8A8_UNORM` image, its view and descriptor set,
    /// ready to bind; its pixels are the [`PendingInit`], to record before
    /// the first draw that samples it (nothing here waits for the GPU).
    unsafe fn upload(
        &self,
        layers: &[RgbaImage],
        mips: &[RgbaImage],
        view_type: vk::ImageViewType,
    ) -> anyhow::Result<(Texture, PendingInit)> {
        let device = self.device;
        let first = layers
            .first()
            .ok_or_else(|| anyhow::anyhow!("Textur-Upload ohne Bilddaten"))?;
        let (width, height) = (first.width(), first.height());
        if layers
            .iter()
            .any(|l| l.width() != width || l.height() != height)
        {
            anyhow::bail!("Textur-Layer haben unterschiedliche Groessen");
        }
        if !mips.is_empty() && layers.len() != 1 {
            anyhow::bail!("Mipmaps nur für einzelne Texturen");
        }
        for (level, mip) in mips.iter().enumerate() {
            let shift = level + 1;
            if mip.dimensions() != ((width >> shift).max(1), (height >> shift).max(1)) {
                anyhow::bail!("Mipmap-Stufe {shift} hat die falsche Größe");
            }
        }
        let parts: Vec<(u32, u32, RgbaImage)> = layers
            .iter()
            .enumerate()
            .map(|(layer, image)| (layer as u32, 0, image.clone()))
            .chain(
                mips.iter()
                    .enumerate()
                    .map(|(level, image)| (0, level as u32 + 1, image.clone())),
            )
            .collect();

        // Everything made on the way is freed again when a later step
        // fails (the descriptor pool full, the VRAM taken by another
        // program): an upload that fails must not leave its image behind.
        let mut made = Made::default();
        let result = self.upload_parts(&parts, (width, height), view_type, &mut made);
        if result.is_err() {
            self.descriptor_pools.free(device, made.set);
            if made.view != vk::ImageView::null() {
                device.destroy_image_view(made.view, None);
            }
            if made.image != vk::Image::null() {
                device.destroy_image(made.image, None);
            }
            if made.memory != vk::DeviceMemory::null() {
                device.free_memory(made.memory, None);
            }
        }
        let texture = result?;
        Ok((
            texture,
            PendingInit {
                image: texture.image,
                mip_levels: texture.mip_levels,
                layer_count: layers.len() as u32,
                parts,
            },
        ))
    }

    /// [`Self::upload`]'s work; what it creates goes into `made` as soon as
    /// it exists, for the caller to free.
    unsafe fn upload_parts(
        &self,
        parts: &[(u32, u32, RgbaImage)],
        (width, height): (u32, u32),
        view_type: vk::ImageViewType,
        made: &mut Made,
    ) -> anyhow::Result<Texture> {
        let device = self.device;
        let layer_count = parts.iter().filter(|&&(_, level, _)| level == 0).count() as u32;
        let mip_levels = 1 + parts.iter().map(|&(_, level, _)| level).max().unwrap_or(0);
        let mut image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(layer_count)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        if view_type == vk::ImageViewType::CUBE {
            image_info = image_info.flags(vk::ImageCreateFlags::CUBE_COMPATIBLE);
        }
        let image = device.create_image(&image_info, None)?;
        made.image = image;
        let image_req = device.get_image_memory_requirements(image);
        let memory = device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(image_req.size)
                .memory_type_index(find_memory_type(
                    self.mem_props,
                    image_req.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )?),
            None,
        )?;
        made.memory = memory;
        device.bind_image_memory(image, memory, 0)?;

        let subresource = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .base_mip_level(0)
            .level_count(mip_levels)
            .base_array_layer(0)
            .layer_count(layer_count);

        let view = device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(view_type)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(subresource),
            None,
        )?;
        made.view = view;

        let descriptor_set = self
            .descriptor_pools
            .allocate(device, self.descriptor_set_layout)?;
        made.set = descriptor_set;

        let descriptor_image = [vk::DescriptorImageInfo::default()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(view)
            .sampler(self.sampler)];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&descriptor_image);
        device.update_descriptor_sets(&[write], &[]);

        Ok(Texture {
            image,
            memory,
            view,
            descriptor_set,
            mip_levels,
            shared: false,
        })
    }
}

/// What an upload has created so far (null: not yet).
#[derive(Default)]
struct Made {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    set: vk::DescriptorSet,
}
