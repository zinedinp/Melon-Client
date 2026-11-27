//! The world camera's projection.

use glam::Mat4;

/// The world camera's near plane, in blocks (vanilla `GameRenderer.PROJECTION_Z_NEAR`).
pub const WORLD_Z_NEAR: f32 = 0.05;

/// The world camera's projection (Vulkan clip space).
pub fn world_projection(fov_y_degrees: f32, aspect: f32, far: f32) -> Mat4 {
    glam::camera::rh::proj::vulkan::perspective(
        fov_y_degrees.to_radians(),
        aspect,
        far,
        WORLD_Z_NEAR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_is_reversed_and_keeps_millimetres_apart_far_out() {
        let projection = world_projection(70.0, 1.0, 512.0);
        let depth = |distance: f32| {
            let clip = projection * glam::Vec4::new(0.0, 0.0, -distance, 1.0);
            clip.z / clip.w
        };
        assert!((depth(WORLD_Z_NEAR) - 1.0).abs() < 1e-5);
        assert!(depth(512.0).abs() < 1e-5);
        assert!(depth(100.0) > depth(100.001));
    }
}
