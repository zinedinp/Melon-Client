//! Vanilla 26.3 `WorldClock` / `ClientClockManager` and the overworld day.

use rustc_hash::FxHashMap;

/// Length of one overworld day, in ticks (`Timelines.OVERWORLD_DAY`).
pub const DAY_TICKS: i32 = 24_000;
/// `MoonPhase.PHASE_LENGTH`.
pub const MOON_PHASE_TICKS: i32 = DAY_TICKS;
/// `MoonPhase.COUNT`.
pub const MOON_PHASES: i32 = 8;

/// One clock's network snapshot (`ClockNetworkState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockUpdate {
    /// Registry id (`WorldClock.STREAM_CODEC` holder id). Overworld is 0.
    pub id: i32,
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
}

/// `ClientClockManager`.
#[derive(Clone, Debug)]
pub struct ClientClocks {
    clocks: FxHashMap<i32, ClockInstance>,
    last_game_time: i64,
    /// `ClientLevelData.gameTime`.
    game_time: i64,
}

/// `ClientClockManager.ClientClockInstance`.
#[derive(Clone, Copy, Debug)]
struct ClockInstance {
    total_ticks: i64,
    partial_tick: f32,
    rate: f32,
}

impl Default for ClockInstance {
    fn default() -> Self {
        Self {
            total_ticks: 0,
            partial_tick: 0.0,
            rate: 1.0,
        }
    }
}

impl Default for ClientClocks {
    fn default() -> Self {
        let mut clocks = FxHashMap::default();
        clocks.insert(0, ClockInstance::default());
        Self {
            clocks,
            last_game_time: 0,
            game_time: 0,
        }
    }
}

impl ClientClocks {
    /// `ClientLevel.tickTime`: one client tick, then the clocks catch up.
    pub fn tick(&mut self) {
        self.game_time += 1;
        self.advance_to(self.game_time);
    }

    /// `ClientPacketListener.handleSetTime`.
    pub fn handle_updates(&mut self, game_time: i64, updates: &[ClockUpdate]) {
        self.game_time = game_time;
        self.advance_to(game_time);
        for update in updates {
            let clock = self.clocks.entry(update.id).or_default();
            clock.total_ticks = update.total_ticks;
            clock.partial_tick = update.partial_tick;
            clock.rate = update.rate;
        }
    }

    fn advance_to(&mut self, game_time: i64) {
        let delta = game_time - self.last_game_time;
        self.last_game_time = game_time;
        if delta == 0 {
            return;
        }
        for clock in self.clocks.values_mut() {
            let new_partial = f64::from(clock.partial_tick) + delta as f64 * f64::from(clock.rate);
            let full = new_partial.floor() as i64;
            clock.partial_tick = (new_partial - full as f64) as f32;
            clock.total_ticks += full;
        }
    }

    /// Overworld day clock (`WorldClocks.OVERWORLD`, registry id 0).
    fn overworld(&self) -> ClockInstance {
        self.clocks.get(&0).copied().unwrap_or_default()
    }

    /// Fractional overworld ticks for this frame.
    pub fn overworld_time(&self, partial: f32) -> f64 {
        let clock = self.overworld();
        clock.total_ticks as f64
            + f64::from(clock.partial_tick)
            + f64::from(partial.clamp(0.0, 1.0)) * f64::from(clock.rate)
    }

    /// Environment attributes of the overworld day timeline at `partial`.
    pub fn environment(&self, partial: f32) -> Environment {
        Environment::at(self.overworld_time(partial))
    }

    pub fn game_time(&self) -> i64 {
        self.game_time
    }
}

/// What the sky and the world's lighting read off the overworld day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    /// `EnvironmentAttributes.SUN_ANGLE`, radians.
    pub sun_angle: f32,
    /// `EnvironmentAttributes.MOON_ANGLE`, radians.
    pub moon_angle: f32,
    /// `EnvironmentAttributes.STAR_ANGLE`, radians.
    pub star_angle: f32,
    /// `EnvironmentAttributes.STAR_BRIGHTNESS`.
    pub star_brightness: f32,
    /// The `SKY_COLOR` track: black from dusk to dawn.
    pub sky_multiplier: [f32; 3],
    /// The `FOG_COLOR` track: `NIGHT_FOG_COLOR_MULTIPLIER` at night.
    pub fog_multiplier: [f32; 3],
    /// The `CLOUD_COLOR` track.
    pub cloud_multiplier: [f32; 4],
    /// `EnvironmentAttributes.SUNRISE_SUNSET_COLOR`.
    pub sunrise_sunset: [f32; 4],
    /// The `SKY_LIGHT_FACTOR` track (1 by day, 0.24 at night).
    pub sky_light_factor: f32,
    /// The `SKY_LIGHT_LEVEL` track.
    pub sky_level_multiplier: f32,
    /// The `SKY_LIGHT_COLOR` track.
    pub sky_light_color: [f32; 3],
    /// `MoonPhase.index`.
    pub moon_phase: u8,
    /// Whole overworld days completed (`Timeline.getPeriodCount`).
    pub day: i64,
    /// Ticks into the current day, `0..DAY_TICKS`.
    pub day_time: i64,
}

impl Environment {
    /// Sample the hardcoded overworld day / moon timelines at `time` ticks.
    pub fn at(time: f64) -> Self {
        let day_time = time.rem_euclid(f64::from(DAY_TICKS));
        let sun_deg = sample_sun_angle(day_time);
        Self {
            sun_angle: sun_deg.to_radians(),
            moon_angle: (sun_deg + 180.0).to_radians(),
            star_angle: sun_deg.to_radians(),
            star_brightness: sample_star_brightness(day_time),
            sky_multiplier: sample_sky_multiplier(day_time),
            fog_multiplier: sample_fog_multiplier(day_time),
            cloud_multiplier: sample_cloud_multiplier(day_time),
            sunrise_sunset: sample_sunrise_sunset(day_time),
            sky_light_factor: sample_sky_light_factor(day_time),
            sky_level_multiplier: sample_sky_level_multiplier(day_time),
            sky_light_color: sample_sky_light_color(day_time),
            moon_phase: ((time.div_euclid(f64::from(MOON_PHASE_TICKS)) as i64)
                .rem_euclid(i64::from(MOON_PHASES))) as u8,
            day: time.div_euclid(f64::from(DAY_TICKS)) as i64,
            day_time: day_time as i64,
        }
    }

    /// The timeline with no effect: the sun at noon, full light.
    pub fn still() -> Self {
        Self {
            sky_multiplier: [1.0; 3],
            fog_multiplier: [1.0; 3],
            cloud_multiplier: [1.0; 4],
            sunrise_sunset: [0.0; 4],
            sky_light_factor: 1.0,
            sky_level_multiplier: 1.0,
            sky_light_color: [1.0; 3],
            star_brightness: 0.0,
            ..Self::at(6000.0)
        }
    }
}

/// Sun/moon/star angle in degrees. Keyframes `(6000, 360)` then `(6000, 0)`
fn sample_sun_angle(day_time: f64) -> f32 {
    let t = if day_time < 6000.0 {
        (day_time + 18_000.0) / 24_000.0
    } else {
        (day_time - 6_000.0) / 24_000.0
    };
    sky_angle_ease(t as f32) * 360.0
}

/// `EasingType.CubicBezier` for `symmetricCubicBezier(0.362, 0.241)`.
fn sky_angle_ease(x: f32) -> f32 {
    const X1: f32 = 0.362;
    const Y1: f32 = 0.241;
    const X2: f32 = 1.0 - X1;
    const Y2: f32 = 1.0 - Y1;
    cubic_bezier_y(x.clamp(0.0, 1.0), X1, Y1, X2, Y2)
}

fn cubic_bezier_y(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x_curve = bezier_curve(x1, x2);
    let y_curve = bezier_curve(y1, y2);
    y_curve.sample(solve_t(x, x_curve))
}

#[derive(Clone, Copy)]
struct CubicCurve {
    a: f32,
    b: f32,
    c: f32,
}

impl CubicCurve {
    fn sample(self, t: f32) -> f32 {
        ((self.a * t + self.b) * t + self.c) * t
    }

    fn sample_gradient(self, t: f32) -> f32 {
        (3.0 * self.a * t + 2.0 * self.b) * t + self.c
    }
}

fn bezier_curve(v1: f32, v2: f32) -> CubicCurve {
    CubicCurve {
        a: 3.0 * v1 - 3.0 * v2 + 1.0,
        b: -6.0 * v1 + 3.0 * v2,
        c: 3.0 * v1,
    }
}

fn solve_t(x: f32, x_curve: CubicCurve) -> f32 {
    let mut t = x;
    for _ in 0..4 {
        let error = x_curve.sample(t) - x;
        if error.abs() < 1.0e-5 {
            return t;
        }
        let gradient = x_curve.sample_gradient(t);
        if gradient < 1.0e-5 {
            break;
        }
        t -= (error / gradient).clamp(-0.25, 0.25);
    }
    let mut t0 = 0.0;
    let mut t1 = 1.0;
    t = t.clamp(0.0, 1.0);
    while t0 < t1 {
        let error = x_curve.sample(t) - x;
        if error.abs() < 1.0e-5 {
            return t;
        }
        if error < 0.0 {
            t0 = t;
        } else {
            t1 = t;
        }
        t = (t1 + t0) / 2.0;
    }
    t
}

/// `Timelines`' night multipliers.
const NIGHT_FOG: [f32; 3] = [0.06, 0.06, 0.09];
const NIGHT_CLOUD: [f32; 4] = [0.1, 0.1, 0.15, 1.0];

/// The `SKY_COLOR` track's multiplier on the biome's sky colour.
fn sample_sky_multiplier(day_time: f64) -> [f32; 3] {
    sample_wrapped_vec3(
        day_time,
        &[
            (133.0, [1.0, 1.0, 1.0]),
            (11_867.0, [1.0, 1.0, 1.0]),
            (13_670.0, [0.0, 0.0, 0.0]),
            (22_330.0, [0.0, 0.0, 0.0]),
        ],
    )
}

/// The `FOG_COLOR` track.
fn sample_fog_multiplier(day_time: f64) -> [f32; 3] {
    sample_wrapped_vec3(
        day_time,
        &[
            (133.0, [1.0; 3]),
            (11_867.0, [1.0; 3]),
            (13_670.0, NIGHT_FOG),
            (22_330.0, NIGHT_FOG),
        ],
    )
}

/// The `CLOUD_COLOR` track.
fn sample_cloud_multiplier(day_time: f64) -> [f32; 4] {
    sample_wrapped_vec4(
        day_time,
        &[
            (133.0, [1.0; 4]),
            (11_867.0, [1.0; 4]),
            (13_670.0, NIGHT_CLOUD),
            (22_330.0, NIGHT_CLOUD),
        ],
    )
}

/// The `SUNRISE_SUNSET_COLOR` track, `ARGB` ints in the timeline.
fn sample_sunrise_sunset(day_time: f64) -> [f32; 4] {
    const KEYS: [(f64, i32); 32] = [
        (71.0, 1_609_540_403),
        (310.0, 703_969_843),
        (565.0, 117_167_155),
        (730.0, 16_770_355),
        (11_270.0, 16_770_355),
        (11_397.0, 83_679_283),
        (11_522.0, 268_028_723),
        (11_690.0, 703_969_843),
        (11_929.0, 1_609_540_403),
        (12_243.0, -1_310_226_637),
        (12_358.0, -857_440_717),
        (12_512.0, -371_166_669),
        (12_613.0, -153_261_261),
        (12_732.0, -19_242_189),
        (12_841.0, -19_440_589),
        (13_035.0, -321_760_973),
        (13_252.0, -1_043_577_037),
        (13_775.0, 918_435_635),
        (13_888.0, 532_362_547),
        (14_039.0, 163_001_139),
        (14_192.0, 11_744_051),
        (21_807.0, 11_678_515),
        (21_961.0, 163_001_139),
        (22_112.0, 532_362_547),
        (22_225.0, 918_435_635),
        (22_748.0, -1_043_577_037),
        (22_965.0, -321_760_973),
        (23_159.0, -19_440_589),
        (23_272.0, -19_242_189),
        (23_488.0, -371_166_669),
        (23_642.0, -857_440_717),
        (23_757.0, -1_310_226_637),
    ];
    let keys: Vec<(f64, [f32; 4])> = KEYS
        .iter()
        .map(|&(t, argb)| (t, crate::math::argb(argb as u32)))
        .collect();
    sample_wrapped_vec4(day_time, &keys)
}

/// The `SKY_LIGHT_LEVEL` track's multiplier.
fn sample_sky_level_multiplier(day_time: f64) -> f32 {
    sample_wrapped_scalar(
        day_time,
        &[
            (133.0, 1.0),
            (11_867.0, 1.0),
            (13_670.0, 0.266_666_68),
            (22_330.0, 0.266_666_68),
        ],
    )
}

fn sample_sky_light_factor(day_time: f64) -> f32 {
    sample_wrapped_scalar(
        day_time,
        &[
            (730.0, 1.0),
            (11_270.0, 1.0),
            (13_140.0, 0.24),
            (22_860.0, 0.24),
        ],
    )
}

fn sample_sky_light_color(day_time: f64) -> [f32; 3] {
    const NIGHT: [f32; 3] = [0.48, 0.48, 1.0];
    sample_wrapped_vec3(
        day_time,
        &[
            (730.0, [1.0; 3]),
            (11_270.0, [1.0; 3]),
            (13_140.0, NIGHT),
            (22_860.0, NIGHT),
        ],
    )
}

fn sample_star_brightness(day_time: f64) -> f32 {
    sample_wrapped_scalar(
        day_time,
        &[
            (92.0, 0.037),
            (627.0, 0.0),
            (11_373.0, 0.0),
            (11_732.0, 0.016),
            (11_959.0, 0.044),
            (12_399.0, 0.143),
            (12_729.0, 0.258),
            (13_228.0, 0.5),
            (22_772.0, 0.5),
            (23_032.0, 0.364),
            (23_356.0, 0.225),
            (23_758.0, 0.101),
        ],
    )
    .max(0.0)
}

/// Period-24000 piecewise-linear sample.
fn sample_wrapped_scalar(day_time: f64, keys: &[(f64, f32)]) -> f32 {
    let (t0, a, t1, b) = wrap_segment(day_time, keys);
    let alpha = ((day_time - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
    a + (b - a) * alpha
}

fn sample_wrapped_vec3(day_time: f64, keys: &[(f64, [f32; 3])]) -> [f32; 3] {
    let (t0, a, t1, b) = wrap_segment(day_time, keys);
    let alpha = ((day_time - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
    [
        a[0] + (b[0] - a[0]) * alpha,
        a[1] + (b[1] - a[1]) * alpha,
        a[2] + (b[2] - a[2]) * alpha,
    ]
}

fn sample_wrapped_vec4(day_time: f64, keys: &[(f64, [f32; 4])]) -> [f32; 4] {
    let (t0, a, t1, b) = wrap_segment(day_time, keys);
    let alpha = ((day_time - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * alpha)
}

fn wrap_segment<T: Copy>(day_time: f64, keys: &[(f64, T)]) -> (f64, T, f64, T) {
    let n = keys.len();
    if day_time < keys[0].0 {
        return (
            keys[n - 1].0 - f64::from(DAY_TICKS),
            keys[n - 1].1,
            keys[0].0,
            keys[0].1,
        );
    }
    for i in 0..n - 1 {
        let t1 = keys[i + 1].0;
        if day_time < t1 {
            return (keys[i].0, keys[i].1, t1, keys[i + 1].1);
        }
    }
    (
        keys[n - 1].0,
        keys[n - 1].1,
        keys[0].0 + f64::from(DAY_TICKS),
        keys[0].1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_time_overwrites_and_local_ticks_advance() {
        let mut clocks = ClientClocks::default();
        clocks.handle_updates(
            100,
            &[ClockUpdate {
                id: 0,
                total_ticks: 6000,
                partial_tick: 0.0,
                rate: 1.0,
            }],
        );
        assert_eq!(clocks.overworld().total_ticks, 6000);
        clocks.tick();
        assert_eq!(clocks.game_time(), 101);
        assert_eq!(clocks.overworld().total_ticks, 6001);
    }

    #[test]
    fn paused_clock_stays_put() {
        let mut clocks = ClientClocks::default();
        clocks.handle_updates(
            0,
            &[ClockUpdate {
                id: 0,
                total_ticks: 18000,
                partial_tick: 0.0,
                rate: 0.0,
            }],
        );
        for _ in 0..20 {
            clocks.tick();
        }
        assert_eq!(clocks.overworld().total_ticks, 18000);
    }

    #[test]
    fn noon_sun_is_at_zenith() {
        let env = Environment::at(6000.0);
        assert!(
            env.sun_angle.abs() < 0.02,
            "noon sun_angle {} rad",
            env.sun_angle
        );
        assert!(env.sky_light_factor > 0.99);
        assert!(env.star_brightness < 0.01);
    }

    #[test]
    fn midnight_is_dark_with_stars() {
        let env = Environment::at(18_000.0);
        assert!(env.sky_light_factor < 0.3);
        assert!(env.star_brightness > 0.4);
        assert!(env.sky_multiplier.iter().all(|&c| c < 0.05));
        assert!((env.fog_multiplier[0] - 0.06).abs() < 1e-5);
    }

    #[test]
    fn the_horizon_glows_at_sunrise_and_sunset_only() {
        assert_eq!(Environment::at(6000.0).sunrise_sunset[3], 0.0);
        assert!(Environment::at(12_732.0).sunrise_sunset[3] > 0.9);
        assert!(Environment::at(23_272.0).sunrise_sunset[3] > 0.9);
        assert_eq!(Environment::still().sunrise_sunset[3], 0.0);
    }

    #[test]
    fn day_count_and_moon_phase_advance() {
        let env = Environment::at(f64::from(DAY_TICKS) * 3.0 + 100.0);
        assert_eq!(env.day, 3);
        assert_eq!(env.moon_phase, 3);
    }
}
