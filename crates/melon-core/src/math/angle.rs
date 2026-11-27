//! Angles and the circle functions every part of Melon shares.

use glam::{Mat3, Mat4, Quat, Vec3};

pub use std::f32::consts::FRAC_PI_2 as HALF_PI;
pub use std::f32::consts::FRAC_PI_3 as THIRD_PI;
pub use std::f32::consts::FRAC_PI_4 as QUARTER_PI;
pub use std::f32::consts::FRAC_PI_6 as SIXTH_PI;
pub use std::f32::consts::FRAC_PI_8 as EIGHTH_PI;
pub use std::f32::consts::PI;
pub use std::f32::consts::TAU;

/// The same in `f64`, for vanilla's `Math.PI` sums in doubles.
pub mod double {
    pub use std::f64::consts::FRAC_PI_2 as HALF_PI;
    pub use std::f64::consts::PI;
    pub use std::f64::consts::TAU;
}

/// `Mth.DEG_TO_RAD`.
pub const DEG_TO_RAD: f32 = PI / 180.0;
/// `Mth.RAD_TO_DEG`.
pub const RAD_TO_DEG: f32 = 180.0 / PI;

/// Vanilla's steps around the circle (`Mth.SIN.length`).
const STEPS: i64 = 1 << 16;
/// `65536 / 2π`, vanilla's literal `10430.378F`.
const STEPS_PER_RADIAN: f32 = 10430.378;
/// A quarter turn in steps: the cosine is the sine this far on.
const QUARTER: f32 = 16384.0;
/// Steps between two entries of [`COARSE`].
const FINE_BITS: u32 = 8;
const COARSE_LEN: usize = 1 << (16 - FINE_BITS);
const FINE_LEN: usize = 1 << FINE_BITS;
/// One step in radians.
const STEP: f64 = double::TAU / STEPS as f64;

/// `sin(x)` for `0 <= x <= π/2`, Taylor series to `x^23`: below `1e-17`
const fn sin_quarter(x: f64) -> f64 {
    let x2 = x * x;
    let mut sum = 0.0;
    let mut n = 23;
    while n > 1 {
        sum = 1.0 - x2 / ((n * (n - 1)) as f64) * sum;
        n -= 2;
    }
    x * sum
}

/// `cos(x)` for small `x` (under a 256th of a turn), as a Taylor series.
const fn cos_small(x: f64) -> f64 {
    let x2 = x * x;
    let mut sum = 0.0;
    let mut n = 16;
    while n > 0 {
        sum = 1.0 - x2 / ((n * (n - 1)) as f64) * sum;
        n -= 2;
    }
    sum
}

/// A table on its own cache line, so none of it shares a line with other data.
#[repr(C, align(64))]
struct Aligned<T>(T);

/// The sine at every 256th step, by quadrant symmetry from the first.
static COARSE: Aligned<[f64; COARSE_LEN]> = Aligned({
    let quarter = COARSE_LEN / 4;
    let step = double::TAU / COARSE_LEN as f64;
    let mut table = [0.0; COARSE_LEN];
    let mut i = 0;
    while i < COARSE_LEN {
        let within = i % quarter;
        table[i] = match i / quarter {
            0 => sin_quarter(within as f64 * step),
            1 => sin_quarter((quarter - within) as f64 * step),
            2 => -sin_quarter(within as f64 * step),
            _ => -sin_quarter((quarter - within) as f64 * step),
        };
        i += 1;
    }
    table[COARSE_LEN / 2] = 1.224_646_799_147_353_2e-16;
    table
});

/// Sine and cosine of the steps between two coarse entries, side by side.
static FINE: Aligned<[[f64; 2]; FINE_LEN]> = Aligned({
    let mut table = [[0.0; 2]; FINE_LEN];
    let mut i = 0;
    while i < FINE_LEN {
        let d = i as f64 * STEP;
        table[i] = [sin_quarter(d), cos_small(d)];
        i += 1;
    }
    table
});

/// Sine and cosine of vanilla's step `index`.
#[inline]
fn step(index: i64) -> (f32, f32) {
    let index = (index & (STEPS - 1)) as usize;
    let coarse = index >> FINE_BITS;
    let [sin_d, cos_d] = FINE.0[index & (FINE_LEN - 1)];
    let sin_a = COARSE.0[coarse];
    let cos_a = COARSE.0[(coarse + COARSE_LEN / 4) & (COARSE_LEN - 1)];
    (
        (sin_a * cos_d + cos_a * sin_d) as f32,
        (cos_a * cos_d - sin_a * sin_d) as f32,
    )
}

/// `Mth.sin`.
#[inline]
pub fn sin(radians: f32) -> f32 {
    step((radians * STEPS_PER_RADIAN) as i64).0
}

/// `Mth.cos`: vanilla's own index, a quarter turn on before rounding.
#[inline]
pub fn cos(radians: f32) -> f32 {
    step((radians * STEPS_PER_RADIAN + QUARTER) as i64).0
}

/// [`sin`] and [`cos`] together.
#[inline]
pub fn sin_cos(radians: f32) -> (f32, f32) {
    let scaled = radians * STEPS_PER_RADIAN;
    let sin_index = scaled as i64;
    let cos_index = (scaled + QUARTER) as i64;
    let (sin, cos) = step(sin_index);
    if cos_index == sin_index + QUARTER as i64 {
        (sin, cos)
    } else {
        (sin, step(cos_index).0)
    }
}

/// Sine and cosine of one and the same table step.
#[inline]
pub fn unit(radians: f32) -> (f32, f32) {
    step((radians * STEPS_PER_RADIAN) as i64)
}

/// `(sin, cos)` of an angle in degrees, as vanilla's movement code takes it.
#[inline]
pub fn sin_cos_deg(degrees: f32) -> (f32, f32) {
    sin_cos(degrees * DEG_TO_RAD)
}

/// `Mth.wrapDegrees`: the same angle in `[-180, 180)`.
#[inline]
pub fn wrap_degrees(degrees: f32) -> f32 {
    let wrapped = degrees % 360.0;
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else if wrapped < -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

/// `2 / π`, for the quarter turns in an angle.
pub const FRAC_2_PI: f64 = std::f64::consts::FRAC_2_PI;
/// `π / 2` in two parts (fdlibm's `pio2_1`, `pio2_1t`): the first has
/// 33 significant bits, so a quarter-turn count below `2^20` times it
/// is exact.
pub const PIO2_HI: f64 = 1.570_796_326_734_125_6;
pub const PIO2_LO: f64 = 6.077_100_506_506_192e-11;
/// Past this the split above loses digits: [`sin_cos_exact`] asks libm.
pub const REDUCE_LIMIT: f32 = 400_000.0;
/// Adding and taking away `1.5 · 2^52` rounds a double to an integer.
pub const ROUND_MAGIC: f64 = 6_755_399_441_055_744.0;

/// fdlibm's `__kernel_sin` coefficients, `S1` first.
pub const SIN_COEFFS: [f64; 6] = [
    -1.666_666_666_666_663_2e-01,
    8.333_333_333_322_49e-03,
    -1.984_126_982_985_795e-04,
    2.755_731_370_707_006_8e-06,
    -2.505_076_025_340_686_3e-08,
    1.589_690_995_211_55e-10,
];
/// fdlibm's `__kernel_cos` coefficients, `C1` first.
pub const COS_COEFFS: [f64; 6] = [
    4.166_666_666_666_66e-02,
    -1.388_888_888_887_411e-03,
    2.480_158_728_947_673e-05,
    -2.755_731_435_139_066_3e-07,
    2.087_572_321_298_175e-09,
    -1.135_964_755_778_819_5e-11,
];

/// fdlibm's `__kernel_sin` and `__kernel_cos` for `|r| <= π/4`, both
/// well under a double's last bit off. The SIMD kernels
/// (`super::sin_cos`) do these operations in this order.
#[inline(always)]
fn kernel_sin_cos(r: f64) -> (f64, f64) {
    let [s1, s2, s3, s4, s5, s6] = SIN_COEFFS;
    let [c1, c2, c3, c4, c5, c6] = COS_COEFFS;
    let z = r * r;
    let sin = {
        let tail = s2 + z * (s3 + z * (s4 + z * (s5 + z * s6)));
        let sin = r + z * r * (s1 + z * tail);
        // Keeps `-0.0`'s sign, as Java's sine does (a select, no jump).
        if r == 0.0 { r } else { sin }
    };
    let cos = {
        let tail = z * (c1 + z * (c2 + z * (c3 + z * (c4 + z * (c5 + z * c6)))));
        let half = 0.5 * z;
        let w = 1.0 - half;
        w + (((1.0 - w) - half) + z * tail)
    };
    (sin, cos)
}

/// `(float) Math.sin(x)` and `(float) Math.cos(x)`: what JOML's
/// `Math.sin` / `Math.cos` give vanilla's rotations (`Axis`,
/// `Quaternionf.rotationYXZ`, `ModelPart`), unlike [`sin`]'s table.
/// Exact in `f64` and rounded once, as Java does, without libm's call
/// for the angles a frame turns by.
// Many at once: [`sin_cos_exact_slice`].
#[inline]
pub fn sin_cos_exact(x: f32) -> (f32, f32) {
    if x.is_nan() || x.abs() > REDUCE_LIMIT {
        let x = f64::from(x);
        return (x.sin() as f32, x.cos() as f32);
    }
    let x = f64::from(x);
    let rounded = x * FRAC_2_PI + ROUND_MAGIC;
    // The quarter turns, rounded, sit in the low bits of `rounded`.
    let quadrant = rounded.to_bits();
    let quarters = rounded - ROUND_MAGIC;
    let r = (x - quarters * PIO2_HI) - quarters * PIO2_LO;
    let (s, c) = kernel_sin_cos(r);
    // Quadrant 1: (c, -s); 2: (-s, -c); 3: (-c, s).
    let swap = quadrant & 1 != 0;
    let (sin, cos) = if swap { (c, s) } else { (s, c) };
    let sin_sign = (quadrant & 2) << 62;
    let cos_sign = (quadrant.wrapping_add(1) & 2) << 62;
    (
        f64::from_bits(sin.to_bits() ^ sin_sign) as f32,
        f64::from_bits(cos.to_bits() ^ cos_sign) as f32,
    )
}

/// [`sin_cos_exact`]'s sine: JOML `Math.sin(float)`.
#[inline]
pub fn sin_exact(x: f32) -> f32 {
    sin_cos_exact(x).0
}

/// [`sin_cos_exact`]'s cosine: JOML `Math.cos(float)`.
#[inline]
pub fn cos_exact(x: f32) -> f32 {
    sin_cos_exact(x).1
}

/// [`sin_cos_exact`] of every angle in `angles`, bit for bit, as many
/// at once as the CPU's vector unit takes ([`super::sin_cos`]).
pub fn sin_cos_exact_slice(angles: &[f32], sin: &mut [f32], cos: &mut [f32]) {
    super::sin_cos::sin_cos(angles, sin, cos);
}

/// JOML `Math.cosFromSin(sin, angle)`: the cosine from the sine already
/// known, `√(1 - sin²)` with the sign of the angle's half turn.
#[inline]
pub fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    // JOML's `PI_f`, `PI2_f` and `PIHalf_f`: the same floats.
    let cos = (1.0 - sin * sin).sqrt();
    let a = angle + HALF_PI;
    let mut b = a - (a / TAU) as i32 as f32 * TAU;
    if f64::from(b) < 0.0 {
        b += TAU;
    }
    if b >= PI { -cos } else { cos }
}

/// Rotations about the axes and from Euler angles, with [`sin_cos_exact`]:
/// JOML's exact sines, as vanilla's `Axis` and `ModelPart` turn by. A
/// model's parts get theirs all at once ([`sin_cos_exact_slice`]).
pub trait Rotation: Sized {
    /// About X by `radians`.
    fn rot_x(radians: f32) -> Self;
    fn rot_y(radians: f32) -> Self;
    fn rot_z(radians: f32) -> Self;

    /// `Axis.XP.rotationDegrees`.
    #[inline]
    fn rot_x_deg(degrees: f32) -> Self {
        Self::rot_x(degrees * DEG_TO_RAD)
    }
    #[inline]
    fn rot_y_deg(degrees: f32) -> Self {
        Self::rot_y(degrees * DEG_TO_RAD)
    }
    #[inline]
    fn rot_z_deg(degrees: f32) -> Self {
        Self::rot_z(degrees * DEG_TO_RAD)
    }

    /// Euler angles `(x, y, z)` in radians, applied Z first, then Y, then X.
    fn euler_zyx(radians: Vec3) -> Self;
}

/// [`Rotation::euler_zyx`] from its angles' sines and cosines, worked
/// out beforehand (many at once by [`sin_cos_exact_slice`]).
#[inline]
pub fn euler_zyx_of(sin: Vec3, cos: Vec3) -> Mat3 {
    let (sx, sy, sz) = (sin.x, sin.y, sin.z);
    let (cx, cy, cz) = (cos.x, cos.y, cos.z);
    Mat3::from_cols(
        Vec3::new(cz * cy, sz * cy, -sy),
        Vec3::new(cz * sy * sx - sz * cx, sz * sy * sx + cz * cx, cy * sx),
        Vec3::new(cz * sy * cx + sz * sx, sz * sy * cx - cz * sx, cy * cx),
    )
}

impl Rotation for Mat3 {
    #[inline]
    fn rot_x(radians: f32) -> Self {
        let (s, c) = sin_cos_exact(radians);
        Mat3::from_cols(Vec3::X, Vec3::new(0.0, c, s), Vec3::new(0.0, -s, c))
    }

    #[inline]
    fn rot_y(radians: f32) -> Self {
        let (s, c) = sin_cos_exact(radians);
        Mat3::from_cols(Vec3::new(c, 0.0, -s), Vec3::Y, Vec3::new(s, 0.0, c))
    }

    #[inline]
    fn rot_z(radians: f32) -> Self {
        let (s, c) = sin_cos_exact(radians);
        Mat3::from_cols(Vec3::new(c, s, 0.0), Vec3::new(-s, c, 0.0), Vec3::Z)
    }

    #[inline]
    fn euler_zyx(radians: Vec3) -> Self {
        // A part that doesn't turn about an axis (most) skips its sine.
        let pair = |r: f32| if r == 0.0 { (r, 1.0) } else { sin_cos_exact(r) };
        let (sx, cx) = pair(radians.x);
        let (sy, cy) = pair(radians.y);
        let (sz, cz) = pair(radians.z);
        euler_zyx_of(Vec3::new(sx, sy, sz), Vec3::new(cx, cy, cz))
    }
}

impl Rotation for Mat4 {
    #[inline]
    fn rot_x(radians: f32) -> Self {
        Mat4::from_mat3(Mat3::rot_x(radians))
    }
    #[inline]
    fn rot_y(radians: f32) -> Self {
        Mat4::from_mat3(Mat3::rot_y(radians))
    }
    #[inline]
    fn rot_z(radians: f32) -> Self {
        Mat4::from_mat3(Mat3::rot_z(radians))
    }
    #[inline]
    fn euler_zyx(radians: Vec3) -> Self {
        Mat4::from_mat3(Mat3::euler_zyx(radians))
    }
}

/// A rotation of `radians` about `axis` (a unit vector), as a quaternion.
#[inline]
pub fn axis_angle(axis: Vec3, radians: f32) -> Quat {
    let (s, c) = sin_cos_exact(radians * 0.5);
    let v = axis * s;
    Quat::from_xyzw(v.x, v.y, v.z, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla's table: `(float) Math.sin(i * 2π / 65536)`.
    fn vanilla(i: i64) -> f32 {
        ((i as f64) * double::PI * 2.0 / STEPS as f64).sin() as f32
    }

    /// The shortened literals are fdlibm's longer ones to the bit (its hex notes).
    #[test]
    fn the_fdlibm_constants_keep_their_bits() {
        let bits = |values: &[f64]| values.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&[PIO2_HI, PIO2_LO]),
            [0x3FF9_21FB_5440_0000, 0x3DD0_B461_1A62_6331]
        );
        assert_eq!(
            bits(&SIN_COEFFS),
            [
                0xBFC5_5555_5555_5549,
                0x3F81_1111_1110_F8A6,
                0xBF2A_01A0_19C1_61D5,
                0x3EC7_1DE3_57B1_FE7D,
                0xBE5A_E5E6_8A2B_9CEB,
                0x3DE5_D93A_5ACF_D57C,
            ]
        );
        assert_eq!(
            bits(&COS_COEFFS),
            [
                0x3FA5_5555_5555_554C,
                0xBF56_C16C_16C1_5177,
                0x3EFA_01A0_19CB_1590,
                0xBE92_7E4F_809C_52AD,
                0x3E21_EE9E_BDB4_B1C4,
                0xBDA8_FAE9_BE88_38D4,
            ]
        );
    }

    #[test]
    fn every_step_is_vanillas() {
        for i in 0..STEPS {
            let (s, c) = step(i);
            assert_eq!(s, vanilla(i), "sin step {i}");
            assert_eq!(c, vanilla((i + 16384) & (STEPS - 1)), "cos step {i}");
        }
    }

    #[test]
    fn lookups_index_like_mth() {
        for k in -7200..=7200 {
            let x = k as f32 * 0.001_7;
            let vanilla_sin = vanilla((x * STEPS_PER_RADIAN) as i64 & (STEPS - 1));
            let vanilla_cos = vanilla((x * STEPS_PER_RADIAN + QUARTER) as i64 & (STEPS - 1));
            assert_eq!(sin(x), vanilla_sin, "sin {x}");
            assert_eq!(cos(x), vanilla_cos, "cos {x}");
            assert_eq!(sin_cos(x), (vanilla_sin, vanilla_cos), "sin_cos {x}");
            assert!((sin(x) - x.sin()).abs() < 1e-4);
        }
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(sin(HALF_PI), 1.0);
    }

    #[test]
    fn rotations_match_glams() {
        for (a, b, c) in [(0.3, -1.2, 2.5), (-2.0, 0.7, 0.1), (3.0, 3.0, -3.0)] {
            let euler = Mat3::euler_zyx(Vec3::new(a, b, c));
            let glam = Mat3::from_quat(Quat::from_euler(glam::EulerRot::ZYX, c, b, a));
            assert!(euler.abs_diff_eq(glam, 2e-4), "{euler} {glam}");
            assert!(Mat3::rot_x(a).abs_diff_eq(Mat3::from_rotation_x(a), 2e-4));
            assert!(Mat3::rot_y(b).abs_diff_eq(Mat3::from_rotation_y(b), 2e-4));
            assert!(Mat3::rot_z(c).abs_diff_eq(Mat3::from_rotation_z(c), 2e-4));
            let axis = Vec3::new(1.0, 2.0, -0.5).normalize();
            assert!(axis_angle(axis, a).abs_diff_eq(Quat::from_axis_angle(axis, a), 2e-4));
        }
    }

    #[test]
    fn rotations_keep_lengths() {
        for k in -100..100 {
            let a = k as f32 * 0.037;
            let (s, c) = unit(a);
            assert!((s * s + c * c - 1.0).abs() < 1e-6, "{a}");
            let m = Mat3::euler_zyx(Vec3::new(a, -a * 0.5, a * 2.0));
            assert!(((m * Vec3::new(1.0, 2.0, 3.0)).length() - 14f32.sqrt()).abs() < 1e-5);
        }
    }

    /// The tables a frame's pipelines look up (here and nowhere else) stay small enough for L1.
    #[test]
    fn tables_stay_small_in_l1() {
        const L1_SHARE: usize = 8 * 1024;
        let tables = [
            (
                std::mem::size_of_val(&COARSE),
                std::ptr::from_ref(&COARSE) as usize,
            ),
            (
                std::mem::size_of_val(&FINE),
                std::ptr::from_ref(&FINE) as usize,
            ),
        ];
        assert!(tables.iter().map(|t| t.0).sum::<usize>() <= L1_SHARE);
        for (_, at) in tables {
            assert_eq!(at % 64, 0, "a table starts mid cache line");
        }
    }

    /// Java's `(float) Math.sin(x)`: the exact value rounded once.
    #[test]
    fn exact_sines_are_javas() {
        let mut mismatches = 0;
        let mut check = |x: f32| {
            let (s, c) = sin_cos_exact(x);
            let (want_s, want_c) = (f64::from(x).sin() as f32, f64::from(x).cos() as f32);
            if s.to_bits() != want_s.to_bits() || c.to_bits() != want_c.to_bits() {
                mismatches += 1;
                assert!(
                    (s - want_s).abs() <= f32::EPSILON && (c - want_c).abs() <= f32::EPSILON,
                    "{x}: {s} {c} vs {want_s} {want_c}"
                );
            }
        };
        // Every float in the first turn, both signs.
        let mut x = 0.0f32;
        while x < 6.3 {
            check(x);
            check(-x);
            x = f32::from_bits(x.to_bits() + 97);
        }
        // Degrees and spins as vanilla hands them in, far out too.
        for k in 0..2_000_000u32 {
            let x = (k as f32 * 0.618_034 - 600_000.0) * DEG_TO_RAD;
            check(x);
        }
        for x in [
            0.0,
            -0.0,
            1.0e-30,
            HALF_PI,
            PI,
            TAU,
            1.0e6,
            -3.0e7,
            f32::MAX,
        ] {
            check(x);
        }
        // Rounding to a float twice can, very rarely, land a step off.
        assert!(mismatches <= 2, "{mismatches} off by a step");
        assert!(sin_cos_exact(f32::NAN).0.is_nan());
    }

    #[test]
    fn cos_from_sin_takes_the_half_turns_sign() {
        for k in -40..40 {
            let a = k as f32 * 0.3;
            let s = sin_exact(a);
            assert!((cos_from_sin(s, a) - a.cos()).abs() < 1e-6, "{a}");
        }
    }

    #[test]
    fn wraps_degrees() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(180.0), -180.0);
    }
}
