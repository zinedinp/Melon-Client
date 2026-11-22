//! The one `java.util.Random` / vanilla `LegacyRandomSource`.

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const ADDEND: u64 = 0xB;
const MASK: u64 = (1 << 48) - 1;

/// `java.util.Random` with a 48-bit LCG and the polar Gaussian.
#[derive(Clone, Debug)]
pub struct JavaRandom {
    seed: u64,
    next_gaussian: Option<f64>,
}

impl JavaRandom {
    pub fn with_seed(seed: i64) -> Self {
        let mut rng = Self {
            seed: 0,
            next_gaussian: None,
        };
        rng.set_seed(seed);
        rng
    }

    pub fn set_seed(&mut self, seed: i64) {
        self.seed = (seed as u64 ^ MULTIPLIER) & MASK;
        self.next_gaussian = None;
    }

    /// Vanilla `RandomSource.create(seed)` / `new Random(seed)`.
    pub fn new(seed: i64) -> Self {
        Self::with_seed(seed)
    }

    /// `Random.next(bits)`: the top `bits` bits of the next state, signed.
    pub fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `Random.nextInt(bound)`, for a positive `bound`.
    pub fn next_int(&mut self, bound: i32) -> i32 {
        debug_assert!(bound > 0);
        if bound.isolate_lowest_one() == bound {
            return ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    /// `Random.nextInt()`: all 32 bits.
    pub fn next_i32(&mut self) -> i32 {
        self.next(32)
    }

    /// `Random.nextLong`.
    pub fn next_long(&mut self) -> i64 {
        let upper = i64::from(self.next(32));
        let lower = i64::from(self.next(32));
        (upper << 32).wrapping_add(lower)
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / ((1 << 24) as f32)
    }

    pub fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26)) << 27;
        let low = i64::from(self.next(27));
        (high + low) as f64 * (1.0 / ((1i64 << 53) as f64))
    }

    /// `Random.nextGaussian`: polar rejection, one spare sample kept.
    pub fn next_gaussian(&mut self) -> f64 {
        if let Some(spare) = self.next_gaussian.take() {
            return spare;
        }
        loop {
            let v1 = 2.0 * self.next_double() - 1.0;
            let v2 = 2.0 * self.next_double() - 1.0;
            let s = v1 * v1 + v2 * v2;
            if s < 1.0 && s != 0.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                self.next_gaussian = Some(v2 * m);
                return v1 * m;
            }
        }
    }
}

/// Java `String.hashCode()`: 31-polynomial over the UTF-16 code units.
pub fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, unit| {
        h.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_floats_match_java() {
        let mut rng = JavaRandom::with_seed(0);
        assert!((rng.next_float() - 0.730_967_76).abs() < 1e-6);
    }

    #[test]
    fn bounded_ints_match_java() {
        let mut rng = JavaRandom::with_seed(42);
        let tens: Vec<_> = (0..6).map(|_| rng.next_int(10)).collect();
        assert_eq!(tens, [0, 3, 8, 4, 0, 5]);
        let mut rng = JavaRandom::with_seed(42);
        let sixteens: Vec<_> = (0..6).map(|_| rng.next_int(16)).collect();
        assert_eq!(sixteens, [11, 0, 10, 0, 4, 15]);
    }

    #[test]
    fn ints_and_longs_match_java() {
        assert_eq!(JavaRandom::new(0).next_i32(), -1_155_484_576);
        assert_eq!(JavaRandom::new(0).next_long(), -4_962_768_465_676_381_896);
    }

    #[test]
    fn string_hash_matches_java() {
        assert_eq!(java_string_hash(""), 0);
        assert_eq!(java_string_hash("hello"), 99_162_322);
        assert_eq!(java_string_hash("Ä€"), 196 * 31 + 0x20AC);
    }
}
