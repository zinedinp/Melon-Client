//! `Interaction` entities.

use crate::network::play::game::DataValue;

/// `Interaction.DATA_*`: the entity data indices after `Entity`'s 0-7.
mod index {
    pub const WIDTH: u8 = 8;
    pub const HEIGHT: u8 = 9;
    pub const RESPONSE: u8 = 10;
}

/// What an `Interaction` entity syncs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InteractionBox {
    pub width: f32,
    pub height: f32,
    pub response: bool,
}

impl Default for InteractionBox {
    /// `Interaction`'s `define` defaults.
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            response: false,
        }
    }
}

impl InteractionBox {
    /// Takes in one synced value; whether it was one of this entity's.
    pub fn apply(&mut self, i: u8, value: &DataValue) -> bool {
        match i {
            index::WIDTH => self.width = value.float().unwrap_or(self.width),
            index::HEIGHT => self.height = value.float().unwrap_or(self.height),
            index::RESPONSE => self.response = value.bool().unwrap_or(self.response),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_values_replace_the_defaults() {
        let mut b = InteractionBox::default();
        assert!(b.apply(8, &DataValue::Float(0.6)));
        assert!(b.apply(9, &DataValue::Float(2.0)));
        assert!(b.apply(10, &DataValue::Bool(true)));
        assert!(!b.apply(3, &DataValue::Bool(true)));
        assert_eq!((b.width, b.height, b.response), (0.6, 2.0, true));
    }
}
