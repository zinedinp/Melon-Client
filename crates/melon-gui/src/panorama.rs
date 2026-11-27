//! Rotating cubemap behind the title screen.

use std::time::Duration;

use glam::Mat4;
use image::RgbaImage;

use crate::math::angle::{PI, Rotation};
use crate::renderer::{CubeMapId, Renderer, SceneFrame, Sky};
use melon_assets::assets::Assets;

/// `CubeMap.PROJECTION_FOV`.
const FOV_Y_DEGREES: f32 = 85.0;
/// Fixed pitch. Vanilla `GuiRenderer`: `cubeMap.render(10.0F, spin)`.
const PITCH_DEGREES: f32 = 10.0;
/// `Panorama.extractRenderState`: `spin += deltaTicks * panoramaSpeed * 0.1`.
const DEGREES_PER_SECOND: f32 = 20.0 * 0.1;

/// Vanilla `CubeMapTexture.SUFFIXES`: files are not in cubemap face order.
const FACE_FILES: [usize; 6] = [1, 3, 5, 4, 0, 2];

const BASE_PATH: &str = "minecraft:gui/title/background/panorama";

pub struct Panorama {
    cubemap: CubeMapId,
    spin_degrees: f32,
}

impl Panorama {
    /// Load `panorama_0..5.png` as a cubemap.
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Option<Self>> {
        let mut faces: Vec<RgbaImage> = Vec::with_capacity(6);
        for &file in &FACE_FILES {
            let Ok(image) = assets.image(&format!("{BASE_PATH}_{file}.png")) else {
                return Ok(None);
            };
            if image.width() <= 1 || image.height() <= 1 {
                return Ok(None);
            }
            faces.push(image::imageops::flip_vertical(&image));
        }
        let faces: [RgbaImage; 6] = faces
            .try_into()
            .map_err(|_| anyhow::anyhow!("Panorama: unexpected number of faces"))?;
        Ok(Some(Self {
            cubemap: renderer.load_cubemap(&faces)?,
            spin_degrees: 0.0,
        }))
    }

    /// Frees the cube map.
    pub fn release(self, renderer: &mut Renderer) {
        renderer.free_cubemap(self.cubemap);
    }

    pub fn tick(&mut self, dt: Duration) {
        self.spin_degrees =
            (self.spin_degrees + dt.as_secs_f32() * DEGREES_PER_SECOND).rem_euclid(360.0);
    }

    pub fn scene(&self) -> SceneFrame {
        let view = Mat4::rot_x(PI)
            * Mat4::rot_x_deg(PITCH_DEGREES)
            * Mat4::rot_y(-self.spin_degrees.to_radians());
        SceneFrame {
            sky: Some(Sky {
                cubemap: Some(self.cubemap),
                view,
                fov_y_degrees: FOV_Y_DEGREES,
                celestials: Vec::new(),
            }),
            world: None,
            entities: Vec::new(),
            particles: Vec::new(),
            clouds: None,
            weather: Vec::new(),
            lines: Vec::new(),
            glint_item: None,
            glint_armor: None,
            glowing: Vec::new(),
            post_effects: Vec::new(),
        }
    }
}
