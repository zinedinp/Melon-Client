//! Fog, as vanilla's `FogRenderer`.

use glam::Vec3;

use crate::math::angle;
use crate::world::atmosphere::Atmosphere;
use crate::world::dimension::Rgb;

/// `FogType`, without `NONE`: what the camera is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FogType {
    Atmospheric,
    Water,
    Lava,
    PowderSnow,
}

/// `FogData`: what a frame's fog shaders read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogData {
    /// Also what the frame is cleared to. Alpha 1.
    pub color: [f32; 4],
    pub environmental_start: f32,
    pub environmental_end: f32,
    pub render_distance_start: f32,
    pub render_distance_end: f32,
    /// Where the sky disc has faded into the fog.
    pub sky_end: f32,
    pub cloud_end: f32,
}

impl FogData {
    /// No fog anywhere: the title screen, the hand, `FogMode.NONE`.
    pub const NONE: Self = Self {
        color: [0.0; 4],
        environmental_start: f32::MAX,
        environmental_end: f32::MAX,
        render_distance_start: f32::MAX,
        render_distance_end: f32::MAX,
        sky_end: f32::MAX,
        cloud_end: f32::MAX,
    };

    /// `fog.glsl`'s `total_fog_value`, `0..=1`.
    pub fn amount(&self, spherical: f32, cylindrical: f32) -> f32 {
        linear(spherical, self.environmental_start, self.environmental_end).max(linear(
            cylindrical,
            self.render_distance_start,
            self.render_distance_end,
        ))
    }
}

/// `linear_fog_value`.
fn linear(distance: f32, start: f32, end: f32) -> f32 {
    if distance <= start {
        0.0
    } else if distance >= end {
        1.0
    } else {
        (distance - start) / (end - start)
    }
}

/// `fog_spherical_distance` and `fog_cylindrical_distance` of a position.
pub fn distances(position: Vec3) -> (f32, f32) {
    (
        position.length(),
        glam::Vec2::new(position.x, position.z)
            .length()
            .max(position.y.abs()),
    )
}

/// What a frame's fog depends on besides the atmosphere.
#[derive(Clone, Copy, Debug)]
pub struct FogContext {
    pub fog_type: FogType,
    /// A spectator sees through lava and powder snow.
    pub spectator: bool,
    /// The render distance option, in chunks.
    pub render_distance_chunks: u8,
    /// The camera's height and the level's bottom: below it is the void.
    pub camera_y: f32,
    pub min_y: i32,
    /// `ClientLevel.voidDarknessOnsetRange`: 32, 1 in a flat world.
    pub void_darkness_range: f32,
    /// `LocalPlayer.getWaterVision`, `0..=1`.
    pub water_vision: f32,
    /// Fire resistance sees further in lava.
    pub fire_resistance: bool,
    /// The camera's forward vector, for the glow of a low sun in the fog.
    pub forward: Vec3,
    /// The status effects that fog the view or dim it; none are tracked yet.
    pub effects: FogEffects,
    /// `Options.cloudRange`, in chunks.
    pub cloud_range_chunks: u8,
    /// The rain and thunder levels.
    pub rain_level: f32,
    pub thunder_level: f32,
    /// `AtmosphericFogEnvironment.rainFogMultiplier`.
    pub rain_fog: f32,
}

/// `BlindnessFogEnvironment`, `DarknessFogEnvironment` and night vision.
#[derive(Clone, Copy, Debug, Default)]
pub struct FogEffects {
    /// Blindness' remaining ticks (`None`.
    pub blindness: Option<(i32, bool)>,
    /// `MobEffectInstance.getBlendFactor` of darkness.
    pub darkness: Option<f32>,
    /// `GameRenderer.nightVisionScale`, `0` without the effect.
    pub night_vision: f32,
}

impl FogContext {
    /// The atmosphere of an overworld camera at the origin.
    pub fn plain(fog_type: FogType, render_distance_chunks: u8) -> Self {
        Self {
            fog_type,
            spectator: false,
            render_distance_chunks,
            camera_y: 70.0,
            min_y: -64,
            void_darkness_range: 32.0,
            water_vision: 0.0,
            fire_resistance: false,
            forward: Vec3::NEG_Z,
            effects: FogEffects::default(),
            cloud_range_chunks: 128,
            rain_level: 0.0,
            thunder_level: 0.0,
            rain_fog: 0.0,
        }
    }
}

/// `FogRenderer.setupFog`.
pub fn setup(atmosphere: &Atmosphere, context: &FogContext) -> FogData {
    let render_distance = f32::from(context.render_distance_chunks) * 16.0;
    let mut fog = FogData {
        color: [0.0; 4],
        environmental_start: 0.0,
        environmental_end: 0.0,
        render_distance_start: 0.0,
        render_distance_end: 0.0,
        sky_end: 0.0,
        cloud_end: 0.0,
    };
    let color = color(atmosphere, context);
    fog.color = [color[0], color[1], color[2], 1.0];

    let effects = &context.effects;
    match context.fog_type {
        FogType::Lava => lava(&mut fog, context, render_distance),
        FogType::PowderSnow => powder_snow(&mut fog, context, render_distance),
        _ if effects.blindness.is_some() => blindness(&mut fog, effects, render_distance),
        _ if effects.darkness.is_some() => {
            let blend = effects.darkness.unwrap_or(0.0);
            let distance = lerp(blend, render_distance, 15.0);
            fog.environmental_start = distance * 0.75;
            fog.environmental_end = distance;
            fog.sky_end = distance;
            fog.cloud_end = distance;
        }
        FogType::Water => {
            fog.environmental_start = atmosphere.water_fog_start_distance;
            fog.environmental_end =
                atmosphere.water_fog_end_distance * context.water_vision.max(0.25);
            fog.sky_end = fog.environmental_end;
            fog.cloud_end = fog.environmental_end;
        }
        FogType::Atmospheric => {
            fog.environmental_start = atmosphere.fog_start_distance - 160.0 * context.rain_fog;
            fog.environmental_end = (atmosphere.fog_end_distance - 256.0 * context.rain_fog)
                .max(96.0_f32.min(atmosphere.fog_end_distance));
            fog.sky_end = render_distance.min(atmosphere.sky_fog_end_distance);
            fog.cloud_end = (f32::from(context.cloud_range_chunks) * 16.0)
                .min(atmosphere.cloud_fog_end_distance);
        }
    }

    let span = (render_distance / 10.0).clamp(4.0, 64.0);
    fog.render_distance_start = render_distance - span;
    fog.render_distance_end = render_distance;
    fog
}

fn lava(fog: &mut FogData, context: &FogContext, render_distance: f32) {
    if context.spectator {
        fog.environmental_start = -8.0;
        fog.environmental_end = render_distance * 0.5;
    } else if context.fire_resistance {
        fog.environmental_start = 0.0;
        fog.environmental_end = 5.0;
    } else {
        fog.environmental_start = 0.25;
        fog.environmental_end = 1.0;
    }
    fog.sky_end = fog.environmental_end;
    fog.cloud_end = fog.environmental_end;
}

fn powder_snow(fog: &mut FogData, context: &FogContext, render_distance: f32) {
    if context.spectator {
        fog.environmental_start = -8.0;
        fog.environmental_end = render_distance * 0.5;
    } else {
        fog.environmental_start = 0.0;
        fog.environmental_end = 2.0;
    }
    fog.sky_end = fog.environmental_end;
    fog.cloud_end = fog.environmental_end;
}

fn blindness(fog: &mut FogData, effects: &FogEffects, render_distance: f32) {
    let (ticks, infinite) = effects.blindness.unwrap_or((0, false));
    let distance = if infinite {
        5.0
    } else {
        lerp((ticks as f32 / 20.0).min(1.0), render_distance, 5.0)
    };
    fog.environmental_start = distance * 0.25;
    fog.environmental_end = distance;
    fog.sky_end = distance * 0.8;
    fog.cloud_end = distance * 0.8;
}

/// `FogRenderer.computeFogColor`.
fn color(atmosphere: &Atmosphere, context: &FogContext) -> Rgb {
    let base = match context.fog_type {
        FogType::Lava => crate::world::dimension::rgb(0x991900),
        FogType::PowderSnow => crate::world::dimension::rgb(0x9fbbcc),
        FogType::Water => atmosphere.water_fog_color,
        FogType::Atmospheric => air_color(atmosphere, context),
    };
    let [mut r, mut g, mut b] = base;

    let range = context.void_darkness_range;
    let mut darkness = ((range + context.min_y as f32 - context.camera_y) / range).clamp(0.0, 1.0);
    if let Some(blend) = context.effects.darkness {
        darkness = darkness.max(blend);
    }
    if let Some((ticks, _)) = context.effects.blindness {
        darkness = if ticks <= 19 {
            (ticks as f32 / 20.0).max(darkness)
        } else {
            1.0
        };
    }
    if darkness > 0.0 && !matches!(context.fog_type, FogType::Lava | FogType::PowderSnow) {
        let brightness = (1.0 - darkness) * (1.0 - darkness);
        r *= brightness;
        g *= brightness;
        b *= brightness;
    }

    let brighten = if context.fog_type == FogType::Water {
        context.water_vision
    } else if context.effects.darkness.is_none() {
        context.effects.night_vision
    } else {
        0.0
    };
    if r != 0.0 && g != 0.0 && b != 0.0 {
        let target = 1.0 / r.max(g).max(b);
        r = lerp(brighten, r, r * target);
        g = lerp(brighten, g, g * target);
        b = lerp(brighten, b, b * target);
    }
    [r, g, b]
}

/// `AtmosphericFogEnvironment.getBaseColor`.
fn air_color(atmosphere: &Atmosphere, context: &FogContext) -> Rgb {
    let mut fog = atmosphere.fog_color;
    if context.render_distance_chunks >= 4 {
        let sun_x = if angle::sin(atmosphere.day.sun_angle) > 0.0 {
            -1.0
        } else {
            1.0
        };
        let facing_sun = context.forward.x * sun_x;
        let [r, g, b, alpha] = atmosphere.sunrise_sunset_color;
        if facing_sun > 0.0 && alpha > 0.0 {
            fog = mix(facing_sun * alpha, fog, [r, g, b]);
        }
    }
    let sky_end =
        (atmosphere.sky_fog_end_distance / 16.0).min(f32::from(context.render_distance_chunks));
    let factor = clamped_lerp(sky_end / 32.0, 0.25, 1.0);
    let sky_mix = 1.0 - factor.powf(0.25);
    mix(
        sky_mix,
        fog,
        weather_darkened(atmosphere.sky_color, context),
    )
}

/// `AtmosphericFogEnvironment.applyWeatherDarken`.
fn weather_darkened(color: Rgb, context: &FogContext) -> Rgb {
    let [mut r, mut g, mut b] = color;
    if context.rain_level > 0.0 {
        let rain = 1.0 - context.rain_level * 0.5;
        let blue = 1.0 - context.rain_level * 0.4;
        (r, g, b) = (r * rain, g * rain, b * blue);
    }
    if context.thunder_level > 0.0 {
        let thunder = 1.0 - context.thunder_level * 0.5;
        (r, g, b) = (r * thunder, g * thunder, b * thunder);
    }
    [r, g, b]
}

fn lerp(t: f32, a: f32, b: f32) -> f32 {
    a + (b - a) * t
}

fn clamped_lerp(t: f32, min: f32, max: f32) -> f32 {
    if t < 0.0 {
        min
    } else if t > 1.0 {
        max
    } else {
        lerp(t, min, max)
    }
}

fn mix(t: f32, a: Rgb, b: Rgb) -> Rgb {
    [
        lerp(t, a[0], b[0]),
        lerp(t, a[1], b[1]),
        lerp(t, a[2], b[2]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::atmosphere::BiomeMix;
    use crate::world::biome::{Biome, Biomes, Visuals};
    use crate::world::clock::Environment as Day;
    use crate::world::dimension::{DimensionType, rgb};

    fn overworld(time: f64) -> Atmosphere {
        Atmosphere::resolve(
            &DimensionType::overworld(),
            &Biomes::new(vec![("minecraft:plains".into(), Biome::default())]),
            &BiomeMix::single(0),
            &Day::at(time),
        )
    }

    fn nether() -> Atmosphere {
        let wastes = Biome {
            visuals: Visuals {
                fog_color: Some(rgb(0x330808)),
                ..Visuals::default()
            },
            ..Biome::default()
        };
        Atmosphere::resolve(
            &DimensionType::nether(),
            &Biomes::new(vec![("minecraft:nether_wastes".into(), wastes)]),
            &BiomeMix::single(0),
            &Day::at(0.0),
        )
    }

    #[test]
    fn the_overworld_fogs_only_at_the_render_distance() {
        let fog = setup(
            &overworld(6000.0),
            &FogContext::plain(FogType::Atmospheric, 12),
        );
        assert_eq!(fog.render_distance_end, 192.0);
        assert!((fog.render_distance_start - 172.8).abs() < 1e-3);
        assert_eq!(fog.environmental_start, 0.0);
        assert_eq!(fog.environmental_end, 1024.0);
        assert_eq!(fog.amount(100.0, 100.0), 100.0 / 1024.0);
        assert_eq!(fog.amount(200.0, 192.0), 1.0);
        assert_eq!(fog.sky_end, 192.0);
    }

    #[test]
    fn the_span_of_the_render_distance_fog_is_between_four_and_sixty_four_blocks() {
        let at = |chunks| {
            let fog = setup(
                &overworld(6000.0),
                &FogContext::plain(FogType::Atmospheric, chunks),
            );
            fog.render_distance_end - fog.render_distance_start
        };
        assert_eq!(at(2), 4.0);
        assert!((at(12) - 19.2).abs() < 1e-4);
        assert!((at(32) - 51.2).abs() < 1e-4);
    }

    #[test]
    fn the_nether_is_foggy_from_ten_blocks() {
        let far = setup(&nether(), &FogContext::plain(FogType::Atmospheric, 32));
        assert_eq!([far.color[0], far.color[1], far.color[2]], rgb(0x330808));
        let fog = setup(&nether(), &FogContext::plain(FogType::Atmospheric, 12));
        assert!(
            fog.color[0] < far.color[0],
            "the nether's black sky takes a part"
        );
        assert_eq!(
            (fog.environmental_start, fog.environmental_end),
            (10.0, 96.0)
        );
        assert_eq!(fog.amount(10.0, 0.0), 0.0);
        assert!((fog.amount(53.0, 0.0) - 0.5).abs() < 1e-6);
        assert_eq!(fog.amount(96.0, 0.0), 1.0);
    }

    #[test]
    fn short_render_distances_take_some_sky_into_the_fog() {
        let air = overworld(6000.0);
        let near = setup(&air, &FogContext::plain(FogType::Atmospheric, 4)).color;
        let far = setup(&air, &FogContext::plain(FogType::Atmospheric, 32)).color;
        assert_eq!([far[0], far[1], far[2]], rgb(0xc0d8ff));
        assert!(near[0] < far[0]);
    }

    #[test]
    fn water_fog_shows_the_biomes_colour_and_closes_in_without_vision() {
        let air = overworld(6000.0);
        let fog = setup(&air, &FogContext::plain(FogType::Water, 12));
        assert_eq!([fog.color[0], fog.color[1], fog.color[2]], rgb(0x050533));
        assert_eq!(fog.environmental_start, -8.0);
        assert_eq!(fog.environmental_end, 24.0);
        let seasoned = setup(
            &air,
            &FogContext {
                water_vision: 1.0,
                ..FogContext::plain(FogType::Water, 12)
            },
        );
        assert_eq!(seasoned.environmental_end, 96.0);
        assert!((seasoned.color[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn lava_and_powder_snow_are_all_but_opaque() {
        let air = overworld(6000.0);
        let lava = setup(&air, &FogContext::plain(FogType::Lava, 12));
        assert_eq!(
            (lava.environmental_start, lava.environmental_end),
            (0.25, 1.0)
        );
        let resistant = setup(
            &air,
            &FogContext {
                fire_resistance: true,
                ..FogContext::plain(FogType::Lava, 12)
            },
        );
        assert_eq!(resistant.environmental_end, 5.0);
        let spectating = setup(
            &air,
            &FogContext {
                spectator: true,
                ..FogContext::plain(FogType::Lava, 12)
            },
        );
        assert_eq!(spectating.environmental_start, -8.0);
        assert_eq!(spectating.environmental_end, 96.0);
        let snow = setup(&air, &FogContext::plain(FogType::PowderSnow, 12));
        assert_eq!(snow.environmental_end, 2.0);
    }

    #[test]
    fn falling_into_the_void_turns_the_fog_black() {
        let air = overworld(6000.0);
        let at = |y: f32| {
            setup(
                &air,
                &FogContext {
                    camera_y: y,
                    ..FogContext::plain(FogType::Atmospheric, 12)
                },
            )
            .color
        };
        assert_eq!(at(0.0)[0], at(-32.0)[0]);
        assert!(at(-33.0)[0] < at(0.0)[0]);
        assert!((at(-64.0 + 16.0)[0] - at(0.0)[0] * 0.25).abs() < 1e-5);
        assert_eq!(at(-64.0 - 1.0)[0], 0.0);
        let lava = setup(
            &air,
            &FogContext {
                camera_y: -70.0,
                ..FogContext::plain(FogType::Lava, 12)
            },
        );
        assert!(lava.color[0] > 0.5);
    }

    #[test]
    fn blindness_closes_in_to_five_blocks() {
        let air = overworld(6000.0);
        let blind = setup(
            &air,
            &FogContext {
                effects: FogEffects {
                    blindness: Some((100, false)),
                    ..FogEffects::default()
                },
                ..FogContext::plain(FogType::Atmospheric, 12)
            },
        );
        assert_eq!(
            (blind.environmental_start, blind.environmental_end),
            (1.25, 5.0)
        );
        assert_eq!(blind.sky_end, 4.0);
        assert_eq!(blind.color[0], 0.0);
    }

    #[test]
    fn the_sunset_glows_in_the_fog_towards_the_sun() {
        let sunset = overworld(12_732.0);
        let toward = |forward: Vec3| {
            setup(
                &sunset,
                &FogContext {
                    forward,
                    ..FogContext::plain(FogType::Atmospheric, 12)
                },
            )
            .color
        };
        let west = toward(Vec3::NEG_X);
        let east = toward(Vec3::X);
        let away = toward(Vec3::NEG_Z);
        assert_eq!(east, away);
        assert_ne!(west, away);
    }

    #[test]
    fn distances_are_a_sphere_and_a_cylinder() {
        assert_eq!(distances(Vec3::new(3.0, 0.0, 4.0)), (5.0, 5.0));
        let (sphere, cylinder) = distances(Vec3::new(3.0, 12.0, 4.0));
        assert!((sphere - 13.0).abs() < 1e-5);
        assert_eq!(cylinder, 12.0);
    }
}
