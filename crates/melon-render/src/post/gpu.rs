//! The GPU side of post chains, majestix-vk's but for the outline's entity draw and blit.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use ash::vk;
use image::RgbaImage;
use majestix_vk::{
    Attachment, Binding, Buffer, BufferDesc, Descriptor, DescriptorSet, DescriptorSetLayout,
    Graphics, Image, ImageDesc, ImageView, MemoryUse, Pipeline, PipelineLayout, Queue, Sampler,
    Shader, Simple as _, Target, ViewDesc, Write,
};

use super::config::{Config, ENTITY_OUTLINE, Input, MAIN, Pass};
use super::shader::Compiled;
use super::uniform::Std140;
use crate::pipeline;

/// Most inputs a pass may have, and uniform blocks.
const MAX_INPUTS: u32 = 16;
const MAX_BLOCKS: u32 = 8;
/// The room each block has in the blocks' buffer.
const BLOCK_STRIDE: u64 = 256;
/// Where the chains' own images sit between passes.
pub(crate) const READABLE: vk::ImageLayout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;

/// What a pass samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
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
pub(crate) enum Dest {
    Main,
    Outline,
    Internal(usize),
}

pub(crate) struct GpuPass {
    pub pipeline: Pipeline,
    pub layout: PipelineLayout,
    sampler_layout: DescriptorSetLayout,
    block_layout: DescriptorSetLayout,
    /// Sets 0 and 2, made for the current targets; none before the first resize.
    pub sets: Option<[DescriptorSet; 2]>,
    pub inputs: Vec<Source>,
    pub bilinear: Vec<bool>,
    pub output: Dest,
    /// The JSON's blocks, built once.
    blocks: Vec<Vec<u8>>,
    /// Where `SamplerInfo` and then `blocks` start in [`Post::blocks`].
    block_offset: u64,
}

/// An image passes draw into, its view and its target.
pub(crate) struct Drawn {
    pub image: Image,
    view: ImageView,
    pub target: Target,
}

struct Internal {
    image: Option<Drawn>,
    extent: vk::Extent2D,
    clear: [f32; 4],
    /// Kept between runs of its chain: cleared only when made, resized or its chain stopped.
    persistent: bool,
    /// A persistent target whose chain stopped running (`closePersistentTargets`): cleared
    /// before the next run.
    stale: Cell<bool>,
}

struct EffectTexture {
    image: Image,
    view: ImageView,
    /// The size the JSON gives it, for `SamplerInfo`.
    width: u32,
    height: u32,
}

pub(crate) struct Chain {
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

/// majestix-vk's part; given up in [`Post::destroy`], before the device goes.
struct Own {
    device: majestix_vk::Device,
    /// For the effect textures and the blocks' buffer.
    queue: Queue,
    /// One colour attachment, loaded: every chain pass, into its targets and the swapchain.
    pass: majestix_vk::Pass,
    nearest: Sampler,
    linear: Sampler,
    /// By the number of samplers (set 0) or blocks (set 2).
    sampler_layouts: Vec<DescriptorSetLayout>,
    block_layouts: Vec<DescriptorSetLayout>,
}

/// What is sized with the swapchain.
#[derive(Default)]
struct Screen {
    /// What glowing entities draw into.
    outline: Option<Drawn>,
    /// [`Post::color_pass`]'s framebuffer over `outline`, for the entity draw.
    outline_framebuffer: vk::Framebuffer,
    /// A copy of the swapchain image, for the passes that read `main`.
    snapshot: Option<(Image, ImageView)>,
    /// A depth-only view of the shared depth buffer.
    depth_view: Option<ImageView>,
    /// The swapchain's views as targets of [`Own::pass`], by image index.
    main_targets: Vec<Target>,
    /// The blit's set: the outline, nearest.
    blit_set: Option<DescriptorSet>,
}

/// What the chains and the entity outline share.
pub(crate) struct Post {
    pub format: vk::Format,
    /// Colour only, loaded on begin and left readable: the outline's entity draw (raw).
    pub color_pass: vk::RenderPass,
    own: Option<Own>,
    pub chains: HashMap<String, Chain>,
    pub extent: vk::Extent2D,
    screen: Option<Screen>,
    /// Every pass's `SamplerInfo` and blocks, rebuilt with the sets.
    blocks: Option<Buffer>,
    /// Draws the entity outline onto `main` in the scene's render pass (raw).
    blit: Option<pipeline::Pass>,
    /// Whether `main` was drawn into since the snapshot was copied.
    pub main_dirty: Cell<bool>,
    /// The chains' own targets still hold nothing defined.
    pub needs_init: Cell<bool>,
    /// The chains the last frame ran, for [`Self::running`].
    last_running: RefCell<Vec<String>>,
}

impl Post {
    /// The chains' pass, samplers and set layouts on `layer`, effect textures and blocks
    /// uploaded on `queue`.
    pub fn new(
        layer: &majestix_vk::Device,
        queue: &Queue,
        format: vk::Format,
    ) -> anyhow::Result<Self> {
        let sampler = |filter| {
            let clamp = vk::SamplerAddressMode::CLAMP_TO_EDGE;
            layer.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(filter)
                    .min_filter(filter)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(clamp)
                    .address_mode_v(clamp)
                    .address_mode_w(clamp)
                    .max_lod(0.0),
            )
        };
        let layouts = |max: u32, ty, stages| -> majestix_vk::Result<Vec<DescriptorSetLayout>> {
            (0..=max)
                .map(|count| {
                    let bindings: Vec<_> = (0..count)
                        .map(|binding| Binding {
                            binding,
                            ty,
                            count: 1,
                            stages,
                        })
                        .collect();
                    layer.create_descriptor_set_layout(&bindings)
                })
                .collect()
        };
        let own = Own {
            device: layer.clone(),
            queue: queue.clone(),
            pass: majestix_vk::Pass::new(layer, &[Attachment::loaded(format)], None)?,
            nearest: sampler(vk::Filter::NEAREST)?,
            linear: sampler(vk::Filter::LINEAR)?,
            sampler_layouts: layouts(
                MAX_INPUTS,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::ShaderStageFlags::FRAGMENT,
            )?,
            block_layouts: layouts(
                MAX_BLOCKS + 1,
                vk::DescriptorType::UNIFORM_BUFFER,
                vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            )?,
        };
        // SAFETY: `layer`'s device is live, and the info names no handles.
        let color_pass = unsafe { create_color_pass(layer.raw(), format) }?;
        Ok(Self {
            format,
            color_pass,
            own: Some(own),
            chains: HashMap::new(),
            extent: vk::Extent2D::default(),
            screen: None,
            blocks: None,
            blit: None,
            main_dirty: Cell::new(true),
            needs_init: Cell::new(false),
            last_running: RefCell::default(),
        })
    }

    /// Builds (or, with `None`, drops) the outline blit.
    ///
    /// # Safety
    ///
    /// `render_pass` is a live render pass of the layer's device.
    pub unsafe fn set_blit(
        &mut self,
        render_pass: vk::RenderPass,
        compiled: Option<&Compiled>,
    ) -> anyhow::Result<()> {
        let own = self
            .own
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("post effects destroyed"))?;
        if let Some(set) = self.screen.as_mut().and_then(|s| s.blit_set.take()) {
            own.device.retire(set);
        }
        if let Some(pass) = self.blit.take() {
            let _ = own.device.wait_idle();
            // SAFETY: made by this device, and the wait leaves no frame using it.
            unsafe { pass.destroy(own.device.raw()) };
        }
        let Some(compiled) = compiled else {
            return Ok(());
        };
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
        // SAFETY: the device is live, `render_pass` is its own (the caller's promise), and so
        // is the set layout.
        let pass = unsafe {
            pipeline::PipelineDesc {
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
            .build(own.device.raw(), render_pass, own.sampler_layouts[1].raw())
        }?;
        self.blit = Some(pass);
        self.make_blit_set()
    }

    /// The blit's set, once there is an outline to sample.
    fn make_blit_set(&mut self) -> anyhow::Result<()> {
        let (Some(own), Some(screen), Some(_)) = (&self.own, &mut self.screen, &self.blit) else {
            return Ok(());
        };
        let Some(outline) = &screen.outline else {
            return Ok(());
        };
        let set = own.device.create_descriptor_set(
            own.sampler_layouts[1],
            &[Write::new(
                0,
                Descriptor::sampled(outline.view, own.nearest),
            )],
        )?;
        screen.blit_set = Some(set);
        Ok(())
    }

    /// The blit's pipeline and its set (raw: it is drawn in the scene's render pass).
    pub fn blit(&self) -> Option<(&pipeline::Pass, vk::DescriptorSet)> {
        let set = self.screen.as_ref()?.blit_set?;
        Some((self.blit.as_ref()?, set.raw()))
    }

    /// Whether any chain is loaded, which the depth buffer must allow for.
    pub fn is_active(&self) -> bool {
        !self.chains.is_empty()
    }

    /// Builds the chains of `sources`, uploading their textures; `gui_layout` is set 1.
    pub fn set_chains(
        &mut self,
        gui_layout: DescriptorSetLayout,
        sources: Vec<ChainSource>,
    ) -> Vec<(String, anyhow::Error)> {
        self.clear_chains();
        let Some(own) = &self.own else {
            return sources
                .into_iter()
                .map(|s| (s.id, anyhow::anyhow!("post effects destroyed")))
                .collect();
        };
        let mut failed = Vec::new();
        for source in sources {
            let id = source.id.clone();
            match build_chain(own, gui_layout, source) {
                Ok(chain) => {
                    self.chains.insert(id, chain);
                }
                Err(e) => failed.push((id, e)),
            }
        }
        if let Err(e) = self.bind_all() {
            melon_base::warn!("Post effects: {e:#}");
        }
        self.needs_init.set(true);
        failed
    }

    /// The swapchain's size and views changed (or this is the first time).
    pub fn resize(
        &mut self,
        extent: vk::Extent2D,
        views: &[ImageView],
        depth: Option<Image>,
    ) -> anyhow::Result<()> {
        self.release_sized();
        self.extent = extent;
        if !self.is_active() || extent.width == 0 || extent.height == 0 {
            return Ok(());
        }
        let own = self
            .own
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("post effects destroyed"))?;
        // Kept as it is made, so `release_sized` gives up what a failure leaves.
        let screen = self.screen.insert(Screen::default());
        // Cleared before each outline pass, as `LevelRenderer` starts it from zero.
        let outline = make_drawn(own, self.format, extent)?;
        let outline_view = outline.view.raw();
        screen.outline = Some(outline);
        let snapshot = own.device.create_image(&ImageDesc::d2(
            self.format,
            extent.width,
            extent.height,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        ))?;
        match own.device.create_image_view(&ViewDesc::d2(snapshot)) {
            Ok(view) => screen.snapshot = Some((snapshot, view)),
            Err(e) => {
                own.device.retire(snapshot);
                return Err(e.into());
            }
        }
        for &view in views {
            screen
                .main_targets
                .push(Target::new(&own.pass, &[view], None)?);
        }
        if let Some(image) = depth {
            screen.depth_view = Some(own.device.create_image_view(&ViewDesc {
                range: Some(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::DEPTH,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                }),
                ..ViewDesc::d2(image)
            })?);
        }
        // SAFETY: the layer's device is live, and the pass and the outline's view are its own.
        screen.outline_framebuffer =
            unsafe { framebuffer(own.device.raw(), self.color_pass, outline_view, extent) }?;
        self.make_blit_set()?;
        self.bind_all()?;
        self.needs_init.set(true);
        self.main_dirty.set(true);
        Ok(())
    }

    /// Makes every chain's own targets for the screen and the passes' sets and blocks.
    fn bind_all(&mut self) -> anyhow::Result<()> {
        let (Some(own), Some(screen)) = (&self.own, &self.screen) else {
            return Ok(());
        };
        let extent = self.extent;
        for chain in self.chains.values_mut() {
            for (internal, target) in chain.internal.iter_mut().zip(&chain.config.targets) {
                let size = vk::Extent2D {
                    width: target.width.unwrap_or(extent.width),
                    height: target.height.unwrap_or(extent.height),
                };
                if internal.image.is_some() && internal.extent == size {
                    continue;
                }
                if let Some(drawn) = internal.image.take() {
                    retire_drawn(&own.device, drawn);
                }
                internal.image = Some(make_drawn(own, self.format, size)?);
                internal.extent = size;
            }
        }
        // The sets name the targets and the blocks hold their sizes: new ones for all.
        for pass in self.chains.values_mut().flat_map(|c| c.passes.iter_mut()) {
            retire_sets(&own.device, pass);
        }
        if let Some(buffer) = self.blocks.take() {
            own.device.retire(buffer);
        }
        let mut bytes = Vec::new();
        for chain in self.chains.values_mut() {
            let sizes: Vec<Vec<(u32, u32)>> = chain
                .passes
                .iter()
                .map(|pass| pass_sizes(chain, pass, extent))
                .collect();
            for (pass, sizes) in chain.passes.iter_mut().zip(sizes) {
                pass.block_offset = bytes.len() as u64;
                for block in pass_blocks(pass, &sizes) {
                    let start = bytes.len();
                    bytes.extend_from_slice(&block);
                    bytes.resize(start + BLOCK_STRIDE as usize, 0);
                }
            }
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let buffer = own.device.create_buffer_init(
            &own.queue,
            &BufferDesc {
                size: bytes.len() as u64,
                usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
                memory: MemoryUse::Device,
            },
            &bytes,
        )?;
        self.blocks = Some(buffer);
        for chain in self.chains.values_mut() {
            for i in 0..chain.passes.len() {
                let sets = make_sets(own, screen, chain, &chain.passes[i], buffer, extent)?;
                chain.passes[i].sets = Some(sets);
            }
        }
        Ok(())
    }

    /// What a pass with output `dest` draws into: its target, its image and the layout the
    /// image sits in between passes. `main` is the swapchain's image `image_index`.
    pub fn target<'a>(
        &'a self,
        chain: &'a Chain,
        dest: Dest,
        main: (Image, usize),
    ) -> Option<(&'a Target, Image, vk::ImageLayout)> {
        let screen = self.screen.as_ref()?;
        match dest {
            Dest::Main => Some((
                screen.main_targets.get(main.1)?,
                main.0,
                vk::ImageLayout::PRESENT_SRC_KHR,
            )),
            Dest::Outline => {
                let outline = screen.outline.as_ref()?;
                Some((&outline.target, outline.image, READABLE))
            }
            Dest::Internal(i) => {
                let drawn = chain.internal[i].image.as_ref()?;
                Some((&drawn.target, drawn.image, READABLE))
            }
        }
    }

    /// The outline image and [`Self::color_pass`]'s framebuffer over it.
    pub fn outline(&self) -> Option<(Image, vk::Framebuffer)> {
        let screen = self.screen.as_ref()?;
        let outline = screen.outline.as_ref()?;
        (screen.outline_framebuffer != vk::Framebuffer::null())
            .then_some((outline.image, screen.outline_framebuffer))
    }

    /// The copy of the swapchain image the passes read `main` from.
    pub fn snapshot(&self) -> Option<Image> {
        Some(self.screen.as_ref()?.snapshot?.0)
    }

    /// Every own target of every chain with its clear colour, for the clear after they are
    /// made.
    pub fn internal_images(&self) -> Vec<(Image, [f32; 4])> {
        self.chains
            .values()
            .flat_map(|c| c.internal.iter())
            .filter_map(|t| t.image.as_ref().map(|i| (i.image, t.clear)))
            .collect()
    }

    /// The targets of `chain` to clear before it runs: each one that isn't persistent (taken
    /// fresh for every run in vanilla), and the persistent ones its stopping left stale.
    pub fn run_clears(&self, chain: &Chain) -> Vec<(Image, [f32; 4])> {
        chain
            .internal
            .iter()
            .filter(|t| !t.persistent || t.stale.replace(false))
            .filter_map(|t| t.image.as_ref().map(|i| (i.image, t.clear)))
            .collect()
    }

    /// Notes the chains this frame runs; the persistent targets of the ones that stopped go
    /// stale (`GameRenderer.preparePostEffects`).
    pub fn running(&self, ids: &[String]) {
        let mut last = self.last_running.borrow_mut();
        for id in last.iter().filter(|id| !ids.contains(id)) {
            if let Some(chain) = self.chains.get(id) {
                for target in chain.internal.iter().filter(|t| t.persistent) {
                    target.stale.set(true);
                }
            }
        }
        last.clone_from(&ids.to_vec());
    }

    /// Gives up what is sized with the swapchain, before its views and depth go.
    pub fn release_sized(&mut self) {
        let Some(own) = &self.own else {
            return;
        };
        for chain in self.chains.values_mut() {
            for pass in &mut chain.passes {
                retire_sets(&own.device, pass);
            }
            for internal in &mut chain.internal {
                if let Some(drawn) = internal.image.take() {
                    retire_drawn(&own.device, drawn);
                }
            }
        }
        if let Some(buffer) = self.blocks.take() {
            own.device.retire(buffer);
        }
        let Some(screen) = self.screen.take() else {
            return;
        };
        if screen.outline_framebuffer != vk::Framebuffer::null() {
            let _ = own.device.wait_idle();
            // SAFETY: made by this device, and the wait leaves no frame using it.
            unsafe {
                own.device
                    .raw()
                    .destroy_framebuffer(screen.outline_framebuffer, None);
            };
        }
        let Screen {
            outline,
            snapshot,
            depth_view,
            main_targets,
            blit_set,
            ..
        } = screen;
        drop(main_targets);
        if let Some(set) = blit_set {
            own.device.retire(set);
        }
        if let Some(view) = depth_view {
            own.device.retire(view);
        }
        if let Some((image, view)) = snapshot {
            own.device.retire(view);
            own.device.retire(image);
        }
        if let Some(drawn) = outline {
            retire_drawn(&own.device, drawn);
        }
    }

    fn clear_chains(&mut self) {
        let chains = std::mem::take(&mut self.chains);
        if let Some(own) = &self.own {
            for chain in chains.into_values() {
                retire_chain(&own.device, chain);
            }
        }
    }

    /// Gives up everything, waiting for the device first.
    pub fn destroy(&mut self) {
        self.clear_chains();
        self.release_sized();
        let Some(own) = self.own.take() else {
            return;
        };
        let _ = own.device.wait_idle();
        // SAFETY: made by this device, and the wait leaves no frame using them.
        unsafe {
            if let Some(blit) = self.blit.take() {
                blit.destroy(own.device.raw());
            }
            own.device.raw().destroy_render_pass(self.color_pass, None);
        }
        self.color_pass = vk::RenderPass::null();
        let Own {
            device,
            nearest,
            linear,
            sampler_layouts,
            block_layouts,
            ..
        } = own;
        device.retire(nearest);
        device.retire(linear);
        for layout in sampler_layouts.into_iter().chain(block_layouts) {
            device.retire(layout);
        }
    }
}

/// A chain with its textures and pipelines; what it made is retired when it fails.
fn build_chain(
    own: &Own,
    gui_layout: DescriptorSetLayout,
    source: ChainSource,
) -> anyhow::Result<Chain> {
    let ChainSource {
        config,
        shaders,
        textures,
        ..
    } = source;
    let mut chain = Chain {
        internal: config
            .targets
            .iter()
            .map(|t| Internal {
                image: None,
                extent: vk::Extent2D::default(),
                clear: t.clear,
                persistent: t.persistent,
                stale: Cell::new(false),
            })
            .collect(),
        textures: Vec::new(),
        passes: Vec::new(),
        config,
    };
    match build_parts(own, gui_layout, &mut chain, &shaders, &textures) {
        Ok(()) => Ok(chain),
        Err(e) => {
            retire_chain(&own.device, chain);
            Err(e)
        }
    }
}

/// Each texture input of the chain's passes in order, then the passes.
fn build_parts(
    own: &Own,
    gui_layout: DescriptorSetLayout,
    chain: &mut Chain,
    shaders: &[Compiled],
    textures: &HashMap<String, RgbaImage>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        shaders.len() == chain.config.passes.len(),
        "a shader pair per pass"
    );
    for pass in &chain.config.passes {
        for input in &pass.inputs {
            let Input::Texture {
                location,
                width,
                height,
                ..
            } = input
            else {
                continue;
            };
            let missing;
            let pixels = match textures.get(location) {
                Some(pixels) => pixels,
                None => {
                    missing = melon_assets::assets::missing_texture();
                    &missing
                }
            };
            let (image, view) = upload_texture(own, pixels)?;
            chain.textures.push(EffectTexture {
                image,
                view,
                width: *width,
                height: *height,
            });
        }
    }
    let mut texture_at = 0;
    for (index, compiled) in shaders.iter().enumerate() {
        let pass = &chain.config.passes[index];
        anyhow::ensure!(
            pass.inputs.len() <= MAX_INPUTS as usize && pass.uniforms.len() <= MAX_BLOCKS as usize,
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
        let sampler_layout = own.sampler_layouts[inputs.len()];
        let block_layout = own.block_layouts[pass.uniforms.len() + 1];
        let layout = own
            .device
            .create_pipeline_layout(&[sampler_layout, gui_layout, block_layout], &[])?;
        let pipeline = match create_pipeline(own, layout, compiled) {
            Ok(pipeline) => pipeline,
            Err(e) => {
                own.device.retire(layout);
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
            layout,
            sampler_layout,
            block_layout,
            sets: None,
            inputs,
            bilinear,
            output,
            blocks,
            block_offset: 0,
        });
    }
    Ok(())
}

/// A pass's pipeline: a full-screen triangle into [`Own::pass`], unblended.
fn create_pipeline(
    own: &Own,
    layout: PipelineLayout,
    compiled: &Compiled,
) -> anyhow::Result<Pipeline> {
    let device = &own.device;
    // SAFETY: shaderc's SPIR-V of the pass's shaders, compiled for Vulkan.
    let (vert, frag) = unsafe {
        (
            device.create_shader_module(&compiled.vertex),
            device.create_shader_module(&compiled.fragment),
        )
    };
    let (vert, frag) = match (vert, frag) {
        (Ok(vert), Ok(frag)) => (vert, frag),
        (Ok(made), Err(e)) | (Err(e), Ok(made)) => {
            device.retire(made);
            return Err(e.into());
        }
        (Err(e), Err(_)) => return Err(e.into()),
    };
    let mut desc = Graphics::new(layout, Shader::main(vert), Some(Shader::main(frag)));
    desc.blend = &[majestix_vk::render::pipeline::OPAQUE];
    let pipeline = device.create_graphics_pipeline(&own.pass, &desc);
    device.retire(vert);
    device.retire(frag);
    Ok(pipeline?)
}

/// `pixels` in an image of its own, ready to sample, and its view.
fn upload_texture(own: &Own, pixels: &RgbaImage) -> anyhow::Result<(Image, ImageView)> {
    let device = &own.device;
    let (width, height) = pixels.dimensions();
    let image = device.create_image(&ImageDesc::d2(
        vk::Format::R8G8B8A8_UNORM,
        width,
        height,
        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
    ))?;
    let region = vk::BufferImageCopy::default()
        .image_subresource(COLOR_LAYER)
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    let view = device
        .upload_image(&own.queue, image, pixels.as_raw(), &[region], READABLE)
        .and_then(|()| device.create_image_view(&ViewDesc::d2(image)));
    match view {
        Ok(view) => Ok((image, view)),
        Err(e) => {
            device.retire(image);
            Err(e.into())
        }
    }
}

/// A colour image passes draw into and sample, at `extent`.
fn make_drawn(own: &Own, format: vk::Format, extent: vk::Extent2D) -> anyhow::Result<Drawn> {
    let device = &own.device;
    let image = device.create_image(&ImageDesc::d2(
        format,
        extent.width,
        extent.height,
        vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_DST,
    ))?;
    let made = device
        .create_image_view(&ViewDesc::d2(image))
        .and_then(|view| {
            Target::new(&own.pass, &[view], None)
                .map(|target| (view, target))
                .inspect_err(|_| {
                    device.retire(view);
                })
        });
    match made {
        Ok((view, target)) => Ok(Drawn {
            image,
            view,
            target,
        }),
        Err(e) => {
            device.retire(image);
            Err(e.into())
        }
    }
}

/// The size of `OutSize`, then of each input of `pass`.
fn pass_sizes(chain: &Chain, pass: &GpuPass, screen: vk::Extent2D) -> Vec<(u32, u32)> {
    let internal = |i: usize| {
        let extent = chain.internal[i].extent;
        (extent.width, extent.height)
    };
    let out = match pass.output {
        Dest::Main | Dest::Outline => (screen.width, screen.height),
        Dest::Internal(i) => internal(i),
    };
    let inputs = pass.inputs.iter().map(|&source| match source {
        Source::MainColor | Source::MainDepth | Source::Outline => (screen.width, screen.height),
        Source::Internal(i) => internal(i),
        Source::Texture(i) => (chain.textures[i].width, chain.textures[i].height),
    });
    std::iter::once(out).chain(inputs).collect()
}

/// `SamplerInfo` from `sizes`, then the JSON's blocks, each cut to [`BLOCK_STRIDE`].
fn pass_blocks(pass: &GpuPass, sizes: &[(u32, u32)]) -> Vec<Vec<u8>> {
    let mut info = Std140::default();
    for &(w, h) in sizes {
        info.vec2(w as f32, h as f32);
    }
    std::iter::once(info.finish())
        .chain(pass.blocks.iter().cloned())
        .map(|mut bytes| {
            bytes.truncate(BLOCK_STRIDE as usize);
            bytes
        })
        .collect()
}

/// Sets 0 and 2 of `pass`: its inputs, then its blocks in `buffer`.
fn make_sets(
    own: &Own,
    screen: &Screen,
    chain: &Chain,
    pass: &GpuPass,
    buffer: Buffer,
    extent: vk::Extent2D,
) -> anyhow::Result<[DescriptorSet; 2]> {
    let snapshot = screen
        .snapshot
        .ok_or_else(|| anyhow::anyhow!("no snapshot of the swapchain image"))?
        .1;
    let outline = screen.outline.as_ref().map_or(snapshot, |o| o.view);
    let samplers: Vec<Write> = pass
        .inputs
        .iter()
        .zip(&pass.bilinear)
        .enumerate()
        .map(|(binding, (&source, &bilinear))| {
            let (view, layout, linear) = match source {
                Source::MainColor => (snapshot, READABLE, bilinear),
                Source::MainDepth => screen.depth_view.map_or((snapshot, READABLE, false), |v| {
                    (v, vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL, false)
                }),
                Source::Outline => (outline, READABLE, bilinear),
                Source::Internal(i) => (
                    chain.internal[i]
                        .image
                        .as_ref()
                        .map_or(snapshot, |d| d.view),
                    READABLE,
                    bilinear,
                ),
                Source::Texture(i) => (chain.textures[i].view, READABLE, bilinear),
            };
            Write::new(
                binding as u32,
                Descriptor::Sampled {
                    view,
                    sampler: if linear { own.linear } else { own.nearest },
                    layout,
                },
            )
        })
        .collect();
    let sizes = pass_sizes(chain, pass, extent);
    let blocks: Vec<Write> = pass_blocks(pass, &sizes)
        .iter()
        .enumerate()
        .map(|(binding, bytes)| {
            Write::new(
                binding as u32,
                Descriptor::Buffer {
                    buffer,
                    offset: pass.block_offset + binding as u64 * BLOCK_STRIDE,
                    range: bytes.len() as u64,
                },
            )
        })
        .collect();
    let sampler_set = own
        .device
        .create_descriptor_set(pass.sampler_layout, &samplers)?;
    match own.device.create_descriptor_set(pass.block_layout, &blocks) {
        Ok(block_set) => Ok([sampler_set, block_set]),
        Err(e) => {
            own.device.retire(sampler_set);
            Err(e.into())
        }
    }
}

fn retire_sets(device: &majestix_vk::Device, pass: &mut GpuPass) {
    for set in pass.sets.take().into_iter().flatten() {
        device.retire(set);
    }
}

/// Retires a drawn image's target, view and image.
fn retire_drawn(device: &majestix_vk::Device, drawn: Drawn) {
    let Drawn {
        image,
        view,
        target,
    } = drawn;
    drop(target);
    device.retire(view);
    device.retire(image);
}

fn retire_chain(device: &majestix_vk::Device, chain: Chain) {
    for internal in chain.internal {
        if let Some(drawn) = internal.image {
            retire_drawn(device, drawn);
        }
    }
    for mut pass in chain.passes {
        retire_sets(device, &mut pass);
        device.retire(pass.pipeline);
        device.retire(pass.layout);
    }
    for texture in chain.textures {
        device.retire(texture.view);
        device.retire(texture.image);
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

/// Level 0, layer 0 of a colour image.
pub(crate) const COLOR_LAYER: vk::ImageSubresourceLayers = vk::ImageSubresourceLayers {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    mip_level: 0,
    base_array_layer: 0,
    layer_count: 1,
};

/// One colour attachment, loaded and left readable: what the outline's entity draw and the
/// pack outline are built for.
unsafe fn create_color_pass(
    device: &ash::Device,
    format: vk::Format,
) -> anyhow::Result<vk::RenderPass> {
    let attachments = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(READABLE)
        .final_layout(READABLE)];
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
            .src_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::TRANSFER_WRITE,
            )
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            ),
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
    // SAFETY: `device` is live, and the infos name no handles.
    Ok(unsafe {
        device.create_render_pass(
            &vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies),
            None,
        )
    }?)
}

unsafe fn framebuffer(
    device: &ash::Device,
    pass: vk::RenderPass,
    view: vk::ImageView,
    extent: vk::Extent2D,
) -> anyhow::Result<vk::Framebuffer> {
    let attachments = [view];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_framebuffer(
            &vk::FramebufferCreateInfo::default()
                .render_pass(pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1),
            None,
        )
    }?)
}
