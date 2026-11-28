//! The render passes and every pipeline built at start.

use ash::vk;

use crate::pipeline::world::{Geometry, Layouts, Mode};
use crate::{PassStart, Pipelines, create_render_pass, pipeline};

/// The main pass in each way a frame starts it.
pub(crate) struct RenderPasses {
    /// [`PassStart::Clear`]: the usual one, and what pipelines are built in.
    pub clear: vk::RenderPass,
    /// [`PassStart::Overwrite`]: the pass that follows the menu blur.
    pub overwrite: vk::RenderPass,
    /// [`PassStart::SceneSplit`]: the scene's pass when a post chain follows.
    pub split: vk::RenderPass,
    /// [`PassStart::Resume`]: what comes after the chain.
    pub resume: vk::RenderPass,
    /// [`PassStart::ResumeKeep`]: the terrain's occlusion culling phase 2 on.
    pub resume_keep: vk::RenderPass,
}

impl RenderPasses {
    pub unsafe fn destroy(&self, device: &ash::Device) {
        for pass in [
            self.clear,
            self.overwrite,
            self.split,
            self.resume,
            self.resume_keep,
        ] {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_render_pass(pass, None) };
        }
    }
}

pub(super) unsafe fn create_render_passes(
    device: &ash::Device,
    surface_format: vk::Format,
    depth_format: vk::Format,
) -> anyhow::Result<RenderPasses> {
    // SAFETY: `device` is live, and the infos name only its own live handles.
    let pass = |start| unsafe { create_render_pass(device, surface_format, depth_format, start) };
    Ok(RenderPasses {
        clear: pass(PassStart::Clear)?,
        overwrite: pass(PassStart::Overwrite)?,
        split: pass(PassStart::SceneSplit)?,
        resume: pass(PassStart::Resume)?,
        resume_keep: pass(PassStart::ResumeKeep)?,
    })
}

/// The descriptor set layouts the pipelines are built against.
pub(super) struct SetLayouts {
    /// Set 0: one texture.
    pub texture: vk::DescriptorSetLayout,
    pub gui: vk::DescriptorSetLayout,
    pub world: Layouts,
}

/// The set layouts; the globals' comes from [`GlobalBuffers`](crate::globals::GlobalBuffers),
/// the GUI's from [`GuiUniforms`](crate::gui_uniforms::GuiUniforms).
pub(super) unsafe fn create_set_layouts(
    device: &ash::Device,
    globals: vk::DescriptorSetLayout,
    gui: vk::DescriptorSetLayout,
) -> anyhow::Result<SetLayouts> {
    Ok(SetLayouts {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        texture: unsafe { pipeline::create_descriptor_set_layout(device) }?,
        gui,
        world: Layouts {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            table: unsafe { crate::quad_table::create_layout(device) }?,
            // SAFETY: `device` is live, and the infos name only its own live handles.
            draws: unsafe { crate::indirect::create_layout(device) }?,
            globals,
        },
    })
}

/// Every built-in pipeline, in `render_pass`; the outline in `outline_pass`.
pub(super) unsafe fn create(
    device: &ash::Device,
    render_pass: vk::RenderPass,
    outline_pass: vk::RenderPass,
    layouts: &SetLayouts,
    multi_draw: bool,
) -> anyhow::Result<Pipelines> {
    let texture = layouts.texture;
    let in_pass = |pass, mode, geometry| {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        unsafe { pipeline::world::create(device, pass, texture, &layouts.world, mode, geometry) }
    };
    let world = |mode| in_pass(render_pass, mode, Geometry::Vertices);
    let lines = |mode| in_pass(render_pass, mode, Geometry::Lines);
    let terrain = |mode| {
        in_pass(
            render_pass,
            mode,
            Geometry::Quads {
                indirect: multi_draw,
            },
        )
    };
    Ok(Pipelines {
        layout: texture,
        ui: pipeline::ui::UiPipelines {
            // SAFETY: `device` is live, and the infos name only its own live handles.
            built_in: unsafe {
                pipeline::ui::create_all(
                    device,
                    render_pass,
                    texture,
                    layouts.gui,
                    pipeline::ui::BUILT_IN,
                )
            }?,
            pack: [None, None, None],
            // SAFETY: `device` is live, and the infos name only its own live handles.
            glint: unsafe {
                pipeline::ui::create(
                    device,
                    render_pass,
                    texture,
                    layouts.gui,
                    melon_base::gfx::quad::BlendMode::Alpha,
                    pipeline::ui::GLINT,
                )
            }?,
        },
        // SAFETY: `device` is live, and the infos name only its own live handles.
        sky: unsafe { pipeline::sky::create(device, render_pass, texture) }?,
        solid: world(Mode::Solid)?,
        world: world(Mode::Opaque)?,
        translucent: world(Mode::Translucent)?,
        translucent_terrain: world(Mode::TranslucentTerrain)?,
        translucent_terrain_indirect: if multi_draw {
            Some(in_pass(
                render_pass,
                Mode::TranslucentTerrain,
                Geometry::IndirectVertices,
            )?)
        } else {
            None
        },
        lightning: world(Mode::Lightning)?,
        energy_swirl: world(Mode::EnergySwirl)?,
        scrolled: world(Mode::Scrolled)?,
        text: world(Mode::Text)?,
        entity_translucent: world(Mode::EntityTranslucent)?,
        clouds: world(Mode::Clouds)?,
        weather: world(Mode::Weather)?,
        celestial: world(Mode::Celestial)?,
        sky_disc: world(Mode::SkyDisc)?,
        sky_glow: world(Mode::SkyGlow)?,
        crumbling: world(Mode::Crumbling)?,
        water_mask: world(Mode::WaterMask)?,
        outline: in_pass(outline_pass, Mode::Outline, Geometry::Vertices)?,
        glint: world(Mode::Glint)?,
        lines: [
            lines(Mode::Lines)?,
            lines(Mode::LinesDepthBias)?,
            lines(Mode::LinesTranslucent)?,
            lines(Mode::LinesNoDepthWrite)?,
        ],
        layouts: layouts.world,
        terrain: [terrain(Mode::Solid)?, terrain(Mode::Opaque)?],
        particles: [
            in_pass(render_pass, Mode::Opaque, Geometry::Particles)?,
            in_pass(render_pass, Mode::Translucent, Geometry::Particles)?,
        ],
    })
}
