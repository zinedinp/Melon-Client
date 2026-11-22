//! The names that change how vanilla draws an entity (`LivingEntityRenderer.

/// What a custom name makes a renderer do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Magic {
    #[default]
    None,
    /// `Dinnerbone` and `Grumm`: upside down, head and look inverted.
    UpsideDown,
    /// `jeb_`: a sheep's wool cycles through the dyes.
    Jeb,
    /// `Toast`: a rabbit in its own coat.
    Toast,
}

impl Magic {
    /// The effect of `name` (`Component.getString`.
    pub fn of(name: &str) -> Self {
        match name {
            "Dinnerbone" | "Grumm" => Self::UpsideDown,
            "jeb_" => Self::Jeb,
            "Toast" => Self::Toast,
            _ => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_matched_exactly() {
        assert_eq!(Magic::of("Dinnerbone"), Magic::UpsideDown);
        assert_eq!(Magic::of("Grumm"), Magic::UpsideDown);
        assert_eq!(Magic::of("jeb_"), Magic::Jeb);
        assert_eq!(Magic::of("Toast"), Magic::Toast);
        assert_eq!(Magic::of("dinnerbone"), Magic::None);
        assert_eq!(Magic::of("Dinnerbone "), Magic::None);
        assert_eq!(Magic::of(""), Magic::None);
    }
}
