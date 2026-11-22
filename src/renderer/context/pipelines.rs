//! The render passes and every pipeline built at start.

use ash::vk;

use crate::renderer::pipeline::world::{Geometry, Layouts, Mode};
use crate::renderer::{PassStart, Pipelines, create_render_pass, pipeline};

/// The main pass in each way a frame starts it.
pub(super) struct RenderPasses {
    pub clear: vk::RenderPass,
    pub overwrite: vk::RenderPass,
    pub split: vk::RenderPass,
    pub resume: vk::RenderPass,
}

pub(super) unsafe fn create_render_passes(
    device: &ash::Device,
    surface_format: vk::Format,
    depth_format: vk::Format,
) -> anyhow::Result<RenderPasses> {
    let pass = |start| create_render_pass(device, surface_format, depth_format, start);
    Ok(RenderPasses {
        clear: pass(PassStart::Clear)?,
        overwrite: pass(PassStart::Overwrite)?,
        split: pass(PassStart::SceneSplit)?,
        resume: pass(PassStart::Resume)?,
    })
}

/// The descriptor set layouts the pipelines are built against.
pub(super) struct SetLayouts {
    /// Set 0: one texture.
    pub texture: vk::DescriptorSetLayout,
    pub gui: vk::DescriptorSetLayout,
    pub world: Layouts,
}

pub(super) unsafe fn create_set_layouts(device: &ash::Device) -> anyhow::Result<SetLayouts> {
    Ok(SetLayouts {
        texture: pipeline::create_descriptor_set_layout(device)?,
        gui: crate::renderer::gui_uniforms::create_layout(device)?,
        world: Layouts {
            table: crate::renderer::quad_table::create_layout(device)?,
            draws: crate::renderer::indirect::create_layout(device)?,
            globals: crate::renderer::globals::create_layout(device)?,
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
        pipeline::world::create(device, pass, texture, &layouts.world, mode, geometry)
    };
    let world = |mode| in_pass(render_pass, mode, Geometry::Vertices);
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
            built_in: pipeline::ui::create_all(
                device,
                render_pass,
                texture,
                layouts.gui,
                pipeline::ui::BUILT_IN,
            )?,
            pack: [None, None, None],
            glint: pipeline::ui::create(
                device,
                render_pass,
                texture,
                layouts.gui,
                crate::renderer::quad::BlendMode::Alpha,
                pipeline::ui::GLINT,
            )?,
        },
        sky: pipeline::sky::create(device, render_pass, texture)?,
        solid: world(Mode::Solid)?,
        world: world(Mode::Opaque)?,
        translucent: world(Mode::Translucent)?,
        translucent_terrain: world(Mode::TranslucentTerrain)?,
        lightning: world(Mode::Lightning)?,
        energy_swirl: world(Mode::EnergySwirl)?,
        scrolled: world(Mode::Scrolled)?,
        text: world(Mode::Text)?,
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
            world(Mode::Lines)?,
            world(Mode::LinesDepthBias)?,
            world(Mode::LinesTranslucent)?,
            world(Mode::LinesNoDepthWrite)?,
        ],
        layouts: layouts.world,
        terrain: [terrain(Mode::Solid)?, terrain(Mode::Opaque)?],
    })
}
