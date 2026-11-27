//! Where a sound is, the way vanilla's OpenAL Soft hears it: a mono file is
//! panned to where it is heard from (`AL_POSITION` against the listener's
//! `AL_ORIENTATION`), a stereo file keeps its own left and right and is only
//! quieter with distance (OpenAL does not spatialize multi-channel buffers).
//! Both fade linearly to nothing at their range
//! (`AL_LINEAR_DISTANCE_CLAMPED`, reference 0, rolloff 1).
//!
//! The mixer runs with the output's channels and every sound is rendered
//! straight into them: mono, stereo, quad, 5.1 or 7.1.

use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate};
use rodio::{Sample, Source};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// Frames between two looks at where the listener is: about 5 ms.
const REFRESH_FRAMES: u32 = 256;
/// The most output channels a layout has (7.1).
pub const MAX_CHANNELS: usize = 8;
/// What a channel gets at most from a pan, so the middle of two speakers
/// is as loud as either one alone, as the stereo pan always was.
const PAN_BOOST: f32 = std::f32::consts::SQRT_2;

/// The camera as vanilla's `ListenerTransform`: position, forward and up,
/// written by the game each frame and read by the playing sounds.
pub struct Listener {
    position: [AtomicU32; 3],
    forward: [AtomicU32; 3],
    up: [AtomicU32; 3],
}

impl Default for Listener {
    /// `ListenerTransform.INITIAL`: at the origin, looking north.
    fn default() -> Self {
        let listener = Self {
            position: Default::default(),
            forward: Default::default(),
            up: Default::default(),
        };
        listener.set([0.0; 3], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        listener
    }
}

/// One look at the listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListenerPose {
    pub position: [f32; 3],
    pub forward: [f32; 3],
    pub up: [f32; 3],
}

impl ListenerPose {
    /// `ListenerTransform.right`: forward cross up.
    pub fn right(&self) -> [f32; 3] {
        let (f, u) = (self.forward, self.up);
        [
            f[1] * u[2] - f[2] * u[1],
            f[2] * u[0] - f[0] * u[2],
            f[0] * u[1] - f[1] * u[0],
        ]
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl Listener {
    pub fn set(&self, position: [f64; 3], forward: [f32; 3], up: [f32; 3]) {
        let store = |slots: &[AtomicU32; 3], values: [f32; 3]| {
            for (slot, value) in slots.iter().zip(values) {
                slot.store(value.to_bits(), Ordering::Relaxed);
            }
        };
        store(&self.position, position.map(|c| c as f32));
        store(&self.forward, forward);
        store(&self.up, up);
    }

    pub fn get(&self) -> ListenerPose {
        let load = |slots: &[AtomicU32; 3]| {
            [0, 1, 2].map(|i| f32::from_bits(slots[i].load(Ordering::Relaxed)))
        };
        ListenerPose {
            position: load(&self.position),
            forward: load(&self.forward),
            up: load(&self.up),
        }
    }
}

/// `AL_LINEAR_DISTANCE_CLAMPED`: full volume at the listener, none from
/// `max_distance` on.
pub fn linear_gain(distance: f32, max_distance: f32) -> f32 {
    if max_distance <= 0.0 {
        return 0.0;
    }
    (1.0 - distance / max_distance).clamp(0.0, 1.0)
}

/// The output's speakers, in the order of its channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    channels: usize,
    /// Each channel's direction, radians clockwise from the front; `None`
    /// for the subwoofer and channels nothing is panned to.
    azimuth: [Option<f32>; MAX_CHANNELS],
    front_left: usize,
    front_right: usize,
}

const fn deg(degrees: f32) -> Option<f32> {
    Some(degrees * std::f32::consts::PI / 180.0)
}

impl Layout {
    /// The speakers of an output with `channels` channels, in the order the
    /// platform's audio API puts them: ALSA's on Linux (FL FR RL RR FC LFE
    /// SL SR), WAVE's elsewhere (FL FR FC LFE BL BR SL SR).
    pub fn for_channels(channels: u16) -> Self {
        let channels = usize::from(channels).clamp(1, MAX_CHANNELS);
        let mut azimuth = [None; MAX_CHANNELS];
        let alsa = cfg!(target_os = "linux");
        let speakers: &[Option<f32>] = match channels {
            1 => &[deg(0.0)],
            4 => &[deg(-45.0), deg(45.0), deg(-135.0), deg(135.0)],
            6 if alsa => &[
                deg(-30.0),
                deg(30.0),
                deg(-110.0),
                deg(110.0),
                deg(0.0),
                None,
            ],
            6 => &[
                deg(-30.0),
                deg(30.0),
                deg(0.0),
                None,
                deg(-110.0),
                deg(110.0),
            ],
            8 if alsa => &[
                deg(-30.0),
                deg(30.0),
                deg(-150.0),
                deg(150.0),
                deg(0.0),
                None,
                deg(-90.0),
                deg(90.0),
            ],
            8 => &[
                deg(-30.0),
                deg(30.0),
                deg(0.0),
                None,
                deg(-150.0),
                deg(150.0),
                deg(-90.0),
                deg(90.0),
            ],
            // Stereo, and anything else: the first two are left and right.
            _ => &[deg(-30.0), deg(30.0)],
        };
        azimuth[..speakers.len()].copy_from_slice(speakers);
        Self {
            channels,
            azimuth,
            front_left: 0,
            front_right: if channels == 1 { 0 } else { 1 },
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The speakers that are panned to: more than left and right.
    fn surround(&self) -> bool {
        self.azimuth.iter().flatten().count() > 2
    }

    /// The gain of each channel for a mono sound from `direction`, which is
    /// in the listener's frame (`x` right, `y` up, `z` forward) and of
    /// length 1, or zero for a sound at the listener.
    fn pan(&self, direction: [f32; 3]) -> [f32; MAX_CHANNELS] {
        let mut gains = [0.0; MAX_CHANNELS];
        if self.channels == 1 {
            gains[0] = 1.0;
            return gains;
        }
        if !self.surround() {
            // The constant-power pan between left and right; straight
            // ahead, above or at the ears, both at full.
            let angle = (direction[0].clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
            gains[self.front_left] = (PAN_BOOST * angle.cos()).min(1.0);
            gains[self.front_right] = (PAN_BOOST * angle.sin()).min(1.0);
            return gains;
        }
        let horizontal = direction[0].hypot(direction[2]);
        if horizontal < 1.0e-4 {
            gains[self.front_left] = 1.0;
            gains[self.front_right] = 1.0;
            return gains;
        }
        let azimuth = direction[0].atan2(direction[2]);
        // The two speakers either side of the direction, around the circle.
        let speakers = || {
            self.azimuth
                .iter()
                .enumerate()
                .filter_map(|(i, a)| a.map(|a| (i, a)))
        };
        let tau = std::f32::consts::TAU;
        let ahead = |from: f32| (azimuth - from).rem_euclid(tau);
        let (left, from) = speakers()
            .min_by(|a, b| ahead(a.1).total_cmp(&ahead(b.1)))
            .expect("a surround layout has speakers");
        let (right, to) = speakers()
            .filter(|&(i, _)| i != left)
            .min_by(|a, b| {
                (a.1 - azimuth)
                    .rem_euclid(tau)
                    .total_cmp(&(b.1 - azimuth).rem_euclid(tau))
            })
            .expect("a surround layout has two speakers");
        let span = (to - from).rem_euclid(tau).max(1.0e-6);
        let t = (ahead(from) / span).clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
        // Overhead or underfoot spreads over every speaker.
        let spread = 1.0 - horizontal;
        let each = spread / (speakers().count() as f32).sqrt();
        for (i, _) in speakers() {
            gains[i] = each;
        }
        gains[left] += horizontal * (PAN_BOOST * t.cos()).min(1.0);
        gains[right] += horizontal * (PAN_BOOST * t.sin()).min(1.0);
        gains.map(|g| g.min(1.0))
    }

    /// How each input channel (`inputs` of them, the first two used) is
    /// spread over the outputs: `gains[input][output]`.
    pub fn gains(
        &self,
        inputs: usize,
        place: Option<(ListenerPose, [f32; 3], f32)>,
    ) -> [[f32; MAX_CHANNELS]; 2] {
        let (distance_gain, direction) = match place {
            None => (1.0, [0.0; 3]),
            Some((pose, emitter, range)) => {
                let delta = [0, 1, 2].map(|i| emitter[i] - pose.position[i]);
                let distance = dot(delta, delta).sqrt();
                let direction = if distance < 1.0e-4 {
                    [0.0; 3]
                } else {
                    [pose.right(), pose.up, pose.forward].map(|axis| dot(delta, axis) / distance)
                };
                (linear_gain(distance, range), direction)
            }
        };
        let mut gains = [[0.0; MAX_CHANNELS]; 2];
        if inputs <= 1 {
            gains[0] = self.pan(direction).map(|g| g * distance_gain);
        } else if self.channels == 1 {
            gains[0][0] = 0.5 * distance_gain;
            gains[1][0] = 0.5 * distance_gain;
        } else {
            gains[0][self.front_left] = distance_gain;
            gains[1][self.front_right] = distance_gain;
        }
        gains
    }
}

/// A sound rendered into the output's channels: panned and faded by where
/// it is heard from, if it has a place.
pub struct Placed<I> {
    input: I,
    /// The input's channels; past the second they are skipped.
    inputs: usize,
    layout: Layout,
    place: Option<(Arc<Listener>, [f32; 3], f32)>,
    gains: [[f32; MAX_CHANNELS]; 2],
    frame: [f32; 2],
    /// The output channel the next sample is for.
    channel: usize,
    frames_left: u32,
}

impl<I: Source> Placed<I> {
    /// `place` is the listener, the emitter and how far the sound carries.
    pub fn new(input: I, layout: Layout, place: Option<(Arc<Listener>, [f64; 3], f32)>) -> Self {
        let inputs = usize::from(input.channels().get());
        let place = place.map(|(listener, at, range)| (listener, at.map(|c| c as f32), range));
        let mut placed = Self {
            input,
            inputs,
            layout,
            place,
            gains: [[0.0; MAX_CHANNELS]; 2],
            frame: [0.0; 2],
            channel: 0,
            frames_left: 0,
        };
        placed.refresh();
        placed
    }

    fn refresh(&mut self) {
        let place = self
            .place
            .as_ref()
            .map(|(listener, at, range)| (listener.get(), *at, *range));
        self.gains = self.layout.gains(self.inputs, place);
        self.frames_left = REFRESH_FRAMES;
    }
}

impl<I: Source> Iterator for Placed<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.channel == 0 {
            if self.place.is_some() && self.frames_left == 0 {
                self.refresh();
            }
            self.frames_left = self.frames_left.saturating_sub(1);
            let first = self.input.next()?;
            self.frame = [first, 0.0];
            if self.inputs > 1 {
                self.frame[1] = self.input.next().unwrap_or(0.0);
                for _ in 2..self.inputs {
                    self.input.next();
                }
            }
        }
        let c = self.channel;
        let sample = self.frame[0] * self.gains[0][c] + self.frame[1] * self.gains[1][c];
        self.channel = (c + 1) % self.layout.channels;
        Some(sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let scale = |n: usize| n / self.inputs.max(1) * self.layout.channels;
        let (low, high) = self.input.size_hint();
        (scale(low), high.map(scale))
    }
}

impl<I: Source> Source for Placed<I> {
    fn current_span_len(&self) -> Option<usize> {
        self.input
            .current_span_len()
            .map(|n| n / self.inputs.max(1) * self.layout.channels)
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(self.layout.channels as u16).expect("a layout has a channel")
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.channel = 0;
        self.input.try_seek(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;

    /// Facing south (Minecraft yaw 0).
    fn south() -> ListenerPose {
        ListenerPose {
            position: [0.0; 3],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
        }
    }

    fn mono_gains(layout: &Layout, pose: ListenerPose, at: [f32; 3]) -> [f32; MAX_CHANNELS] {
        layout.gains(1, Some((pose, at, 16.0)))[0]
    }

    #[test]
    fn a_sound_fades_linearly_to_nothing_at_its_range() {
        assert_eq!(linear_gain(0.0, 16.0), 1.0);
        assert_eq!(linear_gain(8.0, 16.0), 0.5);
        assert_eq!(linear_gain(16.0, 16.0), 0.0);
        assert_eq!(linear_gain(40.0, 16.0), 0.0);
        assert_eq!(linear_gain(1.0, 0.0), 0.0);
    }

    /// Facing south, the right hand is -x (west), as `forward x up` says.
    #[test]
    fn a_sound_to_the_right_is_louder_in_the_right_channel() {
        assert_eq!(south().right(), [-1.0, 0.0, 0.0]);
        let stereo = Layout::for_channels(2);
        let g = mono_gains(&stereo, south(), [-4.0, 0.0, 0.0]);
        assert!(g[0] < 1e-6 && (g[1] - 0.75).abs() < 1e-5, "{g:?}");
        let g = mono_gains(&stereo, south(), [4.0, 0.0, 0.0]);
        assert!(g[1] < 1e-6 && (g[0] - 0.75).abs() < 1e-5, "{g:?}");
        let g = mono_gains(&stereo, south(), [0.0, 0.0, 8.0]);
        assert!(
            (g[0] - 0.5).abs() < 1e-5 && (g[1] - 0.5).abs() < 1e-5,
            "{g:?}"
        );
        let g = mono_gains(&stereo, south(), [0.0; 3]);
        assert!(
            (g[0] - 1.0).abs() < 1e-5 && (g[1] - 1.0).abs() < 1e-5,
            "{g:?}"
        );
    }

    #[test]
    fn a_sound_overhead_is_in_both_ears() {
        let stereo = Layout::for_channels(2);
        let g = mono_gains(&stereo, south(), [0.0, 8.0, 0.0]);
        assert!(
            (g[0] - 0.5).abs() < 1e-5 && (g[1] - 0.5).abs() < 1e-5,
            "{g:?}"
        );
    }

    #[test]
    fn looking_down_keeps_the_sides() {
        // Pitch 90: forward is down, up is south; right is still west.
        let down = ListenerPose {
            forward: [0.0, -1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            ..south()
        };
        assert_eq!(down.right(), [-1.0, 0.0, 0.0]);
        let g = mono_gains(&Layout::for_channels(2), down, [-2.0, 0.0, 0.0]);
        assert!(g[0] < 1e-5 && g[1] > 0.8, "{g:?}");
    }

    #[test]
    fn a_stereo_file_keeps_its_sides_and_only_fades() {
        let stereo = Layout::for_channels(2);
        let g = stereo.gains(2, Some((south(), [-8.0, 0.0, 0.0], 16.0)));
        assert_eq!((g[0][0], g[0][1]), (0.5, 0.0));
        assert_eq!((g[1][0], g[1][1]), (0.0, 0.5));
    }

    #[test]
    fn mono_output_hears_only_the_distance() {
        let mono = Layout::for_channels(1);
        assert_eq!(mono_gains(&mono, south(), [-8.0, 0.0, 0.0])[0], 0.5);
        let g = mono.gains(2, None);
        assert_eq!((g[0][0], g[1][0]), (0.5, 0.5));
    }

    #[test]
    fn surround_puts_a_sound_behind_on_the_rear_speakers() {
        let surround = Layout::for_channels(6);
        let g = mono_gains(&surround, south(), [0.0, 0.0, -4.0]);
        let rear: Vec<usize> = (0..6)
            .filter(|&i| surround.azimuth[i].is_some_and(|a| a.abs() > 1.6))
            .collect();
        assert_eq!(rear.len(), 2);
        for i in 0..6 {
            if rear.contains(&i) {
                assert!(g[i] > 0.5, "{g:?}");
            } else {
                assert!(g[i] < 1e-5, "{g:?}");
            }
        }
        // Straight ahead is the centre speaker alone.
        let g = mono_gains(&surround, south(), [0.0, 0.0, 4.0]);
        let centre = (0..6).find(|&i| surround.azimuth[i] == Some(0.0)).unwrap();
        assert!((g[centre] - 0.75).abs() < 1e-5, "{g:?}");
        assert!(
            (0..6).filter(|&i| i != centre).all(|i| g[i] < 1e-5),
            "{g:?}"
        );
    }

    #[test]
    fn a_mono_file_is_rendered_into_every_output_channel() {
        let listener = Arc::new(Listener::default());
        listener.set([0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
        let mono = SamplesBuffer::new(
            NonZero::new(1).unwrap(),
            NonZero::new(44_100).unwrap(),
            vec![1.0; 4],
        );
        let placed = Placed::new(
            mono,
            Layout::for_channels(2),
            Some((listener, [-4.0, 0.0, 0.0], 16.0)),
        );
        assert_eq!(placed.channels().get(), 2);
        let heard: Vec<f32> = placed.collect();
        assert_eq!(heard.len(), 8);
        assert!(heard[0].abs() < 1e-6 && (heard[1] - 0.75).abs() < 1e-5);
        assert!(heard[6].abs() < 1e-6 && (heard[7] - 0.75).abs() < 1e-5);
    }
}
