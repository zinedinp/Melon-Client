//! The overlay layer: the block outline and the debug view.

use super::DebugFont;
use crate::game::InWorld;
use crate::game::camera::Camera;
use crate::game::world_resources::WorldResources;
use crate::renderer::{EntityDraw, LineDraw, Renderer};

/// What the overlay layer draws from.
pub(super) struct OverlayInput<'a> {
    pub camera: &'a Camera,
    pub aspect: f32,
    /// The fov option: the debug frustum, like culling, is never narrower.
    pub fov_setting: f32,
    /// Entities that survived culling, for the hitbox view.
    pub visible: &'a [i32],
    pub partial: f32,
    pub high_contrast: bool,
    pub font: DebugFont<'a>,
}

impl InWorld {
    /// Picks the block under the crosshair and outlines it; adds the debug
    /// view's lines and its labels (to `out`).
    pub(super) fn overlay_layer(
        &mut self,
        renderer: &mut Renderer,
        assets: &WorldResources,
        input: &OverlayInput<'_>,
        out: &mut Vec<EntityDraw>,
    ) -> Vec<LineDraw> {
        self.hit_result = self.pick_hit(self.player.eye());
        let (width, _) = renderer.framebuffer_size();
        let mut lines = Vec::new();
        self.outline.draw(
            renderer,
            &self.level,
            self.hit_block(),
            (
                |state| assets.models.has_translucent_quads(state),
                input.high_contrast,
            ),
            // `Window.getAppropriateLineWidth`.
            (width as f32 / 1920.0 * 2.5).max(2.5),
            &mut lines,
        );
        let camera = input.camera;
        let frustum = camera.frustum_with_fov(input.aspect, camera.fov.max(input.fov_setting));
        self.draw_debug_view(
            renderer,
            camera,
            &frustum,
            input.visible,
            input.partial,
            input.font,
            out,
            &mut lines,
        );
        lines
    }
}
