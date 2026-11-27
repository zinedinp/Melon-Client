//! Vanilla `ChunkBatchSizeCalculator`.

use std::time::Instant;

/// Nanoseconds per chunk assumed before the first batch.
const INITIAL_NANOS_PER_CHUNK: f64 = 2_000_000.0;
/// How many earlier batches the average weighs, at most.
const MAX_OLD_SAMPLES_WEIGHT: u32 = 49;
/// A batch counts as at most this many times faster or slower than the average.
const CLAMP_COEFFICIENT: f64 = 3.0;
/// The time per tick the client offers to spend on chunks.
const BUDGET_NANOS_PER_TICK: f64 = 7_000_000.0;

pub struct ChunkBatchSizeCalculator {
    aggregated_nanos_per_chunk: f64,
    old_samples_weight: u32,
    batch_start: Instant,
}

impl Default for ChunkBatchSizeCalculator {
    fn default() -> Self {
        Self {
            aggregated_nanos_per_chunk: INITIAL_NANOS_PER_CHUNK,
            old_samples_weight: 1,
            batch_start: Instant::now(),
        }
    }
}

impl ChunkBatchSizeCalculator {
    /// `ClientboundChunkBatchStartPacket` arrived.
    pub fn on_batch_start(&mut self) {
        self.batch_start = Instant::now();
    }

    /// `ClientboundChunkBatchFinishedPacket` arrived, closing a batch of `batch_size` chunks.
    pub fn on_batch_finished(&mut self, batch_size: i32) {
        let nanos = self.batch_start.elapsed().as_nanos() as f64;
        self.add_sample(nanos, batch_size);
    }

    fn add_sample(&mut self, batch_nanos: f64, batch_size: i32) {
        if batch_size <= 0 {
            return;
        }
        let per_chunk = batch_nanos / f64::from(batch_size);
        let average = self.aggregated_nanos_per_chunk;
        let clamped = per_chunk.clamp(average / CLAMP_COEFFICIENT, average * CLAMP_COEFFICIENT);
        let weight = f64::from(self.old_samples_weight);
        self.aggregated_nanos_per_chunk = (average * weight + clamped) / (weight + 1.0);
        self.old_samples_weight = (self.old_samples_weight + 1).min(MAX_OLD_SAMPLES_WEIGHT);
    }

    /// The average time a chunk took to take in, for F3.
    pub fn nanos_per_chunk(&self) -> f64 {
        self.aggregated_nanos_per_chunk
    }

    /// What `ServerboundChunkBatchReceivedPacket` reports.
    pub fn desired_chunks_per_tick(&self) -> f32 {
        (BUDGET_NANOS_PER_TICK / self.aggregated_nanos_per_chunk) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_vanillas_rate() {
        let calc = ChunkBatchSizeCalculator::default();
        assert_eq!(calc.desired_chunks_per_tick(), 3.5);
    }

    #[test]
    fn one_fast_batch_moves_the_rate_only_a_third() {
        let mut calc = ChunkBatchSizeCalculator::default();
        calc.add_sample(1_000_000.0, 10);
        let expected = (2_000_000.0 + 2_000_000.0 / 3.0) / 2.0;
        assert!((calc.aggregated_nanos_per_chunk - expected).abs() < 1e-6);
        assert_eq!(calc.old_samples_weight, 2);
    }

    #[test]
    fn settles_on_the_measured_speed() {
        let mut calc = ChunkBatchSizeCalculator::default();
        for _ in 0..500 {
            calc.add_sample(500_000.0 * 8.0, 8);
        }
        assert!((calc.desired_chunks_per_tick() - 14.0).abs() < 0.01);
        assert_eq!(calc.old_samples_weight, MAX_OLD_SAMPLES_WEIGHT);
    }

    #[test]
    fn empty_batches_change_nothing() {
        let mut calc = ChunkBatchSizeCalculator::default();
        calc.add_sample(1.0, 0);
        assert_eq!(calc.aggregated_nanos_per_chunk, INITIAL_NANOS_PER_CHUNK);
        assert_eq!(calc.old_samples_weight, 1);
    }
}
