//! A resource pack's `core/rendertype_outline`: Lumiere (and the like)

use std::path::Path;

use ash::vk;
use glam::Mat4;
use majestix_vk::{
    Binding, Descriptor, DescriptorSet, DescriptorSetLayout, Frames, Mapped, MemoryUse,
    Simple as _, Write,
};

use super::Renderer;
use super::gui_uniforms::{BLOCKS, DynamicTransforms, Globals, IDENTITY, STRIDE};
use super::pipeline::{Pass, PipelineDesc, depth_disabled};
use melon_assets::assets::shaders::{self, Interface, Stage};
use melon_world::mesh::WorldVertex;

/// The glowing entities a frame draws with the pack's shader; the rest go without (logged once).
pub const MAX_ENTITIES: usize = 1024;

/// The blocks' stages.
const STAGES: vk::ShaderStageFlags = vk::ShaderStageFlags::from_raw(
    vk::ShaderStageFlags::VERTEX.as_raw() | vk::ShaderStageFlags::FRAGMENT.as_raw(),
);

/// Set 1: each entity's `DynamicTransforms` (at a dynamic offset), `Projection`, `Globals`.
const BINDINGS: [Binding; 3] = [
    Binding {
        binding: 0,
        ty: vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC,
        count: 1,
        stages: STAGES,
    },
    Binding {
        binding: 1,
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        count: 1,
        stages: STAGES,
    },
    Binding {
        binding: 2,
        ty: vk::DescriptorType::UNIFORM_BUFFER,
        count: 1,
        stages: STAGES,
    },
];
const _: () = assert!(BLOCKS.len() == BINDINGS.len());

/// Where a slot's blocks sit in its range: the projection, the globals, then the entities.
const PROJECTION_AT: usize = 0;
const GLOBALS_AT: usize = STRIDE as usize;
const ENTITIES_AT: u64 = 2 * STRIDE;

/// Compiled pack shaders for the outline pass.
pub struct Shader {
    pub vert: Vec<u32>,
    pub frag: Vec<u32>,
    pub inputs: Vec<u32>,
}

/// The pack's outline pipeline and the uniform blocks it reads.
pub(super) struct PackOutline {
    pub pass: Pass,
    /// The scene's entities [`Self::prepare`] wrote, in draw order.
    drawn: Vec<usize>,
    /// Whether a frame had more glowing entities than [`MAX_ENTITIES`].
    warned: bool,
    /// majestix-vk's part; given up in [`Self::destroy`], before the device goes.
    own: Option<Own>,
}

struct Own {
    device: majestix_vk::Device,
    layout: DescriptorSetLayout,
    /// Per frame slot: the blocks, as [`PROJECTION_AT`] and the rest say.
    ring: Mapped<u8>,
    /// By slot index.
    sets: Vec<DescriptorSet>,
}

/// `core/rendertype_outline` from the active packs, when a pack ships it.
pub fn compile(assets_root: &Path) -> anyhow::Result<Option<Shader>> {
    let vert = shaders::core_source(assets_root, "rendertype_outline", Stage::Vertex);
    let frag = shaders::core_source(assets_root, "rendertype_outline", Stage::Fragment);
    let from_pack =
        |source: &Option<(String, bool)>| source.as_ref().is_some_and(|(_, pack)| *pack);
    if !from_pack(&vert) && !from_pack(&frag) {
        return Ok(None);
    }
    let missing = |stage: &str| anyhow::anyhow!("core/rendertype_outline has no {stage}");
    let (vert, _) = vert.ok_or_else(|| missing("vertex shader"))?;
    let (frag, _) = frag.ok_or_else(|| missing("fragment shader"))?;
    let interface = Interface {
        inputs: &[("Position", 0), ("UV0", 1), ("Color", 2)],
        resources: &[
            ("Sampler0", 0, 0),
            ("DynamicTransforms", 1, 0),
            ("Projection", 1, 1),
            ("Globals", 1, 2),
        ],
    };
    let compile = |stage, source: &str| {
        shaders::compile(
            assets_root,
            "core/rendertype_outline",
            stage,
            source,
            &[],
            &interface,
        )
    };
    let vert = compile(Stage::Vertex, &vert)?;
    Ok(Some(Shader {
        inputs: shaders::input_locations(&vert),
        vert,
        frag: compile(Stage::Fragment, &frag)?,
    }))
}

impl PackOutline {
    /// The pipeline in `color_pass` and a set per slot of `frames`.
    ///
    /// # Safety
    ///
    /// `color_pass` and `texture_layout` are live handles of `layer`'s device.
    pub unsafe fn create(
        layer: &majestix_vk::Device,
        frames: &Frames,
        color_pass: vk::RenderPass,
        texture_layout: vk::DescriptorSetLayout,
        shader: &Shader,
    ) -> anyhow::Result<Self> {
        let layout = layer.create_descriptor_set_layout(&BINDINGS)?;
        let made = Mapped::<u8>::new(
            layer,
            frames,
            (ENTITIES_AT + STRIDE * MAX_ENTITIES as u64) as usize,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryUse::Upload,
        )
        .map_err(anyhow::Error::from)
        .and_then(|ring| Ok((sets(layer, layout, &ring, frames)?, ring)));
        let (sets, ring) = match made {
            Ok(made) => made,
            Err(e) => {
                layer.retire(layout);
                return Err(e);
            }
        };
        let own = Own {
            device: layer.clone(),
            layout,
            ring,
            sets,
        };
        // SAFETY: `layer`'s device is live, and the layouts and pass named are its own.
        match unsafe {
            create_pass(
                layer.raw(),
                color_pass,
                texture_layout,
                layout.raw(),
                shader,
            )
        } {
            Ok(pass) => Ok(Self {
                pass,
                drawn: Vec::new(),
                warned: false,
                own: Some(own),
            }),
            Err(e) => {
                own.retire();
                Err(e)
            }
        }
    }

    /// Writes the frame's projection and globals, and each glowing entity's transforms, the
    /// first [`MAX_ENTITIES`] of `entities` (scene index, model-view, colour) in order.
    pub fn prepare(
        &mut self,
        frame: &majestix_vk::Frame<'_>,
        proj: Mat4,
        globals: &Globals,
        entities: impl Iterator<Item = (usize, Mat4, [f32; 4])>,
    ) {
        let own = self.own.as_mut().expect("pack outline used after destroy");
        own.ring.write(frame, PROJECTION_AT, &proj.to_cols_array());
        own.ring.write(frame, GLOBALS_AT, globals);
        self.drawn.clear();
        for (index, model_view, colour) in entities {
            if self.drawn.len() == MAX_ENTITIES {
                if !std::mem::replace(&mut self.warned, true) {
                    melon_base::warn!(
                        "More than {MAX_ENTITIES} glowing entities: the rest skip the pack's outline shader"
                    );
                }
                break;
            }
            let transforms = DynamicTransforms {
                model_view: model_view.to_cols_array(),
                texture: IDENTITY,
                color_modulator: colour,
                model_offset: [0.0; 4],
            };
            let at = ENTITIES_AT as usize + self.drawn.len() * STRIDE as usize;
            own.ring.write(frame, at, &transforms);
            self.drawn.push(index);
        }
    }

    /// The scene's entities [`Self::prepare`] wrote, each with its dynamic offset.
    pub fn drawn(&self) -> impl Iterator<Item = (usize, u32)> + '_ {
        self.drawn
            .iter()
            .enumerate()
            .map(|(i, &index)| (index, (i as u64 * STRIDE) as u32))
    }

    /// Frame slot `slot`'s set.
    pub fn set(&self, slot: usize) -> vk::DescriptorSet {
        self.own
            .as_ref()
            .expect("pack outline used after destroy")
            .sets[slot]
            .raw()
    }

    /// Frees the pipeline and retires the ring, the sets and the layout.
    ///
    /// # Safety
    ///
    /// `device` is the pipeline's and no frame in flight uses it any more.
    pub unsafe fn destroy(mut self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe { self.pass.destroy(device) };
        if let Some(own) = self.own.take() {
            own.retire();
        }
    }
}

impl Own {
    fn retire(self) {
        for set in self.sets {
            self.device.retire(set);
        }
        self.device.retire(self.layout);
    }
}

/// A set per slot of `frames`: its entity range (dynamic), projection and globals.
fn sets(
    layer: &majestix_vk::Device,
    layout: DescriptorSetLayout,
    ring: &Mapped<u8>,
    frames: &Frames,
) -> anyhow::Result<Vec<DescriptorSet>> {
    let mut sets = Vec::with_capacity(frames.count() as usize);
    for slot in frames.slots() {
        let start = ring.offset(slot);
        let block = |binding, offset| {
            Write::new(
                binding,
                Descriptor::Buffer {
                    buffer: ring.buffer(),
                    offset: start + offset,
                    range: STRIDE,
                },
            )
        };
        let writes = [
            block(0, ENTITIES_AT),
            block(1, PROJECTION_AT as u64),
            block(2, GLOBALS_AT as u64),
        ];
        match layer.create_descriptor_set(layout, &writes) {
            Ok(set) => sets.push(set),
            Err(e) => {
                for set in sets {
                    layer.retire(set);
                }
                return Err(e.into());
            }
        }
    }
    Ok(sets)
}

unsafe fn create_pass(
    device: &ash::Device,
    color_pass: vk::RenderPass,
    texture_layout: vk::DescriptorSetLayout,
    blocks_layout: vk::DescriptorSetLayout,
    shader: &Shader,
) -> anyhow::Result<Pass> {
    let bytes = |words: &[u32]| -> Vec<u8> { words.iter().flat_map(|w| w.to_le_bytes()).collect() };
    let vert = bytes(&shader.vert);
    let frag = bytes(&shader.frag);
    let attributes = [
        (vk::Format::R32G32B32_SFLOAT, 0),
        (
            vk::Format::R32G32_SFLOAT,
            std::mem::offset_of!(WorldVertex, uv) as u32,
        ),
        (
            vk::Format::R8G8B8A8_UNORM,
            std::mem::offset_of!(WorldVertex, color) as u32,
        ),
        (
            vk::Format::R8G8_UINT,
            std::mem::offset_of!(WorldVertex, light) as u32,
        ),
    ];
    // SAFETY: `device` is live, and the layout, pass and shaders named are its own.
    unsafe {
        PipelineDesc {
            vert: &vert,
            frag: &frag,
            push_constant_size: 0,
            push_stages: vk::ShaderStageFlags::VERTEX,
            vertex_stride: std::mem::size_of::<WorldVertex>() as u32,
            attributes: &attributes,
            instance: None,
            used_locations: Some(&shader.inputs),
            cull: vk::CullModeFlags::NONE,
            depth: depth_disabled(),
            blend: vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false),
            extra_sets: &[blocks_layout],
            depth_bias: None,
        }
        .build(device, color_pass, texture_layout)
    }
}

impl Renderer {
    /// The pack's outline shaders from now on, or Melon's flat colour when `None`.
    pub fn set_outline_pack_shader(&mut self, shader: Option<&Shader>) -> Option<anyhow::Error> {
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                melon_base::warn!("Waiting for the GPU failed: {e}");
            }
            if let Some(pack) = self.pack_outline.take() {
                pack.destroy(&self.device);
            }
            let shader = shader?;
            let frames = self
                .ring
                .as_ref()
                .expect("the frame ring is back between frames");
            match PackOutline::create(
                &self.layer.device,
                frames,
                self.post.color_pass,
                self.pipelines.layout,
                shader,
            ) {
                Ok(pack) => {
                    self.pack_outline = Some(pack);
                    None
                }
                Err(e) => Some(e),
            }
        }
    }
}
