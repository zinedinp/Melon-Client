//! `EndermiteModel` and `SilverfishModel`: a row of boxes that wriggle

use super::{ClientEntity, EntityKind, LayerSpec, ModelId as M};
use crate::math::angle::{self, PI};
use crate::world::entity::living::LivingRenderState;
use crate::world::entity::model::{LayerDefinition, Model, ModelPose, PartDefinition, PartId};
use crate::world::model_part::{Cube, Pose};

/// What one of the two models is made of.
struct Shape {
    /// Width, height and depth of each segment, front to back.
    sizes: &'static [[i32; 3]],
    /// Where each segment's texture starts.
    texs: &'static [[i32; 2]],
    /// Yaw and sideways sway, per segment away from the middle one.
    sway: (f32, f32),
    /// Extra boxes laid over a segment: texture, box and which segment
    layers: &'static [Layer],
}

struct Layer {
    tex: [i32; 2],
    /// Left edge, width and height.
    span: (f32, f32, f32),
    /// The segment whose depth sets the box's start (half of it, back
    start_of: usize,
    depth_of: usize,
    y: f32,
    /// The segment it stands at and turns with.
    at: usize,
    /// Slides with that segment too, not just turns.
    sways: bool,
}

const ENDERMITE: Shape = Shape {
    sizes: &[[4, 3, 2], [6, 4, 5], [3, 3, 1], [1, 2, 1]],
    texs: &[[0, 0], [0, 5], [0, 14], [0, 18]],
    sway: (0.01, 0.1),
    layers: &[],
};

const SILVERFISH: Shape = Shape {
    sizes: &[
        [3, 2, 2],
        [4, 3, 2],
        [6, 4, 3],
        [3, 3, 3],
        [2, 2, 3],
        [2, 1, 2],
        [1, 1, 2],
    ],
    texs: &[[0, 0], [0, 4], [0, 9], [0, 16], [0, 22], [11, 0], [13, 4]],
    sway: (0.05, 0.2),
    layers: &[
        Layer {
            tex: [20, 0],
            span: (-5.0, 10.0, 8.0),
            start_of: 2,
            depth_of: 2,
            y: 16.0,
            at: 2,
            sways: false,
        },
        Layer {
            tex: [20, 11],
            span: (-3.0, 6.0, 4.0),
            start_of: 4,
            depth_of: 4,
            y: 20.0,
            at: 4,
            sways: true,
        },
        Layer {
            tex: [20, 18],
            span: (-3.0, 6.0, 5.0),
            start_of: 4,
            depth_of: 1,
            y: 19.0,
            at: 1,
            sways: true,
        },
    ],
};

const SEGMENTS: [&str; 7] = [
    "segment0", "segment1", "segment2", "segment3", "segment4", "segment5", "segment6",
];
const LAYERS: [&str; 3] = ["layer0", "layer1", "layer2"];

fn layer_of(shape: &Shape) -> LayerDefinition {
    let mut root = PartDefinition::root();
    let mut z = -3.5;
    let mut places = Vec::with_capacity(shape.sizes.len());
    for (i, (&[w, h, d], &[u, v])) in shape.sizes.iter().zip(shape.texs).enumerate() {
        let (w, h, d) = (w as f32, h as f32, d as f32);
        root.add_or_replace_child(
            SEGMENTS[i],
            vec![Cube::new(
                (u as f32, v as f32),
                (w * -0.5, 0.0, d * -0.5),
                (w, h, d),
            )],
            Pose::offset(0.0, 24.0 - h, z),
        );
        places.push(z);
        if let Some(&[_, _, next]) = shape.sizes.get(i + 1) {
            z += (d + next as f32) * 0.5;
        }
    }
    for (i, layer) in shape.layers.iter().enumerate() {
        let (left, width, height) = layer.span;
        root.add_or_replace_child(
            LAYERS[i],
            vec![Cube::new(
                (layer.tex[0] as f32, layer.tex[1] as f32),
                (left, 0.0, shape.sizes[layer.start_of][2] as f32 * -0.5),
                (width, height, shape.sizes[layer.depth_of][2] as f32),
            )],
            Pose::offset(0.0, layer.y, places[layer.at]),
        );
    }
    LayerDefinition::new(root, 64.0, 32.0)
}

pub fn endermite() -> LayerDefinition {
    layer_of(&ENDERMITE)
}

pub fn silverfish() -> LayerDefinition {
    layer_of(&SILVERFISH)
}

/// Which of the two a rig is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Endermite,
    Silverfish,
}

impl Kind {
    fn shape(self) -> &'static Shape {
        match self {
            Self::Endermite => &ENDERMITE,
            Self::Silverfish => &SILVERFISH,
        }
    }
}

const MAX_SEGMENTS: usize = 7;
const MAX_LAYERS: usize = 3;

#[derive(Clone, Copy, Debug)]
pub struct SegmentedParts {
    kind: Kind,
    segments: [PartId; MAX_SEGMENTS],
    layers: [PartId; MAX_LAYERS],
}

impl SegmentedParts {
    pub fn find(model: &Model, kind: Kind) -> Self {
        let shape = kind.shape();
        let first = model.part("segment0");
        let mut segments = [first; MAX_SEGMENTS];
        for (i, slot) in segments.iter_mut().enumerate().take(shape.sizes.len()) {
            *slot = model.part(SEGMENTS[i]);
        }
        let mut layers = [first; MAX_LAYERS];
        for (i, slot) in layers.iter_mut().enumerate().take(shape.layers.len()) {
            *slot = model.part(LAYERS[i]);
        }
        Self {
            kind,
            segments,
            layers,
        }
    }
}

/// `setupAnim`: each segment turns and slides with a phase a little
pub fn setup_anim(parts: &SegmentedParts, pose: &mut ModelPose, state: &LivingRenderState) {
    let shape = parts.kind.shape();
    let count = shape.sizes.len();
    let (turn, slide) = shape.sway;
    let mut yaws = [0.0; MAX_SEGMENTS];
    let mut xs = [0.0; MAX_SEGMENTS];
    for i in 0..count {
        let phase = state.age_in_ticks * 0.9 + i as f32 * 0.15 * PI;
        let from_middle = (i as i32 - 2).abs() as f32;
        yaws[i] = angle::cos(phase) * PI * turn * (1.0 + from_middle);
        xs[i] = angle::sin(phase) * PI * slide * from_middle;
        let part = &mut pose[parts.segments[i]].pose;
        part.rotation.y = yaws[i];
        part.offset.x = xs[i];
    }
    for (layer, &part) in shape.layers.iter().zip(&parts.layers) {
        let part = &mut pose[part].pose;
        part.rotation.y = yaws[layer.at];
        if layer.sways {
            part.offset.x = xs[layer.at];
        }
    }
}

/// `EndermiteRenderer` and `SilverfishRenderer`.
pub(super) fn push_layers(entity: &ClientEntity, out: &mut Vec<LayerSpec>) {
    let plain = LayerSpec::plain;
    match entity.kind {
        EntityKind::Endermite => {
            out.push(plain(M::Endermite, "minecraft:entity/endermite/endermite"))
        }
        EntityKind::Silverfish => out.push(plain(
            M::Silverfish,
            "minecraft:entity/silverfish/silverfish",
        )),
        _ => {}
    }
}
