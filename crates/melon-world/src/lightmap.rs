//! Vanilla `Lightmap` / `core/lightmap.fsh`.

/// `BLOCK_LIGHT_TINT`'s default (`DimensionDefaults` -10100 → RGB 0xFFD88C).
pub const DEFAULT_BLOCK_LIGHT_TINT: [f32; 3] = [1.0, 216.0 / 255.0, 140.0 / 255.0];
/// The overworld's `AMBIENT_LIGHT_COLOR` (RGB 0x0A0A0A).
pub const OVERWORLD_AMBIENT: [f32; 3] = [10.0 / 255.0; 3];
/// `LightmapRenderState.blockFactor`.
pub const BLOCK_FACTOR: f32 = 1.4;

/// What the lightmap depends on besides the light levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapParams {
    /// `SKY_LIGHT_COLOR` times `SKY_LIGHT_FACTOR`.
    pub sky: [f32; 3],
    /// `Options.gamma`.
    pub gamma: f32,
    /// `AMBIENT_LIGHT_COLOR`: what light level 0 shows.
    pub ambient: [f32; 3],
    /// `BLOCK_LIGHT_TINT`: the colour of torch light.
    pub block_tint: [f32; 3],
}

impl LightmapParams {
    /// The overworld's ambient colour and torch tint with `sky` light.
    pub const fn overworld(sky: [f32; 3], gamma: f32) -> Self {
        Self {
            sky,
            gamma,
            ambient: OVERWORLD_AMBIENT,
            block_tint: DEFAULT_BLOCK_LIGHT_TINT,
        }
    }
}

/// `Lightmap.getBrightness` for overworld ambient light 0, `level` 0..=15.
pub fn get_brightness(level: u8) -> f32 {
    let v = f32::from(level.min(15)) / 15.0;
    v / (4.0 - 3.0 * v)
}

/// `lightmap.fsh`.
pub fn sample(block: u8, sky: u8, params: &LightmapParams) -> [f32; 3] {
    let block_b = get_brightness(block) * BLOCK_FACTOR;
    let sky_b = get_brightness(sky);
    let t = f32::from(block.min(15)) / 15.0;
    let parabolic = (2.0 * t - 1.0).powi(2);
    let mix = 0.9 * parabolic;
    let tint = params.block_tint;
    let block_color = [
        tint[0] + (1.0 - tint[0]) * mix,
        tint[1] + (1.0 - tint[1]) * mix,
        tint[2] + (1.0 - tint[2]) * mix,
    ];
    apply_brightness(
        std::array::from_fn(|i| {
            params.ambient[i] + params.sky[i] * sky_b + block_color[i] * block_b
        }),
        params.gamma,
    )
}

/// `lightmap.fsh` `notGamma`, mixed with `BrightnessFactor` (`Options.gamma`).
pub fn apply_brightness(color: [f32; 3], gamma: f32) -> [f32; 3] {
    let c = [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
    ];
    let max = c[0].max(c[1]).max(c[2]);
    if max <= 0.0 {
        return [0.0; 3];
    }
    let inv = 1.0 - max;
    let scaled = 1.0 - inv * inv * inv * inv;
    let lift = scaled / max;
    let not_gamma = [c[0] * lift, c[1] * lift, c[2] * lift];
    [
        c[0] + (not_gamma[0] - c[0]) * gamma,
        c[1] + (not_gamma[1] - c[1]) * gamma,
        c[2] + (not_gamma[2] - c[2]) * gamma,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Night outdoor sky light (`SKY_LIGHT_FACTOR` 0.24) after `notGamma`.
    fn night_bright() -> f32 {
        1.0 - 0.76f32.powi(4)
    }

    #[test]
    fn moody_leaves_light_alone() {
        assert_eq!(apply_brightness([0.24; 3], 0.0), [0.24; 3]);
    }

    #[test]
    fn daylight_stays_white() {
        assert_eq!(apply_brightness([1.0; 3], 1.0), [1.0; 3]);
        assert_eq!(apply_brightness([1.0; 3], 0.5), [1.0; 3]);
    }

    #[test]
    fn bright_lifts_night() {
        let night = apply_brightness([0.24; 3], 1.0);
        assert!((night[0] - night_bright()).abs() < 1e-5, "{}", night[0]);
        assert_eq!(night[0], night[1]);
        assert_eq!(night[1], night[2]);
    }

    #[test]
    fn default_is_halfway_to_not_gamma() {
        let night = apply_brightness([0.24; 3], 0.5);
        let expected = 0.24 + (night_bright() - 0.24) * 0.5;
        assert!((night[0] - expected).abs() < 1e-5, "{}", night[0]);
    }

    #[test]
    fn zero_stays_black() {
        assert_eq!(apply_brightness([0.0; 3], 1.0), [0.0; 3]);
    }

    fn params(sky: f32, gamma: f32) -> LightmapParams {
        LightmapParams::overworld([sky; 3], gamma)
    }

    #[test]
    fn the_nether_is_lit_by_its_ambient_colour_alone() {
        let nether = LightmapParams {
            sky: [0.0; 3],
            gamma: 0.0,
            ambient: [
                0x30 as f32 / 255.0,
                0x28 as f32 / 255.0,
                0x21 as f32 / 255.0,
            ],
            block_tint: DEFAULT_BLOCK_LIGHT_TINT,
        };
        let dark = sample(0, 0, &nether);
        assert!(dark[0] > dark[1] && dark[1] > dark[2]);
        assert_eq!(sample(0, 15, &nether), dark);
        let torch = sample(14, 0, &nether);
        assert!(torch[0] > dark[0] && torch[2] > dark[2]);
    }

    #[test]
    fn the_end_has_a_greenish_floor() {
        let end = LightmapParams {
            sky: [0.0; 3],
            gamma: 0.0,
            ambient: [
                0x3f as f32 / 255.0,
                0x47 as f32 / 255.0,
                0x3f as f32 / 255.0,
            ],
            block_tint: DEFAULT_BLOCK_LIGHT_TINT,
        };
        let dark = sample(0, 0, &end);
        assert!(dark[1] > dark[0] && (dark[0] - dark[2]).abs() < 1e-6);
    }

    #[test]
    fn caves_are_darker_than_daylight() {
        let cave = sample(0, 0, &params(1.0, 0.5));
        let ground = sample(0, 15, &params(1.0, 0.5));
        assert!(cave[0] > 0.05 && cave[0] < 0.15, "{}", cave[0]);
        assert_eq!(cave[0], cave[2]);
        assert!((ground[0] - 1.0).abs() < 1e-5);
        assert_eq!(ground[0], ground[1]);
        let night = sample(0, 15, &params(0.24, 0.0));
        assert!((night[0] - (0.24 + OVERWORLD_AMBIENT[0])).abs() < 1e-5);
        let torch = sample(14, 0, &params(1.0, 0.0));
        assert!(torch[0] > cave[0]);
        assert!(torch[0] > torch[2]);
    }
}
