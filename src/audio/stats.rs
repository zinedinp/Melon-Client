//! What the sound engine did, for F3 and the benchmark: sounds started and

use rodio::{ChannelCount, SampleRate, Source};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::time::Duration;

#[derive(Default)]
pub struct AudioStats {
    started: AtomicU64,
    ended: AtomicU64,
    /// Time in [`super::AudioEngine::play`], decoding included.
    play_nanos: AtomicU64,
    decodes: AtomicU64,
    decode_nanos: AtomicU64,
}

/// The counters at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioTotals {
    pub started: u64,
    pub playing: u64,
    pub play_nanos: u64,
    pub decodes: u64,
    pub decode_nanos: u64,
}

impl AudioStats {
    pub(super) fn played(&self, nanos: u64) {
        self.started.fetch_add(1, Relaxed);
        self.play_nanos.fetch_add(nanos, Relaxed);
    }

    pub(super) fn decoded(&self, nanos: u64) {
        self.decodes.fetch_add(1, Relaxed);
        self.decode_nanos.fetch_add(nanos, Relaxed);
    }

    pub fn totals(&self) -> AudioTotals {
        let ended = self.ended.load(Relaxed);
        let started = self.started.load(Relaxed);
        AudioTotals {
            started,
            playing: started.saturating_sub(ended),
            play_nanos: self.play_nanos.load(Relaxed),
            decodes: self.decodes.load(Relaxed),
            decode_nanos: self.decode_nanos.load(Relaxed),
        }
    }
}

/// `Library`'s channel pools as OpenAL Soft sizes them: its 255 mono
/// sources give `clamp(sqrt(255), 2, 8)` = 8 streaming channels and
/// `clamp(255 - 8, 8, 255)` = 247 static ones.
pub const STATIC_CHANNELS: u32 = 247;
pub const STREAMING_CHANNELS: u32 = 8;

/// `Library.Pool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Static = 0,
    Streaming = 1,
}

impl Pool {
    pub fn limit(self) -> u32 {
        match self {
            Pool::Static => STATIC_CHANNELS,
            Pool::Streaming => STREAMING_CHANNELS,
        }
    }
}

/// `Library.CountingChannelPool`: how many channels of each pool are taken.
#[derive(Default)]
pub struct Pools {
    used: [AtomicU32; 2],
}

impl Pools {
    /// `CountingChannelPool.acquire`: a channel, or `None` when the pool is
    /// full; the sound then does not play, nothing is cut off for it.
    pub fn acquire(self: &Arc<Self>, pool: Pool) -> Option<Slot> {
        let limit = pool.limit();
        self.used[pool as usize]
            .try_update(Relaxed, Relaxed, |used| (used < limit).then_some(used + 1))
            .ok()?;
        Some(Slot {
            pools: Arc::clone(self),
            pool,
        })
    }

    pub fn used(&self, pool: Pool) -> u32 {
        self.used[pool as usize].load(Relaxed)
    }

    /// `Library.getChannelDebugString`: "Sounds: 3/247 + 0/8".
    pub fn debug_string(&self) -> String {
        format!(
            "Sounds: {}/{} + {}/{}",
            self.used(Pool::Static),
            STATIC_CHANNELS,
            self.used(Pool::Streaming),
            STREAMING_CHANNELS
        )
    }
}

/// A taken channel, given back when it is dropped: when the sound ends, or
/// when it never got to play (its file was missing or would not decode).
pub struct Slot {
    pools: Arc<Pools>,
    pool: Pool,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.pools.used[self.pool as usize].fetch_sub(1, Relaxed);
    }
}

/// A started sound as the game can still see it (`SoundManager.isActive`,
/// `stop`): it is active until it ends, is stopped, or fails to start.
#[derive(Debug, Default)]
pub struct Playing {
    ended: AtomicBool,
    stop: AtomicBool,
}

impl Playing {
    pub fn is_active(&self) -> bool {
        !self.ended.load(Relaxed)
    }

    /// `SoundManager.stop(instance)`: it goes quiet at the mixer's next look.
    pub fn stop(&self) {
        self.stop.store(true, Relaxed);
    }
}

/// A [`Playing`] held by the sound: dropping it, when the sound ends or
/// never gets to start, marks the sound ended.
pub(super) struct Watch(pub(super) Arc<Playing>);

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.ended.store(true, Relaxed);
    }
}

/// A sound that counts itself out when the mixer drops it, and frees its
/// channel then, it ends early when its [`Playing`] is stopped.
pub(super) struct Counted<S> {
    inner: S,
    stats: Arc<AudioStats>,
    _slot: Slot,
    watch: Option<Watch>,
    generation: Generation,
}

/// The engine's stop count a sound was started under
/// ([`super::AudioEngine::stop_all`]).
pub(super) struct Generation {
    now: Arc<AtomicU64>,
    started: u64,
}

impl Generation {
    pub(super) fn of(now: &Arc<AtomicU64>) -> Self {
        Self {
            now: Arc::clone(now),
            started: now.load(Relaxed),
        }
    }

    /// Whether everything was stopped since.
    fn stopped(&self) -> bool {
        self.now.load(Relaxed) != self.started
    }
}

impl<S> Counted<S> {
    pub(super) fn new(
        inner: S,
        stats: Arc<AudioStats>,
        slot: Slot,
        watch: Option<Watch>,
        generation: Generation,
    ) -> Self {
        Self {
            inner,
            stats,
            _slot: slot,
            watch,
            generation,
        }
    }
}

impl<S> Drop for Counted<S> {
    fn drop(&mut self) {
        self.stats.ended.fetch_add(1, Relaxed);
    }
}

impl<S: Source> Iterator for Counted<S> {
    type Item = S::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.watch.as_ref().is_some_and(|w| w.0.stop.load(Relaxed)) || self.generation.stopped()
        {
            return None;
        }
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for Counted<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    #[test]
    fn a_dropped_sound_stops_counting() {
        let stats = Arc::new(AudioStats::default());
        stats.played(10);
        let one = std::num::NonZeroU16::new(1).unwrap();
        let rate = std::num::NonZeroU32::new(48_000).unwrap();
        let pools = Arc::new(Pools::default());
        let slot = pools.acquire(Pool::Static).unwrap();
        let sound = Counted::new(
            SamplesBuffer::new(one, rate, vec![0.0; 4]),
            Arc::clone(&stats),
            slot,
            None,
            Generation::of(&Arc::default()),
        );
        assert_eq!(stats.totals().playing, 1);
        assert_eq!(pools.used(Pool::Static), 1);
        assert_eq!(sound.count(), 4);
        assert_eq!(stats.totals().playing, 0);
        assert_eq!(stats.totals().started, 1);
        assert_eq!(pools.used(Pool::Static), 0);
    }

    #[test]
    fn a_stopped_sound_ends_and_says_so() {
        let stats = Arc::new(AudioStats::default());
        let pools = Arc::new(Pools::default());
        let playing = Arc::new(Playing::default());
        let one = std::num::NonZeroU16::new(1).unwrap();
        let rate = std::num::NonZeroU32::new(48_000).unwrap();
        let mut sound = Counted::new(
            SamplesBuffer::new(one, rate, vec![0.0; 4]),
            stats,
            pools.acquire(Pool::Static).unwrap(),
            Some(Watch(Arc::clone(&playing))),
            Generation::of(&Arc::default()),
        );
        assert!(sound.next().is_some());
        playing.stop();
        assert!(sound.next().is_none());
        assert!(playing.is_active());
        drop(sound);
        assert!(!playing.is_active());
    }

    #[test]
    fn stop_all_ends_what_was_started_before_it_only() {
        let stats = Arc::new(AudioStats::default());
        let pools = Arc::new(Pools::default());
        let generation = Arc::new(AtomicU64::new(0));
        let one = std::num::NonZeroU16::new(1).unwrap();
        let rate = std::num::NonZeroU32::new(48_000).unwrap();
        let sound = |stats: &Arc<AudioStats>| {
            Counted::new(
                SamplesBuffer::new(one, rate, vec![0.0; 4]),
                Arc::clone(stats),
                pools.acquire(Pool::Static).unwrap(),
                None,
                Generation::of(&generation),
            )
        };
        let mut old = sound(&stats);
        assert!(old.next().is_some());
        // `SoundEngine.stopAll`: the level changed.
        generation.fetch_add(1, Relaxed);
        assert!(old.next().is_none());
        let mut new = sound(&stats);
        assert!(new.next().is_some());
    }

    #[test]
    fn a_full_pool_turns_sounds_away_until_one_ends() {
        let pools = Arc::new(Pools::default());
        let taken: Vec<Slot> = (0..STATIC_CHANNELS)
            .map(|_| pools.acquire(Pool::Static).expect("free channel"))
            .collect();
        assert!(pools.acquire(Pool::Static).is_none());
        assert!(pools.acquire(Pool::Streaming).is_some());
        assert_eq!(pools.debug_string(), "Sounds: 247/247 + 0/8");
        drop(taken);
        assert_eq!(pools.used(Pool::Static), 0);
        assert!(pools.acquire(Pool::Static).is_some());
    }
}
