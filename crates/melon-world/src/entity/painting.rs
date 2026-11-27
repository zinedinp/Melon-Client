//! `PaintingRenderer`: a canvas a block per segment, each segment lit by
//! the block it hangs in, with the back sprite on its back and edges.

use glam::{DVec3, IVec3, Vec3};
use pumpkin_data::painting_variant::PaintingVariant;

use crate::geometry;
use crate::mesh::WorldVertex;

/// `PaintingRenderer.BACK_SPRITE_LOCATION`, on the paintings atlas.
pub const BACK_TEXTURE: &str = "minecraft:painting/back";

/// Half the canvas's thickness (`edgeHalfWidth`).
const HALF_DEPTH: f32 = 0.031_25;

/// The variant by its synced registry id.
pub fn variant(id: i32) -> Option<PaintingVariant> {
    PaintingVariant::all()
        .get(usize::try_from(id).ok()?)
        .copied()
}

/// The front's texture: `painting/<name>` of the variant's asset id.
pub fn front_texture(variant: PaintingVariant) -> String {
    let id = variant.asset_id();
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("{namespace}:painting/{path}")
}

/// `Direction.get2DDataValue` of a `Direction.get3DDataValue`: south 0,
/// west 1, north 2, east 3; `None` for up and down.
fn horizontal(direction: i32) -> Option<i32> {
    match direction {
        2 => Some(2),
        3 => Some(0),
        4 => Some(1),
        5 => Some(3),
        _ => None,
    }
}

/// `extractRenderState`: the block each segment takes its light from,
/// by `segment_x + segment_y * width`.
pub fn light_blocks(variant: PaintingVariant, direction: i32, center: DVec3) -> Vec<IVec3> {
    let (w, h) = (variant.width() as i32, variant.height() as i32);
    let (offset_x, offset_y) = (-w as f64 / 2.0, -h as f64 / 2.0);
    let block = center.floor().as_ivec3();
    let mut out = Vec::with_capacity((w * h) as usize);
    for segment_y in 0..h {
        for segment_x in 0..w {
            let sx = f64::from(segment_x) + offset_x + 0.5;
            let sy = f64::from(segment_y) + offset_y + 0.5;
            let mut at = IVec3::new(block.x, (center.y + sy).floor() as i32, block.z);
            match direction {
                2 => at.x = (center.x + sx).floor() as i32,
                4 => at.z = (center.z - sx).floor() as i32,
                3 => at.x = (center.x - sx).floor() as i32,
                5 => at.z = (center.z + sx).floor() as i32,
                _ => {}
            }
            out.push(at);
        }
    }
    out
}

/// The canvas's front and its back with the edges, relative to `origin`,
/// each segment with its light (block, sky levels).
pub fn quads(
    variant: PaintingVariant,
    direction: i32,
    center: DVec3,
    origin: IVec3,
    lights: &[[u8; 2]],
) -> Option<(Vec<WorldVertex>, Vec<WorldVertex>)> {
    let d2 = horizontal(direction)?;
    let (w, h) = (variant.width() as i32, variant.height() as i32);
    let turn = glam::Quat::from_rotation_y(((180 - d2 * 90) as f32).to_radians());
    let at = (center - origin.as_dvec3()).as_vec3();
    let (offset_x, offset_y) = (-w as f32 / 2.0, -h as f32 / 2.0);
    let mut front = Vec::new();
    let mut back = Vec::new();
    let push = |out: &mut Vec<WorldVertex>,
                corners: [(f32, f32, f32, f32, f32); 4],
                normal: Vec3,
                light: [u8; 2]| {
        let world_normal = turn * normal;
        let shade = geometry::shade(world_normal);
        let corners =
            corners.map(|(x, y, z, u, v)| ((at + turn * Vec3::new(x, y, z)).to_array(), [u, v]));
        let start = out.len();
        geometry::push_quad(out, corners, [shade; 3], false);
        for vertex in &mut out[start..] {
            vertex.light = [light[0] * 16, light[1] * 16];
        }
    };
    let (du, dv) = (1.0 / w as f32, 1.0 / h as f32);
    let z = HALF_DEPTH;
    for segment_x in 0..w {
        for segment_y in 0..h {
            let x0 = offset_x + (segment_x + 1) as f32;
            let x1 = offset_x + segment_x as f32;
            let y0 = offset_y + (segment_y + 1) as f32;
            let y1 = offset_y + segment_y as f32;
            let light = lights
                .get((segment_x + segment_y * w) as usize)
                .copied()
                .unwrap_or([0, 15]);
            let fu0 = du * (w - segment_x) as f32;
            let fu1 = du * (w - (segment_x + 1)) as f32;
            let fv0 = dv * (h - segment_y) as f32;
            let fv1 = dv * (h - (segment_y + 1)) as f32;
            push(
                &mut front,
                [
                    (x0, y1, -z, fu1, fv0),
                    (x1, y1, -z, fu0, fv0),
                    (x1, y0, -z, fu0, fv1),
                    (x0, y0, -z, fu1, fv1),
                ],
                Vec3::NEG_Z,
                light,
            );
            push(
                &mut back,
                [
                    (x0, y0, z, 1.0, 0.0),
                    (x1, y0, z, 0.0, 0.0),
                    (x1, y1, z, 0.0, 1.0),
                    (x0, y1, z, 1.0, 1.0),
                ],
                Vec3::Z,
                light,
            );
            let edge = 0.0625;
            if segment_y == h - 1 {
                push(
                    &mut back,
                    [
                        (x0, y0, -z, 0.0, 0.0),
                        (x1, y0, -z, 1.0, 0.0),
                        (x1, y0, z, 1.0, edge),
                        (x0, y0, z, 0.0, edge),
                    ],
                    Vec3::Y,
                    light,
                );
            }
            if segment_y == 0 {
                push(
                    &mut back,
                    [
                        (x0, y1, z, 0.0, 0.0),
                        (x1, y1, z, 1.0, 0.0),
                        (x1, y1, -z, 1.0, edge),
                        (x0, y1, -z, 0.0, edge),
                    ],
                    Vec3::NEG_Y,
                    light,
                );
            }
            if segment_x == w - 1 {
                push(
                    &mut back,
                    [
                        (x0, y0, z, edge, 0.0),
                        (x0, y1, z, edge, 1.0),
                        (x0, y1, -z, 0.0, 1.0),
                        (x0, y0, -z, 0.0, 0.0),
                    ],
                    Vec3::NEG_X,
                    light,
                );
            }
            if segment_x == 0 {
                push(
                    &mut back,
                    [
                        (x1, y0, -z, edge, 0.0),
                        (x1, y1, -z, edge, 1.0),
                        (x1, y1, z, 0.0, 1.0),
                        (x1, y0, z, 0.0, 0.0),
                    ],
                    Vec3::X,
                    light,
                );
            }
        }
    }
    Some((front, back))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kebab_is_one_block_with_its_back_and_four_edges() {
        let kebab = PaintingVariant::Kebab;
        let center = DVec3::new(0.5, 64.5, 0.031_25);
        let lights = light_blocks(kebab, 3, center);
        assert_eq!(lights, vec![IVec3::new(0, 64, 0)]);
        let (front, back) = quads(kebab, 3, center, IVec3::new(0, 64, 0), &[[0, 15]]).unwrap();
        assert!(!front.is_empty());
        assert_eq!(back.len(), 5 * front.len());
    }

    #[test]
    fn the_front_texture_is_on_the_paintings_atlas() {
        assert_eq!(
            front_texture(PaintingVariant::Kebab),
            "minecraft:painting/kebab"
        );
    }

    #[test]
    fn a_painting_on_the_floor_is_not_drawn() {
        let center = DVec3::ZERO;
        assert!(quads(PaintingVariant::Kebab, 0, center, IVec3::ZERO, &[]).is_none());
    }
}
