//! Values a cycle button steps through.

/// A value with a fixed order to step through, wrapping at the end.
pub trait Cycle: Copy + PartialEq + 'static {
    const ALL: &'static [Self];

    /// The value after this one, the first after the last.
    fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&v| v == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}
