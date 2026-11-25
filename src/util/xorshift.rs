//! Marsaglia's xorshift64 for tests: the same numbers on every run.

/// One state; never seed it with zero.
pub struct XorShift(pub u64);

impl XorShift {
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Uniform in `[0, 1)`, from the top 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}
