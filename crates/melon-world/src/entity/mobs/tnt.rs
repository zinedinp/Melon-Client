//! `TntRenderer`.

/// `TntRenderer.getSwellAmount`.
pub fn swell_amount(fuse: f32) -> f32 {
    let g = (1.0 - fuse / 10.0).clamp(0.0, 1.0);
    let g = g * g;
    g * g * 0.3
}

/// `TntRenderer.isLit`.
pub fn is_lit(fuse: f32) -> bool {
    fuse >= 0.0 && (fuse / 5.0) as i32 % 2 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_swells_only_at_the_end_and_flashes_every_five_ticks() {
        assert_eq!(swell_amount(10.0), 0.0);
        assert_eq!(swell_amount(0.0), 0.3);
        assert!(swell_amount(5.0) > 0.0 && swell_amount(5.0) < 0.3);
        let lit: Vec<bool> = [80.0, 77.0, 74.0, 70.0, 66.0, 5.0, 4.0, -1.0]
            .into_iter()
            .map(is_lit)
            .collect();
        assert_eq!(lit, [true, false, true, true, false, false, true, false]);
    }
}
