//! Recording a frame's draws into its command buffer: the 3D scene, then
//! the UI.

pub(crate) mod scene;
mod ui;

use std::cell::Cell;

use ash::vk;
use glam::Vec4;

use super::pipeline::world::PushBlock;

/// What the command buffer being recorded has bound and pushed, so draws
/// skip what is already there. Forgetting too much only costs a rebind.
pub(crate) struct Recording {
    /// The vertex buffer at binding 0.
    pub vertex_buffer: Cell<vk::Buffer>,
    /// The terrain shader's quad buffer (set 4).
    pub quads: Cell<vk::Buffer>,
    /// The world push constants last sent since the pipeline was bound.
    pub push: Cell<Option<PushBlock>>,
    pub offset: Cell<Option<Vec4>>,
    /// How the fog finds distances (`FogInput` bytes), the same way.
    pub view: Cell<Option<PushBlock<4>>>,
    /// The frame's framebuffer, for the render pass the terrain's
    /// occlusion culling resumes.
    pub framebuffer: Cell<vk::Framebuffer>,
    /// Whether this frame culls the terrain's multi-draws (see `occlusion.rs`).
    pub occluding: Cell<bool>,
}

impl Recording {
    pub fn new() -> Self {
        Self {
            vertex_buffer: Cell::new(vk::Buffer::null()),
            quads: Cell::new(vk::Buffer::null()),
            push: Cell::new(None),
            offset: Cell::new(None),
            view: Cell::new(None),
            framebuffer: Cell::new(vk::Framebuffer::null()),
            occluding: Cell::new(false),
        }
    }

    /// A pipeline was bound: every push constant must be sent again.
    pub fn forget_pushes(&self) {
        self.push.set(None);
        self.offset.set(None);
        self.view.set(None);
    }

    /// A pass began: nothing is bound or pushed.
    pub fn forget_all(&self) {
        self.vertex_buffer.set(vk::Buffer::null());
        self.quads.set(vk::Buffer::null());
        self.forget_pushes();
    }
}
