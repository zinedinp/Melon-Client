//! Recording the 3D scene: sky, world layers, entities.

use crate::renderer::Blending;
use crate::renderer::pipeline::world::{DrawLight, FogInput, Mode};
use crate::renderer::{
    ChunkDraw, LineStyle, MeshId, QuadMeshId, RenderLayer, Renderer, SceneFrame, Sky, SkyKind,
    TerrainMesh, TextureId, WeatherDraw, WorldDraw, WorldVertex, globals, mesh_arena, pipeline,
    world_projection,
};
use ash::vk;
use glam::{Mat4, Vec3};

/// Which layers of the scene a pass draws.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer) enum ScenePart {
    All,
    World,
    Hand,
}

/// The GPU timer's and the profilers' name for `layer`; terrain and
/// translucent terrain are named per mode where they are drawn.
fn layer_label(layer: RenderLayer) -> Option<&'static str> {
    Some(match layer {
        RenderLayer::Sky => "sky",
        RenderLayer::Entities => "entities",
        RenderLayer::Particles => "particles",
        RenderLayer::Shadows => "shadows",
        RenderLayer::Crumbling => "crumbling",
        RenderLayer::WaterMask => "water_mask",
        RenderLayer::Atmosphere => "atmosphere",
        RenderLayer::Hand => "hand",
        RenderLayer::Terrain | RenderLayer::Translucent => return None,
    })
}

impl ScenePart {
    fn has(self, layer: RenderLayer) -> bool {
        match self {
            Self::All => true,
            Self::World => layer != RenderLayer::Hand,
            Self::Hand => layer == RenderLayer::Hand,
        }
    }
}

impl Renderer {
    /// Pass 1.
    pub(in crate::renderer) unsafe fn record_scene(
        &self,
        cmd: vk::CommandBuffer,
        scene: &SceneFrame,
        part: ScenePart,
    ) {
        self.recording.forget_all();
        for layer in RenderLayer::ORDER {
            if !part.has(layer) {
                continue;
            }
            if let Some(label) = layer_label(layer) {
                self.gpu_timer.counted_pass(label);
            }
            if layer.clears_depth() {
                self.clear_depth(cmd);
            }
            match layer {
                RenderLayer::Sky => {
                    if let Some(sky) = &scene.sky {
                        self.record_sky(cmd, sky, scene.world.as_ref());
                    }
                }
                RenderLayer::Terrain | RenderLayer::Translucent => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    let passes: &[(Mode, &[ChunkDraw])] = if layer == RenderLayer::Translucent {
                        &[(Mode::TranslucentTerrain, &world.translucent)]
                    } else {
                        &[(Mode::Solid, &world.chunks), (Mode::Opaque, &world.cutout)]
                    };
                    let light = world.light([1.0; 4]);
                    let multi = self.indirect.is_some();
                    for &(mode, chunks) in passes {
                        let by_multi = multi && matches!(mode, Mode::Solid | Mode::Opaque);
                        let label = match mode {
                            Mode::Solid => "solid",
                            Mode::Opaque => "cutout",
                            Mode::TranslucentTerrain => "translucent",
                            _ => "terrain",
                        };
                        self.gpu_timer.counted_pass(label);
                        if by_multi {
                            self.draw_terrain_multi(cmd, world, mode, chunks, view_proj, &light);
                        }
                        let mut bound: Option<bool> = None;
                        for chunk in chunks {
                            let quads = matches!(chunk.mesh, TerrainMesh::Quads(_));
                            if quads && by_multi {
                                continue;
                            }
                            if bound != Some(quads) {
                                let ok = if quads {
                                    self.bind_terrain_pass(cmd, world.atlas, mode)
                                } else {
                                    self.bind_world_pass(cmd, world.atlas, mode)
                                };
                                if !ok {
                                    bound = None;
                                    continue;
                                }
                                bound = Some(quads);
                            }
                            let at = (view_proj, world.offset(chunk.origin));
                            match chunk.mesh {
                                TerrainMesh::Quads(id) => {
                                    self.draw_terrain_quads(cmd, id, at, &light, &chunk.range);
                                }
                                TerrainMesh::Vertices(id) => self.draw_world_mesh(
                                    cmd,
                                    self.pipelines.world.layout,
                                    id,
                                    at,
                                    &light.fogged(FogInput::Chunk),
                                    Some(&chunk.range),
                                ),
                            }
                        }
                        self.gpu_timer.mark(&self.device, label);
                    }
                    if layer == RenderLayer::Translucent {
                        let fog = self.model_fog(world, layer);
                        self.draw_lines(
                            cmd,
                            scene,
                            world,
                            (view_proj, fog),
                            LineStyle::TranslucentAfterTerrain,
                        );
                    }
                }
                RenderLayer::Atmosphere => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    let fog = FogInput::Chunk;
                    if let Some(clouds) = &scene.clouds
                        && self.bind_world_pass(cmd, clouds.draw.atlas, Mode::Clouds)
                    {
                        let at = world
                            .relative_to(clouds.draw.model, clouds.draw.origin)
                            .w_axis
                            .truncate();
                        self.draw_world_mesh(
                            cmd,
                            self.pipelines.clouds.layout,
                            clouds.draw.mesh,
                            (view_proj, at),
                            &world.light(clouds.color).fogged(fog),
                            None,
                        );
                    }
                    for weather in &scene.weather {
                        if self.bind_world_pass(cmd, weather.atlas, Mode::Weather) {
                            self.draw_weather(
                                cmd,
                                weather,
                                view_proj * world.relative_to(weather.model, weather.origin),
                                &world.light([1.0, 1.0, 1.0, 1.0]).fogged(fog),
                            );
                        }
                    }
                }
                RenderLayer::Entities | RenderLayer::Particles | RenderLayer::Hand => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    // Opaque first, then the blended ones; the see-through
                    // models of the world far to near, as vanilla's
                    // `TranslucentFeatureRenderPhase` sorts its submits, so
                    // a nearer one blends over what lies behind it.
                    let eye = world.eye_fraction();
                    let mut entities: Vec<(f32, &crate::renderer::EntityDraw)> = scene
                        .entities
                        .iter()
                        .filter(|e| e.layer == layer)
                        .map(|e| {
                            let sorted =
                                layer == RenderLayer::Entities && e.blend == Blending::Translucent;
                            let nearness = if sorted {
                                let at = world.relative_to(e.model, e.origin).w_axis.truncate();
                                -at.distance_squared(eye)
                            } else {
                                0.0
                            };
                            (nearness, e)
                        })
                        .collect();
                    entities.sort_by(|a, b| a.1.blend.cmp(&b.1.blend).then(a.0.total_cmp(&b.0)));
                    // Vanilla's `ITEM_TRANSLUCENT` and `ENTITY_TRANSLUCENT`
                    // write depth: a see-through model stays in front of
                    // the water and glass drawn after it, and its glint
                    // (depth `EQUAL`) finds it. Particles keep the pipeline
                    // that leaves depth alone.
                    let writes_depth = layer != RenderLayer::Particles;
                    let fog = self.model_fog(world, layer);
                    // The opaque gizmo lines go with the solid entities
                    // (vanilla's solid phase), before the blended ones.
                    let mut lines_drawn = layer != RenderLayer::Entities;
                    for (_, entity) in entities {
                        if !lines_drawn && entity.blend != Blending::Opaque {
                            lines_drawn = true;
                            for style in [LineStyle::Opaque, LineStyle::OpaqueDepthBias] {
                                self.draw_lines(cmd, scene, world, (view_proj, fog), style);
                            }
                        }
                        let (mode, pass) = match entity.blend {
                            Blending::Additive if entity.scroll.is_some() => {
                                (Mode::EnergySwirl, &self.pipelines.energy_swirl)
                            }
                            _ if entity.scroll.is_some() => {
                                (Mode::Scrolled, &self.pipelines.scrolled)
                            }
                            Blending::Translucent if writes_depth => {
                                (Mode::Text, &self.pipelines.text)
                            }
                            Blending::Translucent => {
                                (Mode::Translucent, &self.pipelines.translucent)
                            }
                            Blending::Text => (Mode::Text, &self.pipelines.text),
                            Blending::Opaque => (Mode::Opaque, &self.pipelines.world),
                            Blending::Additive => (Mode::Lightning, &self.pipelines.lightning),
                        };
                        let at = (
                            view_proj * world.relative_to(entity.model, entity.origin),
                            Vec3::ZERO,
                        );
                        let light = {
                            let [r, g, b] = entity.light.unwrap_or([
                                world.ambient[0],
                                world.ambient[1],
                                world.ambient[2],
                            ]);
                            world
                                .light([r, g, b, entity.alpha])
                                .fogged(fog)
                                .overlaid(entity.overlay)
                                .scrolled(entity.scroll)
                        };
                        if self.bind_world_pass(cmd, entity.atlas, mode) {
                            self.draw_world_mesh(cmd, pass.layout, entity.mesh, at, &light, None);
                        }
                        if entity.glint.on() {
                            let glint = if entity.glint.is_armor() {
                                scene.glint_armor
                            } else {
                                scene.glint_item
                            };
                            if let Some(glint) = glint
                                && self.bind_glint_pass(cmd, entity.atlas, glint)
                            {
                                self.draw_world_mesh(
                                    cmd,
                                    self.pipelines.glint.layout,
                                    entity.mesh,
                                    at,
                                    &light.glinted(entity.glint.scale()),
                                    None,
                                );
                            }
                        }
                    }
                    if !lines_drawn {
                        for style in [LineStyle::Opaque, LineStyle::OpaqueDepthBias] {
                            self.draw_lines(cmd, scene, world, (view_proj, fog), style);
                        }
                    }
                }
                RenderLayer::WaterMask => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    for entity in scene.entities.iter().filter(|e| e.layer == layer) {
                        if self.bind_world_pass(cmd, entity.atlas, Mode::WaterMask) {
                            self.draw_world_mesh(
                                cmd,
                                self.pipelines.water_mask.layout,
                                entity.mesh,
                                (
                                    view_proj * world.relative_to(entity.model, entity.origin),
                                    Vec3::ZERO,
                                ),
                                &DrawLight::tint([1.0; 4]),
                                None,
                            );
                        }
                    }
                }
                RenderLayer::Crumbling => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    for entity in scene.entities.iter().filter(|e| e.layer == layer) {
                        if self.bind_world_pass(cmd, entity.atlas, Mode::Crumbling) {
                            self.draw_world_mesh(
                                cmd,
                                self.pipelines.crumbling.layout,
                                entity.mesh,
                                (
                                    view_proj * world.relative_to(entity.model, entity.origin),
                                    Vec3::ZERO,
                                ),
                                &DrawLight::tint([1.0; 4]),
                                None,
                            );
                        }
                    }
                }
                RenderLayer::Shadows => {
                    let Some(world) = &scene.world else { continue };
                    let view_proj = self.world_view_projection(world, layer);
                    let fog = self.model_fog(world, layer);
                    for entity in scene.entities.iter().filter(|e| e.layer == layer) {
                        if self.bind_world_pass(cmd, entity.atlas, Mode::Translucent) {
                            self.draw_world_mesh(
                                cmd,
                                self.pipelines.translucent.layout,
                                entity.mesh,
                                (
                                    view_proj * world.relative_to(entity.model, entity.origin),
                                    Vec3::ZERO,
                                ),
                                &DrawLight::tint([1.0; 4]).fogged(fog),
                                None,
                            );
                        }
                    }
                    // Vanilla's `shapeOutlines`, then `translucentGizmos`.
                    for style in [LineStyle::Translucent, LineStyle::TranslucentNoDepthWrite] {
                        self.draw_lines(cmd, scene, world, (view_proj, fog), style);
                    }
                }
            }
            if let Some(label) = layer_label(layer) {
                self.gpu_timer.mark(&self.device, label);
            }
        }
        // A layer that had nothing to draw left its counted pass open: it
        // has to end in this render pass.
        self.gpu_timer.close_pass();
    }

    /// How the fog finds the distance of a model drawn in `layer`.
    fn model_fog(&self, world: &WorldDraw, layer: RenderLayer) -> FogInput {
        if layer == RenderLayer::Hand {
            return FogInput::None;
        }
        let fov = world.fov_for(layer);
        let projection = world_projection(fov, self.aspect_ratio(), world.far);
        FogInput::Model(globals::inverse_scale(&projection))
    }

    /// The world camera's `projection * rotation` for `layer`.
    pub(in crate::renderer) fn world_view_projection(
        &self,
        world: &WorldDraw,
        layer: RenderLayer,
    ) -> Mat4 {
        let fov = world.fov_for(layer);
        let mut rotation = world.view;
        rotation.w_axis = glam::Vec4::W;
        let distortion = if layer == RenderLayer::Hand {
            Mat4::IDENTITY
        } else {
            world.distortion
        };
        world_projection(fov, self.aspect_ratio(), world.far)
            * world.bob
            * distortion
            * rotation
            * Mat4::from_translation(-world.eye_fraction())
    }

    /// Resets the depth buffer inside the render pass, for a layer drawn on top.
    pub(in crate::renderer) unsafe fn clear_depth(&self, cmd: vk::CommandBuffer) {
        let clear = vk::ClearAttachment {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            color_attachment: 0,
            clear_value: vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: crate::renderer::frame::DEPTH_CLEAR,
                    stencil: 0,
                },
            },
        };
        let rect = vk::ClearRect {
            rect: vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: self.swapchain.extent,
            },
            base_array_layer: 0,
            layer_count: 1,
        };
        self.device.cmd_clear_attachments(cmd, &[clear], &[rect]);
    }

    pub(in crate::renderer) unsafe fn record_sky(
        &self,
        cmd: vk::CommandBuffer,
        sky: &Sky,
        world: Option<&WorldDraw>,
    ) {
        if let Some(cubemap) = sky.cubemap
            && let Some(texture) = self.textures.cubemaps.get(cubemap.0)
        {
            let projection = glam::camera::rh::proj::vulkan::perspective(
                sky.fov_y_degrees.to_radians(),
                self.aspect_ratio(),
                pipeline::sky::Z_NEAR,
                pipeline::sky::Z_FAR,
            );
            let view_proj = projection * sky.view;
            let push = view_proj.to_cols_array();

            self.device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.sky.pipeline,
            );
            self.device.cmd_push_constants(
                cmd,
                self.pipelines.sky.layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                pipeline::push_constant_bytes(&push, pipeline::sky::PUSH_CONSTANT_SIZE),
            );
            self.device
                .cmd_bind_vertex_buffers(cmd, 0, &[self.sky_cube.buffer], &[0]);
            self.recording.vertex_buffer.set(self.sky_cube.buffer);
            self.device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipelines.sky.layout,
                0,
                &[texture.descriptor_set],
                &[],
            );
            self.device
                .cmd_draw(cmd, pipeline::sky::CUBE_VERTICES.len() as u32, 1, 0, 0);
        }

        if sky.celestials.is_empty() {
            return;
        }
        let far = world.map_or(200.0, |w| w.far);
        let view_proj = world_projection(sky.fov_y_degrees, self.aspect_ratio(), far) * sky.view;
        for body in &sky.celestials {
            if self.bind_sky_pass(cmd, body.atlas, body.kind) {
                self.draw_world_mesh(
                    cmd,
                    self.pipelines.celestial.layout,
                    body.mesh,
                    (view_proj * body.model, Vec3::ZERO),
                    &DrawLight::tint(body.tint),
                    None,
                );
            }
        }
    }

    /// The [`LineDraw`](crate::renderer::LineDraw)s of `style`, through
    /// `shaders/lines.vert`. Each one's offset is the eye in its mesh's
    /// space: the point the shader pulls its lines toward.
    unsafe fn draw_lines(
        &self,
        cmd: vk::CommandBuffer,
        scene: &SceneFrame,
        world: &WorldDraw,
        (view_proj, fog): (Mat4, FogInput),
        style: LineStyle,
    ) {
        let (mode, pass) = match style {
            LineStyle::Opaque => (Mode::Lines, &self.pipelines.lines[0]),
            LineStyle::OpaqueDepthBias => (Mode::LinesDepthBias, &self.pipelines.lines[1]),
            LineStyle::Translucent | LineStyle::TranslucentAfterTerrain => {
                (Mode::LinesTranslucent, &self.pipelines.lines[2])
            }
            LineStyle::TranslucentNoDepthWrite => {
                (Mode::LinesNoDepthWrite, &self.pipelines.lines[3])
            }
        };
        let mut lines = scene
            .lines
            .iter()
            .filter(|line| line.style == style)
            .peekable();
        if lines.peek().is_none() || !self.bind_world_pass(cmd, self.white_texture(), mode) {
            return;
        }
        let light = DrawLight::tint([1.0; 4]).fogged(fog);
        for line in lines {
            let model = world.relative_to(line.model, line.origin);
            let eye = model.inverse().transform_point3(world.eye_fraction());
            self.draw_world_mesh(
                cmd,
                pass.layout,
                line.mesh,
                (view_proj * model, eye),
                &light,
                None,
            );
        }
    }

    /// Binds the world pipeline with `atlas`. `false` if the texture is gone.
    pub(in crate::renderer) unsafe fn bind_world_pass(
        &self,
        cmd: vk::CommandBuffer,
        atlas: TextureId,
        mode: Mode,
    ) -> bool {
        let Some(texture) = self.textures.list.get(atlas.0) else {
            return false;
        };
        let pass = match mode {
            Mode::Solid => &self.pipelines.solid,
            Mode::Opaque => &self.pipelines.world,
            Mode::Translucent => &self.pipelines.translucent,
            Mode::Lightning => &self.pipelines.lightning,
            Mode::EnergySwirl => &self.pipelines.energy_swirl,
            Mode::Scrolled => &self.pipelines.scrolled,
            Mode::TranslucentTerrain => &self.pipelines.translucent_terrain,
            Mode::Text => &self.pipelines.text,
            Mode::Clouds => &self.pipelines.clouds,
            Mode::Weather => &self.pipelines.weather,
            Mode::Celestial => &self.pipelines.celestial,
            Mode::SkyDisc => &self.pipelines.sky_disc,
            Mode::SkyGlow => &self.pipelines.sky_glow,
            Mode::Crumbling => &self.pipelines.crumbling,
            Mode::WaterMask => &self.pipelines.water_mask,
            Mode::Outline => &self.pipelines.outline,
            Mode::Lines => &self.pipelines.lines[0],
            Mode::LinesDepthBias => &self.pipelines.lines[1],
            Mode::LinesTranslucent => &self.pipelines.lines[2],
            Mode::LinesNoDepthWrite => &self.pipelines.lines[3],
            Mode::Glint => return false,
        };
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
        self.recording.forget_pushes();
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            self.pipelines.world.layout,
            0,
            &[texture.descriptor_set],
            &[],
        );
        self.bind_globals(cmd, self.pipelines.world.layout);
        self.device.cmd_bind_index_buffer(
            cmd,
            self.meshes.index_buffer(),
            0,
            vk::IndexType::UINT16,
        );
        true
    }

    /// Glint overlay.
    pub(in crate::renderer) unsafe fn bind_glint_pass(
        &self,
        cmd: vk::CommandBuffer,
        item: TextureId,
        glint: TextureId,
    ) -> bool {
        let (Some(item), Some(glint)) = (
            self.textures.list.get(item.0),
            self.textures.list.get(glint.0),
        ) else {
            return false;
        };
        let pass = &self.pipelines.glint;
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
        self.recording.forget_pushes();
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pass.layout,
            0,
            &[item.descriptor_set, glint.descriptor_set],
            &[],
        );
        self.bind_globals(cmd, pass.layout);
        self.device.cmd_bind_index_buffer(
            cmd,
            self.meshes.index_buffer(),
            0,
            vk::IndexType::UINT16,
        );
        true
    }

    /// Binds this frame's globals (set 3) for the pipeline layout `layout`.
    unsafe fn bind_globals(&self, cmd: vk::CommandBuffer, layout: vk::PipelineLayout) {
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            3,
            &[self.globals.set(self.frames.current)],
            &[],
        );
    }

    /// Binds the terrain quad pipeline for `mode` (solid, cutout or translucent).
    pub(in crate::renderer) unsafe fn bind_terrain_pass(
        &self,
        cmd: vk::CommandBuffer,
        atlas: TextureId,
        mode: Mode,
    ) -> bool {
        let (Some(texture), Some(table)) = (self.textures.list.get(atlas.0), &self.quad_table)
        else {
            return false;
        };
        let pass = match mode {
            Mode::Solid => &self.pipelines.terrain[0],
            Mode::Opaque => &self.pipelines.terrain[1],
            Mode::Translucent
            | Mode::Lightning
            | Mode::EnergySwirl
            | Mode::Scrolled
            | Mode::Text
            | Mode::TranslucentTerrain
            | Mode::Clouds
            | Mode::Weather
            | Mode::Celestial
            | Mode::SkyDisc
            | Mode::SkyGlow
            | Mode::Crumbling
            | Mode::WaterMask
            | Mode::Outline
            | Mode::Glint
            | Mode::Lines
            | Mode::LinesDepthBias
            | Mode::LinesTranslucent
            | Mode::LinesNoDepthWrite => return false,
        };
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
        self.recording.forget_pushes();
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pass.layout,
            0,
            &[texture.descriptor_set, table.set],
            &[],
        );
        self.bind_globals(cmd, pass.layout);
        self.recording.quads.set(vk::Buffer::null());
        self.device.cmd_bind_index_buffer(
            cmd,
            self.meshes.index_buffer(),
            0,
            vk::IndexType::UINT16,
        );
        true
    }

    /// Binds the pipeline of a sky mesh of `kind` with `atlas`.
    pub(in crate::renderer) unsafe fn bind_sky_pass(
        &self,
        cmd: vk::CommandBuffer,
        atlas: TextureId,
        kind: SkyKind,
    ) -> bool {
        let Some(texture) = self.textures.list.get(atlas.0) else {
            return false;
        };
        let pass = match kind {
            SkyKind::Disc => &self.pipelines.sky_disc,
            SkyKind::Glow => &self.pipelines.sky_glow,
            SkyKind::Celestial => &self.pipelines.celestial,
        };
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pass.pipeline);
        self.recording.forget_pushes();
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            pass.layout,
            0,
            &[texture.descriptor_set],
            &[],
        );
        self.bind_globals(cmd, pass.layout);
        self.device.cmd_bind_index_buffer(
            cmd,
            self.meshes.index_buffer(),
            0,
            vk::IndexType::UINT16,
        );
        true
    }

    /// Draws `vertices` of a mesh (all of it for `None`) with the bound pipeline.
    pub(in crate::renderer) unsafe fn draw_world_mesh(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        mesh: MeshId,
        (mvp, offset): (Mat4, Vec3),
        light: &DrawLight,
        vertices: Option<&std::ops::Range<u32>>,
    ) {
        let Some(slot) = self.meshes.slot(mesh.0) else {
            return;
        };
        let vertex_count = slot.count;
        let range = vertices.map_or(0..vertex_count, |r| r.start..r.end.min(vertex_count));
        if range.is_empty() {
            return;
        }
        let (buffer, base) = self.meshes.location(slot);
        self.bind_vertices(cmd, buffer);
        self.push_world(cmd, layout, mvp, offset, light);
        let mut quad = range.start / 4;
        let end = range.end / 4;
        while quad < end {
            let n = (end - quad).min(mesh_arena::QUADS_PER_DRAW);
            self.device
                .cmd_draw_indexed(cmd, n * 6, 1, 0, (base + quad * 4) as i32, 0);
            quad += n;
        }
    }

    /// Rain or snow: the template's streaks once per piece, the pieces read
    /// per instance from binding 1 (see `weather.vert`).
    unsafe fn draw_weather(
        &self,
        cmd: vk::CommandBuffer,
        weather: &WeatherDraw,
        mvp: Mat4,
        light: &DrawLight,
    ) {
        let (Some(template), Some(pieces)) = (
            self.meshes.slot(weather.template.0),
            self.meshes.slot(weather.pieces.0),
        ) else {
            return;
        };
        let copies = if weather.wraps { 1 } else { 2 };
        let (quads, instances) = (template.count / 4 / copies, pieces.count / 2);
        if quads == 0 || instances == 0 {
            return;
        }
        let (buffer, base) = self.meshes.location(template);
        let (piece_buffer, piece_base) = self.meshes.location(pieces);
        self.bind_vertices(cmd, buffer);
        let offset = u64::from(piece_base) * std::mem::size_of::<WorldVertex>() as u64;
        self.device
            .cmd_bind_vertex_buffers(cmd, 1, &[piece_buffer], &[offset]);
        self.push_world(cmd, self.pipelines.weather.layout, mvp, Vec3::ZERO, light);
        let quads = quads.min(mesh_arena::QUADS_PER_DRAW);
        self.device
            .cmd_draw_indexed(cmd, quads * 6, instances, 0, base as i32, 0);
    }

    /// The quad meshes among `chunks` as multi-draws (see [`super::indirect`]):
    unsafe fn draw_terrain_multi(
        &self,
        cmd: vk::CommandBuffer,
        world: &WorldDraw,
        mode: Mode,
        chunks: &[ChunkDraw],
        view_proj: Mat4,
        light: &DrawLight,
    ) {
        let Some(indirect) = &self.indirect else {
            return;
        };
        if !self.bind_terrain_pass(cmd, world.atlas, mode) {
            return;
        }
        let layout = self.pipelines.terrain[0].layout;
        let frame = self.frames.current;
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            2,
            &[indirect.set(frame)],
            &[],
        );
        let mut run: Option<(vk::Buffer, u32, u32)> = None;
        let capacity = indirect.capacity(frame);
        for chunk in chunks {
            let TerrainMesh::Quads(id) = chunk.mesh else {
                continue;
            };
            // TODO(perf): a random read per draw from the arena's slot table
            // (~8 MB with every section loaded), a cache miss each. Could the
            // buffer and base go into `ChunkDraw` when culled, kept right across
            // arena moves by a generation number?
            let Some(slot) = self.quad_meshes.slot(id.0) else {
                continue;
            };
            let (buffer, base) = self.quad_meshes.location(slot);
            if run.is_some_and(|(open, ..)| open != buffer) {
                self.flush_run(cmd, layout, view_proj, light, run.take());
            }
            let offset = world.offset(chunk.origin).to_array();
            for (first, quads) in quad_draws(&chunk.range, slot.count) {
                let index = self.recording.draw_cursor.get();
                if index >= capacity {
                    break;
                }
                indirect.write(
                    frame,
                    index,
                    vk::DrawIndexedIndirectCommand {
                        index_count: quads * 6,
                        instance_count: 1,
                        first_index: 0,
                        vertex_offset: corner_offset(base + first),
                        first_instance: 0,
                    },
                    offset,
                );
                self.recording.draw_cursor.set(index + 1);
                run = Some(match run {
                    Some((buffer, first, count)) => (buffer, first, count + 1),
                    None => (buffer, index, 1),
                });
            }
        }
        self.flush_run(cmd, layout, view_proj, light, run);
    }

    /// Draws a run of [`Self::draw_terrain_multi`].
    unsafe fn flush_run(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        view_proj: Mat4,
        light: &DrawLight,
        run: Option<(vk::Buffer, u32, u32)>,
    ) {
        let (Some((buffer, first, count)), Some(indirect)) = (run, &self.indirect) else {
            return;
        };
        self.bind_quads(cmd, layout, buffer);
        self.push_world(cmd, layout, view_proj, Vec3::ZERO, light);
        let at = [0.0, 0.0, 0.0, f32::from_bits(first)];
        self.device.cmd_push_constants(
            cmd,
            layout,
            pipeline::world::PUSH_STAGES,
            pipeline::world::OFFSET_AT,
            pipeline::push_constant_bytes(&at, pipeline::world::OFFSET_SIZE),
        );
        self.recording.offset.set(None);
        self.device.cmd_draw_indexed_indirect(
            cmd,
            indirect.commands(self.frames.current),
            u64::from(first) * u64::from(super::super::indirect::COMMAND_SIZE),
            count,
            super::super::indirect::COMMAND_SIZE,
        );
    }

    /// Draws `quads` of a terrain quad mesh with the bound terrain pass.
    pub(in crate::renderer) unsafe fn draw_terrain_quads(
        &self,
        cmd: vk::CommandBuffer,
        mesh: QuadMeshId,
        (mvp, offset): (Mat4, Vec3),
        light: &DrawLight,
        quads: &std::ops::Range<u32>,
    ) {
        let Some(slot) = self.quad_meshes.slot(mesh.0) else {
            return;
        };
        let (buffer, base) = self.quad_meshes.location(slot);
        let layout = self.pipelines.terrain[0].layout;
        self.bind_quads(cmd, layout, buffer);
        self.push_world(cmd, layout, mvp, offset, light);
        for (first, n) in quad_draws(quads, slot.count) {
            self.device
                .cmd_draw_indexed(cmd, n * 6, 1, 0, corner_offset(base + first), 0);
        }
    }

    /// Binds mesh buffer `buffer` as the terrain shader's quads (set 4).
    unsafe fn bind_quads(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        buffer: vk::Buffer,
    ) {
        if self.recording.quads.replace(buffer) == buffer {
            return;
        }
        let Some(set) = self.quad_sets.get(buffer) else {
            return;
        };
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::GRAPHICS,
            layout,
            4,
            &[set],
            &[],
        );
    }

    /// Binds `buffer` as the vertex buffer unless it already is.
    pub(in crate::renderer) unsafe fn bind_vertices(
        &self,
        cmd: vk::CommandBuffer,
        buffer: vk::Buffer,
    ) {
        if self.recording.vertex_buffer.replace(buffer) != buffer {
            self.device.cmd_bind_vertex_buffers(cmd, 0, &[buffer], &[0]);
        }
    }

    /// Pushes what changed of a world draw's constants.
    unsafe fn push_world(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        mvp: Mat4,
        offset: Vec3,
        light: &DrawLight,
    ) {
        let push = pipeline::world::PushBlock(pipeline::world::push_bytes(&mvp, light));
        if self.recording.push.get() != Some(push) {
            self.recording.push.set(Some(push));
            let push = push.0;
            self.device.cmd_push_constants(
                cmd,
                layout,
                pipeline::world::PUSH_STAGES,
                0,
                pipeline::push_constant_bytes(&push, pipeline::world::OFFSET_AT),
            );
        }
        let offset = offset.extend(light.overlay);
        if self.recording.offset.get() != Some(offset) {
            self.recording.offset.set(Some(offset));
            let offset = offset.to_array();
            self.device.cmd_push_constants(
                cmd,
                layout,
                pipeline::world::PUSH_STAGES,
                pipeline::world::OFFSET_AT,
                pipeline::push_constant_bytes(&offset, pipeline::world::OFFSET_SIZE),
            );
        }
        let mut view = light.fog.bytes();
        view[3] = light.glint_scale;
        let view = pipeline::world::PushBlock(view);
        if self.recording.view.get() != Some(view) {
            self.recording.view.set(Some(view));
            let view = view.0;
            self.device.cmd_push_constants(
                cmd,
                layout,
                pipeline::world::PUSH_STAGES,
                pipeline::world::VIEW_AT,
                pipeline::push_constant_bytes(&view, pipeline::world::OFFSET_SIZE),
            );
        }
    }
}

/// The draws of quads `range` of a mesh of `count` quads.
fn quad_draws(range: &std::ops::Range<u32>, count: u32) -> impl Iterator<Item = (u32, u32)> {
    let end = range.end.min(count);
    (range.start..end)
        .step_by(mesh_arena::QUADS_PER_DRAW as usize)
        .map(move |first| (first, (end - first).min(mesh_arena::QUADS_PER_DRAW)))
}

/// The `vertexOffset` that makes `gl_VertexIndex` count corners from quad `quad`.
fn corner_offset(quad: u32) -> i32 {
    (quad * 4) as i32
}
