//! The sounds the client makes up itself, with no packet asking for them.

use rand::RngExt;

use super::InWorld;
use super::controls::WorldContext;
use crate::audio::SoundInstance;
use crate::audio::source::SoundSource;
use crate::world::physics::eye_in_water;

/// `UnderwaterAmbientSoundHandler`'s chances per tick.
const ULTRA_RARE: f32 = 1.0e-4;
const RARE: f32 = 1.0e-3;
const COMMON: f32 = 1.0e-2;

/// What the ambient sounds carry from one tick to the next.
#[derive(Default)]
pub(super) struct Ambient {
    /// `LocalPlayer.wasUnderwater`.
    under_water: bool,
    /// `UnderwaterAmbientSoundHandler.tickDelay`.
    delay: i32,
    /// `ClientLevel.rainSoundTime`.
    rain_time: i32,
    /// `BiomeAmbientSoundsHandler.moodiness`: how near the next mood sound
    /// is, 0 to 1.
    moodiness: f32,
}

impl Ambient {
    /// A new `LocalPlayer`, with new ambient sound handlers. The rain's
    /// timer is the level's and stays.
    pub(super) fn respawned(&mut self) {
        *self = Self {
            rain_time: self.rain_time,
            ..Self::default()
        };
    }

    /// A new `ClientLevel`: its `rainSoundTime` starts over.
    pub(super) fn new_level(&mut self) {
        self.rain_time = 0;
    }
}

/// `BiomeAmbientSoundsHandler.SKY_MOOD_RECOVERY_RATE`.
const SKY_MOOD_RECOVERY_RATE: f32 = 0.001;

/// One tick of the mood for the light at the sampled block: daylight wears
/// it off, darkness builds it up. Whether the mood sound plays now; the
/// mood is then back at 0.
fn mood_step(moodiness: &mut f32, tick_delay: i32, sky: u8, block: u8) -> bool {
    if sky > 0 {
        *moodiness -= f32::from(sky) / 15.0 * SKY_MOOD_RECOVERY_RATE;
    } else {
        *moodiness -= (f32::from(block) - 1.0) / tick_delay as f32;
    }
    if *moodiness >= 1.0 {
        *moodiness = 0.0;
        true
    } else {
        *moodiness = moodiness.max(0.0);
        false
    }
}

/// Which of the underwater sounds a roll of `chance` (`0..1`) picks.
fn underwater_addition(chance: f32) -> Option<&'static str> {
    if chance < ULTRA_RARE {
        Some("ambient.underwater.loop.additions.ultra_rare")
    } else if chance < RARE {
        Some("ambient.underwater.loop.additions.rare")
    } else if chance < COMMON {
        Some("ambient.underwater.loop.additions")
    } else {
        None
    }
}

impl InWorld {
    pub(super) fn tick_ambient_sounds(&mut self, ui: &WorldContext) {
        self.tick_underwater_sounds();
        self.tick_rain_sounds(i32::from(ui.config.weather_radius));
        self.tick_biome_ambient();
    }

    /// `LocalPlayer.getCurrentMood`, for F3.
    pub(super) fn mood(&self) -> f32 {
        self.ambient.moodiness
    }

    /// `minecraft:audio/ambient_sounds` where the player is: the biome's,
    /// else the dimension's.
    fn ambient_sounds_here(&self) -> Option<&crate::world::biome::AmbientSounds> {
        let feet = self.player.feet().floor().as_ivec3();
        self.level
            .biome(feet.x, feet.y, feet.z)
            .and_then(|id| self.level_info.biomes.biome(id))
            .and_then(|biome| biome.ambient_sounds.as_ref())
            .or(self
                .level_info
                .dimension_type
                .attributes
                .ambient_sounds
                .as_ref())
    }

    /// `BiomeAmbientSoundsHandler.tick`, but for the loop: the additions and
    /// the mood sound of dark places.
    fn tick_biome_ambient(&mut self) {
        let mut moodiness = self.ambient.moodiness;
        let mut played = Vec::new();
        if let Some(ambient) = self.ambient_sounds_here() {
            let mut random = rand::rng();
            for addition in &ambient.additions {
                if random.random::<f64>() < addition.tick_chance {
                    // `SimpleSoundInstance.forAmbientAddition`: no place.
                    played.push(SoundInstance::of(addition.sound.clone()).on(SoundSource::Ambient));
                }
            }
            if let Some(mood) = &ambient.mood {
                let extent = mood.block_search_extent;
                let span = (extent * 2 + 1).max(1);
                let feet = self.player.feet();
                let eye_y = self.player.tick_eye().y;
                let mut offset = || f64::from(random.random_range(0..span) - extent);
                let sample = [feet.x + offset(), eye_y + offset(), feet.z + offset()]
                    .map(|c| c.floor() as i32);
                let (block, sky) = self.level.light_at(sample[0], sample[1], sample[2]);
                if mood_step(&mut moodiness, mood.tick_delay, sky, block) {
                    // From past the sampled block, along the line to it.
                    let to = [
                        f64::from(sample[0]) + 0.5 - feet.x,
                        f64::from(sample[1]) + 0.5 - eye_y,
                        f64::from(sample[2]) + 0.5 - feet.z,
                    ];
                    let distance = to.iter().map(|d| d * d).sum::<f64>().sqrt();
                    let along = (distance + mood.offset) / distance;
                    let at = [
                        feet.x + to[0] * along,
                        eye_y + to[1] * along,
                        feet.z + to[2] * along,
                    ];
                    // `SimpleSoundInstance.forAmbientMood`.
                    played.push(
                        SoundInstance::of(mood.sound.clone())
                            .on(SoundSource::Ambient)
                            .at(at),
                    );
                }
            }
        }
        self.ambient.moodiness = moodiness;
        self.audio.extend(played);
    }

    fn ambient_sound(
        &mut self,
        event: &'static str,
        at: [f64; 3],
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        let mut sound = SoundInstance::of(event).at(at);
        sound.source = source;
        sound.volume = volume;
        sound.pitch = pitch;
        self.audio.push(sound);
    }

    fn tick_underwater_sounds(&mut self) {
        let eye = self.player.tick_eye();
        let at = [eye.x, eye.y, eye.z];
        let now = eye_in_water(&self.level, eye);
        let spectator = self.game_mode.current.is_spectator();
        let was = self.ambient.under_water;
        if !spectator {
            if now && !was {
                self.ambient_sound(
                    "ambient.underwater.enter",
                    at,
                    SoundSource::Ambient,
                    1.0,
                    1.0,
                );
            } else if was && !now {
                self.ambient_sound(
                    "ambient.underwater.exit",
                    at,
                    SoundSource::Ambient,
                    1.0,
                    1.0,
                );
            }
            self.ambient.under_water = now;
        }
        self.ambient.delay -= 1;
        if self.ambient.delay <= 0
            && self.ambient.under_water
            && let Some(event) = underwater_addition(rand::random::<f32>())
        {
            self.ambient.delay = 0;
            self.ambient_sound(event, at, SoundSource::Ambient, 1.0, 1.0);
        }
    }

    /// `ClientLevel.tickRain`'s sound.
    fn tick_rain_sounds(&mut self, radius: i32) {
        let rain = self.weather.rain;
        if rain <= 0.0 || radius <= 0 {
            return;
        }
        let camera = self.player.tick_eye().floor().as_ivec3();
        let diameter = 2 * radius + 1;
        let drops = ((0.225 * (diameter * diameter) as f32 * rain * rain) as i32).max(0);
        let mut random = rand::rng();
        let mut spot = None;
        for _ in 0..drops {
            let x = camera.x + random.random_range(0..diameter) - radius;
            let z = camera.z + random.random_range(0..diameter) - radius;
            let y = self.level.motion_blocking_height(x, z);
            let rains = self
                .level
                .biome(x, y, z)
                .and_then(|id| self.level_info.biomes.biome(id))
                .is_some_and(|biome| {
                    biome.precipitation() == crate::world::weather::Precipitation::Rain
                });
            if y > self.level.height().0 && (y - camera.y).abs() <= 10 && rains {
                spot = Some([x, y - 1, z]);
            }
        }
        let Some(spot) = spot else {
            return;
        };
        let wait = self.ambient.rain_time;
        self.ambient.rain_time += 1;
        if random.random_range(0..3) >= wait {
            return;
        }
        self.ambient.rain_time = 0;
        let covered = self.level.motion_blocking_height(camera.x, camera.z) > camera.y;
        let at = [
            f64::from(spot[0]) + 0.5,
            f64::from(spot[1]) + 0.5,
            f64::from(spot[2]) + 0.5,
        ];
        if spot[1] > camera.y + 1 && covered {
            self.ambient_sound("weather.rain.above", at, SoundSource::Weather, 0.1, 0.5);
        } else {
            self.ambient_sound("weather.rain", at, SoundSource::Weather, 0.2, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkness_builds_the_mood_and_daylight_wears_it_off() {
        let mut mood = 0.0;
        assert!(!mood_step(&mut mood, 6000, 0, 0));
        assert!((mood - 1.0 / 6000.0).abs() < 1e-9);
        // A torch-lit block (light 1 and up) does not build it.
        let before = mood;
        mood_step(&mut mood, 6000, 0, 1);
        assert_eq!(mood, before);
        mood_step(&mut mood, 6000, 15, 0);
        assert_eq!(mood, 0.0);
        let mut ticks = 0;
        let mut mood = 0.0;
        while !mood_step(&mut mood, 6000, 0, 0) {
            ticks += 1;
        }
        assert!((5990..=6010).contains(&ticks), "{ticks}");
        assert_eq!(mood, 0.0);
    }

    #[test]
    fn the_rarer_the_roll_the_rarer_the_sound() {
        assert_eq!(
            underwater_addition(0.00001),
            Some("ambient.underwater.loop.additions.ultra_rare")
        );
        assert_eq!(
            underwater_addition(0.0005),
            Some("ambient.underwater.loop.additions.rare")
        );
        assert_eq!(
            underwater_addition(0.005),
            Some("ambient.underwater.loop.additions")
        );
        assert_eq!(underwater_addition(0.5), None);
    }
}
