//! Decoded sounds as vanilla keeps them: 16-bit samples
//! (`JOrbisAudioStream`'s `AudioFormat(rate, 16, channels)`), half the
//! memory of floats, read back as floats the way OpenAL does.

use rodio::source::SeekError;
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::sync::Arc;
use std::time::Duration;

/// `ChunkedSampleByteBuf.addSample`: a float sample as vanilla stores it.
pub fn quantize(sample: f32) -> i16 {
    ((sample * 32767.5 - 0.5) as i32).clamp(-32768, 32767) as i16
}

/// A decoded sound, cheap to clone: the samples are shared.
#[derive(Clone, Debug)]
pub struct PcmBuffer {
    data: Arc<[i16]>,
    pos: usize,
    channels: ChannelCount,
    rate: SampleRate,
}

impl PcmBuffer {
    /// Interleaved float samples, quantized as vanilla does.
    pub fn from_samples(
        channels: ChannelCount,
        rate: SampleRate,
        samples: impl Iterator<Item = f32>,
    ) -> Self {
        Self {
            data: samples.map(quantize).collect(),
            pos: 0,
            channels,
            rate,
        }
    }

    /// Bytes the samples take.
    pub fn size(&self) -> usize {
        self.data.len() * std::mem::size_of::<i16>()
    }
}

impl Iterator for PcmBuffer {
    type Item = Sample;

    #[inline]
    fn next(&mut self) -> Option<Sample> {
        let sample = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(f32::from(sample) / 32768.0)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.data.len() - self.pos;
        (left, Some(left))
    }
}

impl Source for PcmBuffer {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.data.len() - self.pos)
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.data.len() / usize::from(self.channels.get());
        Some(Duration::from_secs_f64(
            frames as f64 / f64::from(self.rate.get()),
        ))
    }

    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        let channels = usize::from(self.channels.get());
        let frame = (position.as_secs_f64() * f64::from(self.rate.get())) as usize;
        self.pos = (frame * channels).min(self.data.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_quantized_like_vanilla() {
        assert_eq!(quantize(0.0), 0);
        assert_eq!(quantize(1.0), 32767);
        assert_eq!(quantize(-1.0), -32768);
        assert_eq!(quantize(2.0), 32767);
        assert_eq!(quantize(-3.0), -32768);
        // `(int)` truncates toward zero, as Java's cast does.
        assert_eq!(quantize(0.5), 16383);
        assert_eq!(quantize(-0.5), -16384);
    }

    #[test]
    fn a_buffer_plays_back_close_to_what_went_in() {
        let channels = ChannelCount::new(2).unwrap();
        let rate = SampleRate::new(48_000).unwrap();
        let input = [0.0, 0.25, -0.25, 0.999];
        let buffer = PcmBuffer::from_samples(channels, rate, input.into_iter());
        assert_eq!(buffer.size(), 8);
        let out: Vec<f32> = buffer.clone().collect();
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1.0e-4, "{a} {b}");
        }
        assert_eq!(
            buffer.total_duration(),
            Some(Duration::from_secs_f64(2.0 / 48_000.0))
        );
    }
}
