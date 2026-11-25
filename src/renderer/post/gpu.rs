//! The GPU side of post chains.

use std::cell::Cell;
use std::collections::HashMap;

use ash::vk;
use image::RgbaImage;

use super::config::{Config, ENTITY_OUTLINE, Input, MAIN};
use super::shader::Compiled;
use super::uniform::Std140;
use crate::renderer::blur::{Image, create_image};
use crate::renderer::texture::Texture;
use crate::renderer::vk_util::{self, MAPPED_FOR_GPU, create_buffer_in};
use crate::renderer::{pipeline, post::config::Pass};

/// Most inputs a pass may have, and uniform blocks.
const MAX_INPUTS: u32 = 16;
const MAX_BLOCKS: u32 = 8;
/// The room each block has in a pass's uniform buffer.
const BLOCK_STRIDE: u64 = 256;
/// Sets a pool holds, one pair per pass.
const POOL_SETS: u32 = 512;

/// A chain's texture with the size its JSON gives it.
pub(in crate::renderer) type SizedTexture = (Texture, u32, u32);

/// What a pass samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::renderer) enum Source {
    /// A copy of the swapchain image.
    MainColor,
    /// The shared depth buffer.
    MainDepth,
    /// `minecraft:entity_outline`.
    Outline,
    /// The chain's `n`th own target.
    Internal(usize),
    /// The chain's `n`th texture.
    Texture(usize),
}

/// What a pass draws into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::renderer) enum Dest {
    Main,
    Outline,
    Internal(usize),
}

pub(in crate::renderer) struct GpuPass {
    pub pipeline: pipeline::Pass,
    pub sampler_set: vk::DescriptorSet,
    pub block_set: vk::DescriptorSet,
    pub inputs: Vec<Source>,
    pub bilinear: Vec<bool>,
    pub output: Dest,
    /// `SamplerInfo` then the JSON's blocks, at [`BLOCK_STRIDE`] apart.
    ubo: vk::Buffer,
    ubo_memory: vk::DeviceMemory,
    ubo_ptr: *mut u8,
    /// The JSON's blocks, built once.
    blocks: Vec<Vec<u8>>,
}

struct Internal {
    image: Option<Image>,
    framebuffer: vk::Framebuffer,
    extent: vk::Extent2D,
    clear: [f32; 4],
}

struct EffectTexture {
    texture: Texture,
    width: u32,
    height: u32,
}

pub(in crate::renderer) struct Chain {
    pub config: Config,
    pub passes: Vec<GpuPass>,
    internal: Vec<Internal>,
    textures: Vec<EffectTexture>,
}

/// A chain to build: its JSON, its shaders and its textures.
pub struct ChainSource {
    pub id: String,
    pub config: Config,
    pub shaders: Vec<Compiled>,
    /// `textures/effect/<location>.png` by the input's location.
    pub textures: HashMap<String, RgbaImage>,
}

/// What the chains and the entity outline share.
pub(in crate::renderer) struct Post {
    pub format: vk::Format,
    /// Colour only, cleared on begin and left readable.
    pub color_pass: vk::RenderPass,
    /// The swapchain's colour, written whole and handed back to present.
    pub main_pass: vk::RenderPass,
    nearest: vk::Sampler,
    linear: vk::Sampler,
    pool: vk::DescriptorPool,
    /// By the number of samplers (set 0) or blocks (set 2).
    sampler_layouts: Vec<vk::DescriptorSetLayout>,
    block_layouts: Vec<vk::DescriptorSetLayout>,
    pub chains: HashMap<String, Chain>,
    pub extent: vk::Extent2D,
    /// What glowing entities draw into, and its framebuffer.
    pub outline: Option<Image>,
    pub outline_framebuffer: vk::Framebuffer,
    /// A copy of the swapchain image, for the passes that read `main`.
    pub snapshot: Option<Image>,
    /// A depth-only view of the shared depth buffer.
    depth_view: vk::ImageView,
    /// The swapchain images as colour-only framebuffers of [`Self::main_pass`].
    pub main_framebuffers: Vec<vk::Framebuffer>,
    /// Draws the entity outline onto `main` after its chain.
    pub blit: Option<(pipeline::Pass, vk::DescriptorSet)>,
    /// Whether `main` was drawn into since [`Self::snapshot`] was copied.
    pub main_dirty: Cell<bool>,
    /// The chains' own targets still hold nothing defined.
    pub needs_init: Cell<bool>,
}

impl Post {
    pub unsafe fn new(device: &ash::Device, format: vk::Format) -> anyhow::Result<Self> {
        let color_pass = create_pass(device, format, PassKind::Target)?;
        let main_pass = create_pass(device, format, PassKind::Main)?;
        let clamp = vk::SamplerAddressMode::CLAMP_TO_EDGE;
        let nearest = vk_util::create_sampler(device, vk::Filter::NEAREST, false, clamp)?;
        let linear = vk_util::create_sampler(device, vk::Filter::LINEAR, false, clamp)?;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: POOL_SETS * 4,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: POOL_SETS * 4,
            },
        ];
        let pool = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET)
                .max_sets(POOL_SETS)
                .pool_sizes(&sizes),
            None,
        )?;
        let layout = |count: u32, ty, stages| -> anyhow::Result<vk::DescriptorSetLayout> {
            let bindings: Vec<_> = (0..count)
                .map(|binding| {
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(binding)
                        .descriptor_type(ty)
                        .descriptor_count(1)
                        .stage_flags(stages)
                })
                .collect();
            Ok(device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?)
        };
        let mut sampler_layouts = Vec::new();
        for count in 0..=MAX_INPUTS {
            sampler_layouts.push(layout(
                count,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            )?);
        }
        let mut block_layouts = Vec::new();
        for count in 0..=MAX_BLOCKS + 1 {
            block_layouts.push(layout(
                count,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )?);
        }
        Ok(Self {
            format,
            color_pass,
            main_pass,
            nearest,
            linear,
            pool,
            sampler_layouts,
            block_layouts,
            chains: HashMap::new(),
            extent: vk::Extent2D::default(),
            outline: None,
            outline_framebuffer: vk::Framebuffer::null(),
            snapshot: None,
            blit: None,
            depth_view: vk::ImageView::null(),
            main_framebuffers: Vec::new(),
            main_dirty: Cell::new(true),
            needs_init: Cell::new(false),
        })
    }

    /// Builds (or, with `None`, drops) the outline blit.
    pub unsafe fn set_blit(
        &mut self,
        device: &ash::Device,
        render_pass: vk::RenderPass,
        compiled: Option<&Compiled>,
    ) -> anyhow::Result<()> {
        if let Some((pass, set)) = self.blit.take() {
            let _ = device.device_wait_idle();
            pass.destroy(device);
            let _ = device.free_descriptor_sets(self.pool, &[set]);
        }
        let Some(compiled) = compiled else {
            return Ok(());
        };
        let layout = self.sampler_layout(1);
        let bytes =
            |words: &[u32]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
        let (vert, frag) = (bytes(&compiled.vertex), bytes(&compiled.fragment));
        let blend = vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B,
            )
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD);
        let pass = pipeline::PipelineDesc {
            vert: &vert,
            frag: &frag,
            push_constant_size: 0,
            push_stages: vk::ShaderStageFlags::empty(),
            vertex_stride: 0,
            attributes: &[],
            instance: None,
            used_locations: None,
            cull: vk::CullModeFlags::NONE,
            depth: pipeline::depth_disabled(),
            blend,
            extra_sets: &[],
            depth_bias: None,
        }
        .build(device, render_pass, layout)?;
        let layouts = [layout];
        let sets = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(self.pool)
                .set_layouts(&layouts),
        );
        match sets {
            Ok(sets) => self.blit = Some((pass, sets[0])),
            Err(e) => {
                pass.destroy(device);
                return Err(e.into());
            }
        }
        self.write_blit(device);
        Ok(())
    }

    /// Points the blit's set at the outline target.
    unsafe fn write_blit(&self, device: &ash::Device) {
        let (Some((_, set)), Some(outline)) = (&self.blit, &self.outline) else {
            return;
        };
        let info = [vk::DescriptorImageInfo {
            sampler: self.nearest,
            image_view: outline.view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(*set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&info)],
            &[],
        );
    }

    /// The layout of a pass's set 0 with `count` samplers.
    pub fn sampler_layout(&self, count: usize) -> vk::DescriptorSetLayout {
        self.sampler_layouts[count.min(MAX_INPUTS as usize)]
    }

    /// Whether any chain is loaded, which the depth buffer must allow for.
    pub fn is_active(&self) -> bool {
        !self.chains.is_empty()
    }

    /// The chains' own textures, whose sets come from the texture pools.
    pub fn textures_mut(&mut self) -> impl Iterator<Item = &mut Texture> {
        self.chains
            .values_mut()
            .flat_map(|chain| chain.textures.iter_mut().map(|e| &mut e.texture))
    }

    /// Builds the chains of `sources`, with their textures.
    pub unsafe fn set_chains(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        gui_layout: vk::DescriptorSetLayout,
        sources: Vec<(ChainSource, Vec<SizedTexture>)>,
        free_texture: &dyn Fn(Texture),
    ) -> Vec<(String, anyhow::Error)> {
        self.clear_chains(device, free_texture);
        let mut failed = Vec::new();
        for (source, textures) in sources {
            let id = source.id.clone();
            match self.build_chain(
                device,
                mem_props,
                gui_layout,
                source,
                textures,
                free_texture,
            ) {
                Ok(chain) => {
                    self.chains.insert(id, chain);
                }
                Err(e) => failed.push((id, e)),
            }
        }
        if self.extent.width > 0 {
            let extent = self.extent;
            if let Err(e) = self.bind_all(device, mem_props, extent) {
                crate::warn!("Post effects: {e:#}");
            }
        }
        self.needs_init.set(true);
        failed
    }

    unsafe fn build_chain(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        gui_layout: vk::DescriptorSetLayout,
        source: ChainSource,
        textures: Vec<SizedTexture>,
        free_texture: &dyn Fn(Texture),
    ) -> anyhow::Result<Chain> {
        let ChainSource {
            config, shaders, ..
        } = source;
        let mut chain = Chain {
            internal: config
                .targets
                .iter()
                .map(|t| Internal {
                    image: None,
                    framebuffer: vk::Framebuffer::null(),
                    extent: vk::Extent2D::default(),
                    clear: t.clear,
                })
                .collect(),
            textures: textures
                .into_iter()
                .map(|(texture, width, height)| EffectTexture {
                    texture,
                    width,
                    height,
                })
                .collect(),
            passes: Vec::new(),
            config,
        };
        let built = self.build_passes(device, mem_props, gui_layout, &mut chain, &shaders);
        if let Err(e) = built {
            self.destroy_chain(device, &mut chain, free_texture);
            return Err(e);
        }
        Ok(chain)
    }

    unsafe fn build_passes(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        gui_layout: vk::DescriptorSetLayout,
        chain: &mut Chain,
        shaders: &[Compiled],
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            shaders.len() == chain.config.passes.len(),
            "a shader pair per pass"
        );
        let passes = chain.config.passes.clone();
        let mut texture_at = 0;
        for (pass, compiled) in passes.iter().zip(shaders) {
            anyhow::ensure!(
                pass.inputs.len() <= MAX_INPUTS as usize
                    && pass.uniforms.len() <= MAX_BLOCKS as usize,
                "a pass with too many inputs or blocks"
            );
            let (inputs, bilinear) = resolve_inputs(&chain.config, pass, &mut texture_at)?;
            let output = resolve_output(&chain.config, &pass.output)?;
            anyhow::ensure!(
                !inputs.iter().any(|input| match (input, output) {
                    (Source::Internal(a), Dest::Internal(b)) => *a == b,
                    (Source::Outline, Dest::Outline) => true,
                    _ => false,
                }),
                "a pass reads the target it draws into"
            );
            let sampler_layout = self.sampler_layout(inputs.len());
            let block_count = pass.uniforms.len() + 1;
            let block_layout = self.block_layouts[block_count];
            let bytes =
                |words: &[u32]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
            let (vert, frag) = (bytes(&compiled.vertex), bytes(&compiled.fragment));
            let pipeline = pipeline::PipelineDesc {
                vert: &vert,
                frag: &frag,
                push_constant_size: 0,
                push_stages: vk::ShaderStageFlags::empty(),
                vertex_stride: 0,
                attributes: &[],
                instance: None,
                used_locations: None,
                cull: vk::CullModeFlags::NONE,
                depth: pipeline::depth_disabled(),
                blend: vk::PipelineColorBlendAttachmentState::default()
                    .color_write_mask(vk::ColorComponentFlags::RGBA)
                    .blend_enable(false),
                extra_sets: &[gui_layout, block_layout],
                depth_bias: None,
            }
            .build(device, self.color_pass, sampler_layout)?;
            let layouts = [sampler_layout, block_layout];
            let sets = device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(self.pool)
                    .set_layouts(&layouts),
            );
            let sets = match sets {
                Ok(sets) => sets,
                Err(e) => {
                    pipeline.destroy(device);
                    return Err(e.into());
                }
            };
            let size = BLOCK_STRIDE * block_count as u64;
            let ubo = create_buffer_in(
                device,
                mem_props,
                size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                &MAPPED_FOR_GPU,
            )
            .and_then(|(ubo, memory)| {
                match device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty()) {
                    Ok(ptr) => Ok((ubo, memory, ptr.cast::<u8>())),
                    Err(e) => {
                        device.destroy_buffer(ubo, None);
                        vk_util::free_memory(device, memory);
                        Err(e.into())
                    }
                }
            });
            let (ubo, ubo_memory, ubo_ptr) = match ubo {
                Ok(ubo) => ubo,
                Err(e) => {
                    pipeline.destroy(device);
                    let _ = device.free_descriptor_sets(self.pool, &sets);
                    return Err(e);
                }
            };
            let blocks = pass
                .uniforms
                .iter()
                .map(|(_, members)| {
                    let mut block = Std140::default();
                    for member in members {
                        block.push(member);
                    }
                    block.finish()
                })
                .collect();
            chain.passes.push(GpuPass {
                pipeline,
                sampler_set: sets[0],
                block_set: sets[1],
                inputs,
                bilinear,
                output,
                ubo,
                ubo_memory,
                ubo_ptr,
                blocks,
            });
        }
        Ok(())
    }

    /// The swapchain's size and views changed (or this is the first time).
    pub unsafe fn resize(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        extent: vk::Extent2D,
        views: &[vk::ImageView],
        depth: Option<(vk::Image, vk::Format)>,
    ) -> anyhow::Result<()> {
        self.destroy_sized(device);
        self.extent = extent;
        if !self.is_active() || extent.width == 0 {
            return Ok(());
        }
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let outline = create_image(device, mem_props, self.format, extent, usage)?;
        let outline_view = outline.view;
        // Kept first, so `destroy_sized` frees it if the framebuffer fails.
        self.outline = Some(outline);
        self.outline_framebuffer = framebuffer(device, self.color_pass, outline_view, extent)?;
        self.snapshot = Some(create_image(
            device,
            mem_props,
            self.format,
            extent,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        )?);
        for &view in views {
            self.main_framebuffers
                .push(framebuffer(device, self.main_pass, view, extent)?);
        }
        if let Some((image, format)) = depth {
            self.depth_view = device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::DEPTH,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?;
        }
        self.write_blit(device);
        self.bind_all(device, mem_props, extent)?;
        self.needs_init.set(true);
        self.main_dirty.set(true);
        Ok(())
    }

    /// Makes every chain's own targets for `extent` and writes the passes' descriptor sets.
    unsafe fn bind_all(
        &mut self,
        device: &ash::Device,
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        extent: vk::Extent2D,
    ) -> anyhow::Result<()> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST;
        let mut chains = std::mem::take(&mut self.chains);
        let result = (|| -> anyhow::Result<()> {
            for chain in chains.values_mut() {
                for (internal, target) in chain.internal.iter_mut().zip(&chain.config.targets) {
                    let size = vk::Extent2D {
                        width: target.width.unwrap_or(extent.width),
                        height: target.height.unwrap_or(extent.height),
                    };
                    if internal.image.is_some() && internal.extent == size {
                        continue;
                    }
                    destroy_internal(device, internal);
                    let image = create_image(device, mem_props, self.format, size, usage)?;
                    internal.framebuffer = framebuffer(device, self.color_pass, image.view, size)?;
                    internal.image = Some(image);
                    internal.extent = size;
                }
                self.write_sets(device, chain, extent);
            }
            Ok(())
        })();
        self.chains = chains;
        result
    }

    unsafe fn write_sets(&self, device: &ash::Device, chain: &Chain, screen: vk::Extent2D) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let outline = self.outline.as_ref().map_or(snapshot.view, |o| o.view);
        let size_of = |source: Source| -> (u32, u32) {
            match source {
                Source::MainColor | Source::MainDepth | Source::Outline => {
                    (screen.width, screen.height)
                }
                Source::Internal(i) => (
                    chain.internal[i].extent.width,
                    chain.internal[i].extent.height,
                ),
                Source::Texture(i) => (chain.textures[i].width, chain.textures[i].height),
            }
        };
        for pass in &chain.passes {
            let mut images = Vec::with_capacity(pass.inputs.len());
            for (&source, &bilinear) in pass.inputs.iter().zip(&pass.bilinear) {
                let (view, layout, linear) = match source {
                    Source::MainColor => (
                        snapshot.view,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        bilinear,
                    ),
                    Source::MainDepth => (
                        self.depth_view,
                        vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                        false,
                    ),
                    Source::Outline => {
                        (outline, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, bilinear)
                    }
                    Source::Internal(i) => (
                        chain.internal[i]
                            .image
                            .as_ref()
                            .map_or(snapshot.view, |im| im.view),
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        bilinear,
                    ),
                    Source::Texture(i) => (
                        chain.textures[i].texture.view,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        bilinear,
                    ),
                };
                images.push([vk::DescriptorImageInfo {
                    sampler: if linear { self.linear } else { self.nearest },
                    image_view: view,
                    image_layout: layout,
                }]);
            }
            let writes: Vec<_> = images
                .iter()
                .enumerate()
                .map(|(binding, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(pass.sampler_set)
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(info)
                })
                .collect();
            if !writes.is_empty() {
                device.update_descriptor_sets(&writes, &[]);
            }

            let out = match pass.output {
                Dest::Main | Dest::Outline => (screen.width, screen.height),
                Dest::Internal(i) => (
                    chain.internal[i].extent.width,
                    chain.internal[i].extent.height,
                ),
            };
            let mut info = Std140::default();
            info.vec2(out.0 as f32, out.1 as f32);
            for &source in &pass.inputs {
                let (w, h) = size_of(source);
                info.vec2(w as f32, h as f32);
            }
            let mut ranges = vec![info.finish()];
            ranges.extend(pass.blocks.iter().cloned());
            let mut buffers = Vec::with_capacity(ranges.len());
            for (i, bytes) in ranges.iter().enumerate() {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    pass.ubo_ptr.add(i * BLOCK_STRIDE as usize),
                    bytes.len().min(BLOCK_STRIDE as usize),
                );
                buffers.push([vk::DescriptorBufferInfo {
                    buffer: pass.ubo,
                    offset: i as u64 * BLOCK_STRIDE,
                    range: bytes.len().min(BLOCK_STRIDE as usize) as u64,
                }]);
            }
            let writes: Vec<_> = buffers
                .iter()
                .enumerate()
                .map(|(binding, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(pass.block_set)
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// The target `dest` of `chain`: its framebuffer, size and clear colour.
    pub fn target<'a>(
        &'a self,
        chain: &'a Chain,
        dest: Dest,
    ) -> Option<(vk::Framebuffer, vk::Extent2D, [f32; 4])> {
        match dest {
            Dest::Main => None,
            Dest::Outline => Some((self.outline_framebuffer, self.extent, [0.0; 4])),
            Dest::Internal(i) => {
                let target = &chain.internal[i];
                target.image.as_ref()?;
                Some((target.framebuffer, target.extent, target.clear))
            }
        }
    }

    /// Every own target of every chain, for the first-use clear.
    pub fn internal_images(&self) -> Vec<vk::Image> {
        self.chains
            .values()
            .flat_map(|c| c.internal.iter())
            .filter_map(|t| t.image.as_ref().map(|i| i.image))
            .collect()
    }

    /// Frees what is sized with the swapchain, before its views go.
    pub unsafe fn release_sized(&mut self, device: &ash::Device) {
        self.destroy_sized(device);
    }

    unsafe fn destroy_sized(&mut self, device: &ash::Device) {
        let _ = device.device_wait_idle();
        for fb in self.main_framebuffers.drain(..) {
            device.destroy_framebuffer(fb, None);
        }
        if self.outline_framebuffer != vk::Framebuffer::null() {
            device.destroy_framebuffer(self.outline_framebuffer, None);
            self.outline_framebuffer = vk::Framebuffer::null();
        }
        for image in [self.outline.take(), self.snapshot.take()]
            .into_iter()
            .flatten()
        {
            destroy_image(device, &image);
        }
        if self.depth_view != vk::ImageView::null() {
            device.destroy_image_view(self.depth_view, None);
            self.depth_view = vk::ImageView::null();
        }
        for chain in self.chains.values_mut() {
            for internal in &mut chain.internal {
                destroy_internal(device, internal);
            }
        }
    }

    unsafe fn destroy_chain(
        &mut self,
        device: &ash::Device,
        chain: &mut Chain,
        free_texture: &dyn Fn(Texture),
    ) {
        for internal in &mut chain.internal {
            destroy_internal(device, internal);
        }
        for pass in chain.passes.drain(..) {
            pass.pipeline.destroy(device);
            let _ = device.free_descriptor_sets(self.pool, &[pass.sampler_set, pass.block_set]);
            device.unmap_memory(pass.ubo_memory);
            device.destroy_buffer(pass.ubo, None);
            vk_util::free_memory(device, pass.ubo_memory);
        }
        for texture in chain.textures.drain(..) {
            free_texture(texture.texture);
        }
    }

    unsafe fn clear_chains(&mut self, device: &ash::Device, free_texture: &dyn Fn(Texture)) {
        let _ = device.device_wait_idle();
        let chains = std::mem::take(&mut self.chains);
        for (_, mut chain) in chains {
            self.destroy_chain(device, &mut chain, free_texture);
        }
    }

    pub unsafe fn destroy(&mut self, device: &ash::Device, free_texture: &dyn Fn(Texture)) {
        self.clear_chains(device, free_texture);
        let _ = self.set_blit(device, vk::RenderPass::null(), None);
        self.destroy_sized(device);
        for layout in self
            .sampler_layouts
            .drain(..)
            .chain(self.block_layouts.drain(..))
        {
            device.destroy_descriptor_set_layout(layout, None);
        }
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_sampler(self.nearest, None);
        device.destroy_sampler(self.linear, None);
        device.destroy_render_pass(self.color_pass, None);
        device.destroy_render_pass(self.main_pass, None);
    }
}

fn resolve_inputs(
    config: &Config,
    pass: &Pass,
    texture_at: &mut usize,
) -> anyhow::Result<(Vec<Source>, Vec<bool>)> {
    let mut sources = Vec::new();
    let mut bilinear = Vec::new();
    for input in &pass.inputs {
        let source = match input {
            Input::Texture { .. } => {
                let n = *texture_at;
                *texture_at += 1;
                Source::Texture(n)
            }
            Input::Target { target, depth, .. } => {
                if target == MAIN {
                    if *depth {
                        Source::MainDepth
                    } else {
                        Source::MainColor
                    }
                } else if target == ENTITY_OUTLINE {
                    anyhow::ensure!(!depth, "the outline has no depth to read");
                    Source::Outline
                } else {
                    anyhow::ensure!(!depth, "{target} has no depth to read");
                    Source::Internal(internal_index(config, target)?)
                }
            }
        };
        sources.push(source);
        bilinear.push(input.bilinear());
    }
    Ok((sources, bilinear))
}

fn resolve_output(config: &Config, target: &str) -> anyhow::Result<Dest> {
    Ok(if target == MAIN {
        Dest::Main
    } else if target == ENTITY_OUTLINE {
        Dest::Outline
    } else {
        Dest::Internal(internal_index(config, target)?)
    })
}

fn internal_index(config: &Config, name: &str) -> anyhow::Result<usize> {
    config
        .targets
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown target {name}"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PassKind {
    /// Cleared, left readable.
    Target,
    /// Written whole, back to present.
    Main,
}

/// One colour attachment, the way [`Blur`](crate::renderer::blur) has it.
unsafe fn create_pass(
    device: &ash::Device,
    format: vk::Format,
    kind: PassKind,
) -> anyhow::Result<vk::RenderPass> {
    let (load, final_layout) = match kind {
        PassKind::Target => (
            vk::AttachmentLoadOp::CLEAR,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        ),
        PassKind::Main => (
            vk::AttachmentLoadOp::DONT_CARE,
            vk::ImageLayout::PRESENT_SRC_KHR,
        ),
    };
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(load)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(final_layout)];
    let color = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color)];
    let dependencies = [
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::TRANSFER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            )
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE),
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::TRANSFER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            )
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::TRANSFER_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_READ,
            ),
    ];
    Ok(device.create_render_pass(
        &vk::RenderPassCreateInfo::default()
            .attachments(&attachments)
            .subpasses(&subpasses)
            .dependencies(&dependencies),
        None,
    )?)
}

unsafe fn framebuffer(
    device: &ash::Device,
    pass: vk::RenderPass,
    view: vk::ImageView,
    extent: vk::Extent2D,
) -> anyhow::Result<vk::Framebuffer> {
    let attachments = [view];
    Ok(device.create_framebuffer(
        &vk::FramebufferCreateInfo::default()
            .render_pass(pass)
            .attachments(&attachments)
            .width(extent.width)
            .height(extent.height)
            .layers(1),
        None,
    )?)
}

unsafe fn destroy_image(device: &ash::Device, image: &Image) {
    device.destroy_image_view(image.view, None);
    device.destroy_image(image.image, None);
    vk_util::free_memory(device, image.memory);
}

unsafe fn destroy_internal(device: &ash::Device, internal: &mut Internal) {
    if let Some(image) = internal.image.take() {
        device.destroy_framebuffer(internal.framebuffer, None);
        internal.framebuffer = vk::Framebuffer::null();
        destroy_image(device, &image);
    }
}
