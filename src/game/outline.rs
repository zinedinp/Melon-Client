//! The outline of the block looked at (`LevelRenderer.submitBlockOutline`):
//! its shape's edges, as lines a fixed number of pixels wide.

use glam::{DVec3, IVec3, Mat4};

use melon_render::{LineDraw, LineMeshId, LineStyle, Renderer};
use melon_world::level::Level;
use melon_world::mesh::LineSegment;
use melon_world::physics::Aabb;
use melon_world::pick::{self, BlockHit};

/// `ARGB.black(102)`.
const COLOR: [f32; 4] = [0.0, 0.0, 0.0, 102.0 / 255.0];
/// The high contrast outline's `0xFF57FFE1`, over a black one this wide.
const HIGH_CONTRAST: [f32; 4] = [0x57 as f32 / 255.0, 1.0, 0xE1 as f32 / 255.0, 1.0];
const HIGH_CONTRAST_UNDER_WIDTH: f32 = 7.0;
/// How far from an edge its corners are looked at, to tell a real edge
/// from a seam between two boxes.
const PROBE: f64 = 1.0e-4;

/// What the meshes were built for.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    pos: [i32; 3],
    state: u16,
    /// The line width in sixteenths of a pixel.
    width: u32,
    high_contrast: bool,
}

#[derive(Default)]
pub struct BlockOutline {
    /// The outline's line meshes and how each is drawn, for `key`.
    meshes: Vec<(LineMeshId, LineStyle)>,
    key: Option<Key>,
}

impl BlockOutline {
    /// The outline of `hit`, if any, into `out` (`submitBlockOutline`):
    /// lines `line_width` pixels wide (`Window.getAppropriateLineWidth`).
    /// `translucent` tells whether a block is drawn with the translucent
    /// terrain, which its outline then follows; `high_contrast` is
    /// `Options.highContrastBlockOutline`.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        level: &Level,
        hit: Option<BlockHit>,
        (translucent, high_contrast): (impl Fn(u16) -> bool, bool),
        line_width: f32,
        out: &mut Vec<LineDraw>,
    ) {
        // TODO(parity): vanilla also skips a block outside the world border
        // (`WorldBorder.isWithinBounds`); Melon doesn't track the border yet.
        let hit = hit.filter(|hit| {
            let [x, y, z] = hit.pos;
            level.block_state(x, y, z).is_some_and(|state| state != 0)
        });
        let Some(hit) = hit else {
            self.release(renderer);
            return;
        };
        let [x, y, z] = hit.pos;
        let state = level.block_state(x, y, z).unwrap_or(0);
        let key = Key {
            pos: hit.pos,
            state,
            width: (line_width * 16.0).round() as u32,
            high_contrast,
        };
        if self.key != Some(key) {
            self.release(renderer);
            self.key = Some(key);
            let boxes: Vec<Aabb> = pick::outline_shapes(level, (x, y, z)).collect();
            let edges = edges(&boxes);
            let origin = IVec3::from_array(hit.pos);
            // An opaque outline goes with the solid things
            // (`SubmitNodeCollection.submitShapeOutline`), its cyan over
            // the black by a depth bias; a see-through one after the blob
            // shadows, or after the translucent terrain its block is in.
            let parts = if high_contrast {
                vec![
                    (
                        HIGH_CONTRAST_UNDER_WIDTH,
                        [0.0, 0.0, 0.0, 1.0],
                        LineStyle::Opaque,
                    ),
                    (line_width, HIGH_CONTRAST, LineStyle::OpaqueDepthBias),
                ]
            } else if translucent(state) {
                vec![(line_width, COLOR, LineStyle::TranslucentAfterTerrain)]
            } else {
                vec![(line_width, COLOR, LineStyle::Translucent)]
            };
            for (width, color, style) in parts {
                let segments = lines(&edges, origin, color, width);
                if segments.is_empty() {
                    continue;
                }
                match renderer.upload_lines(&segments) {
                    Ok(mesh) => self.meshes.push((mesh, style)),
                    Err(e) => melon_base::warn!("Outline mesh failed: {e:#}"),
                }
            }
        }
        out.extend(self.meshes.iter().map(|&(mesh, style)| LineDraw {
            mesh,
            model: Mat4::IDENTITY,
            origin: IVec3::from_array(hit.pos),
            style,
        }));
    }

    fn release(&mut self, renderer: &mut Renderer) {
        for (mesh, _) in self.meshes.drain(..) {
            renderer.free_lines(mesh);
        }
        self.key = None;
    }

    pub fn clear(&mut self, renderer: &mut Renderer) {
        self.release(renderer);
    }
}

/// An edge of a shape: its two ends.
type Edge = (DVec3, DVec3);

/// `VoxelShape.forAllEdges`: the edges of the union of `boxes`.
fn edges(boxes: &[Aabb]) -> Vec<Edge> {
    let inside = |p: DVec3| {
        boxes
            .iter()
            .any(|b| (0..3).all(|i| p[i] >= b.min[i] - 1.0e-9 && p[i] <= b.max[i] + 1.0e-9))
    };
    let mut out: Vec<Edge> = Vec::new();
    for b in boxes {
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for &cu in &[b.min[u], b.max[u]] {
                for &cv in &[b.min[v], b.max[v]] {
                    let mut a = DVec3::ZERO;
                    a[axis] = b.min[axis];
                    a[u] = cu;
                    a[v] = cv;
                    let mut e = a;
                    e[axis] = b.max[axis];
                    let mid = (a + e) / 2.0;
                    let mut quarter = [[false; 2]; 2];
                    for (i, su) in [-1.0, 1.0].into_iter().enumerate() {
                        for (j, sv) in [-1.0, 1.0].into_iter().enumerate() {
                            let mut p = mid;
                            p[u] += su * PROBE;
                            p[v] += sv * PROBE;
                            quarter[i][j] = inside(p);
                        }
                    }
                    let count = quarter.iter().flatten().filter(|&&q| q).count();
                    let diagonal = count == 2 && quarter[0][0] == quarter[1][1];
                    if !(count == 1 || count == 3 || diagonal) {
                        continue;
                    }
                    let same = |p: Edge| {
                        (p.0.distance(a) < 1.0e-6 && p.1.distance(e) < 1.0e-6)
                            || (p.0.distance(e) < 1.0e-6 && p.1.distance(a) < 1.0e-6)
                    };
                    if !out.iter().any(|&p| same(p)) {
                        out.push((a, e));
                    }
                }
            }
        }
    }
    out
}

/// Every edge as a line `width` pixels wide (`ShapeOutlineFeatureRenderer`),
/// relative to `origin`.
fn lines(edges: &[Edge], origin: IVec3, color: [f32; 4], width: f32) -> Vec<LineSegment> {
    let origin = origin.as_dvec3();
    let style = LineSegment::style(color, width);
    edges
        .iter()
        .filter_map(|&(a, e)| {
            let (a, e) = ((a - origin).as_vec3(), (e - origin).as_vec3());
            LineSegment::line(a, e, style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Aabb {
        Aabb {
            min: DVec3::ZERO,
            max: DVec3::ONE,
        }
    }

    #[test]
    fn a_cube_has_twelve_edges() {
        assert_eq!(edges(&[cube()]).len(), 12);
    }

    #[test]
    fn two_boxes_forming_a_cube_leave_the_seam_out() {
        let lower = Aabb {
            min: DVec3::ZERO,
            max: DVec3::new(1.0, 0.5, 1.0),
        };
        let upper = Aabb {
            min: DVec3::new(0.0, 0.5, 0.0),
            max: DVec3::ONE,
        };
        let found = edges(&[lower, upper]);
        assert!(
            found.iter().all(|(a, b)| !(a.y == 0.5 && b.y == 0.5)),
            "{found:?}"
        );
    }

    #[test]
    fn a_stairs_step_keeps_its_inner_edge() {
        let lower = Aabb {
            min: DVec3::ZERO,
            max: DVec3::new(1.0, 0.5, 1.0),
        };
        let upper = Aabb {
            min: DVec3::new(0.0, 0.5, 0.5),
            max: DVec3::ONE,
        };
        let found = edges(&[lower, upper]);
        let has = |a: DVec3, b: DVec3| {
            found
                .iter()
                .any(|&(p, q)| (p == a && q == b) || (p == b && q == a))
        };
        assert!(has(DVec3::new(0.0, 0.5, 0.5), DVec3::new(1.0, 0.5, 0.5)));
        assert!(has(DVec3::new(0.0, 1.0, 0.5), DVec3::new(1.0, 1.0, 0.5)));
        assert!(!has(DVec3::new(0.0, 0.5, 1.0), DVec3::new(1.0, 0.5, 1.0)));
    }

    #[test]
    fn every_edge_is_one_line_of_the_width() {
        let found = edges(&[cube()]);
        let segments = lines(&found, IVec3::ZERO, COLOR, 2.5);
        assert_eq!(segments.len(), 12);
        for s in &segments {
            assert_eq!(s.width, 40);
            assert_eq!(s.color, [0, 0, 0, 102]);
        }
    }
}
