//! `LightningBoltRenderer`: a trunk and two branches of eight jagged
//! segments, four times over, each layer wider and fainter, added onto the
//! world. The shape comes from the bolt's seed.

use crate::geometry;
use crate::mesh::WorldVertex;

/// `java.util.Random`, as `RandomSource.createThreadLocalInstance` makes it
/// (`SingleThreadedRandomSource`).
struct JavaRandom(i64);

impl JavaRandom {
    const MULTIPLIER: i64 = 0x5_DEEC_E66D;

    fn new(seed: i64) -> Self {
        Self((seed ^ Self::MULTIPLIER) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as i32
    }

    /// `nextInt(bound)`.
    fn next_int(&mut self, bound: i32) -> i32 {
        if bound.isolate_lowest_one() == bound {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits - value + (bound - 1) >= 0 {
                return value;
            }
        }
    }
}

/// `BOLT_RED`, `BOLT_GREEN`, `BOLT_BLUE` and the alpha of every vertex.
const COLOR: [f32; 3] = [0.45, 0.45, 0.5];
const ALPHA: f32 = 0.3;

/// The bolt's quads around its foot, in blocks.
pub fn quads(seed: i64) -> Vec<WorldVertex> {
    let mut x_offsets = [0.0f32; 8];
    let mut z_offsets = [0.0f32; 8];
    let (mut x_offset, mut z_offset) = (0.0f32, 0.0f32);
    let mut random = JavaRandom::new(seed);
    for segment in (0..8).rev() {
        x_offsets[segment] = x_offset;
        z_offsets[segment] = z_offset;
        x_offset += (random.next_int(11) - 5) as f32;
        z_offset += (random.next_int(11) - 5) as f32;
    }
    let mut out = Vec::new();
    for layer in 0..4 {
        let mut random = JavaRandom::new(seed);
        for branch in 0..3 {
            let trunk = branch == 0;
            let start = 7 - branch;
            let end = if trunk { 0 } else { start - 3 + 1 };
            let mut start_x = x_offsets[start as usize] - x_offset;
            let mut start_z = z_offsets[start as usize] - z_offset;
            for segment in (end..=start).rev() {
                let (end_x, end_z) = (start_x, start_z);
                if trunk {
                    start_x += (random.next_int(11) - 5) as f32;
                    start_z += (random.next_int(11) - 5) as f32;
                } else {
                    start_x += (random.next_int(31) - 15) as f32;
                    start_z += (random.next_int(31) - 15) as f32;
                }
                let mut top = 0.1 + layer as f32 * 0.2;
                let mut bottom = top;
                if trunk {
                    top *= segment as f32 * 0.1 + 1.0;
                    bottom *= (segment - 1) as f32 * 0.1 + 1.0;
                }
                let at = Segment {
                    start: (start_x, start_z),
                    end: (end_x, end_z),
                    segment,
                    top,
                    bottom,
                };
                at.quad(&mut out, [false, false, true, false]);
                at.quad(&mut out, [true, false, true, true]);
                at.quad(&mut out, [true, true, false, true]);
                at.quad(&mut out, [false, true, false, false]);
            }
        }
    }
    out
}

struct Segment {
    start: (f32, f32),
    end: (f32, f32),
    segment: i32,
    top: f32,
    bottom: f32,
}

impl Segment {
    /// `LightningBoltRenderer.quad`: one side of the segment, its corners
    /// pushed out by the radius toward the signs given (right x, right z,
    /// left x, left z), in vanilla's order, in blocks: a segment is 16
    /// high, the whole bolt 128.
    fn quad(&self, out: &mut Vec<WorldVertex>, [rx, rz, lx, lz]: [bool; 4]) {
        let sign = |positive: bool, r: f32| if positive { r } else { -r };
        let y0 = self.segment as f32 * 16.0;
        let y1 = (self.segment + 1) as f32 * 16.0;
        let corner = |x: f32, y: f32, z: f32| ([x, y, z], [0.5, 0.5]);
        let corners = [
            corner(
                self.start.0 + sign(rx, self.bottom),
                y0,
                self.start.1 + sign(rz, self.bottom),
            ),
            corner(
                self.end.0 + sign(rx, self.top),
                y1,
                self.end.1 + sign(rz, self.top),
            ),
            corner(
                self.end.0 + sign(lx, self.top),
                y1,
                self.end.1 + sign(lz, self.top),
            ),
            corner(
                self.start.0 + sign(lx, self.bottom),
                y0,
                self.start.1 + sign(lz, self.bottom),
            ),
        ];
        let first = out.len();
        geometry::push_quad(out, corners, COLOR, false);
        for vertex in &mut out[first..] {
            vertex.color[3] = (ALPHA * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_the_jdk() {
        // `new Random(42).nextInt()`.
        let mut random = JavaRandom::new(42);
        assert_eq!(random.next(32), -1_170_105_035);
    }

    #[test]
    fn a_bolt_has_four_layers_of_a_trunk_and_two_branches() {
        let quads = quads(7);
        // (8 trunk segments + 3 + 3 branch segments) x 4 sides x 4 layers.
        let per_quad = quads.len() / ((8 + 3 + 3) * 4 * 4);
        assert!(per_quad == 4 || per_quad == 6, "{}", quads.len());
        assert!(quads.iter().all(|v| v.color[3] == 77));
    }
}
