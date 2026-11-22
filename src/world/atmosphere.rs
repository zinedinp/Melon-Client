//! The environment attributes at the camera (vanilla `EnvironmentAttributeSystem`).

use crate::math::argb;
use crate::world::biome::{Biome, Biomes};
use crate::world::clock::Environment as Day;
use crate::world::dimension::{DimensionType, Rgb, Skybox, rgb};

/// `EnvironmentAttributes` defaults, for what neither the dimension nor the biome sets.
const FOG_END: f32 = 1024.0;
const SKY_FOG_END: f32 = 512.0;
const CLOUD_FOG_END: f32 = 2048.0;
/// `WATER_FOG_COLOR`: `0xFF050533`.
const WATER_FOG_COLOR: Rgb = rgb(0x050533);
const WATER_FOG_START: f32 = -8.0;
const WATER_FOG_END: f32 = 96.0;
/// `BLOCK_LIGHT_TINT`: `0xFFFFD88C`.
const BLOCK_LIGHT_TINT: Rgb = rgb(0xffd88c);
/// `NIGHT_VISION_COLOR`: `0xFF999999`.
const NIGHT_VISION_COLOR: Rgb = rgb(0x999999);
const CLOUD_HEIGHT: f32 = 192.33;

/// How the biome cells around a point count (`GaussianSampler`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BiomeMix {
    /// Each biome that counts, with its summed weight.
    weights: Vec<(u16, f64)>,
}

impl BiomeMix {
    /// A place with one biome around.
    pub fn single(id: u16) -> Self {
        Self {
            weights: vec![(id, 1.0)],
        }
    }

    /// `GaussianSampler.sample` at `position`, asking `biome_at` for the biome of each cell.
    pub fn gaussian(
        position: glam::DVec3,
        mut biome_at: impl FnMut(i32, i32, i32) -> Option<u16>,
    ) -> Self {
        const KERNEL: [f64; 7] = [0.0, 1.0, 4.0, 6.0, 4.0, 1.0, 0.0];
        let at = position * 0.25 - 0.5;
        let base = at.floor();
        let rel = at - base;
        let (bx, by, bz) = (base.x as i32, base.y as i32, base.z as i32);
        let weight = |rel: f64, i: usize| KERNEL[i + 1] * (1.0 - rel) + KERNEL[i] * rel;
        let mut mix = Self::default();
        for z in 0..6 {
            let wz = weight(rel.z, z);
            for x in 0..6 {
                let wx = weight(rel.x, x);
                for y in 0..6 {
                    let w = wx * weight(rel.y, y) * wz;
                    if w <= 0.0 {
                        continue;
                    }
                    let cell = (bx - 2 + x as i32, by - 2 + y as i32, bz - 2 + z as i32);
                    if let Some(id) = biome_at(cell.0 * 4, cell.1 * 4, cell.2 * 4) {
                        mix.add(id, w);
                    }
                }
            }
        }
        mix
    }

    fn add(&mut self, id: u16, weight: f64) {
        match self.weights.iter_mut().find(|(biome, _)| *biome == id) {
            Some((_, w)) => *w += weight,
            None => self.weights.push((id, weight)),
        }
    }

    /// The weighted mean of what each biome says (`say`), `base` where one says nothing.
    fn mean<T: Mix>(&self, biomes: &Biomes, base: T, say: impl Fn(&Biome) -> Option<T>) -> T {
        let mut total = 0.0;
        let mut sum = T::zero();
        for &(id, weight) in &self.weights {
            let value = biomes.biome(id).and_then(&say).unwrap_or(base);
            sum = sum.add(value.scaled(weight));
            total += weight;
        }
        if total <= 0.0 {
            base
        } else {
            sum.scaled(1.0 / total)
        }
    }

    /// The biome with the most weight, if any.
    pub fn dominant(&self) -> Option<u16> {
        self.weights
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|&(id, _)| id)
    }
}

/// What can be averaged: floats and colours.
trait Mix: Copy {
    fn zero() -> Self;
    fn add(self, other: Self) -> Self;
    fn scaled(self, by: f64) -> Self;
}

impl Mix for f32 {
    fn zero() -> Self {
        0.0
    }
    fn add(self, other: Self) -> Self {
        self + other
    }
    fn scaled(self, by: f64) -> Self {
        (f64::from(self) * by) as f32
    }
}

impl Mix for [f32; 3] {
    fn zero() -> Self {
        [0.0; 3]
    }
    fn add(self, o: Self) -> Self {
        [self[0] + o[0], self[1] + o[1], self[2] + o[2]]
    }
    fn scaled(self, by: f64) -> Self {
        self.map(|c| (f64::from(c) * by) as f32)
    }
}

/// `Timelines.NIGHT_SKY_LIGHT_COLOR`.
const NIGHT_SKY_LIGHT_COLOR: Rgb = [0.48, 0.48, 1.0];

/// One of `WeatherAttributes.RAIN` / `THUNDER`.
struct WeatherLayer {
    /// `BlendToGray(brightness, factor)` of the sky colour.
    sky_gray: (f32, f32),
    /// The same of the cloud colour.
    cloud_gray: (f32, f32),
    /// Multiplied onto the fog and the sunrise colour.
    fog_multiplier: Rgb,
    /// The alpha of the sky light's blend to the night's.
    alpha: f32,
}

static RAIN: WeatherLayer = WeatherLayer {
    sky_gray: (0.6, 0.75),
    cloud_gray: (0.24, 0.5),
    fog_multiplier: [0.5, 0.5, 0.6],
    alpha: 0.3125,
};

static THUNDER: WeatherLayer = WeatherLayer {
    sky_gray: (0.24, 0.94),
    cloud_gray: (0.095, 0.94),
    fog_multiplier: [0.25, 0.25, 0.3],
    alpha: 0.527_343_75,
};

/// The attributes the renderer reads, for one place and time.
#[derive(Clone, Debug, PartialEq)]
pub struct Atmosphere {
    pub skybox: Skybox,
    pub fog_color: Rgb,
    pub sky_color: Rgb,
    pub fog_start_distance: f32,
    pub fog_end_distance: f32,
    pub sky_fog_end_distance: f32,
    pub cloud_fog_end_distance: f32,
    pub water_fog_color: Rgb,
    pub water_fog_start_distance: f32,
    pub water_fog_end_distance: f32,
    /// `AMBIENT_LIGHT_COLOR`: what light level 0 shows.
    pub ambient_light_color: Rgb,
    pub block_light_tint: Rgb,
    /// `SKY_LIGHT_COLOR` and `SKY_LIGHT_FACTOR`, after the day.
    pub sky_light_color: Rgb,
    pub sky_light_factor: f32,
    /// `SKY_LIGHT_LEVEL` (`0..=15`).
    pub sky_light_level: f32,
    pub night_vision_color: Rgb,
    /// `SUNRISE_SUNSET_COLOR`, ARGB as `0..1` red first.
    pub sunrise_sunset_color: [f32; 4],
    pub cloud_color: [f32; 4],
    pub cloud_height: f32,
    /// The day timeline's angles and stars.
    pub day: Day,
    /// Whether the dimension has a day (sun, moon, stars, dusk).
    pub has_day: bool,
}

impl Atmosphere {
    /// The attributes in `dimension`, with the `mix` of biomes around and the time of `day`.
    pub fn resolve(dimension: &DimensionType, biomes: &Biomes, mix: &BiomeMix, day: &Day) -> Self {
        let a = &dimension.attributes;
        let has_day = dimension.skybox == Skybox::Overworld && !dimension.fixed_time;
        let day = if has_day { *day } else { Day::still() };
        let mul3 = |c: Rgb, m: [f32; 3]| [c[0] * m[0], c[1] * m[1], c[2] * m[2]];

        let fog_base = a.fog_color.unwrap_or([0.0; 3]);
        let fog = mix.mean(biomes, fog_base, |b| b.visuals.fog_color);
        let sky_base = a.sky_color.unwrap_or([0.0; 3]);
        let sky = mix.mean(biomes, sky_base, |b| b.visuals.sky_color);

        let water_fog_base = a.water_fog_color.unwrap_or(WATER_FOG_COLOR);
        let water_end_base = a.water_fog_end_distance.unwrap_or(WATER_FOG_END);
        let water_end = mix.mean(biomes, water_end_base, |b| {
            b.visuals.water_fog_end_scale.map(|s| water_end_base * s)
        });

        let cloud = a.cloud_color.unwrap_or(argb(0));
        Self {
            skybox: dimension.skybox,
            fog_color: mul3(fog, day.fog_multiplier),
            sky_color: mul3(sky, day.sky_multiplier),
            fog_start_distance: a.fog_start_distance.unwrap_or(0.0),
            fog_end_distance: a.fog_end_distance.unwrap_or(FOG_END),
            sky_fog_end_distance: a.sky_fog_end_distance.unwrap_or(SKY_FOG_END),
            cloud_fog_end_distance: a.cloud_fog_end_distance.unwrap_or(CLOUD_FOG_END),
            water_fog_color: mix.mean(biomes, water_fog_base, |b| b.visuals.water_fog_color),
            water_fog_start_distance: a.water_fog_start_distance.unwrap_or(WATER_FOG_START),
            water_fog_end_distance: water_end,
            ambient_light_color: a.ambient_light_color.unwrap_or([0.0; 3]),
            block_light_tint: a.block_light_tint.unwrap_or(BLOCK_LIGHT_TINT),
            sky_light_color: mul3(a.sky_light_color.unwrap_or([1.0; 3]), day.sky_light_color),
            sky_light_factor: a.sky_light_factor.unwrap_or(1.0) * day.sky_light_factor,
            sky_light_level: a.sky_light_level.unwrap_or(15.0) * day.sky_level_multiplier,
            night_vision_color: a.night_vision_color.unwrap_or(NIGHT_VISION_COLOR),
            sunrise_sunset_color: day.sunrise_sunset,
            cloud_color: std::array::from_fn(|i| cloud[i] * day.cloud_multiplier[i]),
            cloud_height: a.cloud_height.unwrap_or(CLOUD_HEIGHT),
            day,
            has_day,
        }
    }

    /// `WeatherAttributes`.
    pub fn with_weather(mut self, rain: f32, thunder: f32) -> Self {
        let thunder = thunder.clamp(0.0, 1.0);
        let rain = rain.clamp(0.0, 1.0) - thunder;
        if rain > 0.0 {
            self.apply_weather(rain, &RAIN);
        }
        if thunder > 0.0 {
            self.apply_weather(thunder, &THUNDER);
        }
        self
    }

    fn apply_weather(&mut self, level: f32, weather: &WeatherLayer) {
        let lerp = |t: f32, a: f32, b: f32| a + (b - a) * t;
        let to_gray = |c: [f32; 3], brightness: f32, factor: f32| {
            let gray = (c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11) * brightness;
            let gray = gray.clamp(0.0, 1.0);
            c.map(|v| lerp(factor, v, gray))
        };
        let mix3 = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| lerp(level, a[i], b[i]));
        let (sky_brightness, sky_factor) = weather.sky_gray;
        self.sky_color = mix3(
            self.sky_color,
            to_gray(self.sky_color, sky_brightness, sky_factor),
        );
        let fog = weather.fog_multiplier;
        self.fog_color = mix3(
            self.fog_color,
            [0, 1, 2].map(|i| self.fog_color[i] * fog[i]),
        );
        let [r, g, b, a] = self.cloud_color;
        let (cloud_brightness, cloud_factor) = weather.cloud_gray;
        let gray = to_gray([r, g, b], cloud_brightness, cloud_factor);
        let cloud = mix3([r, g, b], gray);
        self.cloud_color = [cloud[0], cloud[1], cloud[2], a];
        self.sky_light_level = lerp(
            level,
            self.sky_light_level,
            lerp(weather.alpha, self.sky_light_level, 4.0),
        );
        self.sky_light_color = mix3(
            self.sky_light_color,
            [0, 1, 2].map(|i| {
                lerp(
                    weather.alpha,
                    self.sky_light_color[i],
                    NIGHT_SKY_LIGHT_COLOR[i],
                )
            }),
        );
        self.sky_light_factor = lerp(
            level,
            self.sky_light_factor,
            lerp(weather.alpha, self.sky_light_factor, 0.24),
        );
        self.day.star_brightness = lerp(level, self.day.star_brightness, 0.0);
        let [r, g, b, a] = self.sunrise_sunset_color;
        let sunrise = mix3([r, g, b], [0, 1, 2].map(|i| [r, g, b][i] * fog[i]));
        self.sunrise_sunset_color = [sunrise[0], sunrise[1], sunrise[2], a];
    }

    /// `SKY_LIGHT_COLOR` times `SKY_LIGHT_FACTOR`.
    pub fn sky_light(&self) -> Rgb {
        self.sky_light_color.map(|c| c * self.sky_light_factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::biome::{Biome, Visuals};

    fn biomes() -> Biomes {
        let plains = Biome::default();
        let wastes = Biome {
            visuals: Visuals {
                fog_color: Some(rgb(0x330808)),
                ..Visuals::default()
            },
            ..Biome::default()
        };
        let forest = Biome {
            visuals: Visuals {
                fog_color: Some(rgb(0x1a051a)),
                ..Visuals::default()
            },
            ..Biome::default()
        };
        let swamp = Biome {
            visuals: Visuals {
                water_fog_color: Some(rgb(0x232317)),
                water_fog_end_scale: Some(0.85),
                ..Visuals::default()
            },
            ..Biome::default()
        };
        Biomes::new(vec![
            ("minecraft:plains".into(), plains),
            ("minecraft:nether_wastes".into(), wastes),
            ("minecraft:warped_forest".into(), forest),
            ("minecraft:swamp".into(), swamp),
        ])
    }

    #[test]
    fn plains_at_noon_has_the_dimensions_fog_and_its_own_sky() {
        let day = Day::at(6000.0);
        let a = Atmosphere::resolve(
            &DimensionType::overworld(),
            &biomes(),
            &BiomeMix::single(0),
            &day,
        );
        assert_eq!(a.fog_color, rgb(0xc0d8ff));
        assert_eq!(a.sky_color, rgb(0x78a7ff));
        assert_eq!(a.fog_end_distance, 1024.0);
        assert_eq!(a.ambient_light_color, rgb(0x0a0a0a));
        assert!(a.has_day);
        assert!((a.sky_light()[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn night_darkens_fog_and_sky_in_the_overworld_only() {
        let night = Day::at(18_000.0);
        let over = Atmosphere::resolve(
            &DimensionType::overworld(),
            &biomes(),
            &BiomeMix::single(0),
            &night,
        );
        assert!(over.sky_color.iter().all(|&c| c < 0.01));
        assert!((over.fog_color[0] - rgb(0xc0d8ff)[0] * 0.06).abs() < 1e-5);
        assert!((over.sky_light_factor - 0.24).abs() < 1e-5);
        assert!((over.sky_light_level - 4.0).abs() < 1e-4);
        let nether = |day: &Day| {
            Atmosphere::resolve(
                &DimensionType::nether(),
                &biomes(),
                &BiomeMix::single(1),
                day,
            )
        };
        assert_eq!(nether(&night), nether(&Day::at(6000.0)));
        assert_eq!(nether(&night).fog_color, rgb(0x330808));
        assert!(!nether(&night).has_day);
    }

    #[test]
    fn the_nether_takes_its_fog_from_the_biome_and_its_distances_from_the_dimension() {
        let a = Atmosphere::resolve(
            &DimensionType::nether(),
            &biomes(),
            &BiomeMix::single(1),
            &Day::at(0.0),
        );
        assert_eq!(a.fog_color, rgb(0x330808));
        assert_eq!((a.fog_start_distance, a.fog_end_distance), (10.0, 96.0));
        assert_eq!(a.ambient_light_color, rgb(0x302821));
        assert_eq!(a.sky_light_factor, 0.0);
        assert_eq!(a.skybox, Skybox::None);
    }

    #[test]
    fn the_end_is_dark_with_a_purple_sky_light_of_no_strength() {
        let a = Atmosphere::resolve(
            &DimensionType::end(),
            &biomes(),
            &BiomeMix::single(0),
            &Day::at(0.0),
        );
        assert_eq!(a.fog_color, rgb(0x181318));
        assert_eq!(a.sky_light_factor, 0.0);
        assert_eq!(a.ambient_light_color, rgb(0x3f473f));
        assert_eq!(a.skybox, Skybox::End);
    }

    #[test]
    fn fog_blends_between_biomes_over_a_border() {
        let b = biomes();
        let mix = BiomeMix {
            weights: vec![(1, 1.0), (2, 1.0)],
        };
        let a = Atmosphere::resolve(&DimensionType::nether(), &b, &mix, &Day::at(0.0));
        let (x, y) = (rgb(0x330808), rgb(0x1a051a));
        for i in 0..3 {
            assert!((a.fog_color[i] - (x[i] + y[i]) / 2.0).abs() < 1e-5);
        }
    }

    #[test]
    fn a_biome_without_fog_counts_as_the_dimensions() {
        let b = biomes();
        let mix = BiomeMix {
            weights: vec![(0, 1.0), (1, 3.0)],
        };
        let a = Atmosphere::resolve(&DimensionType::nether(), &b, &mix, &Day::at(0.0));
        assert!((a.fog_color[0] - rgb(0x330808)[0] * 0.75).abs() < 1e-5);
    }

    #[test]
    fn swamp_water_is_murky() {
        let b = biomes();
        let swamp = Atmosphere::resolve(
            &DimensionType::overworld(),
            &b,
            &BiomeMix::single(3),
            &Day::at(6000.0),
        );
        assert_eq!(swamp.water_fog_color, rgb(0x232317));
        assert!((swamp.water_fog_end_distance - 96.0 * 0.85).abs() < 1e-4);
        let plains = Atmosphere::resolve(
            &DimensionType::overworld(),
            &b,
            &BiomeMix::single(0),
            &Day::at(6000.0),
        );
        assert_eq!(plains.water_fog_color, rgb(0x050533));
        assert_eq!(plains.water_fog_end_distance, 96.0);
    }

    #[test]
    fn the_gaussian_weights_a_whole_kernel_and_follows_the_position() {
        let mix = BiomeMix::gaussian(glam::DVec3::new(1.0, 70.0, 1.0), |_, _, _| Some(5));
        assert_eq!(mix.weights.len(), 1);
        assert!((mix.weights[0].1 - 16.0 * 16.0 * 16.0).abs() < 1e-6);
        let border = |x: f32| {
            BiomeMix::gaussian(glam::DVec3::new(f64::from(x), 70.0, 8.0), |bx, _, _| {
                Some(if bx < 0 { 1 } else { 2 })
            })
        };
        let weight =
            |mix: &BiomeMix, id| mix.weights.iter().find(|w| w.0 == id).map_or(0.0, |w| w.1);
        let on = border(0.0);
        assert!((weight(&on, 1) - weight(&on, 2)).abs() < 1e-6);
        let inside = border(40.0);
        assert!(weight(&inside, 1) < 1e-9);
        assert!(on.dominant().is_some());
    }

    #[test]
    fn rain_greys_the_sky_and_dims_the_light() {
        let day = Day::at(6000.0);
        let clear = Atmosphere::resolve(
            &DimensionType::overworld(),
            &biomes(),
            &BiomeMix::single(0),
            &day,
        );
        let rainy = clear.clone().with_weather(1.0, 0.0);
        assert!(rainy.sky_color[2] < clear.sky_color[2]);
        assert!(rainy.sky_light_factor < clear.sky_light_factor);
        assert!((rainy.sky_light_level - (15.0 + (4.0 - 15.0) * 0.3125)).abs() < 1e-4);
        assert_eq!(rainy.day.star_brightness, 0.0);
        let storm = clear.clone().with_weather(1.0, 1.0);
        assert!(storm.sky_color[2] < rainy.sky_color[2]);
        assert!(storm.sky_light_factor < rainy.sky_light_factor);
        assert_eq!(clear.clone().with_weather(0.0, 0.0), clear);
    }
}
