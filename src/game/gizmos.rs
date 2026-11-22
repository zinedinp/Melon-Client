//! Debug lines in the world (vanilla's `Gizmos`).

use glam::{DVec3, IVec3, Mat4, Vec3};

use crate::gui::text::font::Font;
use crate::renderer::quad::{DrawList, Vertex};
use crate::renderer::{
    Blending, EntityDraw, LineDraw, LineStyle, MeshId, RenderLayer, Renderer, TextureId,
    WorldVertex,
};
use crate::world::block::direction::Direction;
use crate::world::physics::Aabb;

/// `GizmoStyle.stroke`'s width, in pixels.
pub const DEFAULT_WIDTH: f32 = 2.5;
/// How far ahead of the eye a line is cut off (`GizmoFeatureRenderer.buildLines`).
const NEAR: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub a: DVec3,
    pub b: DVec3,
    /// `ARGB.colorFromFloat`: red, green, blue, alpha.
    pub color: [f32; 4],
    /// Pixels wide, whatever the window's size (`LineGizmo.width`).
    pub width: f32,
}

/// `PointGizmo`: a square `size` pixels wide on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub at: DVec3,
    pub color: [f32; 4],
    pub size: f32,
}

/// A filled quad (`GizmoStyle.fill`), its corners in order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fill {
    pub corners: [DVec3; 4],
    pub color: [f32; 4],
}

/// `TextGizmo`: text turned to the camera, centred on `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldText {
    pub at: DVec3,
    pub text: String,
    pub color: [f32; 4],
    /// `TextGizmo.Style.scale`: sixteenths of a block per text pixel.
    pub scale: f32,
}

/// `TextGizmo.Style.DEFAULT_SCALE`.
pub const TEXT_SCALE: f32 = 0.32;

/// The lines, faces and text of one frame.
#[derive(Default)]
pub struct Gizmos {
    lines: Vec<Line>,
    points: Vec<Point>,
    fills: Vec<Fill>,
    texts: Vec<WorldText>,
}

impl Gizmos {
    pub fn line(&mut self, a: DVec3, b: DVec3, color: [f32; 4], width: f32) {
        self.lines.push(Line { a, b, color, width });
    }

    /// `Gizmos.cuboid`: the twelve edges.
    pub fn cuboid(&mut self, bounds: &Aabb, color: [f32; 4], width: f32) {
        let (lo, hi) = (bounds.min, bounds.max);
        let corner = |x: bool, y: bool, z: bool| {
            DVec3::new(
                if x { hi.x } else { lo.x },
                if y { hi.y } else { lo.y },
                if z { hi.z } else { lo.z },
            )
        };
        for (y, z) in [(false, false), (true, false), (false, true), (true, true)] {
            self.line(corner(false, y, z), corner(true, y, z), color, width);
        }
        for (x, z) in [(false, false), (true, false), (false, true), (true, true)] {
            self.line(corner(x, false, z), corner(x, true, z), color, width);
        }
        for (x, y) in [(false, false), (true, false), (false, true), (true, true)] {
            self.line(corner(x, y, false), corner(x, y, true), color, width);
        }
    }

    /// `Gizmos.point`: a square `size` pixels wide on screen.
    pub fn point(&mut self, at: DVec3, color: [f32; 4], size: f32) {
        self.points.push(Point { at, color, size });
    }

    /// `ArrowGizmo`: the line and four barbs at 45 degrees back from its
    /// tip, a tenth of its length long (0.1 to 1 block) along it and as far
    /// out.
    pub fn arrow(&mut self, from: DVec3, to: DVec3, color: [f32; 4], width: f32) {
        self.line(from, to, color, width);
        let Some(dir) = (to - from).as_vec3().try_normalize() else {
            return;
        };
        let rotation = glam::Quat::from_rotation_arc(Vec3::X, dir);
        let len = (to.distance(from) as f32 * 0.1).clamp(0.1, 1.0);
        for tip in [
            Vec3::new(-len, len, 0.0),
            Vec3::new(-len, 0.0, len),
            Vec3::new(-len, -len, 0.0),
            Vec3::new(-len, 0.0, -len),
        ] {
            self.line(to + (rotation * tip).as_dvec3(), to, color, width);
        }
    }

    /// `Gizmos.rect` with a fill: the face of the box `min`..`max` toward
    /// `face`.
    pub fn face(&mut self, min: DVec3, max: DVec3, face: Direction, color: [f32; 4]) {
        let (lo, hi) = (min, max);
        let corners = match face {
            Direction::Down => [
                DVec3::new(lo.x, lo.y, lo.z),
                DVec3::new(hi.x, lo.y, lo.z),
                DVec3::new(hi.x, lo.y, hi.z),
                DVec3::new(lo.x, lo.y, hi.z),
            ],
            Direction::Up => [
                DVec3::new(lo.x, hi.y, lo.z),
                DVec3::new(lo.x, hi.y, hi.z),
                DVec3::new(hi.x, hi.y, hi.z),
                DVec3::new(hi.x, hi.y, lo.z),
            ],
            Direction::North => [
                DVec3::new(lo.x, lo.y, lo.z),
                DVec3::new(lo.x, hi.y, lo.z),
                DVec3::new(hi.x, hi.y, lo.z),
                DVec3::new(hi.x, lo.y, lo.z),
            ],
            Direction::South => [
                DVec3::new(lo.x, lo.y, hi.z),
                DVec3::new(hi.x, lo.y, hi.z),
                DVec3::new(hi.x, hi.y, hi.z),
                DVec3::new(lo.x, hi.y, hi.z),
            ],
            Direction::West => [
                DVec3::new(lo.x, lo.y, lo.z),
                DVec3::new(lo.x, lo.y, hi.z),
                DVec3::new(lo.x, hi.y, hi.z),
                DVec3::new(lo.x, hi.y, lo.z),
            ],
            Direction::East => [
                DVec3::new(hi.x, lo.y, lo.z),
                DVec3::new(hi.x, hi.y, lo.z),
                DVec3::new(hi.x, hi.y, hi.z),
                DVec3::new(hi.x, lo.y, hi.z),
            ],
        };
        self.fills.push(Fill { corners, color });
    }

    /// `Gizmos.cuboid` with a fill: all six faces.
    pub fn filled_cuboid(&mut self, bounds: &Aabb, color: [f32; 4]) {
        for face in Direction::ALL {
            self.face(bounds.min, bounds.max, face, color);
        }
    }

    /// `Gizmos.billboardText`.
    pub fn text(&mut self, at: DVec3, text: impl Into<String>, color: [f32; 4], scale: f32) {
        self.texts.push(WorldText {
            at,
            text: text.into(),
            color,
            scale,
        });
    }

    pub fn extend_fills(&mut self, fills: &[Fill]) {
        self.fills.extend_from_slice(fills);
    }

    pub fn extend_lines(&mut self, lines: &[Line]) {
        self.lines.extend_from_slice(lines);
    }

    pub fn extend_texts(&mut self, texts: &[WorldText]) {
        self.texts.extend_from_slice(texts);
    }

    pub fn take_fills(&mut self) -> Vec<Fill> {
        std::mem::take(&mut self.fills)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.lines.len()
    }
}

/// The meshes of the last frame's gizmos: the lines, the filled faces
/// (blended), and the text, one mesh per font page.
#[derive(Default)]
pub struct GizmoMesh {
    /// The opaque lines and points, then the see-through ones
    /// (`DrawableGizmoPrimitives`' two groups).
    lines: [Slot; 2],
    fills: Slot,
    texts: Vec<(TextureId, Slot)>,
}

/// The font and the camera's axes, for [`Gizmos::text`].
#[derive(Clone, Copy)]
pub struct TextView<'a> {
    pub font: &'a Font,
    pub pages: &'a [TextureId],
    /// The camera's right and up, in the world.
    pub right: Vec3,
    pub up: Vec3,
}

/// One uploaded mesh, kept while its vertices stay the same.
#[derive(Default)]
struct Slot {
    mesh: Option<MeshId>,
    built: Vec<WorldVertex>,
    origin: IVec3,
}

impl Slot {
    fn update(&mut self, renderer: &mut Renderer, vertices: Vec<WorldVertex>, origin: IVec3) {
        if vertices.is_empty() {
            self.release(renderer);
            return;
        }
        if self.mesh.is_some() && self.origin == origin && self.built == vertices {
            return;
        }
        self.release(renderer);
        match renderer.upload_mesh(&vertices) {
            Ok(mesh) => {
                self.mesh = Some(mesh);
                self.built = vertices;
                self.origin = origin;
            }
            Err(e) => crate::warn!("Gizmo mesh failed: {e:#}"),
        }
    }

    fn release(&mut self, renderer: &mut Renderer) {
        if let Some(mesh) = self.mesh.take() {
            renderer.free_mesh(mesh);
        }
        self.built.clear();
    }

    fn lines(&self, style: LineStyle) -> Option<LineDraw> {
        Some(LineDraw {
            mesh: self.mesh?,
            model: Mat4::IDENTITY,
            origin: self.origin,
            style,
        })
    }

    fn draw(&self, atlas: TextureId, layer: RenderLayer, blend: Blending) -> Option<EntityDraw> {
        Some(EntityDraw {
            scroll: None,
            alpha: 1.0,
            mesh: self.mesh?,
            atlas,
            model: Mat4::IDENTITY,
            origin: self.origin,
            layer,
            blend,
            light: None,
            overlay: 0.0,
            glint: crate::renderer::Glint::None,
        })
    }
}

impl GizmoMesh {
    /// `gizmos` seen from `camera`, looking along `forward`: the lines and
    /// points into `lines`, the rest into `out`. Text needs `text`; without
    /// it, none is drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        gizmos: &Gizmos,
        (camera, forward): (DVec3, Vec3),
        text: Option<TextView<'_>>,
        out: &mut Vec<EntityDraw>,
        lines: &mut Vec<LineDraw>,
    ) {
        let origin = camera.floor().as_ivec3();
        let white = renderer.white_texture();
        for (slot, opaque) in self.lines.iter_mut().zip([true, false]) {
            let vertices = line_quads(gizmos, opaque, origin, (camera, forward.as_dvec3()));
            slot.update(renderer, vertices, origin);
        }
        let fills = fill_quads(&gizmos.fills, origin);
        self.fills.update(renderer, fills, origin);
        let mut pages: Vec<(TextureId, Vec<WorldVertex>)> = Vec::new();
        if let Some(view) = text {
            text_quads(&gizmos.texts, origin, view, &mut pages);
        }
        self.texts.retain_mut(|(texture, slot)| {
            if pages.iter().any(|(t, _)| t == texture) {
                true
            } else {
                slot.release(renderer);
                false
            }
        });
        for (texture, vertices) in pages {
            let at = match self.texts.iter().position(|(t, _)| *t == texture) {
                Some(at) => at,
                None => {
                    self.texts.push((texture, Slot::default()));
                    self.texts.len() - 1
                }
            };
            self.texts[at].1.update(renderer, vertices, origin);
        }
        lines.extend(self.lines[0].lines(LineStyle::Opaque));
        lines.extend(self.lines[1].lines(LineStyle::TranslucentNoDepthWrite));
        out.extend(
            self.fills
                .draw(white, RenderLayer::Shadows, Blending::Translucent),
        );
        for (texture, slot) in &self.texts {
            out.extend(slot.draw(*texture, RenderLayer::Entities, Blending::Text));
        }
    }

    pub fn release(&mut self, renderer: &mut Renderer) {
        for slot in &mut self.lines {
            slot.release(renderer);
        }
        self.fills.release(renderer);
        for (_, mut slot) in self.texts.drain(..) {
            slot.release(renderer);
        }
    }
}

/// Both sides of each filled quad, relative to `origin`.
fn fill_quads(fills: &[Fill], origin: IVec3) -> Vec<WorldVertex> {
    let origin = origin.as_dvec3();
    let mut out = Vec::with_capacity(fills.len() * 8);
    for fill in fills {
        let q = fill
            .corners
            .map(|p| WorldVertex::rgba((p - origin).as_vec3().to_array(), [0.0, 0.0], fill.color));
        out.extend_from_slice(&q);
        out.extend_from_slice(&[q[3], q[2], q[1], q[0]]);
    }
    out
}

/// `GizmoFeatureRenderer.buildTexts`: each text laid out by the font, at
/// `scale / 16` blocks a pixel, turned to the camera; one vertex list per
/// font page.
fn text_quads(
    texts: &[WorldText],
    origin: IVec3,
    view: TextView<'_>,
    pages: &mut Vec<(TextureId, Vec<WorldVertex>)>,
) {
    let origin = origin.as_dvec3();
    let mut list = DrawList::new();
    for text in texts {
        list.clear();
        let width = view.font.text_width(&text.text);
        view.font.push_text_flat(
            &mut list,
            view.pages,
            &text.text,
            -width / 2.0,
            0.0,
            text.color,
        );
        let pixel = text.scale / 16.0;
        let at = (text.at - origin).as_vec3();
        let place = |v: &Vertex| {
            let p = at + view.right * (v.pos[0] * pixel) - view.up * (v.pos[1] * pixel);
            WorldVertex::rgba(p.to_array(), v.uv, v.color)
        };
        for call in &list.calls {
            let from = (call.first / 6 * 4) as usize;
            let quads = &list.vertices[from..from + (call.count / 6 * 4) as usize];
            let at = match pages.iter().position(|(t, _)| *t == call.texture) {
                Some(at) => at,
                None => {
                    pages.push((call.texture, Vec::new()));
                    pages.len() - 1
                }
            };
            for quad in quads.as_chunks::<4>().0 {
                let q = [
                    place(&quad[0]),
                    place(&quad[1]),
                    place(&quad[2]),
                    place(&quad[3]),
                ];
                pages[at].1.extend_from_slice(&q);
                pages[at].1.extend_from_slice(&[q[3], q[2], q[1], q[0]]);
            }
        }
    }
}

/// Whether `color` is in `DrawableGizmoPrimitives`' opaque group.
fn is_opaque(color: [f32; 4]) -> bool {
    (color[3].clamp(0.0, 1.0) * 255.0).round() >= 255.0
}

/// The lines and points of one group (`opaque` or not) as
/// [`WorldVertex::line`]s and [`WorldVertex::point`]s relative to `origin`.
/// A line is cut off where it passes the near plane `camera` sees along
/// `forward` (`GizmoFeatureRenderer.buildLines`); one wholly behind it is
/// left out.
fn line_quads(
    gizmos: &Gizmos,
    opaque: bool,
    origin: IVec3,
    (camera, forward): (DVec3, DVec3),
) -> Vec<WorldVertex> {
    let origin = origin.as_dvec3();
    let ahead = |p: DVec3| (p - camera).dot(forward);
    let mut out = Vec::new();
    for line in gizmos.lines.iter().filter(|l| is_opaque(l.color) == opaque) {
        let (mut a, mut b) = (line.a, line.b);
        let (behind_a, behind_b) = (ahead(a) < NEAR, ahead(b) < NEAR);
        if behind_a && behind_b {
            continue;
        }
        if behind_a || behind_b {
            let denom = ahead(b) - ahead(a);
            if denom.abs() < 1.0e-9 {
                continue;
            }
            let cut = a.lerp(b, ((NEAR - ahead(a)) / denom).clamp(0.0, 1.0));
            if behind_a {
                a = cut;
            } else {
                b = cut;
            }
        }
        let (a, b) = ((a - origin).as_vec3(), (b - origin).as_vec3());
        out.extend(
            WorldVertex::line(a, b, line.color, line.width)
                .into_iter()
                .flatten(),
        );
    }
    for point in gizmos
        .points
        .iter()
        .filter(|p| is_opaque(p.color) == opaque)
    {
        let at = (point.at - origin).as_vec3();
        out.extend(WorldVertex::point(at, point.color, point.size));
    }
    out
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
    fn a_cuboid_is_twelve_line_quads() {
        let mut gizmos = Gizmos::default();
        gizmos.cuboid(&cube(), [1.0; 4], DEFAULT_WIDTH);
        assert_eq!(gizmos.len(), 12);
        let camera = (DVec3::new(0.5, 0.5, -5.0), DVec3::Z);
        let vertices = line_quads(&gizmos, true, IVec3::ZERO, camera);
        assert_eq!(vertices.len(), 48);
        assert!(line_quads(&gizmos, false, IVec3::ZERO, camera).is_empty());
        for v in &vertices {
            assert_eq!(u16::from_le_bytes(v.light), 40, "2.5 px in sixteenths");
        }
    }

    #[test]
    fn lines_are_cut_off_at_the_near_plane() {
        let mut gizmos = Gizmos::default();
        gizmos.line(
            DVec3::new(0.0, 0.0, -3.0),
            DVec3::new(0.0, 0.0, 3.0),
            [1.0; 4],
            1.0,
        );
        gizmos.line(
            DVec3::new(0.0, 1.0, -3.0),
            DVec3::new(0.0, 1.0, -1.0),
            [1.0; 4],
            1.0,
        );
        let vertices = line_quads(&gizmos, true, IVec3::ZERO, (DVec3::ZERO, DVec3::Z));
        assert_eq!(vertices.len(), 4, "the line wholly behind is left out");
        let near = vertices.iter().map(|v| v.pos[2]).fold(f32::MAX, f32::min);
        assert!((near - NEAR as f32).abs() < 1.0e-6, "{near}");
    }

    #[test]
    fn see_through_lines_and_points_go_to_their_own_group() {
        let mut gizmos = Gizmos::default();
        gizmos.line(DVec3::ZERO, DVec3::X, [1.0, 0.0, 0.0, 0.5], 1.0);
        gizmos.point(DVec3::new(0.0, 0.0, 2.0), [1.0; 4], 2.0);
        let camera = (DVec3::new(0.0, 0.0, -5.0), DVec3::Z);
        let opaque = line_quads(&gizmos, true, IVec3::ZERO, camera);
        let see_through = line_quads(&gizmos, false, IVec3::ZERO, camera);
        assert_eq!((opaque.len(), see_through.len()), (4, 4));
        assert!(
            opaque.iter().all(|v| (2..6).contains(&v.face)),
            "a point's corners"
        );
    }

    #[test]
    fn an_arrows_barbs_are_a_tenth_of_it_back_and_out() {
        let mut gizmos = Gizmos::default();
        gizmos.arrow(
            DVec3::ZERO,
            DVec3::new(2.0, 0.0, 0.0),
            [1.0; 4],
            DEFAULT_WIDTH,
        );
        assert_eq!(gizmos.len(), 5);
        for barb in &gizmos.lines[1..] {
            assert_eq!(barb.b, DVec3::new(2.0, 0.0, 0.0));
            let back = barb.a - barb.b;
            assert!((back.x + 0.2).abs() < 1.0e-6, "{back}");
            assert!((back.y.abs() + back.z.abs() - 0.2).abs() < 1.0e-6, "{back}");
        }
    }
}
