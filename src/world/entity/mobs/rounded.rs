//! Vanilla's model angles written to four places (`1.5708F`), not the exact
//! fractions of π: quotients of exact integers, so each has that literal's bits.

pub const PI_2: f32 = 15708.0 / 10000.0;
pub const PI_4: f32 = 7854.0 / 10000.0;
pub const PI_6: f32 = 5236.0 / 10000.0;
pub const PI_8: f32 = 3927.0 / 10000.0;

#[cfg(test)]
mod tests {
    #[test]
    fn each_has_the_bits_of_vanillas_literal() {
        for (value, literal) in [
            (super::PI_2, "1.5708"),
            (super::PI_4, "0.7854"),
            (super::PI_6, "0.5236"),
            (super::PI_8, "0.3927"),
        ] {
            assert_eq!(
                value.to_bits(),
                literal.parse::<f32>().unwrap().to_bits(),
                "{literal}"
            );
        }
    }
}
