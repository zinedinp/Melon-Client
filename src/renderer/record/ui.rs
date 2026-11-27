//! Recording the UI quads over the scene.

use crate::renderer::quad::DrawCall;
use crate::renderer::{Frame, Renderer, pipeline};
use ash::vk;

impl Renderer {
    /// UI quads, in GUI coordinates: `calls` of `frame.ui`.
    pub(in crate::renderer) unsafe fn record_ui(
        &self,
        cmd: vk::CommandBuffer,
        frame: &Frame<'_>,
        calls: &[DrawCall],
        full_scissor: vk::Rect2D,
    ) {
        let target = pipeline::ui::Target {
            device: &self.device,
            cmd,
            pipelines: &self.pipelines.ui,
            gui_set: self.gui_uniforms.set(self.frames.current),
            vertices: self.ui_vertices.bound(self.frames.current),
            extent: self.swapchain.extent,
        };
        // SAFETY: the command buffer is recording, and what it uses lives until the frame's fence.
        unsafe {
            pipeline::ui::record(&target, frame.ui_size, calls, full_scissor, |texture| {
                self.textures.list[texture.0].descriptor_set
            })
        };
    }
}
