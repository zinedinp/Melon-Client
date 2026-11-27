//! "Directional Audio" (`Options.directionalAudio`): vanilla turns on
//! OpenAL Soft's HRTF, which filters each sound through the head-related
//! impulse responses of where it is, for both ears. This is that, with the
//! same data set and OpenAL Soft's way of picking a response
//! (`GetHrtfCoeffs`), done per sound as its default `hrtf-mode = full` does.

use super::spatial::{Listener, ListenerPose, linear_gain};
use melon_core::math::slices;
use rodio::source::{SeekError, UniformSourceIterator};
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// OpenAL Soft's `Default HRTF.mhr`, see `README.md` beside it.
static DEFAULT_MHR: &[u8] = include_bytes!("default.mhr");

/// Taps of a response; the data set has exactly this many.
pub const TAPS: usize = 64;
/// `HRIR_DELAY_FRACBITS`: delays are stored in quarter samples.
const DELAY_FRACTION: f32 = 4.0;
/// Samples kept from the block before: room for the taps and the longest
/// delay a data set can store (255 quarter samples, 64 whole).
const HISTORY: usize = 128;
/// Frames rendered at once, each block fading to the filters for where the
/// listener is then: about 5 ms.
const BLOCK: usize = 256;
/// The rate the responses are for; sounds are brought to it first.
pub const RATE: u32 = 48_000;
/// Where OpenAL Soft puts the two channels of a stereo file (`AL_STEREO_ANGLES`).
const STEREO_ANGLE: f32 = std::f32::consts::FRAC_PI_6;

/// One elevation's ring of measured directions.
#[derive(Clone, Copy, Debug)]
struct Elevation {
    azimuths: usize,
    first: usize,
}

/// A parsed data set: the left ear's responses (the right ear hears the
/// mirrored direction), from straight down to straight up.
#[derive(Debug)]
pub struct HrtfStore {
    rate: u32,
    elevations: Vec<Elevation>,
    /// `[response][tap]`.
    coeffs: Vec<[f32; TAPS]>,
    /// Each response's delay, in samples.
    delays: Vec<f32>,
}

/// The two ears' filters for one direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarFilters {
    /// `[ear][tap]`, left then right, taps reversed for the convolution.
    taps: [[f32; TAPS]; 2],
    /// Whole samples each ear hears it late.
    delays: [usize; 2],
}

impl EarFilters {
    /// The same filters, `gain` times as loud.
    fn scaled(mut self, gain: f32) -> Self {
        for tap in self.taps.as_flattened_mut() {
            *tap *= gain;
        }
        self
    }
}

impl HrtfStore {
    /// `LoadHrtf03`: a `MinPHR03` file with one field, mono (left ear) data.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let slice = bytes.get(at..at + n)?;
            at += n;
            Some(slice)
        };
        if take(8)? != b"MinPHR03" {
            return None;
        }
        let rate = u32::from_le_bytes(take(4)?.try_into().ok()?);
        let channel_type = take(1)?[0];
        let taps = usize::from(take(1)?[0]);
        let fields = take(1)?[0];
        if channel_type != 0 || taps != TAPS || fields != 1 {
            return None;
        }
        let _distance = take(2)?;
        let elevation_count = usize::from(take(1)?[0]);
        let mut elevations = Vec::with_capacity(elevation_count);
        let mut first = 0;
        for &azimuths in take(elevation_count)? {
            let azimuths = usize::from(azimuths);
            elevations.push(Elevation { azimuths, first });
            first += azimuths;
        }
        let count = first;
        let mut coeffs = Vec::with_capacity(count);
        for _ in 0..count {
            let raw = take(TAPS * 3)?;
            let mut response = [0.0; TAPS];
            for (tap, value) in response.iter_mut().zip(raw.as_chunks::<3>().0) {
                let int = i32::from_le_bytes([0, value[0], value[1], value[2]]) >> 8;
                *tap = int as f32 / 8_388_608.0;
            }
            coeffs.push(response);
        }
        let delays = take(count)?
            .iter()
            .map(|&d| f32::from(d) / DELAY_FRACTION)
            .collect();
        (at == bytes.len()).then_some(Self {
            rate,
            elevations,
            coeffs,
            delays,
        })
    }

    /// The data set vanilla hears, parsed once.
    pub fn default_set() -> Option<&'static Self> {
        static STORE: OnceLock<Option<HrtfStore>> = OnceLock::new();
        STORE.get_or_init(|| Self::parse(DEFAULT_MHR)).as_ref()
    }

    pub fn len(&self) -> usize {
        self.coeffs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.coeffs.is_empty()
    }

    /// `GetHrtfCoeffs`: the filters for a sound at `elevation` (up is
    /// positive) and `azimuth` (right is positive), radians, blended from
    /// the four nearest measured directions.
    pub fn filters(&self, elevation: f32, azimuth: f32) -> EarFilters {
        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let last = self.elevations.len() - 1;
        let ev = ((FRAC_PI_2 + elevation) / PI * last as f32).clamp(0.0, last as f32);
        let ev0 = ev.floor() as usize;
        let ev1 = (ev0 + 1).min(last);
        let ev_frac = ev - ev0 as f32;
        // Each ring's two neighbours and their weights, and the mirrored
        // index of each for the right ear.
        let mut picks = [(0usize, 0usize, 0.0f32); 4];
        for (i, (index, weight)) in [(ev0, 1.0 - ev_frac), (ev1, ev_frac)]
            .into_iter()
            .enumerate()
        {
            let ring = self.elevations[index];
            let az = (TAU + azimuth).rem_euclid(TAU) / TAU * ring.azimuths as f32;
            let az0 = az.floor() as usize % ring.azimuths;
            let az1 = (az0 + 1) % ring.azimuths;
            let az_frac = az - az.floor();
            let mirror = |a: usize| ring.first + (ring.azimuths - a) % ring.azimuths;
            picks[i * 2] = (ring.first + az0, mirror(az0), weight * (1.0 - az_frac));
            picks[i * 2 + 1] = (ring.first + az1, mirror(az1), weight * az_frac);
        }
        let mut taps = [[0.0; TAPS]; 2];
        let mut delays = [0.0f32; 2];
        for (left, right, weight) in picks {
            if weight <= 0.0 {
                continue;
            }
            for (ear, index) in [left, right].into_iter().enumerate() {
                delays[ear] += self.delays[index] * weight;
                for (tap, c) in self.coeffs[index].iter().enumerate() {
                    // Reversed, so the convolution reads the history forward.
                    taps[ear][TAPS - 1 - tap] += c * weight;
                }
            }
        }
        EarFilters {
            taps,
            delays: delays.map(|d| d.round() as usize),
        }
    }
}

/// A sound's direction in the listener's frame: elevation and azimuth.
fn direction(pose: &ListenerPose, emitter: [f32; 3]) -> (f32, f32, f32) {
    let delta = [0, 1, 2].map(|i| emitter[i] - pose.position[i]);
    let distance = delta.iter().map(|d| d * d).sum::<f32>().sqrt();
    if distance < 1.0e-4 {
        return (distance, 0.0, 0.0);
    }
    let dot = |a: [f32; 3]| (0..3).map(|i| a[i] * delta[i]).sum::<f32>() / distance;
    let (x, y, z) = (dot(pose.right()), dot(pose.up), dot(pose.forward));
    (distance, y.clamp(-1.0, 1.0).asin(), x.atan2(z))
}

/// Each frame's share of the new filters over a block: 0 up to almost 1.
static FADE: [f32; BLOCK] = {
    let mut fade = [0.0; BLOCK];
    let mut i = 0;
    while i < BLOCK {
        fade[i] = i as f32 / BLOCK as f32;
        i += 1;
    }
    fade
};

/// One source the head hears: its recent samples and its ears' filters
/// (with the distance gain in them), fading from the old to the new over a
/// block.
struct Ears {
    /// The last [`HISTORY`] samples of the block before, then this block's.
    samples: Box<[f32; HISTORY + BLOCK]>,
    old: EarFilters,
    new: EarFilters,
}

impl Ears {
    fn new(filters: EarFilters) -> Self {
        Self {
            samples: Box::new([0.0; HISTORY + BLOCK]),
            old: filters,
            new: filters,
        }
    }

    fn retarget(&mut self, filters: EarFilters) {
        self.old = std::mem::replace(&mut self.new, filters);
    }

    /// The sample of the block's frame `frame`.
    fn set(&mut self, frame: usize, sample: f32) {
        self.samples[HISTORY + frame] = sample;
    }

    /// `out[i]`: the block's frame `i` through one ear's filters.
    fn convolve(&self, filters: &EarFilters, ear: usize, out: &mut [f32]) {
        // The window that ends at frame 0, as late as the ear hears it.
        let first = HISTORY + 1 - TAPS - filters.delays[ear];
        let windows = &self.samples[first..first + out.len() + TAPS - 1];
        slices::sliding_dot(windows, &filters.taps[ear], out);
    }

    /// The block's first `frames` frames through both ears, into `out`;
    /// then the block becomes the history of the next.
    fn hear(&mut self, frames: usize, out: &mut [[f32; BLOCK]; 2]) {
        let (mut old, mut change) = ([0.0; BLOCK], [0.0; BLOCK]);
        for (ear, out) in out.iter_mut().enumerate() {
            let out = &mut out[..frames];
            self.convolve(&self.new, ear, out);
            if self.old != self.new {
                let (old, change) = (&mut old[..frames], &mut change[..frames]);
                self.convolve(&self.old, ear, old);
                // old + fade * (new - old)
                slices::sub(out, old, change);
                slices::fmadd(&FADE[..frames], change, old, out);
            }
        }
        self.samples.copy_within(frames..frames + HISTORY, 0);
    }
}

/// A sound with a place, heard through the HRTF: two channels at
/// [`RATE`], for headphones. Rendered a block at a time.
pub struct Binaural<I: Source> {
    input: UniformSourceIterator<I>,
    /// 1 for a mono file, 2 for a stereo one (each side its own source).
    inputs: usize,
    store: &'static HrtfStore,
    listener: Arc<Listener>,
    emitter: [f32; 3],
    range: f32,
    sources: Vec<Ears>,
    /// The block, `[ear][frame]`.
    heard: Box<[[f32; BLOCK]; 2]>,
    /// Frames in it.
    frames: usize,
    /// The next sample out of it, interleaved.
    next: usize,
}

impl<I: Source> Binaural<I> {
    pub fn new(
        input: I,
        store: &'static HrtfStore,
        listener: Arc<Listener>,
        emitter: [f64; 3],
        range: f32,
    ) -> Self {
        let inputs = usize::from(input.channels().get()).clamp(1, 2);
        let channels = ChannelCount::new(inputs as u16).expect("one or two");
        let rate = SampleRate::new(store.rate).expect("a data set has a rate");
        let mut binaural = Self {
            input: UniformSourceIterator::new(input, channels, rate),
            inputs,
            store,
            listener,
            emitter: emitter.map(|c| c as f32),
            range,
            sources: Vec::with_capacity(inputs),
            heard: Box::new([[0.0; BLOCK]; 2]),
            frames: 0,
            next: 0,
        };
        for filters in binaural.targets().into_iter().take(inputs) {
            binaural.sources.push(Ears::new(filters));
        }
        binaural
    }

    /// Each source's filters, gain included, for where the listener is now.
    /// A stereo file's sides stay at ±30° from the listener and only fade
    /// with distance, as OpenAL Soft renders multi-channel sources.
    /// Without allocating: it runs on the audio thread. Only the first
    /// [`Self::inputs`] entries are meant.
    fn targets(&self) -> [EarFilters; 2] {
        let pose = self.listener.get();
        let (distance, elevation, azimuth) = direction(&pose, self.emitter);
        let gain = linear_gain(distance, self.range);
        if self.inputs == 1 {
            [self.store.filters(elevation, azimuth).scaled(gain); 2]
        } else {
            [-STEREO_ANGLE, STEREO_ANGLE].map(|side| self.store.filters(0.0, side).scaled(gain))
        }
    }

    /// The next block, fading to the filters for where the listener is now,
    /// with as many frames as the input still has; false once it has none.
    fn render(&mut self) -> bool {
        let targets = self.targets();
        for (ears, filters) in self.sources.iter_mut().zip(targets) {
            ears.retarget(filters);
        }
        let mut frames = 0;
        'fill: while frames < BLOCK {
            for ears in &mut self.sources {
                let Some(sample) = self.input.next() else {
                    break 'fill;
                };
                ears.set(frames, sample);
            }
            frames += 1;
        }
        if frames == 0 {
            return false;
        }
        let (first, rest) = self.sources.split_first_mut().expect("one or two sources");
        first.hear(frames, &mut self.heard);
        for ears in rest {
            let mut side = [[0.0; BLOCK]; 2];
            ears.hear(frames, &mut side);
            for (heard, side) in self.heard.iter_mut().zip(&side) {
                for (h, s) in heard[..frames].iter_mut().zip(side) {
                    *h += s;
                }
            }
        }
        self.frames = frames;
        self.next = 0;
        true
    }
}

impl<I: Source> Iterator for Binaural<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.next == 2 * self.frames && !self.render() {
            return None;
        }
        let sample = self.heard[self.next % 2][self.next / 2];
        self.next += 1;
        Some(sample)
    }
}

impl<I: Source> Source for Binaural<I> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).expect("two")
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.store.rate).expect("a data set has a rate")
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.input.try_seek(position)?;
        self.next = 2 * self.frames;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::f32::consts::FRAC_PI_2;

    fn store() -> &'static HrtfStore {
        HrtfStore::default_set().expect("the vendored data set parses")
    }

    #[test]
    fn the_data_set_is_read_whole() {
        let store = store();
        assert_eq!(store.rate, RATE);
        assert_eq!(store.len(), 828);
        assert_eq!(store.elevations.len(), 19);
        assert_eq!(store.elevations[0].azimuths, 1);
        assert_eq!(store.elevations[9].azimuths, 72);
        assert!(HrtfStore::parse(&DEFAULT_MHR[..100]).is_none());
    }

    fn energy(taps: &[f32; TAPS]) -> f32 {
        taps.iter().map(|t| t * t).sum()
    }

    #[test]
    fn a_sound_on_the_right_is_louder_and_sooner_in_the_right_ear() {
        let right = store().filters(0.0, FRAC_PI_2);
        assert!(energy(&right.taps[1]) > 2.0 * energy(&right.taps[0]));
        assert!(right.delays[0] > right.delays[1], "{:?}", right.delays);
        let left = store().filters(0.0, -FRAC_PI_2);
        assert!(energy(&left.taps[0]) > 2.0 * energy(&left.taps[1]));
        // Straight ahead both ears hear the same.
        let ahead = store().filters(0.0, 0.0);
        assert_eq!(ahead.delays[0], ahead.delays[1]);
        let (l, r) = (energy(&ahead.taps[0]), energy(&ahead.taps[1]));
        assert!((l - r).abs() < 1e-6 * l.max(1e-9), "{l} {r}");
    }

    /// A click heard from the right comes out as each ear's response, as
    /// late as that ear hears it, also across a block's end.
    #[test]
    fn a_click_is_heard_as_the_response() {
        let listener = Arc::new(Listener::default());
        let at = [4.0, 0.0, 0.0];
        let (distance, elevation, azimuth) = direction(&listener.get(), at);
        let filters = store()
            .filters(elevation, azimuth)
            .scaled(linear_gain(distance, 16.0));
        let click = BLOCK - 6;
        let mut input = vec![0.0; 3 * BLOCK];
        input[click] = 1.0;
        let one = ChannelCount::new(1).unwrap();
        let rate = SampleRate::new(RATE).unwrap();
        let binaural = Binaural::new(
            SamplesBuffer::new(one, rate, input),
            store(),
            listener,
            at.map(f64::from),
            16.0,
        );
        let heard: Vec<f32> = binaural.collect();
        assert_eq!(heard.len(), 6 * BLOCK);
        for ear in 0..2 {
            let start = click + filters.delays[ear];
            for frame in 0..3 * BLOCK {
                let want = frame
                    .checked_sub(start)
                    .filter(|&k| k < TAPS)
                    .map_or(0.0, |k| filters.taps[ear][TAPS - 1 - k]);
                let got = heard[2 * frame + ear];
                assert!(
                    (got - want).abs() < 1e-7,
                    "ear {ear} frame {frame}: {got} {want}"
                );
            }
        }
    }

    #[test]
    fn a_moving_sound_stays_finite_and_complete() {
        let listener = Arc::new(Listener::default());
        listener.set([0.0; 3], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]);
        let one = ChannelCount::new(1).unwrap();
        let rate = SampleRate::new(RATE).unwrap();
        let noise: Vec<f32> = (0..4000)
            .map(|i| ((i * 7919) % 200) as f32 / 100.0 - 1.0)
            .collect();
        let binaural = Binaural::new(
            SamplesBuffer::new(one, rate, noise),
            store(),
            Arc::clone(&listener),
            [4.0, 0.0, 0.0],
            16.0,
        );
        let mut heard = Vec::new();
        for (i, sample) in binaural.enumerate() {
            if i == 2000 {
                listener.set([0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
            }
            heard.push(sample);
        }
        assert_eq!(heard.len(), 8000);
        assert!(heard.iter().all(|s| s.is_finite()));
        // East of a listener facing north is the right ear.
        let ear = |e: usize| {
            heard[..2000]
                .iter()
                .skip(e)
                .step_by(2)
                .map(|s| s * s)
                .sum::<f32>()
        };
        assert!(ear(1) > ear(0));
    }
}
