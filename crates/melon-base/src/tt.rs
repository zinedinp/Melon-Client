//! TEMP-TIMING
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
static START: OnceLock<Instant> = OnceLock::new();
pub static SHADER: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
pub static PIPE: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
pub static TEX: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
pub static TEXSUB: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
pub fn mark(label: &str) {
    let s = START.get_or_init(Instant::now);
    let f = |c: &[AtomicU64; 2]| (c[0].load(Ordering::Relaxed) as f64 / 1e6, c[1].load(Ordering::Relaxed));
    eprintln!("[TT] {:9.1} ms  {label:<28} shader {:?} pipe {:?} tex {:?} texsub {:?}", s.elapsed().as_secs_f64() * 1e3, f(&SHADER), f(&PIPE), f(&TEX), f(&TEXSUB));
}
pub fn add(c: &[AtomicU64; 2], since: Instant) {
    c[0].fetch_add(since.elapsed().as_nanos() as u64, Ordering::Relaxed);
    c[1].fetch_add(1, Ordering::Relaxed);
}
