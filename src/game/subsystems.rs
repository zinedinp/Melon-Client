//! The subsystems besides drawing, measured together.

use std::time::Instant;

use super::thread_cpu::ThreadTimes;
use crate::audio::stats::{AudioStats, AudioTotals};
use crate::network::play::stats::{NetStats, NetTotals};

/// F3's lines are renewed this often.
const F3_PERIOD_SECONDS: f64 = 1.0;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub at: Instant,
    pub net: NetTotals,
    pub audio: AudioTotals,
    pub threads: ThreadTimes,
}

impl Snapshot {
    pub fn take(net: &NetStats, audio: &AudioStats) -> Self {
        Self {
            at: Instant::now(),
            net: net.totals(),
            audio: audio.totals(),
            threads: ThreadTimes::sample(),
        }
    }
}

/// What happened between two snapshots, per second.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rates {
    pub seconds: f64,
    pub net_kib: f64,
    pub net_packets: f64,
    /// Packets written per second.
    pub net_sent_packets: f64,
    pub net_chunks: f64,
    /// Cores spent decoding packets on the connection thread.
    pub net_decode_cores: f64,
    /// Events waiting for the game loop at the end.
    pub net_backlog: u64,
    /// `ChunkBatchSizeCalculator`'s ms per chunk at the end.
    pub net_ms_per_chunk: f64,
    pub sounds_started: f64,
    pub sounds_playing: u64,
    /// Average ms in `AudioEngine::play` per sound.
    pub sound_play_ms: f64,
    pub sound_decodes: u64,
    pub sound_decode_ms: f64,
    /// Cores busy per thread group, busiest first.
    pub threads: Vec<(String, f64)>,
}

impl Rates {
    pub fn between(from: &Snapshot, to: &Snapshot) -> Self {
        let seconds = to.at.duration_since(from.at).as_secs_f64().max(1e-9);
        let (net_kib, net_packets, net_chunks, net_decode_cores) =
            to.net.rates_since(&from.net, seconds);
        let started = to.audio.started.saturating_sub(from.audio.started);
        let play_nanos = to.audio.play_nanos.saturating_sub(from.audio.play_nanos);
        Self {
            seconds,
            net_kib,
            net_packets,
            net_sent_packets: to.net.sent_packets.saturating_sub(from.net.sent_packets) as f64
                / seconds,
            net_chunks,
            net_decode_cores,
            net_backlog: to.net.backlog,
            net_ms_per_chunk: to.net.nanos_per_chunk / 1e6,
            sounds_started: started as f64 / seconds,
            sounds_playing: to.audio.playing,
            sound_play_ms: if started == 0 {
                0.0
            } else {
                play_nanos as f64 / started as f64 / 1e6
            },
            sound_decodes: to.audio.decodes.saturating_sub(from.audio.decodes),
            sound_decode_ms: to
                .audio
                .decode_nanos
                .saturating_sub(from.audio.decode_nanos) as f64
                / 1e6,
            threads: to.threads.load_since(&from.threads, seconds),
        }
    }

    /// F3's lines.
    pub fn lines(&self, mspt: Option<f64>) -> Vec<String> {
        let mut lines = vec![
            format!(
                "Net: {:.0} KiB/s {:.0} pkt/s {:.1} ch/s, decode {:.1}%, backlog {}, {:.2} ms/ch",
                self.net_kib,
                self.net_packets,
                self.net_chunks,
                self.net_decode_cores * 100.0,
                self.net_backlog,
                self.net_ms_per_chunk,
            ),
            format!(
                "Sound: {} playing, {:.1}/s, {:.3} ms each, {} decoded ({:.1} ms)",
                self.sounds_playing,
                self.sounds_started,
                self.sound_play_ms,
                self.sound_decodes,
                self.sound_decode_ms,
            ),
        ];
        if let Some(mspt) = mspt {
            lines.push(format!("Server: {mspt:.2} mspt"));
        }
        let threads: Vec<String> = self
            .threads
            .iter()
            .filter(|(_, cores)| *cores >= 0.005)
            .map(|(name, cores)| format!("{name} {:.0}%", cores * 100.0))
            .collect();
        for chunk in threads.chunks(3) {
            lines.push(format!("CPU: {}", chunk.join(", ")));
        }
        lines
    }
}

/// F3's subsystem lines, renewed once a second while F3 is open.
#[derive(Default)]
pub struct SubsystemMeter {
    last: Option<Snapshot>,
    lines: Vec<String>,
    /// Packets written and read per second, last second.
    packets: (f64, f64),
}

impl SubsystemMeter {
    pub fn update(&mut self, net: &NetStats, audio: &AudioStats, mspt: Option<f64>) {
        let due = self
            .last
            .as_ref()
            .is_none_or(|last| last.at.elapsed().as_secs_f64() >= F3_PERIOD_SECONDS);
        if !due {
            return;
        }
        let now = Snapshot::take(net, audio);
        if let Some(last) = &self.last {
            let rates = Rates::between(last, &now);
            self.packets = (rates.net_sent_packets, rates.net_packets);
            self.lines = rates.lines(mspt);
        }
        self.last = Some(now);
    }

    /// Forgets the last snapshot, so F3 opened later starts afresh.
    pub fn reset(&mut self) {
        self.last = None;
        self.lines.clear();
        self.packets = (0.0, 0.0);
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Packets written and read per second (`getAverageSentPackets`).
    pub fn packets(&self) -> (f64, f64) {
        self.packets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rates_divide_by_the_time_between() {
        let start = Instant::now();
        let from = Snapshot {
            at: start,
            net: NetTotals::default(),
            audio: AudioTotals::default(),
            threads: ThreadTimes::default(),
        };
        let to = Snapshot {
            at: start + Duration::from_secs(2),
            net: NetTotals {
                bytes: 4096,
                packets: 10,
                chunks: 4,
                ..NetTotals::default()
            },
            audio: AudioTotals {
                started: 4,
                playing: 1,
                play_nanos: 4_000_000,
                ..AudioTotals::default()
            },
            threads: ThreadTimes::default(),
        };
        let rates = Rates::between(&from, &to);
        assert!((rates.net_kib - 2.0).abs() < 1e-9);
        assert!((rates.net_chunks - 2.0).abs() < 1e-9);
        assert!((rates.sounds_started - 2.0).abs() < 1e-9);
        assert!((rates.sound_play_ms - 1.0).abs() < 1e-9);
        assert!(
            rates
                .lines(Some(3.0))
                .iter()
                .any(|l| l.contains("3.00 mspt"))
        );
    }
}
