//! What F3+B and F3+G draw into the world.

use std::path::{Path, PathBuf};

use glam::{DVec3, Mat4};
use serde::{Deserialize, Serialize};

use crate::game::InWorld;
use crate::game::camera::{Camera, CameraMode};
use crate::game::gizmos::{DEFAULT_WIDTH, GizmoMesh, Gizmos, TextView};
use crate::gui::hud::debug_entries::Category;
use crate::gui::text::font::Font;
use crate::math::argb;
use crate::renderer::TextureId;
use crate::renderer::{EntityDraw, Renderer};
use crate::world::culling::frustum::Frustum;
use crate::world::physics::Aabb;

/// `Window`'s default line width, pixels.
const THIN: f32 = 1.0;
const THICK: f32 = 4.0;
/// `ChunkBorderRenderer.MAJOR_LINES`, `CELL_BORDER` and `YELLOW`.
const MAJOR: u32 = 0xFF40_40FF;
const CELL: u32 = 0xFF00_9B9B;
const YELLOW: u32 = 0xFFFF_FF00;

/// Whether F3 was open and F3 + H on, kept between runs; which entries
/// show is `debug-profile.json`'s ([`crate::gui::hud::debug_entries`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugSettings {
    pub overlay: bool,
    pub advanced_tooltips: bool,
}

const FILE: &str = "debug_settings.json";

/// The debug renderers' mesh.
#[derive(Default)]
pub struct DebugView {
    /// The GUI scale, which sizes the 3D crosshair (`guiScale`).
    pub gui_scale: f32,
    mesh: GizmoMesh,
    path: Option<PathBuf>,
}

impl DebugView {
    /// The settings in `game_dir`, or the defaults; also whether they were saved before.
    pub fn load(game_dir: &Path) -> (Self, bool) {
        let path = game_dir.join(FILE);
        let saved: DebugSettings = crate::json_file::load(&path).unwrap_or_default();
        crate::gui::hud::item_slot::set_advanced_tooltips(saved.advanced_tooltips);
        (
            Self {
                path: Some(path),
                ..Self::default()
            },
            saved.overlay,
        )
    }

    /// Writes the settings; `overlay` is whether F3 is shown.
    pub fn save(&self, overlay: bool) {
        let Some(path) = &self.path else { return };
        let settings = DebugSettings {
            overlay,
            advanced_tooltips: crate::gui::hud::item_slot::advanced_tooltips(),
        };
        if let Err(e) = crate::json_file::save(path, &settings) {
            crate::warn!("F3 settings not saved: {e}");
        }
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        self.mesh.release(renderer);
    }
}

/// A world point moved so that it stays put through the camera's sway ([`Camera::bob`]).
fn still_in_view(camera: &Camera) -> impl Fn(DVec3) -> DVec3 {
    let mut rotation = camera.view;
    rotation.w_axis = glam::Vec4::W;
    let undo = rotation.inverse() * camera.bob.inverse() * rotation;
    let swaying = camera.bob != Mat4::IDENTITY;
    let eye = camera.position;
    move |p: DVec3| {
        if !swaying {
            return p;
        }
        eye + undo.transform_point3((p - eye).as_vec3()).as_dvec3()
    }
}

impl InWorld {
    /// The hitboxes, chunk borders and the other world renderers F3 + F6
    /// switched on, in view this frame, into `out`.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::game) fn draw_debug_view(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera,
        frustum: &Frustum,
        visible: &[i32],
        partial: f32,
        (font, pages): (&Font, &[TextureId]),
        out: &mut Vec<EntityDraw>,
        lines: &mut Vec<crate::renderer::LineDraw>,
    ) {
        use crate::gui::hud::debug_entries::Entry;
        let entries = self.debug.entries();
        let crosshair = entries.is_enabled(Entry::ThreeDimensionalCrosshair)
            && !self.hud_hidden
            && self.camera_mode == CameraMode::FirstPerson
            && !self.game_mode.current.is_spectator();
        let hitboxes = entries.is_enabled(Entry::EntityHitboxes);
        let chunk_borders = entries.is_enabled(Entry::ChunkBorders);
        let others = Entry::ALL
            .iter()
            .any(|&e| e.category() == Category::Renderer && e.works() && entries.is_enabled(e));
        drop(entries);
        if !hitboxes && !chunk_borders && !crosshair && !others {
            self.debug_view.clear(renderer);
            return;
        }
        let mut gizmos = Gizmos::default();
        if crosshair {
            self.crosshair_gizmos(&mut gizmos, camera);
        }
        if hitboxes {
            self.hitbox_gizmos(&mut gizmos, visible, partial);
        }
        if chunk_borders {
            self.chunk_border_gizmos(&mut gizmos);
        }
        if others {
            self.visualizer_gizmos(&mut gizmos, camera.position, frustum);
        }
        let forward = camera.forward();
        let up = camera.up();
        let text = TextView {
            font,
            pages,
            right: forward.cross(up).normalize_or_zero(),
            up,
        };
        self.debug_view.mesh.draw(
            renderer,
            &gizmos,
            (camera.position, forward),
            Some(text),
            out,
            lines,
        );
    }

    /// `DebugCrosshairRenderer`.
    fn crosshair_gizmos(&self, gizmos: &mut Gizmos, camera: &Camera) {
        let to_camera = -self.player.forward_vector().as_dvec3();
        let length = 0.01 * f64::from(self.debug_view.gui_scale.max(1.0));
        let still = still_in_view(camera);
        let place = |p: DVec3| still(p);
        let origin = camera.position - to_camera;
        let axes = [
            (DVec3::X, 0xFFFF_0000),
            (DVec3::Y, 0xFF00_FF00),
            (DVec3::Z, 0xFF7F_7FFF),
        ];
        for (axis, _) in axes {
            gizmos.line(
                place(origin),
                place(origin + axis * length),
                argb(0xFF00_0000),
                THICK,
            );
        }
        let front = origin + to_camera * 1.0e-4;
        for (axis, color) in axes {
            gizmos.line(place(front), place(front + axis * length), argb(color), 2.0);
        }
    }

    /// `EntityHitboxDebugRenderer.showHitboxes`.
    fn hitbox_gizmos(&self, gizmos: &mut Gizmos, visible: &[i32], partial: f32) {
        let white = argb(0xFFFF_FFFF);
        let red = argb(0xFFFF_0000);
        let blue = argb(0xFF00_00FF);
        let yellow = argb(0xFFFF_FF00);
        for &id in visible {
            let Some(entity) = self.entities.get(id) else {
                continue;
            };
            if entity.appearance.invisible() {
                continue;
            }
            let (min, max) = entity.bounds(partial);
            let bounds = Aabb { min, max };
            let feet = entity.pos(partial);
            gizmos.cuboid(&bounds, white, DEFAULT_WIDTH);
            gizmos.point(feet, white, 2.0);
            // The seat it rides on: as wide as the narrower of the two, a
            // sixteenth high, moved as far as the rider is between ticks.
            if let Some((seat, vehicle)) = self.entities.riding_position(id) {
                let half = f64::from(vehicle.size().0.min(entity.size().0)) / 2.0;
                let at = seat + (feet - entity.tick_pos());
                let seat = Aabb {
                    min: DVec3::new(at.x - half, at.y, at.z - half),
                    max: DVec3::new(at.x + half, at.y + 0.0625, at.z + half),
                };
                gizmos.cuboid(&seat, yellow, DEFAULT_WIDTH);
            }
            let eye = entity.eye(partial).as_dvec3();
            if entity.is_living() {
                let band = Aabb {
                    min: DVec3::new(bounds.min.x, eye.y - 0.01, bounds.min.z),
                    max: DVec3::new(bounds.max.x, eye.y + 0.01, bounds.max.z),
                };
                gizmos.cuboid(&band, red, DEFAULT_WIDTH);
            }
            // TODO(parity): the ender dragon's part boxes in
            // `ARGB.colorFromFloat(1, 0.25, 1, 0)` (`EnderDragon.getSubEntities`):
            // Melon has the dragon's model parts, but not the
            // `EnderDragonPart` hitboxes its `aiStep` places each tick.
            let look = entity.view_vector(partial).as_dvec3();
            gizmos.arrow(eye, eye + look * 2.0, blue, DEFAULT_WIDTH);
        }
        if self.camera_mode != CameraMode::FirstPerson {
            // TODO(parity): the yellow seat box while the player rides, as
            // the other riders have (`Entities::riding_position`). The
            // tracker doesn't hold the local player as an entity, so it
            // needs the seat point from `own_seat`'s vehicle instead of the
            // feet that one returns.
            let (min, max) = self.player.render_bounds();
            let bounds = Aabb { min, max };
            gizmos.cuboid(&bounds, white, DEFAULT_WIDTH);
            gizmos.point(self.player.pos(), white, 2.0);
            let eye = self.player.eye();
            let band = Aabb {
                min: DVec3::new(bounds.min.x, eye.y - 0.01, bounds.min.z),
                max: DVec3::new(bounds.max.x, eye.y + 0.01, bounds.max.z),
            };
            gizmos.cuboid(&band, red, DEFAULT_WIDTH);
            let look = self.player.forward_vector().as_dvec3();
            gizmos.arrow(eye, eye + look * 2.0, blue, DEFAULT_WIDTH);
        }
    }

    /// `ChunkBorderRenderer.emitGizmos`.
    fn chunk_border_gizmos(&self, gizmos: &mut Gizmos) {
        let (min_y, sections) = self.level.height();
        let (y_min, y_max) = (f64::from(min_y), f64::from(min_y + sections * 16));
        let feet = self.player.pos();
        let (bx, by, bz) = (
            feet.x.floor() as i32,
            feet.y.floor() as i32,
            feet.z.floor() as i32,
        );
        let x0 = f64::from(bx.div_euclid(16) * 16);
        let z0 = f64::from(bz.div_euclid(16) * 16);
        let (major, cell, yellow) = (argb(MAJOR), argb(CELL), argb(YELLOW));
        let post = |gizmos: &mut Gizmos, x: f64, z: f64, color: [f32; 4], width: f32| {
            gizmos.line(
                DVec3::new(x, y_min, z),
                DVec3::new(x, y_max, z),
                color,
                width,
            );
        };
        let red = argb(0x80FF_0000);
        for dx in [-16.0, 0.0, 16.0, 32.0] {
            for dz in [-16.0, 0.0, 16.0, 32.0] {
                post(gizmos, x0 + dx, z0 + dz, red, THICK);
            }
        }
        for i in (2..16).step_by(2) {
            let color = if i % 4 == 0 { cell } else { yellow };
            let t = f64::from(i);
            for edge in [0.0, 16.0] {
                post(gizmos, x0 + t, z0 + edge, color, THIN);
                post(gizmos, x0 + edge, z0 + t, color, THIN);
            }
        }
        let ring = |gizmos: &mut Gizmos, y: f64, color: [f32; 4], width: f32| {
            let corners = [
                DVec3::new(x0, y, z0),
                DVec3::new(x0, y, z0 + 16.0),
                DVec3::new(x0 + 16.0, y, z0 + 16.0),
                DVec3::new(x0 + 16.0, y, z0),
            ];
            for i in 0..4 {
                gizmos.line(corners[i], corners[(i + 1) % 4], color, width);
            }
        };
        let mut y = min_y;
        while f64::from(y) <= y_max {
            ring(
                gizmos,
                f64::from(y),
                if y % 8 == 0 { cell } else { yellow },
                THIN,
            );
            y += 2;
        }
        for x in [0.0, 16.0] {
            for z in [0.0, 16.0] {
                post(gizmos, x0 + x, z0 + z, major, THICK);
            }
        }
        let mut y = min_y;
        while f64::from(y) <= y_max {
            ring(gizmos, f64::from(y), major, THICK);
            y += 16;
        }
        let section_y = f64::from(by.div_euclid(16) * 16);
        gizmos.cuboid(
            &Aabb {
                min: DVec3::new(x0, section_y, z0),
                max: DVec3::new(x0 + 16.0, section_y + 16.0, z0 + 16.0),
            },
            major,
            THIN,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::angle::Rotation;

    #[test]
    fn the_crosshair_stays_put_through_the_views_sway() {
        let mut camera = Camera::new(
            DVec3::new(10.0, 70.0, 5.0),
            crate::math::euler::Look::new(-90.0, 0.0),
            70.0,
            256.0,
        );
        let ahead = camera.position + DVec3::X;
        assert_eq!(still_in_view(&camera)(ahead), ahead);
        camera.bob = Mat4::from_translation(glam::Vec3::new(0.1, -0.05, 0.0))
            * Mat4::rot_z_deg(2.0)
            * Mat4::rot_x_deg(3.0);
        let moved = still_in_view(&camera)(ahead);
        let mut rotation = camera.view;
        rotation.w_axis = glam::Vec4::W;
        let eye = camera.position;
        let unswayed = rotation.transform_point3((ahead - eye).as_vec3());
        let swayed = camera
            .bob
            .transform_point3(rotation.transform_point3((moved - eye).as_vec3()));
        assert!((unswayed - swayed).length() < 1e-5, "{unswayed} {swayed}");
    }

    #[test]
    fn what_f3_switched_on_comes_back_next_run() {
        let dir = std::env::temp_dir().join(format!("melon-debug-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (view, overlay) = DebugView::load(&dir);
        assert!(!overlay);
        view.save(true);
        let (_, overlay) = DebugView::load(&dir);
        assert!(overlay);
        std::fs::remove_dir_all(&dir).ok();
    }
}
