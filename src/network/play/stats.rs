//! What the connection thread took in, for F3 and the benchmark.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Play packet ids are well below this.
const IDS: usize = 256;

pub struct NetStats {
    bytes: AtomicU64,
    packets: AtomicU64,
    /// Packets written (`Connection.getAverageSentPackets`).
    sent_packets: AtomicU64,
    decode_nanos: AtomicU64,
    events_sent: AtomicU64,
    events_taken: AtomicU64,
    chunks: AtomicU64,
    /// `ChunkBatchSizeCalculator`'s average, as `f64` bits.
    nanos_per_chunk: AtomicU64,
    bytes_by_id: Box<[AtomicU64]>,
    count_by_id: Box<[AtomicU64]>,
}

impl Default for NetStats {
    fn default() -> Self {
        let zeros = || (0..IDS).map(|_| AtomicU64::new(0)).collect();
        Self {
            bytes: AtomicU64::new(0),
            packets: AtomicU64::new(0),
            sent_packets: AtomicU64::new(0),
            decode_nanos: AtomicU64::new(0),
            events_sent: AtomicU64::new(0),
            events_taken: AtomicU64::new(0),
            chunks: AtomicU64::new(0),
            nanos_per_chunk: AtomicU64::new(0),
            bytes_by_id: zeros(),
            count_by_id: zeros(),
        }
    }
}

/// The counters at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetTotals {
    pub bytes: u64,
    pub packets: u64,
    pub sent_packets: u64,
    pub decode_nanos: u64,
    pub chunks: u64,
    /// Events sent to the game loop and not yet taken.
    pub backlog: u64,
    pub nanos_per_chunk: f64,
}

impl NetStats {
    /// One packet of `len` body bytes, decoded in `nanos`.
    pub(super) fn packet(&self, id: i32, len: usize, nanos: u64) {
        self.bytes.fetch_add(len as u64, Relaxed);
        self.packets.fetch_add(1, Relaxed);
        self.decode_nanos.fetch_add(nanos, Relaxed);
        if let Some(index) = usize::try_from(id).ok().filter(|&i| i < IDS) {
            self.bytes_by_id[index].fetch_add(len as u64, Relaxed);
            self.count_by_id[index].fetch_add(1, Relaxed);
        }
    }

    /// One packet written to the server.
    pub(super) fn transmitted(&self) {
        self.sent_packets.fetch_add(1, Relaxed);
    }

    pub(super) fn sent(&self, chunk: bool) {
        self.events_sent.fetch_add(1, Relaxed);
        if chunk {
            self.chunks.fetch_add(1, Relaxed);
        }
    }

    pub(super) fn taken(&self, events: usize) {
        self.events_taken.fetch_add(events as u64, Relaxed);
    }

    pub(super) fn set_nanos_per_chunk(&self, nanos: f64) {
        self.nanos_per_chunk.store(nanos.to_bits(), Relaxed);
    }

    pub fn totals(&self) -> NetTotals {
        let sent = self.events_sent.load(Relaxed);
        let taken = self.events_taken.load(Relaxed);
        NetTotals {
            bytes: self.bytes.load(Relaxed),
            packets: self.packets.load(Relaxed),
            sent_packets: self.sent_packets.load(Relaxed),
            decode_nanos: self.decode_nanos.load(Relaxed),
            chunks: self.chunks.load(Relaxed),
            backlog: sent.saturating_sub(taken),
            nanos_per_chunk: f64::from_bits(self.nanos_per_chunk.load(Relaxed)),
        }
    }

    /// The `n` packet ids that brought the most bytes: (id, bytes, count).
    pub fn top_packets(&self, n: usize) -> Vec<(i32, u64, u64)> {
        let mut all: Vec<(i32, u64, u64)> = (0..IDS)
            .map(|i| {
                (
                    i as i32,
                    self.bytes_by_id[i].load(Relaxed),
                    self.count_by_id[i].load(Relaxed),
                )
            })
            .filter(|&(_, _, count)| count > 0)
            .collect();
        all.sort_by_key(|&(_, bytes, _)| std::cmp::Reverse(bytes));
        all.truncate(n);
        all
    }
}

impl NetTotals {
    /// Per second between `earlier` and `self`, `seconds` apart:
    pub fn rates_since(&self, earlier: &NetTotals, seconds: f64) -> (f64, f64, f64, f64) {
        let per = |now: u64, then: u64| now.saturating_sub(then) as f64 / seconds.max(1e-9);
        (
            per(self.bytes, earlier.bytes) / 1024.0,
            per(self.packets, earlier.packets),
            per(self.chunks, earlier.chunks),
            per(self.decode_nanos, earlier.decode_nanos) / 1e9,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_packets_and_backlog() {
        let stats = NetStats::default();
        stats.packet(0x27, 1000, 50);
        stats.packet(0x27, 500, 50);
        stats.packet(0x05, 10, 1);
        stats.sent(true);
        stats.sent(false);
        stats.taken(1);
        let totals = stats.totals();
        assert_eq!(totals.bytes, 1510);
        assert_eq!(totals.packets, 3);
        assert_eq!(totals.chunks, 1);
        assert_eq!(totals.backlog, 1);
        assert_eq!(stats.top_packets(1), vec![(0x27, 1500, 2)]);
        let (kib, packets, _, _) = totals.rates_since(&NetTotals::default(), 2.0);
        assert!((kib - 1510.0 / 1024.0 / 2.0).abs() < 1e-9);
        assert!((packets - 1.5).abs() < 1e-9);
    }
}
