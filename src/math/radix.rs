//! A radix sort for `u64` keys whose low halves count up: an index into what
//! is sorted, below a 32-bit sort key (the draw lists' distances, the
//! translucent quads').

/// Short lists go to `sort_unstable`: under this, the 4 x 256-entry
/// histograms cost more than the compares they save.
const SMALL: usize = 64;

/// Sorts `keys` as `sort_unstable` does. Their low halves count up, so a
/// stable sort by the high half alone is the same order: four passes of a
/// byte each (`O(n)`, no compares to mispredict), the passes whose byte is
/// the same in every key left out. `scratch` is the second buffer.
pub fn sort(keys: &mut Vec<u64>, scratch: &mut Vec<u64>) {
    if keys.len() <= SMALL || keys.len() > u32::MAX as usize {
        keys.sort_unstable();
        return;
    }
    debug_assert!(keys.windows(2).all(|w| (w[0] as u32) < (w[1] as u32)));
    let mut counts = [[0u32; 256]; 4];
    for &key in keys.iter() {
        for (pass, count) in counts.iter_mut().enumerate() {
            count[(key >> (32 + 8 * pass)) as usize & 0xFF] += 1;
        }
    }
    scratch.clear();
    scratch.resize(keys.len(), 0);
    for (pass, count) in counts.iter().enumerate() {
        if count.iter().any(|&n| n as usize == keys.len()) {
            continue;
        }
        let mut at = [0u32; 256];
        let mut sum = 0;
        for (slot, &n) in at.iter_mut().zip(count) {
            *slot = sum;
            sum += n;
        }
        for &key in keys.iter() {
            let digit = (key >> (32 + 8 * pass)) as usize & 0xFF;
            scratch[at[digit] as usize] = key;
            at[digit] += 1;
        }
        std::mem::swap(keys, scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::xorshift::XorShift;

    /// The radix sort gives what a comparison sort does: few keys and
    /// many, all alike and all different.
    #[test]
    fn sorts_like_sort_unstable() {
        let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
        let mut next = || rng.next_u64();
        let mut scratch = Vec::new();
        for len in [0, 1, 63, 64, 65, 500, 6000] {
            for spread in [1u64, 16, 1 << 20, u64::MAX] {
                let mut keys: Vec<u64> = (0..len)
                    .map(|i| (next() % spread) << 32 | i as u64)
                    .collect();
                let mut want = keys.clone();
                want.sort_unstable();
                sort(&mut keys, &mut scratch);
                assert_eq!(keys, want, "{len} keys spread {spread}");
            }
        }
    }
}
